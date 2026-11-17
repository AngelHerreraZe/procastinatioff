// End-to-end tests: several devices in one process, talking real QUIC over this machine's
// addresses. mDNS is simulated with `discovery_tag` + `lan_peer_seen` (what Android NSD, Bonjour
// and the desktop agent do). Needs a private IPv4 address on some interface (for the QR).
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use procastinatioff_core::api::{Core, FamilyAlert};
use procastinatioff_core::clock::Clock;
use procastinatioff_core::crypto::SecretStore;
use procastinatioff_core::error::CoreError;
use procastinatioff_core::model::{
    Change, Config, ContactAlert, DayState, Effective, Method, NoticeKind, OfflineAsk, RequestKind, Role, TamperKind,
};
use procastinatioff_core::peer::PeerNet;

#[derive(Default)]
struct Mem(Mutex<BTreeMap<String, Vec<u8>>>);
impl SecretStore for Mem {
    fn load(&self, n: String) -> Result<Option<Vec<u8>>, CoreError> {
        Ok(self.0.lock().unwrap().get(&n).cloned())
    }
    fn save(&self, n: String, v: Vec<u8>) -> Result<(), CoreError> {
        self.0.lock().unwrap().insert(n, v);
        Ok(())
    }
}

fn device(name: &str) -> Arc<Core> {
    device_with(name, Arc::new(Mem::default()))
}

/// A `Core` with a given keystore (the contact channel shares the device keys).
fn device_with(name: &str, secrets: Arc<Mem>) -> Arc<Core> {
    let dir = std::env::temp_dir().join(format!("pco-it-{}-{}-{name}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::open(dir.join("db").to_string_lossy().into(), secrets).unwrap()
}

fn rand_suffix() -> u64 {
    use std::hash::{BuildHasher, RandomState};
    RandomState::new().hash_one(std::time::Instant::now())
}

const MONDAY_9: Clock = Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 };
const MONO: i64 = 5_000_000;

fn sync(c: &Core, clock: Clock) -> u32 {
    c.sync_now(clock.now_ms, MONO).unwrap().peers
}

/// What mDNS does: `who` announces itself (listening) and `seer` finds it.
fn see(seer: &Core, who: &Arc<Core>, clock: Clock) {
    who.clone().start_listening(0).unwrap();
    let tag = who.discovery_tag(clock.now_ms).expect("member of a profile");
    let port = who.listening_port().unwrap();
    assert!(seer.lan_peer_seen(clock.now_ms, tag, "127.0.0.1".into(), port), "same profile");
}

/// Pairs `joiner` to `inviter`'s profile with the given role; the joiner fetches the history.
fn pair(inviter: &Arc<Core>, joiner: &Core, label: &str, role: Option<Role>, clock: Clock) {
    let qr = inviter.clone().pairing_offer().unwrap();
    let code = joiner.pairing_join(qr, label.into()).unwrap();
    assert_eq!(inviter.pairing_poll().unwrap().as_deref(), Some(code.as_str()), "the 6-digit codes must match");
    assert!(joiner.members().is_empty());
    inviter.pairing_confirm(clock, role).unwrap();
    assert_eq!(sync(joiner, clock), 1);
    assert!(!joiner.members().is_empty(), "the joiner got the profile");
}

#[test]
fn phone_plans_and_completes_goal_and_pc_unlocks() {
    let (phone, pc) = (device("phone"), device("pc"));
    pc.set_time_source(true).unwrap();
    phone.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
    pair(&phone, &pc, "PC", None, MONDAY_9);
    assert!(pc.members().iter().any(|m| m.label == "PC" && m.time_source && !m.node_id.is_empty()));

    let st = pc.status(MONDAY_9).unwrap();
    assert_eq!(st.state, DayState::Locked);
    assert!(st.needs_plan, "on waking up, the PC asks for the day plan");

    // The plan set on the phone reaches the PC (the PC asks the phone)
    phone.plan_day(MONDAY_9, vec![]).unwrap();
    sync(&pc, MONDAY_9);
    assert!(!pc.status(MONDAY_9).unwrap().needs_plan);

    // Now the phone finds the PC on the LAN and pushes the completed goal itself
    see(&phone, &pc, MONDAY_9);
    phone.complete_goal(MONDAY_9, "read-30".into(), Method::HonorDeclaration, 100, 30).unwrap();
    let report = phone.sync_now(MONDAY_9.now_ms, MONO).unwrap();
    assert_eq!((report.peers, report.sent), (1, 1));
    assert_eq!(pc.status(MONDAY_9).unwrap().state, DayState::Free, "the goal completed on the phone unlocks the PC");
}

#[test]
fn trusted_time_follows_the_pc_when_the_phone_clock_moves() {
    let (phone, pc) = (device("time-phone"), device("time-pc"));
    pc.set_time_source(true).unwrap();
    let real = MONDAY_9.now_ms;
    pc.trusted_now_ms(real, MONO); // the service asks for the time every second
    pc.create_profile(MONDAY_9, "PC".into(), Config::example()).unwrap();
    pair(&pc, &phone, "Phone", None, MONDAY_9);
    // The user moves the phone clock 9 hours forward; it syncs with the PC (the time source).
    let nine_hours = 9 * 3_600_000;
    phone.sync_now(real + nine_hours, MONO).unwrap();
    let trusted = phone.trusted_now_ms(real + nine_hours + 10_000, MONO + 10_000);
    assert!((trusted - (real + 10_000)).abs() < 5_000, "trusted time follows the PC, not the phone clock");
    // The moved clock is recorded as tampering once, and the PC hears about it.
    phone.sync_now(real + nine_hours + 20_000, MONO + 20_000).unwrap();
    let clock_changed = |c: &Core| {
        c.family_alerts(MONDAY_9)
            .into_iter()
            .filter(|a| a.alert == FamilyAlert::Tamper { what: TamperKind::ClockChanged })
            .count()
    };
    assert_eq!(clock_changed(&phone), 1);
    assert_eq!(clock_changed(&pc), 1);
}

#[test]
fn trusted_contact_approves_a_change_and_the_pc_applies_it() {
    let clock = MONDAY_9;
    let secrets = Arc::new(Mem::default());
    let (me, channel, laura, pc) =
        (device_with("c-me", secrets.clone()), device_with("c-channel", secrets), device("c-laura"), device("c-pc"));
    me.create_profile(clock, "Phone".into(), Config::example()).unwrap();
    pair(&me, &pc, "PC", None, clock);
    channel.create_contact_channel(clock, "Phone".into()).unwrap();
    pair(&channel, &laura, "Laura", None, clock);
    assert_eq!(laura.my_role(), Some(Role::TrustedContact));
    me.forward_to_contact(channel.clone(), clock).unwrap(); // lists Laura in the profile

    // "My changes need my contact": tightening, immediate
    let mut cfg = me.active_config(clock).unwrap();
    cfg.auth.contact_required = true;
    assert_eq!(me.set_config(clock, cfg.clone()).unwrap(), clock.now_ms);
    let mut laxer = cfg.clone();
    laxer.permits_per_day = 5;
    let refused = me.set_config(clock, laxer.clone()).unwrap_err();
    assert_eq!(refused.code(), "denied.needs_contact");

    // Ask → Laura sees only the typed line → approves "today"
    let id = me.request_change(clock, laxer.clone(), channel.clone()).unwrap();
    sync(&laura, clock);
    let asked = laura.pending_requests();
    let RequestKind::ConfigChange { changes, .. } = &asked[0].what else { panic!("{asked:?}") };
    assert_eq!(changes, &vec![Change::Permits { from: 3, to: 5 }]);
    assert_eq!(
        laura.take_notices().unwrap().into_iter().map(|n| n.kind).collect::<Vec<_>>(),
        vec![NoticeKind::RequestReceived]
    );
    laura.contact_decide(clock, id, true, Effective::Now).unwrap();
    // The channel pulls (what it receives as the dialing side is applied before sync_now returns)
    see(&channel, &laura, clock);
    sync(&channel, clock);
    me.forward_to_contact(channel.clone(), clock).unwrap();
    assert_eq!(me.active_config(clock).unwrap().permits_per_day, 5, "applies at once");
    assert_eq!(
        channel.take_notices().unwrap().into_iter().map(|n| n.kind).collect::<Vec<_>>(),
        vec![NoticeKind::RequestAnswered { approved: true }]
    );
    // The PC checks Laura's signature itself and applies it too; Laura is never its peer.
    sync(&pc, clock);
    assert_eq!(pc.active_config(clock).unwrap().permits_per_day, 5);
    assert!(pc.members().iter().any(|m| m.role == Role::TrustedContact && m.node_id.is_empty()));
    assert!(laura.status(clock).is_none(), "the contact receives no configuration or goals");

    // Without network: a code dictated to Laura gives one more pass
    let later = Clock { now_ms: clock.now_ms + 60_000, ..clock };
    let before = me.status(later).unwrap().permits_left;
    let code = me.offline_request(later, OfflineAsk::ExtraPermit { minutes: 20 }).unwrap();
    let answer = laura.offline_answer(code).unwrap();
    assert_eq!(answer.ask, OfflineAsk::ExtraPermit { minutes: 20 });
    assert_eq!(me.offline_redeem(later, answer.response, Some(channel.clone())).unwrap().permits_left, before + 1);

    // Leaving without Laura's approval: 7 days, and she hears about it (and about the code)
    let mut exit = me.active_config(later).unwrap();
    exit.auth.contact_required = false;
    assert_eq!(me.set_config(later, exit).unwrap(), later.now_ms + 7 * 24 * 3_600_000);
    me.emergency(later).unwrap();
    me.forward_to_contact(channel.clone(), later).unwrap();
    sync(&laura, later);
    let alerts: Vec<ContactAlert> = laura.contact_alerts().into_iter().map(|a| a.alert).collect();
    for a in [ContactAlert::Emergency, ContactAlert::CodeGrant, ContactAlert::ExitScheduled] {
        assert!(alerts.contains(&a), "{a:?} in {alerts:?}");
    }
    let notices: Vec<NoticeKind> = laura.take_notices().unwrap().into_iter().map(|n| n.kind).collect();
    assert!(notices.contains(&NoticeKind::ContactExitScheduled) && notices.contains(&NoticeKind::EmergencyAlert));
    assert!(laura.take_notices().unwrap().is_empty(), "delivered once");
}

#[test]
fn family_request_travels_through_the_other_guardian() {
    let clock = MONDAY_9;
    let (mom, dad, kid) = (device("mom"), device("dad"), device("kid"));
    mom.create_family_profile(clock, "Mom".into(), Config::example()).unwrap();
    pair(&mom, &dad, "Dad", Some(Role::Guardian), clock);
    // The second guardian pairs the minor's phone (the genesis is still mom's)
    pair(&dad, &kid, "Ana's phone", Some(Role::Minor), clock);
    assert_eq!(kid.my_role(), Some(Role::Minor));
    assert_eq!(kid.status(clock).unwrap().permits_left, 3);

    // kid -> dad -> mom: dad forwards what the kid sent
    let id = kid.request(clock, RequestKind::ExtraPermit { minutes: 15 }).unwrap();
    sync(&kid, clock);
    see(&mom, &dad, clock);
    sync(&mom, clock);
    assert_eq!(mom.pending_requests().len(), 1);
    assert_eq!(
        mom.take_notices().unwrap().into_iter().map(|n| n.kind).collect::<Vec<_>>(),
        vec![NoticeKind::RequestReceived]
    );

    mom.decide(clock, id, true).unwrap();
    see(&dad, &mom, clock);
    sync(&dad, clock);
    sync(&kid, clock);
    assert_eq!(kid.status(clock).unwrap().permits_left, 4, "the approved pass reaches the minor");
    assert!(dad.pending_requests().is_empty(), "the other guardian sees the answered request");
    let kid_notices: Vec<NoticeKind> = kid.take_notices().unwrap().into_iter().map(|n| n.kind).collect();
    assert_eq!(kid_notices, vec![NoticeKind::RequestAnswered { approved: true }]);
    // The minor cannot pair devices
    assert!(kid.clone().pairing_offer().is_ok());
    assert!(kid.pairing_confirm(clock, Some(Role::Minor)).is_err());
}

#[test]
fn outsiders_get_nothing() {
    let (phone, pc, mallory) = (device("o-phone"), device("o-pc"), device("o-mallory"));
    phone.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
    // Someone who saw the QR answers first: the codes differ and the owner cancels.
    let qr = phone.clone().pairing_offer().unwrap();
    let mallory_code = mallory.pairing_join(qr.clone(), "PC".into()).unwrap();
    assert_eq!(phone.pairing_poll().unwrap(), Some(mallory_code));
    assert!(pc.pairing_join(qr, "PC".into()).is_err(), "one answer per QR");
    phone.pairing_cancel();
    assert_eq!(sync(&mallory, MONDAY_9), 0);
    assert!(mallory.members().is_empty(), "no confirmation, no profile");

    // A raw connection with an unknown key is closed before it can read a byte.
    let port = phone.listening_port().unwrap();
    let node = phone.members()[0].node_id.clone();
    let stranger = PeerNet::bind(&[42u8; 32], 0, false).unwrap();
    let got = stranger.block_on(async {
        let addr: std::net::SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let conn = stranger.connect(&node, &[addr], procastinatioff_proto::SYNC_ALPN).await.unwrap();
        let Ok((mut send, mut recv)) = conn.open_bi().await else { return 0 };
        let _ = send.write_all(b"hello?").await;
        let _ = send.finish();
        recv.read_to_end(1 << 20).await.map(|v| v.len()).unwrap_or(0)
    });
    assert_eq!(got, 0);
}

#[test]
fn backup_restores_the_same_device() {
    let (phone, pc) = (device("b-phone"), device("b-pc"));
    phone.create_profile(MONDAY_9, "Phone".into(), Config::example()).unwrap();
    pair(&phone, &pc, "PC", None, MONDAY_9);
    phone.plan_day(MONDAY_9, vec![]).unwrap();
    let pass = procastinatioff_core::api::new_backup_passphrase();
    let file = phone.export_backup(pass.clone()).unwrap();

    let dir = std::env::temp_dir().join(format!("pco-it-restore-{}-{}", std::process::id(), rand_suffix()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("db").to_string_lossy().to_string();
    let store = Arc::new(Mem::default());
    let wrong = Core::restore_backup(db.clone(), store.clone(), file.clone(), "wrong-passphrase-wrong".into());
    assert!(wrong.is_err());
    let restored = Core::restore_backup(db, store, file, pass).unwrap();
    assert_eq!(restored.device_id(), phone.device_id());
    assert_eq!(restored.status(MONDAY_9), phone.status(MONDAY_9), "same state of the day");
}
