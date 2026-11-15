// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Desktop verifier (security-critical). The UI launches it INSIDE an AppContainer with only the "webcam"
//! capability: Windows denies it the network. Camera → frame in RAM → model → numbers → `verify` crate
//! → ONE JSON line on stdout with the 5 fields of `VerificationResult`. Never writes to disk.
//!
//! Usage: procastinacioff-verifier --method (reading|pose) --goal <id> --target <min|reps>
//!        procastinacioff-verifier --selftest-net     (audit: inside the AppContainer it MUST fail)

mod models;

use std::time::Duration;

use procastinacioff_verify::result::VerificationResult;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn emit(r: &VerificationResult) {
    let method = format!("{:?}", r.method); // "ReadingPresence" | "PoseReps"
    let line = serde_json::json!({
        "goal_id": r.goal_id, "method": method, "approved": r.approved, "score": r.score, "minutes": r.minutes,
    });
    println!("{line}");
}

fn fail(message: &str) -> ! {
    // Fixed message without data (English, for logs): the UI shows a translated generic error.
    println!("{}", serde_json::json!({ "error": message }));
    std::process::exit(2);
}

fn main() {
    if std::env::args().any(|a| a == "--selftest-net") {
        let target = std::net::SocketAddr::from(([1, 1, 1, 1], 443));
        let r = std::net::TcpStream::connect_timeout(&target, Duration::from_secs(3));
        println!("{}", if r.is_ok() { "NET_OK" } else { "NET_BLOCKED" });
        std::process::exit(if r.is_ok() { 1 } else { 0 });
    }
    let goal = arg("--goal").unwrap_or_default();
    let target: u16 = arg("--target").and_then(|t| t.parse().ok()).unwrap_or(30).clamp(1, 600);
    let method = arg("--method").unwrap_or_default();
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("models"))).unwrap_or_default();

    let result = match method.as_str() {
        "reading" => session::reading(&dir, &goal, target),
        "pose" => session::pose(&dir, &goal, target),
        _ => Err("unknown method".into()),
    };
    match result {
        Ok(Some(r)) => emit(&r),
        Ok(None) => fail("invalid goal"),
        Err(e) => fail(&e),
    }
}

#[cfg(windows)]
mod session {
    //! Camera sessions. Frames only exist in RAM and are overwritten after use.
    use std::time::{Duration, Instant};

    use nokhwa::Camera;
    use nokhwa::pixel_format::RgbFormat;
    use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
    use procastinacioff_verify::pose::{Exercise, RepCounter};
    use procastinacioff_verify::reading::{ReadingTracker, SAMPLE_EVERY_MS};
    use procastinacioff_verify::result::{VMethod, VerificationResult, make_result};

    use crate::models::{Frame, Models};

    type Session = Result<Option<VerificationResult>, String>;

    fn camera() -> Result<Camera, String> {
        let fmt = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let mut cam = Camera::new(CameraIndex::Index(0), fmt).map_err(|_| "no camera".to_string())?;
        cam.open_stream().map_err(|_| "could not open the camera".to_string())?;
        Ok(cam)
    }

    /// Captures a frame, processes it with `f` and wipes it from memory.
    fn with_frame<T>(cam: &mut Camera, f: impl FnOnce(&Frame) -> T) -> Option<T> {
        let img = cam.frame().ok()?.decode_image::<RgbFormat>().ok()?;
        let (width, height) = (img.width() as usize, img.height() as usize);
        let mut rgb = img.into_raw();
        let out = f(&Frame { width, height, rgb: &rgb });
        rgb.fill(0); // SECURITY: do not leave the frame in memory
        Some(out)
    }

    /// Reading: 1 sample every 20 s (person + book). Ends when the target is reached or after +5 min.
    pub fn reading(dir: &std::path::Path, goal: &str, target: u16) -> Session {
        let models = Models::load(dir, false, true)?;
        let mut cam = camera()?;
        let tracker = ReadingTracker::new();
        let start = Instant::now();
        let limit = Duration::from_secs(target as u64 * 60 + 300);
        while start.elapsed() < limit {
            if let Some((person, book)) = with_frame(&mut cam, |f| models.person_and_book(f))
                && tracker.add_sample(start.elapsed().as_millis() as i64, person, book) >= target
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(SAMPLE_EVERY_MS as u64));
        }
        let _ = cam.stop_stream();
        Ok(tracker.finish(goal.to_owned(), target))
    }

    /// Exercise: ~10 frames per second until `target` reps or 15 min.
    pub fn pose(dir: &std::path::Path, goal: &str, target: u16) -> Session {
        let models = Models::load(dir, true, false)?;
        let mut cam = camera()?;
        let counter = RepCounter::new(Exercise::Squat);
        let start = Instant::now();
        let mut reps = 0;
        while reps < target as u32 && start.elapsed() < Duration::from_secs(900) {
            if let Some(Some(lm)) = with_frame(&mut cam, |f| models.pose_as_mediapipe(f)) {
                reps = counter.feed(lm, start.elapsed().as_millis() as i64);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = cam.stop_stream();
        let score = ((reps * 100) / target.max(1) as u32).min(100) as u8;
        Ok(make_result(goal, VMethod::PoseReps, reps >= target as u32, score, 0))
    }
}

#[cfg(not(windows))]
mod session {
    //! macOS/Linux: later.
    use procastinacioff_verify::result::VerificationResult;

    pub fn reading(_: &std::path::Path, _: &str, _: u16) -> Result<Option<VerificationResult>, String> {
        Err("Windows only for now".into())
    }
    pub fn pose(_: &std::path::Path, _: &str, _: u16) -> Result<Option<VerificationResult>, String> {
        Err("Windows only for now".into())
    }
}
