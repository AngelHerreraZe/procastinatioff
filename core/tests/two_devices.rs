// End-to-end test: phone and PC against the local relay (`just relay-dev`).
// Marked #[ignore]; CI runs it with the relay up (`integration` job).
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use procastinacioff_core::api::Core;
use procastinacioff_core::clock::Clock;
use procastinacioff_core::crypto::SecretStore;
use procastinacioff_core::error::CoreError;
use procastinacioff_core::model::{Config, DayState, Method, RequestKind, Role};

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

/// A `Core` with a given keystore (the partner mailbox shares the device keys).
fn device_with(name: &str, secrets: Arc<Mem>) -> Arc<Core> {
    let dir = std::env::temp_dir().join(format!("pco-it-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Core::open(dir.join("db").to_string_lossy().into(), secrets).unwrap()
}

fn real_now() -> Clock {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
    Clock { now_ms: now, utc_offset_min: 0 }
}

const RELAY: &str = "http://127.0.0.1:8080";

#[test]
#[ignore = "necesita el relay en http://127.0.0.1:8080"]
fn trusted_time_ignores_a_moved_device_clock() {
    let phone = device("time");
    let clock = real_now();
    phone.create_profile(clock, "Phone".into(), Config::example()).unwrap();
    phone.connect_relay(RELAY.into()).unwrap();
    let nine_hours = 9 * 3_600_000;
    let mono = 5_000_000;
    // The user moved the clock 9 hours forward: the core anchors to the relay's signed time.
    // (Reference taken right before anchoring: creating the mailbox solves a slow proof of work.)
    let at_anchor = real_now().now_ms;
    phone.anchor_time(at_anchor + nine_hours, mono).unwrap();
    // 10 s later per the monotonic clock, with the system clock still ahead
    let trusted = phone.trusted_now_ms(at_anchor + nine_hours + 10_000, mono + 10_000);
    assert!((trusted - (at_anchor + 10_000)).abs() < 5_000, "trusted time follows the relay");
    // Anchoring again with the same key works (key pinned the first time)
    phone.anchor_time(clock.now_ms, mono + 20_000).unwrap();
}

#[test]
#[ignore = "necesita el relay en http://127.0.0.1:8080"]
fn partner_sees_emergency_but_nothing_else() {
    let clock = real_now();
    let secrets = Arc::new(Mem::default());
    let (me, my_partner_box, partner) =
        (device_with("me", secrets.clone()), device_with("me-partner", secrets), device("partner"));
    me.create_profile(clock, "Phone".into(), Config::example()).unwrap();
    my_partner_box.create_partner_mailbox(clock, "Phone".into()).unwrap();
    my_partner_box.connect_relay(RELAY.into()).unwrap();

    let qr = my_partner_box.pairing_offer().unwrap();
    let code_partner = partner.pairing_join(qr, "Laura".into()).unwrap();
    assert_eq!(my_partner_box.pairing_poll().unwrap().as_deref(), Some(code_partner.as_str()));
    my_partner_box.pairing_confirm(clock, None).unwrap();

    me.emergency(clock).unwrap();
    assert_eq!(me.forward_to_partner(my_partner_box.clone(), clock).unwrap(), 1);
    my_partner_box.sync(clock).unwrap();
    partner.sync(clock).unwrap();

    let alerts = partner.partner_alerts();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].alert, procastinacioff_core::model::PartnerAlert::Emergency);
    assert!(partner.status(clock).is_none(), "the partner receives no configuration or goals");
    let roles: Vec<_> = partner.members().into_iter().map(|m| m.role).collect();
    assert!(roles.contains(&procastinacioff_core::model::Role::Partner));
}

#[test]
#[ignore = "necesita el relay en http://127.0.0.1:8080"]
fn phone_plans_and_completes_goal_and_pc_unlocks() {
    let clock = Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 }; // Monday 09:00 UTC
    let (phone, pc) = (device("phone"), device("pc"));

    phone.create_profile(clock, "Phone".into(), Config::example()).unwrap();
    phone.connect_relay("http://127.0.0.1:8080".into()).unwrap();
    phone.sync(clock).unwrap();

    let qr = phone.pairing_offer().unwrap();
    let code_pc = pc.pairing_join(qr, "PC".into()).unwrap();
    let code_phone = phone.pairing_poll().unwrap().expect("the PC already answered");
    assert_eq!(code_pc, code_phone, "the 6-digit codes must match");
    phone.pairing_confirm(clock, None).unwrap();
    pc.sync(clock).unwrap();

    let st = pc.status(clock).unwrap();
    assert_eq!(st.state, DayState::Locked);
    assert!(st.needs_plan, "on waking up, the PC asks for the day plan");

    // The plan set on the phone reaches the PC
    phone.plan_day(clock, vec![]).unwrap();
    phone.sync(clock).unwrap();
    pc.sync(clock).unwrap();
    assert!(!pc.status(clock).unwrap().needs_plan);

    phone.complete_goal(clock, "read-30".into(), Method::HonorDeclaration, 100, 30).unwrap();
    phone.sync(clock).unwrap();
    pc.sync(clock).unwrap();
    assert_eq!(pc.status(clock).unwrap().state, DayState::Free, "the goal completed on the phone unlocks the PC");
}

/// Pairs `joiner` to `inviter`'s profile with the given role and syncs both.
fn pair(inviter: &Core, joiner: &Core, label: &str, role: Option<Role>, clock: Clock) {
    let qr = inviter.pairing_offer().unwrap();
    let code = joiner.pairing_join(qr, label.into()).unwrap();
    assert_eq!(inviter.pairing_poll().unwrap().as_deref(), Some(code.as_str()));
    inviter.pairing_confirm(clock, role).unwrap();
    joiner.sync(clock).unwrap();
}

#[test]
#[ignore = "necesita el relay en http://127.0.0.1:8080"]
fn family_request_is_approved_by_a_second_guardian() {
    let clock = Clock { now_ms: 1_773_046_800_000, utc_offset_min: 0 }; // Monday 09:00 UTC
    let (mom, dad, kid) = (device("mom"), device("dad"), device("kid"));
    mom.create_family_profile(clock, "Mom".into(), Config::example()).unwrap();
    mom.connect_relay(RELAY.into()).unwrap();
    mom.sync(clock).unwrap();
    pair(&mom, &dad, "Dad", Some(Role::Guardian), clock);
    // The second guardian pairs the minor's phone (the genesis is still mom's)
    pair(&dad, &kid, "Ana's phone", Some(Role::Minor), clock);
    assert_eq!(kid.my_role(), Some(Role::Minor));
    assert_eq!(kid.status(clock).unwrap().permits_left, 3);

    let id = kid.request(clock, RequestKind::ExtraPermit { minutes: 15 }).unwrap();
    kid.sync(clock).unwrap();
    mom.sync(clock).unwrap();
    assert_eq!(mom.pending_requests().len(), 1);
    mom.decide(clock, id, true).unwrap();
    mom.sync(clock).unwrap();
    kid.sync(clock).unwrap();
    dad.sync(clock).unwrap();
    assert_eq!(kid.status(clock).unwrap().permits_left, 4, "the approved pass reaches the minor");
    assert!(dad.pending_requests().is_empty(), "the other guardian sees the answered request");
    // The minor cannot pair devices
    assert!(kid.pairing_offer().is_ok());
    assert!(kid.pairing_confirm(clock, Some(Role::Minor)).is_err());
}
