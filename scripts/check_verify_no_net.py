#!/usr/bin/env python3
# scripts/check_verify_no_net.py
"""Fails if the procastinacioff-verify crate depends (directly or indirectly) on network crates."""
import pathlib
import subprocess
import sys

FORBIDDEN = {"tokio", "mio", "socket2", "hyper", "h2", "reqwest", "ureq", "rustls",
             "native-tls", "openssl", "tungstenite", "quinn", "curl"}

# ponytail: the verify/ crate must exist; otherwise there is nothing to check.
if not (pathlib.Path(__file__).resolve().parent.parent / "verify" / "Cargo.toml").exists():
    print("verify/ does not exist: OK")
    sys.exit(0)

out = subprocess.run(
    ["cargo", "tree", "-p", "procastinacioff-verify", "--edges", "normal,build",
     "--prefix", "none", "--format", "{p}"],
    check=True, capture_output=True, text=True,
).stdout
found = sorted({line.split()[0] for line in out.splitlines() if line.strip()} & FORBIDDEN)
if found:
    print("verify/ depends on network crates:", ", ".join(found))
    sys.exit(1)
print("verify/ has no network dependencies: OK")
