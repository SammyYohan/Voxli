/** Interface de l'application — miroir TS du schéma v5 (camelCase) :
 * `uiLanguage` (langue d'affichage : « system » ou un code de i18n/) est
 * distinct de `language` (langue d'écoute ASR, RF-17). Textes via i18n/. */
interface ConfigJson {
  schema: number;
  general: {
    language: string;
    uiLanguage: string;
    /** Additif v5 : « auto » | « cpu » | « gpu » (inconnu = auto côté noyau). */
    compute: string;
    engine: string;
    model: string;
    postProcessing: string;
    history: { enabled: boolean; storage: string; retentionDays: number };
  };
  hotkeys: {
    pushToTalk: string;
    pasteLastTranscript: string;
    showLastTranscript: string;
    useLowLevelHook: boolean;
    mode: string;
  };
  audio: {
    device: string;
    preRollMs: number;
    maxRecordingSeconds: number;
    vad: { enabled: boolean; minSpeechMs: number; aggressiveness: string };
  };
  insertion: {
    rememberTargetAtStart: boolean;
    clipboardHoldMs: { default: number; slowApps: number; remoteDesktop: number };
    excludeFromClipboardHistory: boolean;
    excludedApps: string[];
  };
  privacy: {
    strictLocalMode: boolean;
    contextRead: string;
    telemetry: string;
    updates: string;
    consent: { remoteAsr: boolean; cloudPost: boolean };
  };
  profiles: {
    default: ProfileJson;
    code: ProfileJson;
    email: ProfileJson;
  };
}

interface ProfileJson {
  style: string;
  dictionary: string[];
  match: string[];
}

interface StateInfo {
  state: string;
  engine: string;
}

/** État d'un moteur retourné par `engines_list` : compilé dans ce build
 * et réellement utilisable (modèle présent sur le disque). */
interface EngineInfo {
  id: string;
  compiled: boolean;
  usable: boolean;
}

interface DeviceInfo {
  id: string;
  name: string;
  isDefault: boolean;
}

interface ModelInfo {
  id: string;
  label: string;
  kind: string;
  sizeMb: number | null;
  license: string;
  installed: boolean;
  sizeBytes: number | null;
  sha256: string | null;
  verified: boolean;
}

interface HistoryItem {
  id: number;
  text: string;
  engine: string;
  createdMs: number;
}

interface LicenseInfo {
  state: string;
  licensee: string;
  features: string[];
  expires: string | null;
  expiresAtSecs: number | null;
}

type Notice = { kind: string; message: string };

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyDom, applyUiLanguage, localeFor, t } from "./i18n";
import { openUrl } from "@tauri-apps/plugin-opener";

// ================================ Don (Ko-fi / PayPal.me) ==================
// Discret par conception (RF-29 : visible, jamais intrusif) :
// - rappel PERMANENT dans le pied de page (ligne donate-row) ;
// - encart périodique : à des paliers d'usage (100/400/1000/3000 dictées)
//   il s'affiche dans les RÉGLAGES — jamais pendant la dictée (la pastille
//   est une autre fenêtre), fermable d'un clic, et « ne plus me le
//   proposer » est définitif (état local, hors configuration).
// Les liens s'ouvrent via le plugin opener, dont les permissions sont
// limitées aux deux URLs (capabilities/default.json).
const DONATE_KOFI = "https://ko-fi.com/sammyyohan";
const DONATE_PAYPAL = "https://paypal.me/sammyyohan";
const DONATE_MILESTONES = [100, 400, 1000, 3000];
const DONATE_KEY_COUNT = "voxli.donate.count";
const DONATE_KEY_MILESTONE = "voxli.donate.milestone";
const DONATE_KEY_FOREVER = "voxli.donate.forever";
const DONATE_KEY_PENDING = "voxli.donate.pending";

function donateCount(): number {
  const raw = Number.parseInt(localStorage.getItem(DONATE_KEY_COUNT) ?? "0", 10);
  return Number.isFinite(raw) ? raw : 0;
}

/** Un enregistrement de plus. Si un palier est franchi pour la première
 * fois, arme l'encart (réservé à la prochaine ouverture des réglages) et
 * retourne true pour un montage immédiat si la fenêtre est déjà ouverte. */
function donateTick(): boolean {
  const count = donateCount() + 1;
  localStorage.setItem(DONATE_KEY_COUNT, String(count));
  if (localStorage.getItem(DONATE_KEY_FOREVER) === "1") return false;
  const reached = DONATE_MILESTONES.filter((m) => count >= m).length;
  const shown = Number.parseInt(localStorage.getItem(DONATE_KEY_MILESTONE) ?? "0", 10);
  if (Number.isFinite(shown) && reached > shown) {
    localStorage.setItem(DONATE_KEY_MILESTONE, String(reached));
    localStorage.setItem(DONATE_KEY_PENDING, "1");
    return true;
  }
  return false;
}

async function openDonate(url: string): Promise<void> {
  try {
    await openUrl(url);
  } catch (err) {
    // Jamais silencieux (RF-29) : l'URL reste lisible dans la notice,
    // copiable même si l'ouverture système échoue.
    showNotice(`${url} — ${String(err)}`);
  }
}

function showDonateBanner(): void {
  ($id("donate-banner") as HTMLElement).hidden = false;
}

function wireDonate(): void {
  $id("btn-donate-kofi").addEventListener("click", () => void openDonate(DONATE_KOFI));
  $id("btn-donate-paypal").addEventListener("click", () => void openDonate(DONATE_PAYPAL));
  $id("link-donate-kofi").addEventListener("click", (e) => {
    e.preventDefault();
    void openDonate(DONATE_KOFI);
  });
  $id("link-donate-paypal").addEventListener("click", (e) => {
    e.preventDefault();
    void openDonate(DONATE_PAYPAL);
  });
  $id("btn-donate-later").addEventListener("click", () => {
    ($id("donate-banner") as HTMLElement).hidden = true;
  });
  $id("btn-donate-forever").addEventListener("click", () => {
    localStorage.setItem(DONATE_KEY_FOREVER, "1");
    localStorage.removeItem(DONATE_KEY_PENDING);
    ($id("donate-banner") as HTMLElement).hidden = true;
  });
}
// ============================== fin don ====================================

let config: ConfigJson | null = null;

/** Catalogue relayé par `models_list` : libellés affichés pendant la
 * progression (l'événement `models://progress` ne porte que l'id). */
const knownModels: ModelInfo[] = [];

/** Téléchargement en cours (RF-36 : un seul à la fois). Pilote l'état
 * « occupé » des boutons et la visibilité du bouton Annuler. */
let downloadingId: string | null = null;

function $id(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (el === null) throw new Error(`élément manquant : #${id}`);
  return el;
}

let noticeTimer = 0;

function showNotice(message: string): void {
  const box = $id("notice") as HTMLDivElement;
  box.textContent = message;
  box.hidden = false;
  window.clearTimeout(noticeTimer);
  noticeTimer = window.setTimeout(() => {
    box.hidden = true;
  }, 6000);
}

/** Lancement d'un téléchargement de modèle (onboarding ou catalogue) :
 * bande de progression visible, nom du fichier affiché, bouton Annuler
 * branché sur `models_download_cancel` (RF-36 : action explicite). Un
 * seul téléchargement à la fois — les autres demandes sont ignorées. */
async function startDownload(id: string): Promise<void> {
  if (downloadingId !== null) {
    showNotice(t("models.busy"));
    return;
  }
  downloadingId = id;
  const bar = $id("model-progress");
  ($id("btn-model-cancel") as HTMLElement).hidden = false;
  // Reprise visible à zéro : la première mise à jour serveur réajuste
  // (le % résiduel de l'essai précédent n'a pas de sens).
  (document.getElementById("model-progress-fill") as HTMLElement).style.width = "0%";
  bar.classList.remove("indeterminate");
  bar.hidden = false;
  const label = knownModels.find((m) => m.id === id)?.label ?? id;
  ($id("model-status") as HTMLElement).textContent = t("models.downloadingName", {
    name: label,
  });
  try {
    await invoke("models_download", { id });
    showNotice(t("notice.modelStarted"));
  } catch (err) {
    // Refus immédiat (déjà en cours, modèle inconnu…) : remettre l'écran
    // au propre — jamais de barre fantôme.
    downloadingId = null;
    ($id("btn-model-cancel") as HTMLElement).hidden = true;
    bar.hidden = true;
    ($id("model-status") as HTMLElement).textContent = "";
    showNotice(String(err));
  }
}

function paintConfig(cfg: ConfigJson, lic: LicenseInfo): void {
  ($id("hk-push") as HTMLInputElement).value = cfg.hotkeys.pushToTalk;
  ($id("hk-paste") as HTMLInputElement).value = cfg.hotkeys.pasteLastTranscript;
  ($id("hk-show") as HTMLInputElement).value = cfg.hotkeys.showLastTranscript;
  ($id("hk-mode") as HTMLSelectElement).value = cfg.hotkeys.mode;
  refreshHotkeyConflicts();
  ($id("pre-roll") as HTMLInputElement).value = String(cfg.audio.preRollMs);
  ($id("max-rec") as HTMLInputElement).value = String(cfg.audio.maxRecordingSeconds);
  ($id("gen-post") as HTMLSelectElement).value = cfg.general.postProcessing;
  ($id("vad-aggr") as HTMLSelectElement).value = cfg.audio.vad.aggressiveness;
  ($id("pv-strict") as HTMLSelectElement).value = cfg.privacy.strictLocalMode ? "on" : "off";
  ($id("pv-context") as HTMLSelectElement).value = cfg.privacy.contextRead;
  ($id("pv-consent-asr") as HTMLSelectElement).value = cfg.privacy.consent.remoteAsr ? "on" : "off";
  ($id("pv-consent-post") as HTMLSelectElement).value = cfg.privacy.consent.cloudPost ? "on" : "off";
  ($id("hist-enabled") as HTMLSelectElement).value = cfg.general.history.enabled ? "yes" : "no";
  ($id("hist-storage") as HTMLSelectElement).value = cfg.general.history.storage;
  ($id("hist-retention") as HTMLInputElement).value = String(cfg.general.history.retentionDays);
  ($id("prof-code-apps") as HTMLInputElement).value = cfg.profiles.code.match.join(", ");
  ($id("prof-email-apps") as HTMLInputElement).value = cfg.profiles.email.match.join(", ");
  ($id("dict-code") as HTMLTextAreaElement).value = cfg.profiles.code.dictionary.join("\n");
  ($id("dict-email") as HTMLTextAreaElement).value = cfg.profiles.email.dictionary.join("\n");
  // Langue d'écoute : garder une option libre si le fichier contient
  // autre chose ; la langue d'affichage (v5) est normalisée fr|en.
  const langInput = ($id("gen-language") as HTMLSelectElement);
  if (![...langInput.options].some((o) => o.value === cfg.general.language)) {
    const opt = document.createElement("option");
    opt.value = cfg.general.language;
    opt.textContent = cfg.general.language;
    langInput.appendChild(opt);
  }
  langInput.value = cfg.general.language;
  // La valeur brute de la config (« system » ou un code) est assignée
  // telle quelle ; si elle n'est plus dans la liste, le sélecteur garde
  // sa première option (« system », auto-réparant au prochain envoi).
  const uiLangSel0 = $id("gen-ui-language") as HTMLSelectElement;
  if ([...uiLangSel0.options].some((o) => o.value === cfg.general.uiLanguage)) {
    uiLangSel0.value = cfg.general.uiLanguage;
  }
  // Choix de calcul (additif v5) : si la valeur stockée n'est plus dans la
  // liste (config à la main), le sélecteur reste sur « auto » — sûr.
  const calcSel0 = $id("calc-select") as HTMLSelectElement;
  calcSel0.value = [...calcSel0.options].some((o) => o.value === cfg.general.compute)
    ? cfg.general.compute
    : "auto";
  // RF-39 : profils multiples = fonctionnalité Pro — avertissement clair.
  const lock = $id("profiles-lock");
  const pro =
    (lic.state === "active" || lic.state === "grace") &&
    lic.features.includes("extra-profiles");
  lock.textContent = pro ? "" : t("profiles.lockNotice");
  lock.hidden = pro;

  renderVocabChips();
}

/** Poussée au noyau (RF-01 : live, réglages + pipeline). */
async function sendConfig(): Promise<void> {
  if (config === null) return;
  // Raccourcis : une combinaison en conflit (« occupé » visible) n'est
  // PAS enregistrée — l'écran reste véridique ; le noyau applique la
  // même règle (validate_before_register) et le champ corrigé redevient
  // enregistrable (RF-01 : les raccourcis prennent à l'enregistrement).
  const hkConflict = document.querySelector(".conflict") !== null;
  if (hkConflict) showNotice(t("hk.conflict"));
  if (!hkConflict) {
    config.hotkeys.pushToTalk =
      ($id("hk-push") as HTMLInputElement).value.trim() || config.hotkeys.pushToTalk;
    config.hotkeys.pasteLastTranscript =
      ($id("hk-paste") as HTMLInputElement).value.trim() || config.hotkeys.pasteLastTranscript;
    config.hotkeys.showLastTranscript =
      ($id("hk-show") as HTMLInputElement).value.trim() || config.hotkeys.showLastTranscript;
  }
  config.hotkeys.mode = ($id("hk-mode") as HTMLSelectElement).value;
  const preRoll = Number(($id("pre-roll") as HTMLInputElement).value);
  if (Number.isFinite(preRoll)) config.audio.preRollMs = preRoll;
  const maxRec = Number(($id("max-rec") as HTMLInputElement).value);
  if (Number.isFinite(maxRec)) config.audio.maxRecordingSeconds = maxRec;
  const retention = Number(($id("hist-retention") as HTMLInputElement).value);
  if (Number.isFinite(retention)) config.general.history.retentionDays = retention;
  config.general.language = ($id("gen-language") as HTMLSelectElement).value;
  config.general.uiLanguage = ($id("gen-ui-language") as HTMLSelectElement).value;
  // Choix de calcul (additif v5) : miroir comme tous les autres, invalidé
  // à `auto` côté noyau si jamais une valeur inattendue arrive.
  config.general.compute = ($id("calc-select") as HTMLSelectElement).value;
  config.general.postProcessing = ($id("gen-post") as HTMLSelectElement).value;
  config.audio.vad.aggressiveness = ($id("vad-aggr") as HTMLSelectElement).value;
  config.audio.device = ($id("audio-device") as HTMLSelectElement).value;
  config.privacy.strictLocalMode = ($id("pv-strict") as HTMLSelectElement).value === "on";
  config.privacy.contextRead = ($id("pv-context") as HTMLSelectElement).value;
  config.privacy.consent.remoteAsr = ($id("pv-consent-asr") as HTMLSelectElement).value === "on";
  config.privacy.consent.cloudPost = ($id("pv-consent-post") as HTMLSelectElement).value === "on";
  config.general.history.enabled = ($id("hist-enabled") as HTMLSelectElement).value === "yes";
  config.general.history.storage = ($id("hist-storage") as HTMLSelectElement).value;
  config.profiles.code.match = listOfApps("prof-code-apps");
  config.profiles.email.match = listOfApps("prof-email-apps");

  try {
    await invoke("config_set", { newConfig: config });
    ($id("save-result") as HTMLElement).textContent = t("general.saved");
    window.setTimeout(() => {
      ($id("save-result") as HTMLElement).textContent = "";
    }, 4000);
  } catch (err) {
    showNotice(String(err));
  }
}

function listOfApps(inputId: string): string[] {
  return ($id(inputId) as HTMLInputElement).value
    .split(",")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
}

/** Raccourcis : capture à la volée (Wispr-simple — on appuie, ça s'écrit).
 * Jetons = whitelist stricte du noyau (`voxli-hotkeys`, `Key::parse`) :
 * lettres, chiffres, F1..F24, Espace, Tab, Entrée, `. Rien d'autre n'est
 * accepté, jamais d'échec muet à l'enregistrement côté noyau.
 * Note : la combinaison d'un raccourci ACTUEL reste interceptée par le
 * système tant qu'il est enregistré (comportement voulu du noyau) — la
 * capture sert à choisir une combinaison LIBRE. */
const HK_IDS = ["hk-push", "hk-paste", "hk-show"] as const;
const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta", "AltGraph"]);
const NAMED_KEYS = new Map([
  [" ", "Space"],
  ["Enter", "Enter"],
  ["Tab", "Tab"],
  ["`", "Backquote"],
]);

function hotkeyToken(ev: KeyboardEvent): string | null {
  const k = ev.key;
  const named = NAMED_KEYS.get(k);
  if (named !== undefined) return named;
  if (k.length === 1 && /[a-z0-9]/i.test(k)) return k.toUpperCase();
  const fn = /^F(\d{1,2})$/.exec(k);
  if (fn !== null) {
    const n = Number(fn[1]);
    if (n >= 1 && n <= 24) return k;
  }
  return null;
}

/** Conflit : la même combinaison assignée à deux actions — marquée
 * « occupé » et NON enregistrée (règle miroir de
 * `validate_before_register`, la vérité reste celle du noyau). */
function refreshHotkeyConflicts(): void {
  const values = HK_IDS.map((id) => ($id(id) as HTMLInputElement).value.trim());
  const conflicted = new Set<number>();
  for (let i = 0; i < values.length; i += 1) {
    if (values[i] === "") continue;
    for (let j = i + 1; j < values.length; j += 1) {
      if (values[i] === values[j]) {
        conflicted.add(i);
        conflicted.add(j);
      }
    }
  }
  HK_IDS.forEach((id, i) => {
    ($id(id) as HTMLInputElement).classList.toggle("conflict", conflicted.has(i));
  });
  const line = document.getElementById("hk-conflict");
  if (line !== null) line.hidden = conflicted.size === 0;
}

function wireHotkeyCapture(input: HTMLInputElement): void {
  const before = input.value;
  input.addEventListener("focus", () => {
    input.dataset.capture = "1";
  });
  input.addEventListener("blur", () => {
    delete input.dataset.capture;
    if (input.value.trim() === "") input.value = before;
    refreshHotkeyConflicts();
  });
  input.addEventListener("keydown", (ev) => {
    ev.preventDefault();
    ev.stopPropagation();
    if (ev.key === "Escape") {
      input.value = before;
      input.blur();
      refreshHotkeyConflicts();
      return;
    }
    const parts: string[] = [];
    if (ev.ctrlKey) parts.push("Ctrl");
    if (ev.altKey) parts.push("Alt");
    if (ev.shiftKey) parts.push("Shift");
    if (ev.metaKey) parts.push("Win");
    const token = hotkeyToken(ev);
    if (token === null) {
      // Modificateurs seuls : la capture continue, l'affichage suit.
      // Une touche réelle hors liste blanche (↓, Home, «, »…) n'écrit
      // rien et l'explique — jamais d'échec muet.
      if (parts.length === 0 && !MODIFIER_KEYS.has(ev.key)) {
        showNotice(t("hk.unsupported"));
      }
      input.value = parts.length > 0 ? `${parts.join("+")}+` : "";
      return;
    }
    if (parts.length === 0) {
      // Touche seule sans modificateur : refus polis, jamais muet.
      showNotice(t("hk.needModifier"));
      input.value = before;
      return;
    }
    parts.push(token);
    input.value = parts.join("+");
    refreshHotkeyConflicts();
  });
  input.addEventListener("input", refreshHotkeyConflicts);
}

/** Simple / Avancé : préférence locale à la fenêtre (localStorage) —
 * choix de présentation, pas une configuration produit. */
const MODE_KEY = "voxli.ui.avance";

/** Thème : clair / sombre / celui du système. Préférence locale, appliquée
 * dès le script de tête de la page (pas de flash). */
const THEME_KEY = "voxli.ui.theme";
const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function applyTheme(): void {
  const pref = localStorage.getItem(THEME_KEY) ?? "system";
  const effective =
    pref === "light" || pref === "dark"
      ? pref
      : darkQuery.matches
        ? "dark"
        : "light";
  document.documentElement.dataset.theme = effective;
}

function applyUiMode(): void {
  const avance = localStorage.getItem(MODE_KEY) === "1";
  document.body.classList.toggle("mode-avance", avance);
  ($id("mode-avance") as HTMLInputElement).checked = avance;
}

function escapeHtml(s: string): string {
  return s
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

/** Historique : lecture + peinture bornée (RF-33). */
async function refreshHistory(): Promise<void> {
  try {
    const items: HistoryItem[] = await invoke("history_query", { n: 50 });
    const locale = localeFor();
    const body = $id("history-body") as HTMLElement;
    body.replaceChildren();
    for (const it of items) {
      const tr = document.createElement("tr");
      const date = new Date(it.createdMs).toLocaleString(locale);
      tr.innerHTML =
        `<td>${escapeHtml(date)}</td>` +
        `<td>${escapeHtml(it.engine)}</td>` +
        `<td class="wrap">${escapeHtml(it.text)}</td>`;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = t("history.delete");
      btn.addEventListener("click", async () => {
        await invoke("history_delete", { id: it.id });
        void refreshHistory();
      });
      const td = document.createElement("td");
      td.appendChild(btn);
      tr.appendChild(td);
      body.appendChild(tr);
    }
    if (items.length === 0) {
      const tr = document.createElement("tr");
      tr.innerHTML = `<td colspan="4" class="muted">${escapeHtml(t("history.empty"))}</td>`;
      body.appendChild(tr);
    }
  } catch {
    // Pas de store → tableau vide.
  }
}

async function refreshLicense(): Promise<LicenseInfo> {
  const data = await invoke<{ license: LicenseInfo; path: string }>("license_status");
  const lic = data.license;
  const line = $id("license-line");
  if (lic.state === "active") {
    line.textContent = t("license.active", { name: lic.licensee });
  } else if (lic.state === "grace") {
    const remaining = lic.expiresAtSecs ?? 0;
    const days = Math.max(0, Math.ceil((remaining - Date.now() / 1000) / 86_400));
    line.textContent = t("license.grace", { days });
  } else {
    line.textContent = t("license.free");
  }
  const list = $id("license-features") as HTMLElement;
  list.replaceChildren();
  if (lic.features.length === 0) {
    const li = document.createElement("li");
    li.textContent = t("license.noFeatures");
    list.appendChild(li);
  }
  for (const f of lic.features) {
    const li = document.createElement("li");
    li.textContent = f;
    list.appendChild(li);
  }
  return lic;
}

/** Activation d'un modèle whisper installé : `general.model` = id et
 * `general.engine` = whisper — écriture live (RF-01), le worker
 * reconstruit son bundle ; la table et les radios sont deux vues de la
 * MÊME config (jamais deux vérités). */
async function activateWhisperModel(id: string): Promise<void> {
  if (config === null) return;
  const changed =
    config.general.model !== id || config.general.engine !== "whisper";
  if (!changed) return void refreshModels();
  config.general.model = id;
  config.general.engine = "whisper";
  try {
    await invoke("config_set", { newConfig: config });
  } catch (err) {
    showNotice(String(err));
  }
  void refreshEngines();
  void effectiveEngineLine();
}

async function refreshModels(): Promise<void> {
  const models: ModelInfo[] = await invoke("models_list");
  knownModels.splice(0, knownModels.length, ...models);

  // Migration : un id de modèle non-whisper dans la config (empoisonné
  // jadis par l'ancienne auto-connexion sans garde — moteur factice au
  // démarrage) retourne à « auto », écrit UNE fois ; les tours suivants
  // constatent une config déjà saine et ne réécrivent rien.
  if (config !== null) {
    const cfg = config;
    if (cfg.general.model !== "auto") {
      const info = models.find((m) => m.id === cfg.general.model);
      if (info === undefined || info.kind !== "whisper") {
        cfg.general.model = "auto";
        cfg.general.engine = "auto";
        try {
          await invoke("config_set", { newConfig: cfg });
        } catch (err) {
          showNotice(String(err));
        }
      }
    }
  }

  const body = $id("models-body") as HTMLElement;
  body.replaceChildren();

  // Sélection à la Handy : la LIGNE est le sélecteur. La première ligne
  // n'est pas un modèle — c'est le choix « auto » (le noyau prend le
  // meilleur modèle présent ; cf. `resolve_whisper_model`, une seule
  // vérité worker/engines_list).
  const selectedAuto =
    config !== null &&
    config.general.model === "auto" &&
    config.general.engine === "auto";
  const autoRow = document.createElement("tr");
  autoRow.className = "selectable";
  autoRow.tabIndex = 0;
  autoRow.innerHTML =
    `<td><b>${escapeHtml(t("engines.auto"))}</b><br />` +
    `<span class="muted">${escapeHtml(t("models.autoHint"))}</span></td>` +
    `<td>${selectedAuto ? "✓" : ""}</td><td></td><td></td>`;
  autoRow.addEventListener("click", async () => {
    if (config === null) return;
    if (selectedAuto) return;
    config.general.engine = "auto";
    config.general.model = "auto";
    try {
      await invoke("config_set", { newConfig: config });
    } catch (err) {
      showNotice(String(err));
    }
    void refreshModels();
    void refreshEngines();
    void effectiveEngineLine();
  });
  body.appendChild(autoRow);

  for (const m of models) {
    const tr = document.createElement("tr");
    const footprint = m.sha256 ? `${m.sha256.slice(0, 16)}…` : "";
    const sizeTxt = m.sizeMb !== null ? ` ${t("models.sizeMb", { n: m.sizeMb })}` : "";
    const selected =
      config !== null &&
      config.general.engine === "whisper" &&
      config.general.model === m.id;
    // La ligne EST le sélecteur (Handy-style) : ✓ dans la colonne État
    // = le choix actif. Offlistes (nemo/parakeet) : non sélectionnables.
    tr.innerHTML =
      `<td>${escapeHtml(m.label)}<br /><span class="muted">` +
      `${escapeHtml(m.id)}${escapeHtml(sizeTxt)}` +
      `<br /><span class="license">${escapeHtml(m.license)}</span></span></td>` +
      `<td>${selected ? "✓ " : ""}${
        m.installed ? `<b>${escapeHtml(t("models.installed"))}</b>` : escapeHtml(t("models.notInstalled"))
      }</td>` +
      `<td class="mono">${escapeHtml(footprint)}${m.verified ? ` <b title="${escapeHtml(t("models.verifiedBadge"))}">✓</b>` : ""}</td>`;
    const td = document.createElement("td");
    if (m.installed) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = t("models.remove");
      btn.addEventListener("click", async (ev) => {
        ev.stopPropagation(); // la ligne sélectionne, le bouton retire
        try {
          await invoke("models_remove", { id: m.id });
          // Le modèle actif vient de partir : retour à « auto » écrit
          // immédiatement (le noyau résout le meilleur whisper présent) —
          // jamais de moteur muet sur une config pointant vers rien.
          if (config !== null && config.general.model === m.id) {
            config.general.model = "auto";
            config.general.engine = "auto";
            await invoke("config_set", { newConfig: config });
            void refreshEngines();
          }
          void refreshModels();
        } catch (err) {
          showNotice(String(err));
        }
      });
      td.appendChild(btn);
    } else {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = t("models.download");
      btn.addEventListener("click", (ev) => {
        ev.stopPropagation(); // la ligne télécharge, le bouton aussi —
        // pas de double déclenchement sur le clic de ligne.
        void startDownload(m.id);
      });
      td.appendChild(btn);
    }
    tr.appendChild(td);

    // La ligne EST le sélecteur (Handy-style) : survol cadré + clic.
    // whisper installé = activation ; whisper absent = téléchargement ;
    // nemo/parakeet = non sélectionnable (SDK requis dans un autre build)
    // — l'incohérence du 21/10 : télécharger un moteur inactivable
    // poussaient la config vers un modèle que le worker ne sait pas
    // monter (repli factice silencieux… réglé par le garde ci-dessous).
    if (m.kind !== "whisper") {
      tr.title = t("models.noActivation");
    } else {
      // classes + clic ; `selected` est déjà calculé en haut de boucle.
      tr.classList.add("selectable");
      if (selected) tr.classList.add("selected");
      tr.tabIndex = 0;
      if (m.installed) {
        tr.addEventListener("click", () => {
          void activateWhisperModel(m.id);
        });
      } else {
        tr.addEventListener("click", () => {
          void startDownload(m.id);
        });
      }
    }
    body.appendChild(tr);
  }
  // Règle Simple/Avancé : le catalogue détaillé (table des modèles) vit
  // dans la classe `advanced` (index.html) — il n'est plus masqué d'ici.
  // Ce qui reste VISIBLE en mode Simple, c'est la sélection du moteur
  // (les radios, avec leur état réel) : c'est elle qui répond à « quelle
  // voix est installée, laquelle est disponible ».

  // Bannière d'onboarding 1-clic : visible tant qu'aucun modèle n'est installé
  const onboarding = document.getElementById("onboarding-card");
  if (onboarding) {
    onboarding.hidden = models.some((m) => m.installed);
  }
}

/** Vue des moteurs de dictée : la sélection EST la config
 * `general.engine` ; l'écriture passe par `config_set` et le worker
 * reconstruit son bundle en live (jamais de redémarrage). Chaque moteur
 * est affiché avec son état réel du build et de la machine. */
async function refreshEngines(): Promise<void> {
  const engines: EngineInfo[] = await invoke("engines_list");
  const list = document.getElementById("engines-list");
  if (list === null) return;
  list.replaceChildren();
  for (const e of engines) {
    const row = document.createElement("label");
    row.className = "engine-choice";
    const radio = document.createElement("input");
    radio.type = "radio";
    radio.name = "engine-choice";
    radio.value = e.id;
    radio.checked = config !== null && config.general.engine === e.id;
    radio.disabled = !e.usable;
    row.appendChild(radio);
    const span = document.createElement("span");
    span.textContent = t(`engines.${e.id}`);
    row.appendChild(span);
    if (!e.usable) {
      row.classList.add("engine-unavailable");
      const hint = document.createElement("span");
      hint.className = "muted engine-hint";
      hint.textContent = e.compiled ? t("engines.hintModel") : t("engines.hintSdk");
      row.appendChild(hint);
    }
    radio.addEventListener("change", () => {
      if (radio.checked) void setEngine(e.id);
    });
    list.appendChild(row);
  }
  void effectiveEngineLine();
}

/** Sélection d'un moteur : config écrite immédiatement (RF-01 : le reste
 * de la config transmise est l'objet validé actuel, miroir de l'affichage). */
async function setEngine(id: string): Promise<void> {
  if (config === null || config.general.engine === id) return;
  try {
    config.general.engine = id;
    await invoke("config_set", { newConfig: config });
    void refreshEngines();
  } catch (err) {
    showNotice(String(err));
  }
}

/** Choix de calcul (CPU/GPU, additif v5) : la même sémantique que le
 * moteur — écrit via sendConfig (miroir complet), le noyau reconstruit
 * son bundle à chaud ; le journal consigne « calcul:… » (le demandé) à
 * chaque montage de moteur. */
async function setCompute(choice: string): Promise<void> {
  if (config === null || config.general.compute === choice) return;
  await sendConfig();
}

/** Moteur effectivement monté (source : `state_get`, donc le worker) —
 * le réel, pas le choix ; ligne distincte quand il diffère de la sélection.
 * Repaint AUSSI le badge d'état et le mot moteur : appelée au changement
 * de langue d'interface pour que le dynamique suive le statique. */
let lastEffectiveEngine: string | null = null;

async function effectiveEngineLine(): Promise<void> {
  const line = document.getElementById("engine-effective");
  if (line === null) return;
  try {
    const s = await invoke<StateInfo>("state_get");
    const raw = t(`engine.${s.engine}`);
    const name = raw === `engine.${s.engine}` ? s.engine : raw;
    line.hidden = false;
    line.textContent = t("engines.effective", { engine: name });
    // Badge + mot moteur : la même source que `state://changed`, sans
    // attendre l'événement suivant (changement de langue notamment).
    const badgeText = t(`state.${s.state}`);
    ($id("badge") as HTMLElement).textContent =
      badgeText === `state.${s.state}` ? s.state : badgeText;
    ($id("engine") as HTMLElement).textContent = name;
    // Transition → factice : avertissement UNE fois par transition
    // (le repli noyau est déjà consigné côté journal ; ici l'UI dit
    // pourquoi la dictée semble « fausse » et comment se soigner).
    if (s.engine === "fake" && lastEffectiveEngine !== null && lastEffectiveEngine !== "fake") {
      showNotice(t("engines.fellFake"));
    }
    lastEffectiveEngine = s.engine;
  } catch {
    // Lecture impossible : la ligne reste masquée (jamais de fausse info).
  }
}

let currentVocabProfile: "default" | "code" | "email" = "default";

function renderVocabChips(): void {
  if (!config) return;
  const dict = config.profiles[currentVocabProfile].dictionary;
  const container = $id("vocab-chips");
  container.replaceChildren();

  if (dict.length === 0) {
    const empty = document.createElement("span");
    empty.className = "vocab-empty";
    empty.textContent = t("vocab.empty");
    container.appendChild(empty);
    return;
  }

  dict.forEach((entry, idx) => {
    const parts = entry.split("|");
    const target = parts[0]?.trim() ?? entry;
    const variants = parts.slice(1).map((v) => v.trim()).filter((v) => v.length > 0);

    const chip = document.createElement("div");
    chip.className = "vocab-chip";

    const strong = document.createElement("strong");
    strong.textContent = target;
    chip.appendChild(strong);

    if (variants.length > 0) {
      const varsSpan = document.createElement("span");
      varsSpan.className = "variants";
      varsSpan.textContent = ` (${variants.join(", ")})`;
      chip.appendChild(varsSpan);
    }

    const delBtn = document.createElement("button");
    delBtn.className = "btn-del";
    delBtn.type = "button";
    delBtn.innerHTML = "&times;";
    delBtn.title = t("vocab.deleteTitle");
    delBtn.addEventListener("click", () => {
      void removeVocabWord(idx);
    });
    chip.appendChild(delBtn);

    container.appendChild(chip);
  });
}

async function addVocabWord(): Promise<void> {
  if (!config) return;
  const wordInput = $id("vocab-new-word") as HTMLInputElement;
  const variantsInput = $id("vocab-new-variants") as HTMLInputElement;
  const word = wordInput.value.trim();
  if (!word) return;

  const variants = variantsInput.value
    .split(",")
    .map((v) => v.trim())
    .filter((v) => v.length > 0);

  const entry = variants.length > 0 ? `${word}|${variants.join("|")}` : word;
  const dict = config.profiles[currentVocabProfile].dictionary;

  if (!dict.includes(entry)) {
    dict.push(entry);
    await importDict(currentVocabProfile, dict.join("\n"));
  }

  wordInput.value = "";
  variantsInput.value = "";
  wordInput.focus();
  renderVocabChips();
}

async function removeVocabWord(index: number): Promise<void> {
  if (!config) return;
  const dict = config.profiles[currentVocabProfile].dictionary;
  if (index >= 0 && index < dict.length) {
    dict.splice(index, 1);
    await importDict(currentVocabProfile, dict.join("\n"));
    renderVocabChips();
  }
}

async function refreshDevices(): Promise<void> {
  const current = config?.audio.device ?? "default";
  let devices: DeviceInfo[] = [];
  try {
    devices = await invoke<DeviceInfo[]>("devices_list");
  } catch {
    devices = [];
  }
  const select = $id("audio-device") as HTMLSelectElement;
  select.replaceChildren();
  const def = document.createElement("option");
  def.value = "default";
  def.textContent = t("audio.deviceDefault");
  select.appendChild(def);
  for (const d of devices) {
    const opt = document.createElement("option");
    opt.value = d.id;
    opt.textContent = d.isDefault ? `${d.name} ${t("audio.defaultSuffix")}` : d.name;
    select.appendChild(opt);
  }
  select.value =
    current === "default" || devices.some((d) => d.id === current)
      ? current
      : "default";
}

function applyMeter(level: number): void {
  const pct = Math.max(0, Math.min(1, level)) * 100;
  const bar = $id("meter") as HTMLElement;
  bar.style.width = `${pct.toFixed(1)}%`;
}

function downloadTextFile(name: string, content: string): void {
  const blob = new Blob([content], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

async function importDict(profile: string, csv: string): Promise<void> {
  try {
    const n = (await invoke("dictionaries_import", { profile, csv })) as number;
    showNotice(t("dict.imported", { n }));
  } catch (err) {
    showNotice(String(err));
  }
}

function wireEvents(): void {
  // Capture à la volée des raccourcis (cf. wireHotkeyCapture) + marquage
  // « occupé » immédiat en cas de doublon.
  for (const id of HK_IDS) {
    wireHotkeyCapture($id(id) as HTMLInputElement);
  }

  // Simple / Avancé (préférence de présentation, hors configuration) :
  // « mode-avance » plie/délie tout ce qui est marqué classe « advanced ».
  const modeBox = $id("mode-avance") as HTMLInputElement;
  modeBox.addEventListener("change", () => {
    localStorage.setItem(MODE_KEY, modeBox.checked ? "1" : "0");
    document.body.classList.toggle("mode-avance", modeBox.checked);
  });
  // Thème : la préférence enregistrée pilote data-theme ; l'attribut est
  // déjà posé avant la première peinture (script de tête de la page).
  const themeSel = $id("ui-theme") as HTMLSelectElement;
  themeSel.value = localStorage.getItem(THEME_KEY) ?? "system";
  themeSel.addEventListener("change", () => {
    localStorage.setItem(THEME_KEY, themeSel.value);
    applyTheme();
  });
  darkQuery.addEventListener("change", () => {
    if ((localStorage.getItem(THEME_KEY) ?? "system") === "system") applyTheme();
  });
  $id("btn-save-general").addEventListener("click", () => {
    void sendConfig();
  });
  $id("btn-devices").addEventListener("click", () => {
    void refreshDevices();
  });
  $id("btn-history-purge").addEventListener("click", async () => {
    try {
      await invoke("history_purge");
      ($id("history-result") as HTMLElement).textContent = t("history.purged");
      void refreshHistory();
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("btn-license-import").addEventListener("click", async () => {
    const blob = ($id("license-blob") as HTMLTextAreaElement).value;
    try {
      await invoke("license_import", { blob });
      ($id("license-result") as HTMLElement).textContent = t("license.imported");
      await refreshLicense();
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("btn-context-preview").addEventListener("click", async () => {
    try {
      const res = (await invoke("context_preview")) as [string, string] | null;
      const out = $id("context-result") as HTMLElement;
      out.textContent = res === null ? t("privacy.contextEmpty") : `${res[0]} — ${res[1]}`;
    } catch (err) {
      ($id("context-result") as HTMLElement).textContent = String(err);
    }
  });
  $id("btn-export").addEventListener("click", async () => {
    try {
      const path = (await invoke("diagnostics_export")) as string;
      ($id("export-result") as HTMLElement).textContent = path;
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("btn-data-export").addEventListener("click", async () => {
    try {
      const dest = (await invoke("data_export", { destination: null })) as string;
      ($id("data-result") as HTMLElement).textContent = dest;
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("dict-code-import").addEventListener("click", () => {
    const csv = ($id("dict-code") as HTMLTextAreaElement).value;
    void importDict("code", csv);
  });
  $id("dict-email-import").addEventListener("click", () => {
    const csv = ($id("dict-email") as HTMLTextAreaElement).value;
    void importDict("email", csv);
  });
  $id("dict-code-export").addEventListener("click", async () => {
    const csv = (await invoke("dictionaries_export", { profile: "code" })) as string;
    downloadTextFile("voxli-dictionnaire-code.csv", csv);
  });
  $id("dict-email-export").addEventListener("click", async () => {
    const csv = (await invoke("dictionaries_export", { profile: "email" })) as string;
    downloadTextFile("voxli-dictionnaire-email.csv", csv);
  });

  // Vocabulaire & Dictionnaire actif
  const vocabProfSel = $id("vocab-profile-select") as HTMLSelectElement;
  vocabProfSel.addEventListener("change", () => {
    currentVocabProfile = vocabProfSel.value as "default" | "code" | "email";
    renderVocabChips();
  });

  $id("vocab-btn-add").addEventListener("click", () => {
    void addVocabWord();
  });

  const wordInput = $id("vocab-new-word") as HTMLInputElement;
  const variantsInput = $id("vocab-new-variants") as HTMLInputElement;

  wordInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void addVocabWord();
    }
  });

  variantsInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      void addVocabWord();
    }
  });

  $id("vocab-btn-export").addEventListener("click", async () => {
    const csv = (await invoke("dictionaries_export", { profile: currentVocabProfile })) as string;
    downloadTextFile(`voxli-dictionnaire-${currentVocabProfile}.csv`, csv);
  });

  const fileInput = $id("vocab-file-input") as HTMLInputElement;
  $id("vocab-btn-import-file").addEventListener("click", () => {
    fileInput.click();
  });

  fileInput.addEventListener("change", () => {
    const file = fileInput.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = async () => {
      const text = String(reader.result ?? "");
      await importDict(currentVocabProfile, text);
      if (config) {
        config.profiles[currentVocabProfile].dictionary = text
          .split("\n")
          .map((l) => l.trim())
          .filter((l) => l.length > 0 && !l.startsWith("#"));
      }
      renderVocabChips();
      fileInput.value = "";
    };
    reader.readAsText(file);
  });

  // Langue de l'interface (v5) : application immédiate des textes ; la
  // valeur n'est persistée qu'à « Enregistrer » (= cohérence des autres
  // réglages, RF-01). Les libellés de langue ASR suivent la langue.
  const uiLangSel = $id("gen-ui-language") as HTMLSelectElement;
  uiLangSel.addEventListener("change", () => {
    applyUiLanguage(uiLangSel.value);
    rebuildAsrLanguageLabels();
    // Le textes construits APRÈS applyDom (boutons reconstruits, ligne
    // moteur effectif, badge, puces vocabulaire) suivent la langue :
    // sans ces repeints, des zones restaient dans la langue précédente
    // (incohérence constatée le 21/10).
    void refreshModels();
    void effectiveEngineLine();
    renderVocabChips();
  });

  // Calcul CPU/GPU : écriture immédiate (même sémantique que le moteur —
  // le bundle du noyau est reconstruit à chaud).
  $id("calc-select").addEventListener("change", () => {
    const choice = ($id("calc-select") as HTMLSelectElement).value;
    void setCompute(choice);
  });

  // Onboarding 1-clic : téléchargement immédiat du modèle Whisper base
  $id("btn-onboarding-download").addEventListener("click", async () => {
    void startDownload("ggml-base-q5_1.bin");
  });

  // Annulation d'un téléchargement en cours : l'arrêt est demandé au
  // noyau, il se produit au prochain segment réseau (≤ 256 KiB). Le
  // `models://done` (ok=false, reason "annulé") referme la bande.
  $id("btn-model-cancel").addEventListener("click", () => {
    void invoke("models_download_cancel");
  });

  // Don : boutons de l'encart + cœurs de la barre du haut.
  wireDonate();
}

/** Libellés de langue de dictée : au lieu d'une liste figée en français,
 * les noms sont rendus par Intl.DisplayNames dans la LANGUE DE L'INTERFACE
 * (de → « allemand » / « Deutsch » / « ドイツ語 »). Simplification réelle :
 * la liste des valeurs ASR n'est JAMAIS touchée (aucun réglage n'est
 * perdu) ; si la locale est inconnue, les libellés statiques restent. */
function rebuildAsrLanguageLabels(): void {
  const sel = document.getElementById("gen-language") as HTMLSelectElement | null;
  if (sel === null) return;
  try {
    const names = new Intl.DisplayNames([localeFor()], { type: "language" });
    for (const opt of Array.from(sel.options)) {
      if (opt.value === "") continue; // « auto » : déjà traduit via data-i18n
      const label = names.of(opt.value);
      if (label !== undefined && label !== opt.value) opt.textContent = label;
    }
  } catch {
    // Locale non supportée par le moteur : libellés statiques conservés.
  }
}

async function main(): Promise<void> {
  // Thème AVANT tout : ce premier appel (synchronisé, sans await) peint la
  // fenêtre dans la préférence locale ; le script de tête de index.html
  // couvre la toute première peinture, celui-ci garantit la cohérence
  // même si la CSP du build a bloqué le script inline.
  applyTheme();
  config = await invoke<ConfigJson>("config_get");
  // Langue d'affichage (v5) avant tout peinture de texte dynamique.
  applyUiLanguage(config.general.uiLanguage);
  applyDom();
  rebuildAsrLanguageLabels();
  const lic = await refreshLicense();
  paintConfig(config, lic);
  wireEvents();
  applyUiMode();

  // Encart don réservé à l'ouverture des réglages : si un palier a été
  // franchi hors fenêtre ouverte, il attend ici — jamais au fil d'une
  // dictée.
  if (localStorage.getItem(DONATE_KEY_PENDING) === "1") {
    localStorage.removeItem(DONATE_KEY_PENDING);
    showDonateBanner();
  }

  const state = await invoke<StateInfo>("state_get");
  ($id("badge") as HTMLElement).textContent =
    t(`state.${state.state}`) === `state.${state.state}` ? state.state : t(`state.${state.state}`);
  ($id("engine") as HTMLElement).textContent =
    t(`engine.${state.engine}`) === `engine.${state.engine}` ? state.engine : t(`engine.${state.engine}`);

  void refreshDevices();
  void refreshModels();
  void refreshEngines();
  void refreshHistory();

  // Événements (docs/05 §3 + additions documentées).
  let lastDonateState = "idle";
  void listen("state://changed", async (ev) => {
    const payload = ev.payload as { state: string };
    // Compteur don : un passage en « recording » (anti-doublon via le
    // dernier état vu) = une prise de parole de l'utilisateur. L'encart
    // éventuel se peint dans CETTE fenêtre : jamais dans la dictée.
    if (payload.state === "recording" && lastDonateState !== "recording") {
      if (donateTick()) showDonateBanner();
    }
    lastDonateState = payload.state;
    const badgeText = t(`state.${payload.state}`);
    ($id("badge") as HTMLElement).textContent =
      badgeText === `state.${payload.state}` ? payload.state : badgeText;
    // Le moteur peut changer sans passage par la configuration (modèle
    // installé, rechargement automatique) : relecture de l'état complet
    // plutôt que déduit du seul badge.
    try {
      const s = await invoke<StateInfo>("state_get");
      const engineText = t(`engine.${s.engine}`);
      ($id("engine") as HTMLElement).textContent =
        engineText === `engine.${s.engine}` ? s.engine : engineText;
    } catch {
      // badge inchangé si la lecture échoue (jamais de fausse info)
    }
    void effectiveEngineLine();
  });
  void listen<Notice>("notice://notice", (ev) => {
    showNotice(ev.payload.message);
  });
  void listen<{ level: number }>("meter://level", (ev) => {
    applyMeter(ev.payload.level);
  });
  void listen<{ id: string; downloaded: number; total: number }>(
    "models://progress",
    (ev) => {
      const bar = $id("model-progress");
      const fill = document.getElementById("model-progress-fill") as HTMLElement;
      if (fill === null) return;
      if (ev.payload.total > 0) {
        bar.classList.remove("indeterminate");
        fill.style.width = `${((ev.payload.downloaded / ev.payload.total) * 100).toFixed(1)}%`;
      } else {
        // Taille inconnue (serveur sans content-length) : bande animée
        // indéterminée au lieu d'un « 0 % » figé.
        bar.classList.add("indeterminate");
        fill.style.width = "40%";
      }
      const label = knownModels.find((m) => m.id === ev.payload.id)?.label ?? ev.payload.id;
      ($id("model-status") as HTMLElement).textContent =
        ev.payload.total > 0
          ? t("models.progress", {
              name: label,
              n: Math.round(ev.payload.downloaded / (1024 * 1024)),
              n2: Math.round(ev.payload.total / (1024 * 1024)),
              p: Math.round((ev.payload.downloaded / ev.payload.total) * 100),
            })
          : t("models.progressUnknown", {
              name: label,
              n: Math.round(ev.payload.downloaded / (1024 * 1024)),
            });
    },
  );
  void listen<{ id: string; ok?: boolean; reason?: string }>("models://done", async (ev) => {
    downloadingId = null;
    ($id("btn-model-cancel") as HTMLElement).hidden = true;
    ($id("model-progress") as HTMLElement).hidden = true;
    ($id("model-status") as HTMLElement).textContent = "";
    if (ev.payload.ok === false) {
      const reason = ev.payload.reason ?? "";
      if (reason === "annulé") {
        // Annulation demandée par l'utilisateur : un acquittement neutre,
        // pas une erreur — la reprise `Range` est possible à tout moment.
        showNotice(t("models.cancelled"));
      } else {
        const clean =
          reason !== "" ? reason : t("notice.networkError");
        showNotice(t("notice.modelInterrupted", { reason: clean }));
      }
    } else {
      showNotice(t("notice.modelOk"));
      // Connexion automatique : UNIQUEMENT pour les modèles whisper
      // (activables du build courant). Nemotron/Parakeet restent
      // téléchargés mais ne JAMAIS être écrits dans la config — un modèle
      // que le worker ne peut pas monter = repli factice silentieux
      // (incohérence constatée le 21/10). La liste reste modifiable à
      // tout moment dans le catalogue.
      const id = ev.payload.id;
      const info = knownModels.find((m) => m.id === id);
      if (config !== null && info !== undefined && info.kind === "whisper") {
        try {
          config.general.model = id;
          config.general.engine = "whisper";
          await invoke("config_set", { newConfig: config });
          void refreshEngines();
          void effectiveEngineLine();
        } catch (err) {
          showNotice(String(err));
        }
      } else if (config !== null && info !== undefined) {
        showNotice(t("models.noActivation"));
      }
    }
    void refreshModels();
    void refreshEngines();
  });
}

void main();
