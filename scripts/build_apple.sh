#!/usr/bin/env bash
# Núcleo + verificador para iOS y macOS (08 · Paso 1 y 08b · Paso 1). Se ejecuta en un Mac con Xcode.
# Resultado (en target/, fuera de git):
#   target/apple/ProcastinaciOFF.xcframework   librería estática única (iOS, simulador, macOS universal)
#   target/apple/swift/*.swift                  bindings de UniFFI (núcleo y verificador)
set -euo pipefail
cd "$(dirname "$0")/.."

export IPHONEOS_DEPLOYMENT_TARGET=16.0   # Family Controls con autorización individual
export MACOSX_DEPLOYMENT_TARGET=13.0     # requisito de firma en NSXPCListener

OUT=target/apple
rm -rf "$OUT"
mkdir -p "$OUT/headers" "$OUT/swift" "$OUT/macos"

# 1) Bindings Swift desde las librerías del Mac (solo se leen los metadatos de UniFFI)
cargo build -p procastinacioff-core -p procastinacioff-verify --release
for c in core verify; do
  cargo run -q -p uniffi-bindgen -- generate --library "target/release/libprocastinacioff_$c.dylib" \
    --language swift --out-dir "$OUT/swift"
  mv "$OUT/swift/procastinacioff_${c}FFI.h" "$OUT/headers/"
  # Un solo module.modulemap con los dos módulos FFI (dos archivos con el mismo nombre chocarían)
  cat "$OUT/swift/procastinacioff_${c}FFI.modulemap" >> "$OUT/headers/module.modulemap"
  rm "$OUT/swift/procastinacioff_${c}FFI.modulemap"
done

# 2) Una librería estática por plataforma (núcleo + verificador juntos: ver apple/ffi)
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
