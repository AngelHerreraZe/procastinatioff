# justfile — run `just` to see the list of tasks
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# The Tauri app needs the built UI (desktop/app/ui/dist): built with `just app`.
app_crate := "procastinacioff-app"

default:
    @just --list

# Everything CI runs, locally. Run it before every push.
check: fmt-check lint test deny audit privacy i18n

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

# Translation files: no stale keys, same placeholders as English, every key used exists
i18n:
    python scripts/check_i18n.py

sbom:
    cargo cyclonedx --format json --all

# Fuzzing of one target for N seconds (needs nightly)
fuzz target secs="60":
    cd core; cargo +nightly fuzz run {{target}} -- -max_total_time={{secs}}

# End-to-end test: devices pairing and syncing directly (real QUIC on this machine)
it:
    cargo test -p procastinacioff-core --test two_devices

# Desktop UI (Svelte) + Tauri app
app:
    npm --prefix desktop/app/ui ci
    npm --prefix desktop/app/ui run build
    cargo clippy -p {{app_crate}} -- -D warnings

# Linux packages (run on Linux): the agent (.deb with the systemd unit) and the UI (.deb that depends on it).
# Needs cargo-deb 2 and tauri-cli 2. Plan 09 · A.7.
linux-packages: app
    cargo deb -p procastinacioff-agent
    cd desktop/app/src-tauri; cargo tauri build --bundles deb --config tauri.linux.bundle.json

# Signed Windows NSIS installer (service + UI). Needs tauri-cli 2 and the signing certificate.
installer: app
    cargo build -p procastinacioff-agent --release
    New-Item -ItemType Directory -Force desktop/app/src-tauri/binaries | Out-Null
    Copy-Item target/release/procastinacioff-agent.exe desktop/app/src-tauri/binaries/procastinacioff-agent-x86_64-pc-windows-msvc.exe
    cd desktop/app/src-tauri; cargo tauri build --config tauri.bundle.json

# Kotlin bindings from the host library (library mode). Debug on purpose: the release library is
# stripped and uniffi-bindgen then generates nothing without failing (stale bindings, crash on start).
bindings:
    cargo build -p procastinacioff-core
    cargo run -p uniffi-bindgen -- generate --library target/debug/{{ if os() == "windows" { "procastinacioff_core.dll" } else if os() == "macos" { "libprocastinacioff_core.dylib" } else { "libprocastinacioff_core.so" } }} --language kotlin --out-dir android/app/src/main/java
