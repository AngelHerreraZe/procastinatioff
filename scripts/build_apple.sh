#!/usr/bin/env bash
# Core + verifier for iOS and macOS. Runs on a Mac with Xcode.
# Output (in target/, outside git):
#   target/apple/ProcastinaciOFF.xcframework   single static library (iOS, simulator, universal macOS)
#   target/apple/swift/*.swift                  UniFFI bindings (core and verifier)
set -euo pipefail
cd "$(dirname "$0")/.."

export IPHONEOS_DEPLOYMENT_TARGET=16.0   # Family Controls with individual authorization
export MACOSX_DEPLOYMENT_TARGET=13.0     # signing requirement in NSXPCListener

OUT=target/apple
rm -rf "$OUT"
mkdir -p "$OUT/headers" "$OUT/swift" "$OUT/macos"

# 1) Swift bindings from the Mac libraries (only the UniFFI metadata is read). Debug on purpose: the
#    release library is stripped and uniffi-bindgen then generates nothing without failing.
cargo build -p procastinacioff-core -p procastinacioff-verify
for c in core verify; do
  cargo run -q -p uniffi-bindgen -- generate --library "target/debug/libprocastinacioff_$c.dylib" \
    --language swift --out-dir "$OUT/swift"
  mv "$OUT/swift/procastinacioff_${c}FFI.h" "$OUT/headers/"
  # A single module.modulemap with both FFI modules (two files with the same name would clash)
  cat "$OUT/swift/procastinacioff_${c}FFI.modulemap" >> "$OUT/headers/module.modulemap"
  rm "$OUT/swift/procastinacioff_${c}FFI.modulemap"
done

# 2) One static library per platform (core + verifier together: see apple/ffi)
for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$t" >/dev/null
  cargo build -p procastinacioff-apple --release --target "$t"
done
lipo -create -output "$OUT/macos/libprocastinacioff_apple.a" \
  target/aarch64-apple-darwin/release/libprocastinacioff_apple.a \
  target/x86_64-apple-darwin/release/libprocastinacioff_apple.a

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libprocastinacioff_apple.a -headers "$OUT/headers" \
  -library target/aarch64-apple-ios-sim/release/libprocastinacioff_apple.a -headers "$OUT/headers" \
  -library "$OUT/macos/libprocastinacioff_apple.a" -headers "$OUT/headers" \
  -output "$OUT/ProcastinaciOFF.xcframework"

echo "Listo: $OUT/ProcastinaciOFF.xcframework y $OUT/swift/"
