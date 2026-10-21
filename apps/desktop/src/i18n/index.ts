/** i18n minimal de Voxli v2 (RF-17 amendé, docs/11) : la langue de
 * l'interface (`uiLanguage`) est indépendante de la langue d'écoute ASR
 * (`general.language`). Valeurs : `system` (détection automatique) ou un
 * code parmi LANGS ; toute valeur inconnue retombe sur « fr » — jamais
 * d'écran abscons (RF-29 : messages ouverts, jamais muets).
 *
 * Mécanisme :
 * - `setUiLanguage("system"|code)` normalise (système → langue détectée
 *   dans `navigator.languages`, appartenance par préfixe : pt-BR → pt,
 *   zh-TW → zh…), pose `<html lang>` ET `<html dir>` (ar → rtl) ;
 * - `t("clé")` rend la chaîne courante, repli en CHAÎNE : langue
 *   courante → `en` → `fr` → la clé elle-même ;
 * - `applyDom()` applique la langue aux éléments statiques :
 *   `[data-i18n]` (textContent), `[data-i18n-html]` (innerHTML — table
 *   de chaînes propre, JAMAIS de contenu utilisateur), `[data-i18n-title]`
 *   (attribut title) et `[data-i18n-placeholder]` (placeholder).
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

/** Langues d'interface disponibles (les plus parlées/utilisées au monde). */
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

/** Valeur de configuration : détectée du système, ou un code fixe. */
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

/** Locale {date/number formatting} de chaque langue. */
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

/** Codes disponibles, ordre de présentation du sélecteur. */
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

/** Langue courante RÉSOLUE (jamais « system » : le sélecteur montrera la
 * valeur brute de la config, le moteur rend la langue effective). */
let current: LangCode = "fr";

/** Table de correspondance préfixe BCP-47 → code. */
const REGION_MAP: { prefix: string; code: LangCode }[] = LANGS.map((code) => ({
  prefix: code,
  code,
}));

/** Détection de la langue système : premier langage de
 * `navigator.languages` dont le code principal est supporté
 * (pt-BR, pt-PT → pt ; zh-TW → zh ; fr-FR → fr…). Repli « en » : la
 * détection ne court-circuite jamais l'utilisateur (RF-29). */
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

/** Entrée d'un dictionnaire en chaîne : langue courante → en → fr. */
function lookup(key: string): string {
  const own = DICTS[current][key];
  if (own !== undefined) return own;
  const fallback = DICTS.en[key];
  if (fallback !== undefined) return fallback;
  return DICTS.fr[key] ?? key;
}

/** Normalise une valeur de configuration (RÉSOLUE) et pose lang/dir. */
export function setUiLanguage(raw: string): LangCode {
  const code = resolveConfiguredLanguage(raw);
  current = code;
  document.documentElement.lang = current;
  document.documentElement.dir = current === "ar" ? "rtl" : "ltr";
  return current;
}

/** Valeur brute de config → code effectif ("system" → détection). */
export function resolveConfiguredLanguage(raw: string | undefined): LangCode {
  if (raw === undefined || raw === "system") return resolveSystemLanguage();
  const d = DICTS[raw as LangCode];
  return d !== undefined ? (raw as LangCode) : "fr";
}

export function uiLanguage(): LangCode {
  return current;
}

/** Locale de formatage pour la langue courante (dates, nombres). */
export function localeFor(): string {
  return LOCALES[current];
}

/** Chaîne courante d'une clé, avec interpolation optionnelle {n}-like. */
export function t(key: string, args?: Record<string, string | number>): string {
  let s = lookup(key);
  if (args !== undefined) {
    for (const [name, value] of Object.entries(args)) {
      s = s.split(`{${name}}`).join(String(value));
    }
  }
  return s;
}

/** Applique la langue courante aux éléments statiques de la fenêtre. */
export function applyDom(): void {
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n]"))) {
    const key = el.dataset.i18n;
    if (key !== undefined && key !== "") el.textContent = t(key);
  }
  for (const el of Array.from(document.querySelectorAll<HTMLElement>("[data-i18n-html]"))) {
    const key = el.dataset.i18nHtml;
    // innerHTML ici ne porte JAMAIS de contenu utilisateur : les valeurs
    // sont les chaînes statiques du dictionnaire (seul le balisage de
    // présentation).
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

/** Capsule : définir la langue puis peindre le document statique. */
export function applyUiLanguage(raw: string): void {
  setUiLanguage(raw);
  applyDom();
}
