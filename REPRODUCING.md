# Reproducing a release

Anyone can check that a published binary was built from this code.

## Linux agent

1. Install Rust with the version in `rust-toolchain.toml` (rustup picks it up automatically).
2. Clone the repository and run, for the release you want to check:

   ```sh
   scripts/repro_check.sh v1.2.3
   ```

   It builds the agent twice from the tagged code, in two different folders, with the release
   flags (`SOURCE_DATE_EPOCH` = commit date, `--remap-path-prefix`), and prints its SHA-256.
3. Compare that hash with `procastinatioff-agent-linux-x86_64` in the release's `SHA256SUMS`.

## Android

The APK is built without the dependency metadata block (not reproducible) and published unsigned
in each release's CI run, so it can be compared before signing. Independent verification follows
F-Droid's reproducible builds process.

## Windows, macOS, iOS

Not reproducible yet (the Windows linker adds timestamps; Apple builds come from Xcode). Planned.

If your hash differs, please open an issue with the tag, your OS and the hash you got.
