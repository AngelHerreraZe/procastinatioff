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

use std::net::IpAddr;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use procastinatioff_core::api::Core;
use procastinatioff_core::model::Role;

use crate::common::{load_bundled_list, start_dns, upstreams};

/// Known public DoH resolvers (extend with the list maintained in the repository).
const DOH_IPS: &str = "1.1.1.1,1.0.0.1,8.8.8.8,8.8.4.4,9.9.9.9,149.112.112.112,94.140.14.14,94.140.15.15,\
208.67.222.222,208.67.220.220,2606:4700:4700::1111,2606:4700:4700::1001,2001:4860:4860::8888,\
2001:4860:4860::8844,2620:fe::fe";
const RULES: [&str; 3] = ["PCO-block-DoT", "PCO-block-DoT-udp", "PCO-block-DoH"];
const REPOINT_EVERY: Duration = Duration::from_secs(5 * 60);

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

/// Every minute: if this PC belongs to a minor, enables (once) the local DNS, the firewall and the
/// category list, and re-points the adapters every 5 min (another network, manual changes).
pub fn run(core: Arc<Core>, data_dir: std::path::PathBuf, install_dir: std::path::PathBuf, stop: Arc<AtomicBool>) {
    let mut started = false;
    let mut next_point = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if core.my_role() == Some(Role::Minor) {
            if !started {
                load_bundled_list(&core, &install_dir);
                start_dns(core.clone(), upstreams(&data_dir, current_upstreams));
                firewall_apply();
                started = true;
                tracing::info!("family-mode web filter active");
            }
            if Instant::now() >= next_point {
                point_to_local();
                next_point = Instant::now() + REPOINT_EVERY;
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}
