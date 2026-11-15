#!/usr/bin/env python3
"""Checks the local translation files (English is the source language everywhere).

Fails if:
  - the code uses a key that does not exist in English (desktop: t('key') / tn('key', …));
  - a translation has a key/name that no longer exists in English (stale entry);
  - a translation's placeholders differ from English ({n} on desktop, %1$s / %1$d on Android,
    %@ / %lld on Apple).
Missing translations are fine: they fall back to English. They are only counted.

Usage: python scripts/check_i18n.py          (also runs in `just check`)
"""
import json
import pathlib
import re
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
errors: list[str] = []


def placeholders(text: str, pattern: str) -> list[str]:
    return sorted(re.findall(pattern, text))


def compare(where: str, source: dict, other: dict, pattern: str) -> int:
    """Stale keys and placeholder mismatches are errors; returns how many keys are untranslated."""
    for key, value in other.items():
        if key not in source:
            errors.append(f"{where}: '{key}' does not exist in English (stale)")
        elif placeholders(value, pattern) != placeholders(source[key], pattern):
            errors.append(f"{where}: '{key}' placeholders differ from English")
    return len(source.keys() - other.keys())


# ── Desktop (Svelte): src/locales/<lang>.json ──
LOC = ROOT / "desktop/app/ui/src/locales"
en = json.loads((LOC / "en.json").read_text(encoding="utf-8"))
for path in sorted(LOC.glob("*.json")):
    if path.name != "en.json":
        missing = compare(f"desktop {path.name}", en, json.loads(path.read_text(encoding="utf-8")), r"\{\w+\}")
        print(f"desktop {path.stem}: {missing} untranslated (English is used)")
for path in (ROOT / "desktop/app/ui/src").rglob("*"):
    if path.suffix in (".svelte", ".ts"):
        for fn, key in re.findall(r"\b(tn?)\('([\w.]+)'", path.read_text(encoding="utf-8")):
            ok = key in en if fn == "t" else f"{key}.other" in en
            if not ok:
                errors.append(f"{path.relative_to(ROOT).as_posix()}: key '{key}' missing in en.json")


# ── Android: res/values/strings.xml (source) and res/values-<lang>/strings.xml ──
def android_strings(path: pathlib.Path) -> dict:
    out = {}
    for el in ET.parse(path).getroot():
        if el.tag == "string" and el.get("translatable") != "false":
            out[el.get("name")] = el.text or ""
        elif el.tag == "plurals":
            for item in el:
                out[f"{el.get('name')}#{item.get('quantity')}"] = item.text or ""
    return out


for module in ("app", "verifier"):
    res = ROOT / f"android/{module}/src/main/res"
    source = android_strings(res / "values/strings.xml")
    plural_other = {k.split("#")[0]: v for k, v in source.items() if k.endswith("#other")}
    for path in sorted(res.glob("values-*/strings.xml")):
        other = android_strings(path)
        # Each language has its own plural categories (one, few, many…): compare them with English "other"
        # A plural form may omit the count (%1$d, e.g. "one goal left"), but no other placeholder.
        flat = {k: v for k, v in other.items() if "#" not in k}
        args = lambda s: sorted(set(re.findall(r"%\d\$[sd]", s)) - {"%1$d"})  # noqa: E731
        for k, v in other.items():
            if "#" in k:
                name = k.split("#")[0]
                if name not in plural_other:
                    errors.append(f"android {module} {path.parent.name}: plural '{name}' does not exist in English (stale)")
                elif args(v) != args(plural_other[name]):
                    errors.append(f"android {module} {path.parent.name}: plural '{name}' placeholders differ from English")
        missing = compare(f"android {module} {path.parent.name}", {k: v for k, v in source.items() if "#" not in k}, flat, r"%\d\$[sd]")
        print(f"android {module} {path.parent.name}: {missing} untranslated strings (English is used)")


# ── Apple: String Catalog (source language: English) ──
catalog = json.loads((ROOT / "apple/ios/Shared/Localizable.xcstrings").read_text(encoding="utf-8"))
if catalog.get("sourceLanguage") != "en":
    errors.append("Localizable.xcstrings: sourceLanguage must be 'en'")
for key, entry in catalog["strings"].items():
    locs = entry.get("localizations", {})
    reference = locs.get("en", {}).get("stringUnit", {}).get("value", key)
    for lang, loc in locs.items():
        value = loc.get("stringUnit", {}).get("value", "")
        if placeholders(value, r"%(?:lld|@)") != placeholders(reference, r"%(?:lld|@)"):
            errors.append(f"apple {lang}: '{key}' placeholders differ from English")

if errors:
    print("\nTranslation check failed:", *errors, sep="\n  ")
    sys.exit(1)
print("Translation check: OK")
