// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Web filter on a minor's PC (security-critical). Layers:
//! 2. Local DNS (`netfilter`) with the core's policy; network adapters point to 127.0.0.1.
//! 3. Firewall: no external encrypted DNS (known public DoT and DoH).
//!
//! Honest limit: the Windows Firewall cannot block port 53 "for everyone except
//! our service" (block wins over allow); querying the router's DNS directly is still
//! possible. ponytail: WFP filters with a higher-weight permit for the agent if the pentest exploits it.

use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use procastinacioff_core::api::Core;
use procastinacioff_core::model::Role;

use crate::clock::now_clock;

/// Known public DoH resolvers (extend with the list maintained in the repository).
const DOH_IPS: &str = "1.1.1.1,1.0.0.1,8.8.8.8,8.8.4.4,9.9.9.9,149.112.112.112,94.140.14.14,94.140.15.15,\
208.67.222.222,208.67.220.220,2606:4700:4700::1111,2606:4700:4700::1001,2001:4860:4860::8888,\
2001:4860:4860::8844,2620:fe::fe";
const RULES: [&str; 3] = ["PCO-block-DoT", "PCO-block-DoT-udp", "PCO-block-DoH"];
const REPOINT_EVERY: Duration = Duration::from_secs(5 * 60);
const LIST_EVERY: Duration = Duration::from_secs(24 * 3600);
const LIST_RETRY: Duration = Duration::from_secs(3600);

fn ps(script: &str) -> std::io::Result<String> {
    let out = Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script]).output()?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn netsh(args: &[&str]) {
    let _ = Command::new("netsh").args(args).output();
}

/// Current DNS servers of the network (except ours). Called BEFORE pointing the adapters to 127.0.0.1.
fn current_upstreams() -> Vec<IpAddr> {
    ps("Get-DnsClientServerAddress | ForEach-Object { $_.ServerAddresses }")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .filter(|ip: &IpAddr| !ip.is_loopback())
        .collect()
}

/// Real DNS servers that allowed queries are forwarded to: the network's when the filter is enabled.
/// Saved in `upstream.txt` the first time (afterwards the adapters already point to us).
/// ponytail: if the computer changes network, the saved DNS servers are used; if they do not answer,
/// resolution fails and shows up in testing (improvement: read each network's DHCP DNS).
fn upstreams(data_dir: &Path) -> Vec<SocketAddr> {
    let file = data_dir.join("upstream.txt");
    if !file.exists() {
        let found: Vec<String> = current_upstreams().iter().map(IpAddr::to_string).collect();
        if !found.is_empty() {
            let _ = std::fs::write(&file, found.join("\n"));
        }
    }
    std::fs::read_to_string(&file)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse::<IpAddr>().ok())
        .map(|ip| SocketAddr::new(ip, 53))
        .collect()
}

fn point_to_local() {
    let _ = ps("Get-NetAdapter | Where-Object Status -eq 'Up' | ForEach-Object { \
        Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ServerAddresses ('127.0.0.1','::1') }; Clear-DnsClientCache");
}

fn firewall_apply() {
    firewall_remove();
    netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=PCO-block-DoT",
        "dir=out",
        "action=block",
        "protocol=TCP",
        "remoteport=853",
    ]);
    netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=PCO-block-DoT-udp",
        "dir=out",
        "action=block",
        "protocol=UDP",
        "remoteport=853",
    ]);
    let ips = format!("remoteip={DOH_IPS}");
    netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=PCO-block-DoH",
        "dir=out",
        "action=block",
        "protocol=TCP",
        "remoteport=443",
        &ips,
    ]);
}

fn firewall_remove() {
    for name in RULES {
        netsh(&["advfirewall", "firewall", "delete", "rule", &format!("name={name}")]);
    }
}

/// Undoes everything: DNS via DHCP and no rules. Called by the uninstaller (`agent restore-dns`).
pub fn restore(data_dir: &Path) {
    let _ = ps(
        "Get-NetAdapter | ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_.ifIndex -ResetServerAddresses }; \
        Clear-DnsClientCache",
    );
    firewall_remove();
    let _ = std::fs::remove_file(data_dir.join("upstream.txt"));
}

/// Starts the DNS server on its own runtime (127.0.0.1:53 and [::1]:53).
fn start_dns(core: Arc<Core>, upstreams: Vec<SocketAddr>) {
    std::thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => rt,
            Err(e) => return tracing::error!(error = %e, "no runtime for the local DNS"),
        };
        rt.block_on(async move {
            let clock_core = core.clone();
            let clock: procastinacioff_netfilter::ClockFn = Arc::new(move || now_clock(&clock_core));
            let mut tasks = Vec::new();
            for addr in ["127.0.0.1:53", "[::1]:53"] {
                match tokio::net::UdpSocket::bind(addr).await {
                    Ok(sock) => tasks.push(tokio::spawn(procastinacioff_netfilter::serve(
                        sock,
                        core.clone(),
                        upstreams.clone(),
                        clock.clone(),
                    ))),
                    // Another program (e.g. "Internet Connection Sharing") holds port 53.
                    Err(e) => tracing::error!(error = %e, "could not open port 53"),
                }
            }
            for t in tasks {
                if let Ok(Err(e)) = t.await {
                    tracing::error!(error = %e, "the local DNS stopped");
                }
            }
        });
    });
}

/// Every minute: if this PC belongs to a minor, enables (once) the local DNS and the firewall, re-points
/// the adapters every 5 min (another network, manual changes) and refreshes the category list daily.
pub fn run(core: Arc<Core>, data_dir: std::path::PathBuf, stop: Arc<AtomicBool>) {
    let mut started = false;
    let (mut next_point, mut next_list) = (Instant::now(), Instant::now());
    while !stop.load(Ordering::Relaxed) {
        if core.my_role() == Some(Role::Minor) {
            if !started {
                start_dns(core.clone(), upstreams(&data_dir));
                firewall_apply();
                started = true;
                tracing::info!("family-mode web filter active");
            }
            if Instant::now() >= next_point {
                point_to_local();
                next_point = Instant::now() + REPOINT_EVERY;
            }
            if Instant::now() >= next_list {
                let wait = match core.refresh_category_list() {
                    Ok(_) => LIST_EVERY,
                    Err(e) => {
                        tracing::warn!(error = %e, "category list not updated");
                        LIST_RETRY
                    }
                };
                next_list = Instant::now() + wait;
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}
