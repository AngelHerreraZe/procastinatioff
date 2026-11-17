// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Web filter on a minor's Linux computer (security-critical). Layers:
//! 2. Local DNS (`netfilter`) on 127.0.0.1:53; systemd-resolved sends every name to it.
//! 3. nftables: people's accounts and systemd-resolved cannot ask any other DNS, nor use DoT or
//!    the known public DoH. Unlike Windows, Linux limits DNS per account: only root (this service)
//!    reaches the network's real DNS.
//!
//! ponytail: applies to every account with uid ≥ 1000 on the computer (a minor's computer). Per
//! account when shared family computers show up in the pilot.

use std::io::Write;
use std::net::IpAddr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use procastinatioff_core::api::Core;
use procastinatioff_core::model::Role;

use super::policy::FIRST_USER_UID;
use crate::common::{load_bundled_list, start_dns, upstreams};

const RESOLVED_DROPIN: &str = "/etc/systemd/resolved.conf.d/procastinatioff.conf";
const RESOLVED_CONF: &str = "[Resolve]\nDNS=127.0.0.1\nDomains=~.\nDNSOverTLS=no\nDNSSEC=no\n";
/// Known public DoH resolvers (the same list as Windows).
const DOH4: [&str; 10] = [
    "1.1.1.1",
    "1.0.0.1",
    "8.8.8.8",
    "8.8.4.4",
    "9.9.9.9",
    "149.112.112.112",
    "94.140.14.14",
    "94.140.15.15",
    "208.67.222.222",
    "208.67.220.220",
];
const DOH6: [&str; 5] =
    ["2606:4700:4700::1111", "2606:4700:4700::1001", "2001:4860:4860::8888", "2001:4860:4860::8844", "2620:fe::fe"];
const REAPPLY_EVERY: Duration = Duration::from_secs(5 * 60);

/// The table, replaced atomically when loaded with `nft -f -`. `resolver`: systemd-resolved's
/// account, if it exists (it must only talk to our DNS too).
pub fn nft_rules(resolver: Option<&str>) -> String {
    let mut who = vec![format!("meta skuid >= {FIRST_USER_UID}")];
    if let Some(r) = resolver {
        who.push(format!("meta skuid \"{r}\""));
    }
    let mut rules = String::new();
    for w in &who {
        for proto in ["udp", "tcp"] {
            rules += &format!("    {w} {proto} dport 53 ip daddr != 127.0.0.0/8 drop\n");
            rules += &format!("    {w} {proto} dport 53 ip6 daddr != ::1 drop\n");
        }
    }
    let user = &who[0];
    rules += &format!("    {user} meta l4proto {{ tcp, udp }} th dport 853 drop\n");
    rules += &format!("    {user} ip daddr @doh4 meta l4proto {{ tcp, udp }} th dport 443 drop\n");
    rules += &format!("    {user} ip6 daddr @doh6 meta l4proto {{ tcp, udp }} th dport 443 drop\n");
    format!(
        "table inet procastinatioff\ndelete table inet procastinatioff\n\
table inet procastinatioff {{\n  set doh4 {{ type ipv4_addr; elements = {{ {} }} }}\n  set doh6 {{ type ipv6_addr; elements = {{ {} }} }}\n  chain output {{\n    type filter hook output priority 0; policy accept;\n{rules}  }}\n}}\n",
        DOH4.join(", "),
        DOH6.join(", ")
    )
}

fn resolver_account() -> Option<&'static str> {
    let passwd = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().any(|l| l.starts_with("systemd-resolve:")).then_some("systemd-resolve")
}

fn nft(script: &str) -> std::io::Result<bool> {
    let mut child = Command::new("nft").args(["-f", "-"]).stdin(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    child.stdin.take().ok_or_else(|| std::io::Error::other("no stdin"))?.write_all(script.as_bytes())?;
    Ok(child.wait()?.success())
}

/// The network's DNS servers as systemd-resolved knows them (DHCP), except ours.
fn current_upstreams() -> Vec<IpAddr> {
    std::fs::read_to_string("/run/systemd/resolve/resolv.conf")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix("nameserver")?.trim().parse().ok())
        .filter(|ip: &IpAddr| !ip.is_loopback())
        .collect()
}

fn restart_resolved() {
    let _ = Command::new("systemctl").args(["restart", "systemd-resolved"]).status();
}

/// Points the system to the local DNS and closes the ways around it. Idempotent.
fn apply() {
    if std::fs::read_to_string(RESOLVED_DROPIN).ok().as_deref() != Some(RESOLVED_CONF) {
        let written = std::fs::create_dir_all("/etc/systemd/resolved.conf.d")
            .and_then(|_| std::fs::write(RESOLVED_DROPIN, RESOLVED_CONF));
        match written {
            Ok(()) => restart_resolved(),
            Err(e) => tracing::error!(error = %e, "could not point systemd-resolved to the local DNS"),
        }
    }
    match nft(&nft_rules(resolver_account())) {
        Ok(true) => {}
        Ok(false) => tracing::error!("nftables rejected the DNS rules"),
        Err(e) => tracing::error!(error = %e, "nft is not available"),
    }
}

/// Undoes everything. The package calls it before removing the agent (`agent restore-dns`).
pub fn restore(data_dir: &Path) {
    if std::fs::remove_file(RESOLVED_DROPIN).is_ok() {
        restart_resolved();
    }
    let _ = nft("delete table inet procastinatioff\n");
    let _ = std::fs::remove_file(data_dir.join("upstream.txt"));
}

/// Every minute: if this computer belongs to a minor, enables (once) the local DNS and the list,
/// and re-applies resolved + nftables every 5 min (another network, manual changes as root).
pub fn run(core: Arc<Core>, data_dir: std::path::PathBuf, install_dir: std::path::PathBuf, stop: Arc<AtomicBool>) {
    let mut started = false;
    let mut next_apply = Instant::now();
    while !stop.load(Ordering::Relaxed) {
        if core.my_role() == Some(Role::Minor) {
            if !started {
                load_bundled_list(&core, &install_dir);
                // Read the real DNS servers BEFORE pointing the system to ourselves.
                start_dns(core.clone(), upstreams(&data_dir, current_upstreams));
                started = true;
                tracing::info!("family-mode web filter active");
            }
            if Instant::now() >= next_apply {
                apply();
                next_apply = Instant::now() + REAPPLY_EVERY;
            }
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_cover_users_and_the_resolver() {
        let r = nft_rules(Some("systemd-resolve"));
        assert!(r.starts_with("table inet procastinatioff\ndelete table inet procastinatioff\n"), "atomic replace");
        assert!(r.contains("meta skuid >= 1000 udp dport 53 ip daddr != 127.0.0.0/8 drop"));
        assert!(r.contains("meta skuid \"systemd-resolve\" tcp dport 53 ip6 daddr != ::1 drop"));
        assert!(r.contains("meta skuid >= 1000 meta l4proto { tcp, udp } th dport 853 drop"));
        assert!(!r.contains("\"systemd-resolve\" ip daddr @doh4"), "the resolver only needs port 53 closed");
        assert!(!nft_rules(None).contains("systemd-resolve"));
    }

    /// The real `nft` accepts the ruleset (check mode: nothing is changed).
    #[test]
    #[ignore = "needs root and nft"]
    fn nft_accepts_the_rules() {
        let mut child = Command::new("nft").args(["-c", "-f", "-"]).stdin(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(nft_rules(resolver_account()).as_bytes()).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
