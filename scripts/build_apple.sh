#!/usr/bin/env bash
# Core + verifier for iOS and macOS. Runs on a Mac with Xcode.
# Output (in target/, outside git):
#   target/apple/ProcastinatiOFF.xcframework   single static library (iOS, simulator, universal macOS)
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
cargo build -p procastinatioff-core -p procastinatioff-verify
for c in core verify; do
  cargo run -q -p uniffi-bindgen -- generate --library "target/debug/libprocastinatioff_$c.dylib" \
    --language swift --out-dir "$OUT/swift"
  mv "$OUT/swift/procastinatioff_${c}FFI.h" "$OUT/headers/"
  # A single module.modulemap with both FFI modules (two files with the same name would clash)
  cat "$OUT/swift/procastinatioff_${c}FFI.modulemap" >> "$OUT/headers/module.modulemap"
  rm "$OUT/swift/procastinatioff_${c}FFI.modulemap"
done

# 2) One static library per platform (core + verifier together: see apple/ffi)
for t in aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin x86_64-apple-darwin; do
  rustup target add "$t" >/dev/null
  cargo build -p procastinatioff-apple --release --target "$t"
done
lipo -create -output "$OUT/macos/libprocastinatioff_apple.a" \
  target/aarch64-apple-darwin/release/libprocastinatioff_apple.a \
  target/x86_64-apple-darwin/release/libprocastinatioff_apple.a

xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/libprocastinatioff_apple.a -headers "$OUT/headers" \
  -library target/aarch64-apple-ios-sim/release/libprocastinatioff_apple.a -headers "$OUT/headers" \
  -library "$OUT/macos/libprocastinatioff_apple.a" -headers "$OUT/headers" \
  -output "$OUT/ProcastinatiOFF.xcframework"

echo "Listo: $OUT/ProcastinatiOFF.xcframework y $OUT/swift/"
