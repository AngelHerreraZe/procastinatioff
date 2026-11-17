#!/usr/bin/env bash
# Reproducible builds check (plan 09 · B.2): builds the Linux agent twice from the committed tree,
# in two different folders, and compares the bytes. Different folders catch absolute paths;
# SOURCE_DATE_EPOCH and --remap-path-prefix remove the usual sources of difference.
#
# Usage (Linux, same toolchain as rust-toolchain.toml):  scripts/repro_check.sh [git ref]
# A third party runs the same command on the release tag and compares with SHA256SUMS.
set -euo pipefail
REF=${1:-HEAD}
ROOT=$(git rev-parse --show-toplevel)
CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
export SOURCE_DATE_EPOCH
SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct "$REF")
OUT=$(mktemp -d)
trap 'rm -rf "$OUT"' EXIT

for i in 1 2; do
  W="$OUT/build-$i"
  mkdir -p "$W"
  git -C "$ROOT" archive "$REF" | tar -x -C "$W"
  (
    cd "$W"
    RUSTFLAGS="--remap-path-prefix=$W=/build --remap-path-prefix=$CARGO_HOME=/cargo" \
      CARGO_TARGET_DIR="$W/target" cargo build --release --locked -p procastinatioff-agent
  )
  sha256sum "$W/target/release/procastinatioff-agent" | cut -d' ' -f1 >"$OUT/hash-$i"
done

if diff -q "$OUT/hash-1" "$OUT/hash-2" >/dev/null; then
  echo "REPRODUCIBLE procastinatioff-agent $(cat "$OUT/hash-1")"
else
  echo "NOT REPRODUCIBLE: $(cat "$OUT/hash-1") != $(cat "$OUT/hash-2")" >&2
  exit 1
fi
