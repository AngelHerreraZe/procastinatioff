#!/usr/bin/env python3
# scripts/privacy_scan.py
"""Falla (exit 1) si el código usa APIs prohibidas por el contrato de privacidad.

Uso:  python scripts/privacy_scan.py
Excepciones: scripts/privacy_allowlist.txt, una por línea:  ruta/al/archivo::PATRON  # motivo
"""
from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE_EXT = {".rs", ".kt", ".kts", ".java", ".swift", ".ts", ".tsx", ".svelte", ".xml", ".plist", ".m"}
SKIP_DIRS = {"target", "node_modules", ".git", "build", ".gradle", "DerivedData", "spikes"}

# Patrón -> motivo. Ampliar solo con revisión de seguridad.
FORBIDDEN: dict[str, str] = {
    # Grabación / guardado de cámara y audio
    r"\bAVCaptureMovieFileOutput\b": "graba video a disco",
    r"\bAVCapturePhotoOutput\b": "captura fotos fuera del flujo en memoria",
    r"\bAVAudioRecorder\b": "graba audio a disco",
    r"\bMediaRecorder\b": "graba audio/video a disco",
    r"\bImageCapture\b": "CameraX ImageCapture guarda fotos; usar solo ImageAnalysis",
    r"\bMediaStore\b": "escribe en la galería",
    r"\bPHPhotoLibrary\b": "accede a la galería",
    r"UIImageWriteToSavedPhotosAlbum": "escribe en la galería",
    # Captura de pantalla
    r"\bMediaProjection\b": "captura de pantalla en Android",
    r"\bCGWindowListCreateImage\b": "captura de pantalla en macOS",
    r"\bSCScreenshotManager\b": "captura de pantalla en macOS",
    r"\bBitBlt\b": "captura de pantalla en Windows",
    r"CopyFromScreen": "captura de pantalla en Windows",
    # Lectura de mensajes / notificaciones / teclado
    r"\bNotificationListenerService\b": "lee notificaciones (WhatsApp, etc.)",
    r"\bSetWindowsHookEx\w*\b": "posible keylogger",
    r"\bCGEventTapCreate\b": "posible keylogger",
    # SDKs de terceros
    r"firebase-analytics|firebase-crashlytics|FirebaseAnalytics|Crashlytics": "SDK de analítica",
    r"io\.sentry|sentry-android|SentrySDK": "SDK de reporte de errores",
    r"play-services-ads|GoogleMobileAds": "SDK de anuncios",
    r"mixpanel|amplitude|appsflyer|com\.adjust|segment\.analytics": "SDK de analítica",
}

# Reglas extra solo para el crate verify/ (no puede tocar disco ni red)
VERIFY_FORBIDDEN: dict[str, str] = {
    r"\bstd::fs\b": "verify/ no puede usar el sistema de archivos",
    r"\bFile::(create|open)\b": "verify/ no puede usar el sistema de archivos",
    r"\bstd::net\b": "verify/ no puede usar la red",
}


# Verificador de iOS (08 · 7.1): iOS no deja quitar la red a una parte de la app, así que la garantía
# es de código. Sin red y sin escribir nada a disco; solo AVCaptureVideoDataOutput (memoria).
IOS_VERIFIER = "apple/ios/VerifierKit/"
IOS_VERIFIER_FORBIDDEN: dict[str, str] = {
    r"import\s+Network\b|URLSession|NWConnection|CFSocket|CFStream|import\s+Alamofire": "VerifierKit no puede usar red",
    r"AVAssetWriter|AVAudioRecorder|\bwrite\(to:|FileManager|UserDefaults": "VerifierKit no puede escribir a disco",
}


def load_allowlist() -> set[tuple[str, str]]:
    path = ROOT / "scripts" / "privacy_allowlist.txt"
    allowed: set[tuple[str, str]] = set()
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            line = line.split("#", 1)[0].strip()
            if "::" in line:
                file, pattern = line.split("::", 1)
                allowed.add((file.strip(), pattern.strip()))
    return allowed


def sqlcipher_enabled() -> bool:
    """La base de datos local DEBE ir cifrada: rusqlite solo con SQLCipher."""
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    line = next((l for l in cargo.splitlines() if l.strip().startswith("rusqlite")), "")
    return "bundled-sqlcipher" in line


def main() -> int:
    allowed = load_allowlist()
    failures: list[str] = []
    if not sqlcipher_enabled():
        failures.append("Cargo.toml: rusqlite sin SQLCipher — la base de datos local quedaría sin cifrar")
    for path in ROOT.rglob("*"):
        if not path.is_file() or path.suffix not in SOURCE_EXT:
            continue
        rel = path.relative_to(ROOT).as_posix()
        if any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        rules = dict(FORBIDDEN)
        if rel.startswith("verify/"):
            rules.update(VERIFY_FORBIDDEN)
        if rel.startswith(IOS_VERIFIER):
            rules.update(IOS_VERIFIER_FORBIDDEN)
        for pattern, reason in rules.items():
            if (rel, pattern) in allowed:
                continue
            for match in re.finditer(pattern, text):
                line_no = text.count("\n", 0, match.start()) + 1
                failures.append(f"{rel}:{line_no}: '{match.group(0)}' — {reason}")
    if failures:
        print("Escáner de privacidad: APIs prohibidas encontradas\n")
        print("\n".join(failures))
        return 1
    print("Escáner de privacidad: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
