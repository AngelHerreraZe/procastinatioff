#!/usr/bin/env python3
"""Merges domain lists (hosts format or one per line), normalizes, deduplicates and writes the v1 format.

Usage:  python scripts/build_category_list.py categories.txt sources/*.txt
        python scripts/build_category_list.py --test
Then, on the air-gapped machine: minisign -S -s lists.key -m categories.txt
Only sources registered in lists/sources.toml with a compatible license (MIT, CC0, CC-BY…).
"""
import re
import sys

DOMAIN = re.compile(r"^(?=.{1,253}$)([a-z0-9-]{1,63}\.)+[a-z]{2,63}$")
HEADER = "procastinacioff-categories v1"
# Never block what the filter needs (ESSENTIAL_DOMAINS also protects it in the core).
NEVER = {"localhost", "localhost.localdomain", "local", "broadcasthost"}


def domains(lines):
    for line in lines:
        line = line.split("#", 1)[0].strip().lower()
        if not line:
            continue
        parts = line.split()
        d = parts[-1] if len(parts) > 1 else parts[0]  # "0.0.0.0 example.com" or "example.com"
        d = d.rstrip(".")
        if d not in NEVER and DOMAIN.match(d):
            yield d


def build(sources):
    return HEADER + "\n" + "".join(d + "\n" for d in sorted({d for lines in sources for d in domains(lines)}))


def _test():
    out = build([["0.0.0.0 Ejemplo.COM", "# comentario", "127.0.0.1 localhost", "x.y.org.  # fin"], ["ejemplo.com", "1.2.3.4", "sin_punto"]])
    assert out == HEADER + "\nejemplo.com\nx.y.org\n", out
    print("ok")


if __name__ == "__main__":
    if sys.argv[1:] == ["--test"]:
        _test()
        sys.exit(0)
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    out, paths = sys.argv[1], sys.argv[2:]
    text = build(open(p, encoding="utf-8", errors="ignore") for p in paths)
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"{text.count(chr(10)) - 1} dominios → {out}")
