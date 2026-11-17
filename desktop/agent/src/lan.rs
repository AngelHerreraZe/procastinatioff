// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! LAN discovery (DNS-SD over mDNS): announces this PC under the profile's rotating tag of the day
//! and tells the core about the profile's devices it finds. SECURITY: no names, no stable keys and
//! no TXT records: instance and host names are the tag, which changes every day.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use procastinatioff_core::api::Core;

use crate::clock::now_clock;

/// UDP port of the service (QUIC). The installer opens it in the firewall, Private profile only.
pub const PORT: u16 = 47800;
const SERVICE: &str = "_pco._udp.local.";

/// Runs until `stop`. `wake`: asks the sync loop to sync now (a device of the profile appeared).
pub fn run(core: Arc<Core>, stop: Arc<AtomicBool>, wake: Sender<()>) {
    let mdns = match ServiceDaemon::new() {
        Ok(d) => d,
        Err(e) => return tracing::error!(error = %e, "no mDNS: peers must be paired again to be found"),
    };
    let found = match mdns.browse(SERVICE) {
        Ok(r) => r,
        Err(e) => return tracing::error!(error = %e, "mDNS browse failed"),
    };
    let mut announced: Option<String> = None;
    while !stop.load(Ordering::Relaxed) {
        let now = now_clock(&core).now_ms;
        let tag = core.discovery_tag(now);
        if tag != announced {
            if let Some(old) = &announced {
                let _ = mdns.unregister(&format!("{old}.{SERVICE}"));
            }
            if let Some(t) = &tag {
                // Empty address + auto: the daemon announces this PC's current interface addresses.
                let info = ServiceInfo::new(
                    SERVICE,
                    t,
                    &format!("{t}.local."),
                    "",
                    PORT,
                    None::<std::collections::HashMap<String, String>>,
                )
                .map(ServiceInfo::enable_addr_auto);
                if let Err(e) = info.and_then(|i| mdns.register(i)) {
                    tracing::warn!(error = %e, "could not announce over mDNS");
                }
            }
            announced = tag;
        }
        if let Ok(ServiceEvent::ServiceResolved(s)) = found.recv_timeout(Duration::from_secs(30)) {
            let Some(name) = s.fullname.strip_suffix(&format!(".{SERVICE}")) else { continue };
            let seen =
                s.addresses.iter().any(|ip| core.lan_peer_seen(now, name.into(), ip.to_ip_addr().to_string(), s.port));
            if seen {
                let _ = wake.send(());
            }
        }
    }
    let _ = mdns.shutdown();
}
