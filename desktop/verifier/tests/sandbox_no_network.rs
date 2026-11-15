// Auditoría de la Fase 2, prueba 4 🔒: el verificador lanzado en el AppContainer NO tiene red.
// Copia el ejecutable a una carpeta legible por "ALL APPLICATION PACKAGES" y lo ejecuta aislado.
#![cfg(windows)]

use std::process::Command;

#[test]
fn verifier_inside_appcontainer_has_no_network() {
    let dir = std::env::temp_dir().join(format!("pco-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let exe = dir.join("procastinacioff-verifier.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_procastinacioff-verifier"), &exe).unwrap();
    // S-1-15-2-1 = ALL APPLICATION PACKAGES: sin lectura, Windows no deja arrancar el proceso aislado
    let ok = Command::new("icacls").arg(&dir).args(["/grant", "*S-1-15-2-1:(OI)(CI)RX"]).output().unwrap();
    assert!(ok.status.success(), "icacls falló");

    let out = procastinacioff_sandbox::run_sandboxed(&format!("\"{}\" --selftest-net", exe.display()), 30_000, |_| {})
        .expect("no se pudo lanzar el AppContainer");
    assert_eq!(out.line, "NET_BLOCKED", "el verificador aislado consiguió red");
    assert_eq!(out.exit_code, 0);
    std::fs::remove_dir_all(dir).ok();
}
