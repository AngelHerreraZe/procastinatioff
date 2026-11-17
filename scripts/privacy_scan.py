#!/usr/bin/env python3
# scripts/privacy_scan.py
"""Fails (exit 1) if the code uses APIs forbidden by the privacy contract.

Usage:  python scripts/privacy_scan.py
Exceptions: scripts/privacy_allowlist.txt, one per line:  path/to/file::PATTERN  # reason
"""
from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE_EXT = {".rs", ".kt", ".kts", ".java", ".swift", ".ts", ".tsx", ".svelte", ".xml", ".plist", ".m", ".toml"}
SKIP_DIRS = {"target", "node_modules", ".git", "build", ".gradle", "DerivedData", "spikes", "uniffi"}  # uniffi: generated

# Pattern -> reason. Extend only with a security review.
FORBIDDEN: dict[str, str] = {
    # Recording / saving camera and audio
    r"\bAVCaptureMovieFileOutput\b": "records video to disk",
    r"\bAVCapturePhotoOutput\b": "captures photos outside the in-memory stream",
    r"\bAVAudioRecorder\b": "records audio to disk",
    r"\bMediaRecorder\b": "records audio/video to disk",
    r"\bImageCapture\b": "CameraX ImageCapture saves photos; use ImageAnalysis only",
    r"\bMediaStore\b": "writes to the photo gallery",
    r"\bPHPhotoLibrary\b": "accesses the photo gallery",
    r"UIImageWriteToSavedPhotosAlbum": "writes to the photo gallery",
    # Screen capture
    r"\bMediaProjection\b": "screen capture on Android",
    r"\bCGWindowListCreateImage\b": "screen capture on macOS",
    r"\bSCScreenshotManager\b": "screen capture on macOS",
    r"\bBitBlt\b": "screen capture on Windows",
    r"CopyFromScreen": "screen capture on Windows",
    # Reading messages / notifications / keyboard
    r"\bNotificationListenerService\b": "reads notifications (WhatsApp, etc.)",
    r"\bSetWindowsHookEx\w*\b": "possible keylogger",
    r"\bCGEventTapCreate\b": "possible keylogger",
    # Third-party SDKs
    r"firebase-analytics|firebase-crashlytics|FirebaseAnalytics|Crashlytics": "analytics SDK",
    r"io\.sentry|sentry-android|SentrySDK": "crash reporting SDK",
    r"play-services-ads|GoogleMobileAds": "ads SDK",
    r"mixpanel|amplitude|appsflyer|com\.adjust|segment\.analytics": "analytics SDK",
}

# Extra rules only for the verify/ crate (it cannot touch disk or network)
VERIFY_FORBIDDEN: dict[str, str] = {
    r"\bstd::fs\b": "verify/ cannot use the file system",
    r"\bFile::(create|open)\b": "verify/ cannot use the file system",
    r"\bstd::net\b": "verify/ cannot use the network",
}


# No servers: the core, the protocol, the desktop service and the Android and Apple apps only talk to
# paired devices. An HTTP client/server or a URL there means someone is adding a server back.
NO_SERVER_PATHS = ("core/", "proto/", "desktop/agent/", "android/app/", "apple/")
NO_SERVER_FORBIDDEN: dict[str, str] = {
    r"\b(ureq|reqwest|axum|hyper|okhttp3?|retrofit2?)\b": "HTTP client/server: devices only talk to their peers",
    r"\bHttpsURLConnection\b|\bHttpURLConnection\b|\bURLSession\b": "HTTP client: devices only talk to their peers",
    r"https://(?!mozilla\.org/MPL/)": "URL in code that must not contact servers",
}


# iOS verifier: iOS cannot remove the network from part of an app, so the guarantee
# is in the code. No network and no writing to disk; only AVCaptureVideoDataOutput (memory).
IOS_VERIFIER = "apple/ios/VerifierKit/"
IOS_VERIFIER_FORBIDDEN: dict[str, str] = {
    r"import\s+Network\b|URLSession|NWConnection|CFSocket|CFStream|import\s+Alamofire": "VerifierKit cannot use the network",
    r"AVAssetWriter|AVAudioRecorder|\bwrite\(to:|FileManager|UserDefaults": "VerifierKit cannot write to disk",
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
    """The local database MUST be encrypted: rusqlite only with SQLCipher."""
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    line = next((l for l in cargo.splitlines() if l.strip().startswith("rusqlite")), "")
    return "bundled-sqlcipher" in line


def main() -> int:
    allowed = load_allowlist()
    failures: list[str] = []
    if not sqlcipher_enabled():
        failures.append("Cargo.toml: rusqlite without SQLCipher — the local database would be unencrypted")
    for path in ROOT.rglob("*"):
        if not path.is_file() or path.suffix not in SOURCE_EXT:
            continue
        rel = path.relative_to(ROOT).as_posix()
        if any(part in SKIP_DIRS for part in path.relative_to(ROOT).parts):
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        if path.suffix == ".toml":
            # Manifests: only the dependency rule ("no servers").
            if rel.startswith(NO_SERVER_PATHS):
                rules = dict(NO_SERVER_FORBIDDEN)
                for pattern, reason in rules.items():
                    for match in re.finditer(pattern, text):
                        line_no = text.count("\n", 0, match.start()) + 1
                        failures.append(f"{rel}:{line_no}: '{match.group(0)}' — {reason}")
            continue
        rules = dict(FORBIDDEN)
        if rel.startswith("verify/"):
            rules.update(VERIFY_FORBIDDEN)
        if rel.startswith(IOS_VERIFIER):
            rules.update(IOS_VERIFIER_FORBIDDEN)
        if rel.startswith(NO_SERVER_PATHS):
            rules.update(NO_SERVER_FORBIDDEN)
        for pattern, reason in rules.items():
            if (rel, pattern) in allowed:
                continue
            for match in re.finditer(pattern, text):
                line_no = text.count("\n", 0, match.start()) + 1
                failures.append(f"{rel}:{line_no}: '{match.group(0)}' — {reason}")
    if failures:
        print("Privacy scanner: forbidden APIs found\n")
        print("\n".join(failures))
        return 1
    print("Privacy scanner: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
