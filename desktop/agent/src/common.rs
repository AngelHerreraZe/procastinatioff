// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the Windows and Linux services share: syncing, downtime and clock-jump detection,
//! heartbeats, the 60 s grace and the IPC line loop. Each platform only adds how it blocks.

use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use procastinacioff_core::api::Core;
use procastinacioff_core::clock::Clock;
use procastinacioff_core::model::TamperKind;
use procastinacioff_ipc::{AgentStatus, MAX_LINE};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};

use crate::clock::{device_now_ms, mono_ms, now_clock};

pub type Shared = Arc<Mutex<AgentStatus>>;

/// Domains always needed to sign in to study platforms (browser allowlist in FOCUS).
pub const LOGIN_DOMAINS: [&str; 2] = ["accounts.google.com", "login.microsoftonline.com"];

pub const TICK: Duration = Duration::from_millis(300);
const GRACE: Duration = Duration::from_secs(60);
const HEARTBEAT_EVERY: Duration = Duration::from_secs(15 * 60);

pub fn new_shared() -> Shared {
    Arc::new(Mutex::new(AgentStatus {
        core: None,
        blocked_app: None,
        blocked_at_ms: None,
        grace_until_ms: None,
        synced_at_ms: None,
    }))
}

fn alive_file(data_dir: &Path) -> PathBuf {
    data_dir.join("alive")
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// At startup: was the service down while the computer was on? (uptime = monotonic clock)
pub fn check_downtime(core: &Core, data_dir: &Path) {
    let last: i64 = std::fs::read_to_string(alive_file(data_dir)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
    let gap = now_ms() - last;
    if last > 0 && gap > 5 * 60_000 && mono_ms() > gap {
        let _ = core.report_tamper(now_clock(core), TamperKind::ServiceStopped);
    }
}

/// Syncs with the profile's devices on the LAN every 30 s, or as soon as mDNS finds one (`woken`).
/// Unreachable devices are normal (phones leave home): each device blocks with its own copy.
pub fn sync_loop(core: Arc<Core>, shared: Shared, stop: Arc<AtomicBool>, woken: Receiver<()>) {
    while !stop.load(Ordering::Relaxed) {
        match core.sync_now(device_now_ms(), mono_ms()) {
            Ok(r) if r.peers > 0 => shared.lock().unwrap_or_else(|p| p.into_inner()).synced_at_ms = Some(now_ms()),
            Ok(_) => {}
            Err(e) => tracing::debug!(error = %e, "sync failed (retrying)"),
        }
        // While joining a profile the UI waits for the other device to confirm: ask more often.
        let wait = if core.members().is_empty() { Duration::from_secs(3) } else { Duration::from_secs(30) };
        let _ = woken.recv_timeout(wait);
    }
}

/// Every tick: clock jumps (> 2 min against the monotonic clock) are tampering, a heartbeat every
/// 15 min, and the "alive" mark that `check_downtime` reads after a restart.
pub struct Upkeep {
    last_wall: i64,
    last_mono: Instant,
    last_hb: Instant,
}

impl Upkeep {
    pub fn new() -> Self {
        Self { last_wall: now_ms(), last_mono: Instant::now(), last_hb: Instant::now() - HEARTBEAT_EVERY }
    }

    pub fn tick(&mut self, core: &Core, clock: Clock, data_dir: &Path) {
        let (wall, mono) = (now_ms(), Instant::now());
        let drift = (wall - self.last_wall) - mono.duration_since(self.last_mono).as_millis() as i64;
        if drift.abs() > 120_000 {
            let _ = core.report_tamper(clock, TamperKind::ClockChanged);
        }
        (self.last_wall, self.last_mono) = (wall, mono);
        if self.last_hb.elapsed() > HEARTBEAT_EVERY {
            self.last_hb = Instant::now();
            let _ = core.heartbeat(clock);
        }
        let _ = std::fs::write(alive_file(data_dir), now_ms().to_string());
    }
}

/// Going from a permissive to a restrictive state gives 60 s of grace ("save your work").
#[derive(Default)]
pub struct Grace {
    since: Option<Instant>,
}

impl Grace {
    /// Updates the state and returns how much grace is left (zero = block now). Publishes it for the UI.
    pub fn update(&mut self, permissive: bool, clock: Clock, shared: &Shared) -> Duration {
        self.since = if permissive { None } else { self.since.or(Some(Instant::now())) };
        let left = self.since.map(|t| GRACE.saturating_sub(t.elapsed())).unwrap_or_default();
        let mut s = shared.lock().unwrap_or_else(|p| p.into_inner());
        s.grace_until_ms = (!permissive && !left.is_zero()).then(|| clock.now_ms + left.as_millis() as i64);
        if permissive {
            s.blocked_app = None;
            s.blocked_at_ms = None;
        }
        left
    }
}

/// Tells the UI (shield) which app was just blocked. SECURITY: the name never goes to the log.
pub fn note_blocked(shared: &Shared, app: String, now_ms: i64) {
    let mut s = shared.lock().unwrap_or_else(|p| p.into_inner());
    s.blocked_app = Some(app);
    s.blocked_at_ms = Some(now_ms);
}

/// One trusted UI connection: one JSON request per line, one JSON response per line.
pub async fn serve_lines<S>(stream: S, core: Arc<Core>, shared: Shared)
where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    let (reader, mut writer) = tokio::io::split(stream);
    // Line limit: a client cannot exhaust the service's memory.
    let mut lines = BufReader::new(reader.take(MAX_LINE as u64 * 64)).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let (core, shared) = (core.clone(), shared.clone());
        let reply = tokio::task::spawn_blocking(move || {
            let agent = shared.lock().unwrap_or_else(|p| p.into_inner()).clone();
            procastinacioff_core::desktop::handle_json(&core, now_clock(&core), agent, &line)
        })
        .await;
        let Ok(mut out) = reply else { break };
        out.push('\n');
        if writer.write_all(out.as_bytes()).await.is_err() {
            break;
        }
    }
}

// ── Family mode: the local DNS filter (the platform points the system to it) ──

/// Starts the DNS server on its own runtime (127.0.0.1:53 and [::1]:53).
pub fn start_dns(core: Arc<Core>, upstreams: Vec<SocketAddr>) {
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

/// The signed category list ships with the app (`lists/` next to the service): updating it means
/// updating the app. Without a valid signature it is not loaded and only the allowlist modes filter.
pub fn load_bundled_list(core: &Core, install_dir: &Path) {
    let dir = install_dir.join("lists");
    let (data, sig) =
        (std::fs::read(dir.join("categories.txt")), std::fs::read_to_string(dir.join("categories.txt.minisig")));
    match (data, sig) {
        (Ok(data), Ok(sig)) => match core.load_category_list(data, sig) {
            Ok(n) => tracing::info!(domains = n, "category list loaded"),
            Err(e) => tracing::warn!(error = %e, "category list rejected"),
        },
        _ => tracing::warn!("no category list in the installation"),
    }
}

/// Real DNS servers that allowed queries are forwarded to: the network's when the filter is enabled.
/// Saved in `upstream.txt` the first time (afterwards the system already points to us).
/// ponytail: if the computer changes network, the saved DNS servers are used; if they do not answer,
/// resolution fails and shows up in testing (improvement: read each network's DHCP DNS).
pub fn upstreams(data_dir: &Path, current: impl FnOnce() -> Vec<IpAddr>) -> Vec<SocketAddr> {
    let file = data_dir.join("upstream.txt");
    if !file.exists() {
        let found: Vec<String> = current().iter().map(IpAddr::to_string).collect();
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
