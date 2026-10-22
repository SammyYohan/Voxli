/** Minimal i18n of Voxli v2 (RF-17 amended, docs/11): the UI
 * language (`uiLanguage`) is independent from the ASR listening language
 * (`general.language`). Values: `system` (automatic detection) or a
 * code among LANGS; any unknown value falls back to "fr" — never
 * an obscure screen (RF-29: open messages, never mute).
 *
 * Mechanics:
 * - `setUiLanguage("system"|code)` normalizes (system → the language
 *   detected in `navigator.languages`, membership by prefix: pt-BR → pt,
 *   zh-TW → zh…), sets `<html lang>` AND `<html dir>` (ar → rtl);
 * - `t("key")` renders the current string, CHAIN fallback:
 *   current language → `en` → `fr` → the key itself;
 * - `applyDom()` applies the language to the static elements:
 *   `[data-i18n]` (textContent), `[data-i18n-html]` (innerHTML — a clean
 *   string table, NEVER user content), `[data-i18n-title]`
 *   (title attribute) and `[data-i18n-placeholder]` (placeholder).
 */

import { dict as fr } from "./fr";
import { dict as en } from "./en";
import { dict as es } from "./es";
import { dict as pt } from "./pt";
import { dict as de } from "./de";
import { dict as it } from "./it";
import { dict as nl } from "./nl";
import { dict as ru } from "./ru";
import { dict as pl } from "./pl";
import { dict as tr } from "./tr";
import { dict as ar } from "./ar";
import { dict as hi } from "./hi";
import { dict as zh } from "./zh";
import { dict as ja } from "./ja";
import { dict as ko } from "./ko";

/** Available UI languages (the most spoken/used in the world). */
export type LangCode =
  | "fr"
  | "en"
  | "es"
  | "pt"
  | "de"
  | "it"
  | "nl"
  | "ru"
  | "pl"
  | "tr"
  | "ar"
  | "hi"
  | "zh"
  | "ja"
  | "ko";

/** Configuration value: detected from the system, or a fixed code. */
export type UiLanguage = "system" | LangCode;

const DICTS: Record<LangCode, Record<string, string>> = {
  fr,
  en,
  es,
  pt,
  de,
  it,
  nl,
  ru,
  pl,
  tr,
  ar,
  hi,
  zh,
  ja,
  ko,
};

/** Locale {date/number formatting} of each language. */
const LOCALES: Record<LangCode, string> = {
  fr: "fr-FR",
  en: "en-GB",
  es: "es-ES",
  pt: "pt-BR",
  de: "de-DE",
  it: "it-IT",
  nl: "nl-NL",
  ru: "ru-RU",
  pl: "pl-PL",
  tr: "tr-TR",
  ar: "ar",
  hi: "hi-IN",
  zh: "zh-CN",
  ja: "ja-JP",
  ko: "ko-KR",
};

/** Available codes, presentation order of the selector. */
export const LANGS: LangCode[] = [
  "fr",
  "en",
  "es",
  "pt",
  "de",
  "it",
  "nl",
  "ru",
  "pl",
  "tr",
  "ar",
  "hi",
  "zh",
  "ja",
  "ko",
];

/** RESOLVED current language (never "system": the selector shows the
 * raw config value, the engine renders the effective language). */
let current: LangCode = "fr";

/** BCP-47 prefix → code mapping table. */
const REGION_MAP: { prefix: string; code: LangCode }[] = LANGS.map((code) => ({
  prefix: code,
  code,
}));

/** System language detection: the first language of
 * `navigator.languages` whose primary code is supported
 * (pt-BR, pt-PT → pt; zh-TW → zh; fr-FR → fr…). Fallback "en": the
 * detection never short-circuits the user (RF-29). */
export function resolveSystemLanguage(): LangCode {
  const candidates =
    (navigator.languages && navigator.languages.length > 0
      ? navigator.languages
      : navigator.language
        ? [navigator.language]
        : []);
  for (const raw of candidates) {
    const base = raw.toLowerCase().split("-")[0];
    const hit = REGION_MAP.find((m) => m.prefix === base);
    if (hit !== undefined) return hit.code;
  }
  return "en";
}

/** Dictionary lookup in chain: current language → en → fr. */
function lookup(key: string): string {
  const own = DICTS[current][key];
  if (own !== undefined) return own;
  const fallback = DICTS.en[key];
  if (fallback !== undefined) return fallback;
  return DICTS.fr[key] ?? key;
}

/** Normalizes a configuration value (RESOLVED) and sets lang/dir. */
export function setUiLanguage(raw: string): LangCode {
  const code = resolveConfiguredLanguage(raw);
  current = code;
  document.documentElement.lang = current;
  document.documentElement.dir = current === "ar" ? "rtl" : "ltr";
  return current;
}

/** Raw config value → effective code ("system" → detection). */
export function resolveConfiguredLanguage(raw: string | undefined): LangCode {
  if (raw === undefined || raw === "system") return resolveSystemLanguage();
  const d = DICTS[raw as LangCode];
  return d !== undefined ? (raw as LangCode) : "fr";
}

export function uiLanguage(): LangCode {
  return current;
}

/** Formatting locale for the current language (dates, numbers). */
export function localeFor(): string {
  return LOCALES[current];
}

/** Current string of a key, with optional {n}-like interpolation. */
export function t(key: string, args?: Record<string, string | number>): string {
  let s = lookup(key);
  if (args !== undefined) {
    for (const [name, value] of Object.entries(args)) {
      s = s.split(`{${name}}`).join(String(value));
    }
  }
  return s;
}

/** Applies the current language to the static elements of the window. */
export function applyDom(): void {
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n]"))) {
    const key = el.dataset.i18n;
    if (key !== undefined && key !== "") el.textContent = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-html]"))) {
    const key = el.dataset.i18nHtml;
    // innerHTML here NEVER carries user content: the values
    // are the static strings of the dictionary (presentation
    // markup only).
    if (key !== undefined && key !== "") el.innerHTML = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-title]"))) {
    const key = el.dataset.i18nTitle;
    if (key !== undefined && key !== "") el.title = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-placeholder]"))) {
    const key = el.dataset.i18nPlaceholder;
    if (key !== undefined && key !== "") (el as HTMLInputElement).placeholder = t(key);
  }
}

/** Capsule: set the language then paint the static document. */
export function applyUiLanguage(raw: string): void {
  setUiLanguage(raw);
  applyDom();
}
