// Privacy audit, test 4 (security-critical): the verifier launched in the AppContainer has NO network.
// Copies the executable to a folder readable by "ALL APPLICATION PACKAGES" and runs it sandboxed.
#![cfg(windows)]

use std::process::Command;

#[test]
fn verifier_inside_appcontainer_has_no_network() {
    let dir = std::env::temp_dir().join(format!("pco-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("procastinatioff-verifier.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_procastinatioff-verifier"), &exe).unwrap();
    // S-1-15-2-1 = ALL APPLICATION PACKAGES: without read access, Windows will not start the sandboxed process
    let ok = Command::new("icacls").arg(&dir).args(["/grant", "*S-1-15-2-1:(OI)(CI)RX"]).output().unwrap();
    assert!(ok.status.success(), "icacls failed");

    let out = procastinatioff_sandbox::run_sandboxed(&format!("\"{}\" --selftest-net", exe.display()), 30_000, |_| {})
        .expect("could not launch the AppContainer");
    assert_eq!(out.line, "NET_BLOCKED", "the sandboxed verifier got network access");
    assert_eq!(out.exit_code, 0);
    std::fs::remove_dir_all(dir).ok();
}
