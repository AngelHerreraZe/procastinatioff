#!/usr/bin/env python3
# scripts/check_verify_no_net.py
"""Falla si el crate procastinacioff-verify depende (directa o indirectamente) de crates de red."""
import pathlib
import subprocess
import sys

FORBIDDEN = {"tokio", "mio", "socket2", "hyper", "h2", "reqwest", "ureq", "rustls",
             "native-tls", "openssl", "tungstenite", "quinn", "curl"}

# ponytail: el crate verify/ llega en la Fase 2; hasta entonces no hay nada que comprobar.
if not (pathlib.Path(__file__).resolve().parent.parent / "verify" / "Cargo.toml").exists():
    print("verify/ aún no existe (Fase 2): OK")
    sys.exit(0)

out = subprocess.run(
    ["cargo", "tree", "-p", "procastinacioff-verify", "--edges", "normal,build",
     "--prefix", "none", "--format", "{p}"],
    check=True, capture_output=True, text=True,
).stdout
found = sorted({line.split()[0] for line in out.splitlines() if line.strip()} & FORBIDDEN)
if found:
    print("verify/ depende de crates de red:", ", ".join(found))
    sys.exit(1)
print("verify/ sin dependencias de red: OK")
