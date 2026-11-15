# justfile — ejecuta `just` para ver la lista de tareas
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# La app Tauri necesita la UI compilada (desktop/app/ui/dist): se compila con `just app`.
app_crate := "procastinacioff-app"

default:
    @just --list

# Todo lo que corre el CI, en local. Ejecútalo antes de cada push.
check: fmt-check lint test deny audit privacy

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --workspace --exclude {{app_crate}} --all-targets -- -D warnings

test:
    cargo test --workspace --exclude {{app_crate}}

deny:
    cargo deny check

audit:
    cargo audit

privacy:
    python scripts/privacy_scan.py
    python scripts/check_verify_no_net.py
    python scripts/build_category_list.py --test

sbom:
    cargo cyclonedx --format json --all

# Fuzzing de un objetivo durante N segundos (requiere nightly)
fuzz target secs="60":
    cd core; cargo +nightly fuzz run {{target}} -- -max_total_time={{secs}}

relay-dev:
    cargo run -p procastinacioff-relay

# Prueba de punta a punta: dos dispositivos contra el relay local (arrancar antes `just relay-dev`)
it:
    cargo test -p procastinacioff-core --test two_devices -- --ignored

# UI de escritorio (Svelte) + app Tauri
app:
    npm --prefix desktop/app/ui ci
    npm --prefix desktop/app/ui run build
    cargo clippy -p {{app_crate}} -- -D warnings

# Instalador NSIS firmado de Windows (servicio + UI). Requiere tauri-cli 2 y el certificado de firma.
installer: app
    cargo build -p procastinacioff-agent --release
    New-Item -ItemType Directory -Force desktop/app/src-tauri/binaries | Out-Null
    Copy-Item target/release/procastinacioff-agent.exe desktop/app/src-tauri/binaries/procastinacioff-agent-x86_64-pc-windows-msvc.exe
    cd desktop/app/src-tauri; cargo tauri build --config tauri.bundle.json

# Bindings Kotlin desde la librería del host (library mode)
bindings:
    cargo build -p procastinacioff-core --release
    cargo run -p uniffi-bindgen -- generate --library target/release/{{ if os() == "windows" { "procastinacioff_core.dll" } else if os() == "macos" { "libprocastinacioff_core.dylib" } else { "libprocastinacioff_core.so" } }} --language kotlin --out-dir android/app/src/main/java
