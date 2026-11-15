#!/usr/bin/env python3
"""Downloads the Android verifier models pinned in android/verifier/models.lock and checks
their SHA-256. A model with a different hash is NOT saved. Models are not committed to the repository.

Usage: python scripts/fetch_models.py
"""
import hashlib
import pathlib
import sys
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCK = ROOT / "android" / "verifier" / "models.lock"
DEST = ROOT / "android" / "verifier" / "src" / "main" / "assets" / "models"


def entries():
    for raw in LOCK.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if line:
            sha, name, url = line.split()
            yield sha.lower(), name, url


def main() -> int:
    items = list(entries())
    if not items:
        print("models.lock does not pin any model yet (spike E).")
        return 0
    DEST.mkdir(parents=True, exist_ok=True)
    for sha, name, url in items:
        if not url.startswith("https://"):
            print(f"{name}: https only")
            return 1
        data = urllib.request.urlopen(url, timeout=60).read()
        got = hashlib.sha256(data).hexdigest()
        if got != sha:
            print(f"{name}: different hash ({got}); NOT saved")
            return 1
        (DEST / name).write_bytes(data)
        print(f"{name}: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
