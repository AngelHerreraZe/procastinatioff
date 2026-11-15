/// <reference types="vite/client" />
// Local translations. Every `../locales/<lang>.json` file is bundled at build time; nothing is ever
// downloaded. English (`en.json`) is the source language and the fallback for missing keys.
// To add a language: copy `en.json` to `<lang>.json` (BCP 47 code: `de`, `pt-BR`…) and translate the values.

type Dict = Record<string, string>;

const files = import.meta.glob<Dict>('../locales/*.json', { eager: true, import: 'default' });
const dicts: Record<string, Dict> = {};
for (const [path, dict] of Object.entries(files)) dicts[path.replace(/^.*\/|\.json$/g, '')] = dict;

/** Best match for the system languages: "es-MX" → "es-MX", then "es", then English. */
function pick(): string {
  for (const tag of navigator.languages?.length ? navigator.languages : [navigator.language]) {
    if (dicts[tag]) return tag;
    const base = tag.split('-')[0];
    if (dicts[base]) return base;
  }
  return 'en';
}

export const locale = pick();
const dict = dicts[locale] ?? {};
const fallback = dicts.en ?? {};
const plurals = new Intl.PluralRules(locale);

type Vars = Record<string, string | number>;

/** Translates `key`, replacing `{name}` with `vars.name`. Missing key → English → the key itself. */
export function t(key: string, vars: Vars = {}): string {
  const s = dict[key] ?? fallback[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (m, k: string) => (k in vars ? String(vars[k]) : m));
}

/** Plural: uses `key.one`, `key.few`, `key.other`… (CLDR categories) for `n`, exposed as `{n}`. */
export function tn(key: string, n: number, vars: Vars = {}): string {
  const k = `${key}.${plurals.select(n)}`;
  return t(k in dict || k in fallback ? k : `${key}.other`, { n, ...vars });
}

/**
 * Translated text for an error. The service and the native layer send codes, never sentences:
 * `denied.*` / `error.*` (core) and `verify.*` (desktop verifier), optionally followed by ":detail".
 */
export function errorText(e: unknown): string {
  const err = e as { code?: string; cooldown_minutes?: number } | string;
  const raw = typeof err === 'string' ? err : (err?.code ?? '');
  const code = raw.split(':')[0];
  const minutes = typeof err === 'object' ? (err?.cooldown_minutes ?? 0) : 0;
  return code in dict || code in fallback ? t(code, { minutes }) : t('error.generic');
}

/** Short weekday names, Monday first, in the user's language (no translation files needed). */
export const weekdayNames = (style: 'narrow' | 'long'): string[] =>
  // 2026-03-09 is a Monday
  Array.from({ length: 7 }, (_, i) => new Date(2026, 2, 9 + i).toLocaleDateString(locale, { weekday: style }));
