/** Application interface — TS mirror of the v5 schema (camelCase):
 * `uiLanguage` (display language: "system" or an i18n/ code) is
 * distinct from `language` (ASR listening language, RF-17). Texts via i18n/. */
interface ConfigJson {
  schema: number;
  general: {
    language: string;
    uiLanguage: string;
    /** v5 addition: "auto" | "cpu" | "gpu" (unknown = auto on the core side). */
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

/** Engine state returned by `engines_list`: compiled in this build
 * and actually usable (model present on disk). Source of truth for the
 * selectability of the nemo/parakeet rows of the catalogue — an SDK
 * compiled yesterday cannot be hard-coded today. */
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

// ================================ Donations (Ko-fi / PayPal.me) =============
// Discreet by design (RF-29: visible, never intrusive):
// - PERMANENT reminder in the header bar (donate-chip links);
// - periodic banner: at usage milestones (100/400/1000/3000 dictations)
//   it shows in the SETTINGS — never during a dictation (the pill
//   is another window), closable with one click, and "don't offer it
//   to me again" is final (local state, outside configuration).
// The links open via the opener plugin, whose permissions are
// limited to the two URLs (capabilities/default.json).
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

/** One more recording. If a milestone is crossed for the first time,
 * it arms the banner (kept for the next opening of the settings) and
 * returns true for immediate mounting when the window is already open. */
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
    // Never silent (RF-29): the URL stays readable in the notice,
    // copyable even if the system-level opening fails.
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

/** Catalogue relayed by `models_list`: labels displayed during
 * progress (the `models://progress` event only carries the id). */
const knownModels: ModelInfo[] = [];

/** Download in progress (RF-36: one at a time). Drives the "busy"
 * state of the buttons and the visibility of the Cancel button. */
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

/** Starts a model download (onboarding or catalogue): visible
 * progress bar, file name displayed, Cancel button wired to
 * `models_download_cancel` (RF-36: explicit action). One
 * download at a time — other requests are ignored. */
async function startDownload(id: string): Promise<void> {
  if (downloadingId !== null) {
    showNotice(t("models.busy"));
    return;
  }
  downloadingId = id;
  const bar = $id("model-progress");
  ($id("btn-model-cancel") as HTMLElement).hidden = false;
  // Resume visible at zero: the first server update re-adjusts it
  // (the residual % of the previous attempt makes no sense).
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
    // Immediate refusal (already running, unknown model…): reset the
    // screen to a clean state — never a ghost bar.
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
  // Listening language: keep a free option if the file contains
  // something else; the display language (v5) is normalized to the i18n/ codes.
  const langInput = ($id("gen-language") as HTMLSelectElement);
  if (![...langInput.options].some((o) => o.value === cfg.general.language)) {
    const opt = document.createElement("option");
    opt.value = cfg.general.language;
    opt.textContent = cfg.general.language;
    langInput.appendChild(opt);
  }
  langInput.value = cfg.general.language;
  // The raw config value ("system" or a code) is assigned
  // as is; if it is no longer in the list, the selector keeps
  // its first option ("system", self-repairing at the next save).
  const uiLangSel0 = $id("gen-ui-language") as HTMLSelectElement;
  if ([...uiLangSel0.options].some((o) => o.value === cfg.general.uiLanguage)) {
    uiLangSel0.value = cfg.general.uiLanguage;
  }
  // Compute choice (v5 addition): if the stored value is no longer in the
  // list (hand-edited config), the selector stays on "auto" — safe.
  const calcSel0 = $id("calc-select") as HTMLSelectElement;
  calcSel0.value = [...calcSel0.options].some((o) => o.value === cfg.general.compute)
    ? cfg.general.compute
    : "auto";
  // RF-39: multiple profiles = Pro feature — clear warning.
  const lock = $id("profiles-lock");
  const pro =
    (lic.state === "active" || lic.state === "grace") &&
    lic.features.includes("extra-profiles");
  lock.textContent = pro ? "" : t("profiles.lockNotice");
  lock.hidden = pro;

  renderVocabChips();
}

/** Pushed to the core (RF-01: live, settings + pipeline). */
async function sendConfig(): Promise<void> {
  if (config === null) return;
  // Hotkeys: a conflicting combination ("busy" visible) is
  // NOT saved — the screen stays truthful; the core applies the
  // same rule (validate_before_register) and the corrected field becomes
  // savable again (RF-01: hotkeys commit on save).
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
  // Compute choice (v5 addition): mirrored like all the others, invalidated
  // to `auto` on the core side if an unexpected value ever arrives.
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

/** Hotkeys: on-the-fly capture (Wispr-simple — press, it types).
 * Tokens = strict core whitelist (`voxli-hotkeys`, `Key::parse`):
 * letters, digits, F1..F24, Space, Tab, Enter, `. Nothing else is
 * accepted, never a silent registration failure on the core side.
 * Note: an ACTUAL hotkey's combination stays grabbed by the
 * system while it is registered (intended core behavior) — capture
 * exists to pick a FREE combination. */
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

/** Conflict: the same combination assigned to two actions — marked
 * "busy" and NOT saved (mirror rule of
 * `validate_before_register`, the truth stays on the core side). */
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
      // Modifiers alone: the capture continues, the display follows.
      // A real key outside the whitelist (↓, Home, quotes, commas…) writes
      // nothing and explains why — never a silent failure.
      if (parts.length === 0 && !MODIFIER_KEYS.has(ev.key)) {
        showNotice(t("hk.unsupported"));
      }
      input.value = parts.length > 0 ? `${parts.join("+")}+` : "";
      return;
    }
    if (parts.length === 0) {
      // Bare key without a modifier: polite refusal, never silent.
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

/** Simple / Advanced: window-local preference (localStorage) —
 * a presentation choice, not a product configuration. */
const MODE_KEY = "voxli.ui.avance";

/** Theme: light / dark / system. Local preference, applied
 * from the top-of-page head script (no flash). */
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

/** History: read + bounded paint (RF-33). */
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
    // No store → empty table.
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

/** Activating an installed whisper model: `general.model` = id and
 * `general.engine` = whisper — live write (RF-01), the worker
 * rebuilds its bundle; the table is the single selection surface of
 * the SAME config (never two truths). */
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
  void refreshModels();
  void effectiveEngineLine();
}

async function refreshModels(): Promise<void> {
  const models: ModelInfo[] = await invoke("models_list");
  // Engines: nemo/parakeet selectability depends on the BUILD (SDK compiled?)
  // and on DISK (model present?) — never hard-coded; a read failure
  // = non-selectable rows with the SDK notice (never a false promise).
  let engines: EngineInfo[] | null = null;
  try {
    engines = await invoke<EngineInfo[]>("engines_list");
  } catch {
    engines = null;
  }
  knownModels.splice(0, knownModels.length, ...models);

  // Migration: a non-whisper model id in the config (poisoned
  // back then by the old un-guarded auto-connect — fake engine at
  // startup) reverts to "auto", written ONCE; later passes
  // find an already-sane config and rewrite nothing.
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

  // Handy-style selection: the ROW is the selector. The first row
  // is not a model — it is the "auto" choice (the core takes the
  // best model present; cf. `resolve_whisper_model`, a single
  // worker/engines_list truth).
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
    void effectiveEngineLine();
  });
  body.appendChild(autoRow);

  for (const m of models) {
    const tr = document.createElement("tr");
    const footprint = m.sha256 ? `${m.sha256.slice(0, 16)}…` : "";
    const sizeTxt = m.sizeMb !== null ? ` ${t("models.sizeMb", { n: m.sizeMb })}` : "";
    // The selection reflects the SAME config whatever the kind:
    // whisper = explicit engine+model; nemo/parakeet = its engine
    // (one model per kind — the row is the selector).
    const selected =
      config !== null &&
      (m.kind === "whisper"
        ? config.general.engine === "whisper" && config.general.model === m.id
        : config.general.engine === m.kind);
    // The row IS the selector (Handy-style): ✓ in the State column
    // = the active choice. Off-list (nemo/parakeet): not selectable.
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
        ev.stopPropagation(); // the row selects, the button removes
        try {
          await invoke("models_remove", { id: m.id });
          // The active model just left: revert to "auto" written
          // immediately (the core resolves the best whisper present) —
          // never a silent engine on a config pointing at nothing.
          if (config !== null && config.general.model === m.id) {
            config.general.model = "auto";
            config.general.engine = "auto";
            await invoke("config_set", { newConfig: config });
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
        ev.stopPropagation(); // the row downloads, the button too —
        // no double trigger on the row click.
        void startDownload(m.id);
      });
      td.appendChild(btn);
    }
    tr.appendChild(td);

    // The row IS the selector (Handy-style): row highlight + click.
    // installed whisper = activation; missing whisper = download;
    // nemo/parakeet = not selectable (SDK required in another build)
    // — the 21/10 inconsistency: downloading a non-activatable engine
    // pushed the config toward a model the worker cannot mount
    // (silent fake fallback… fixed by the guard below).
    if (m.kind !== "whisper") {
      // Nemo/parakeet selectability follows the real BUILD (engines_list):
      // compiled SDK + model present = ACTIVATABLE on click; compiled SDK
      // without model = the click starts the download; SDK not compiled =
      // visible notice (never an unkeepable promise).
      const eng = engines?.find((e) => e.id === m.kind);
      if (eng?.compiled) {
        // `selected` is already computed at the top of the loop (kind-aware).
        tr.classList.add("selectable");
        tr.tabIndex = 0;
        if (selected) tr.classList.add("selected");
        if (eng.usable) {
          tr.addEventListener("click", async () => {
            if (config === null) return;
            const cfg = config;
            if (cfg.general.engine === m.kind) return void refreshModels();
            cfg.general.engine = m.kind;
            cfg.general.model = m.id;
            try {
              await invoke("config_set", { newConfig: cfg });
            } catch (err) {
              showNotice(String(err));
            }
            void refreshModels();
            void effectiveEngineLine();
          });
        } else {
          tr.addEventListener("click", () => {
            void startDownload(m.id);
          });
        }
      } else {
        tr.title = t("models.noActivation");
        // Hint VISIBLE under the row, not only on hover: never a
        // mute "why can't I select this voice" zone.
        const hint = document.createElement("span");
        hint.className = "license";
        hint.textContent = t("engines.hintSdk");
        const firstTd = tr.firstElementChild;
        if (firstTd !== null) {
          firstTd.appendChild(document.createElement("br"));
          firstTd.appendChild(hint);
        }
      }
    } else {
      // classes + click; `selected` is already computed at the top of the loop.
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
  // Simple/Advanced rule: the detailed catalogue (models table) lives
  // in the `models-advanced` block (index.html) — it is no longer hidden
  // from here. What stays VISIBLE in Simple mode is the engine
  // selection (the engines-box, with its real state): it answers "which
  // voice is installed, which one is available".

  // One-click onboarding banner: visible as long as NO whisper
  // model usable for dictation is installed — a downloaded nemo/parakeet does not make
  // the app dictable (SDK required) and must therefore NOT hide it
  // (otherwise: fake fallback WITHOUT a visible remedy indication — seen
  // on 21/10 when only Nemotron was installed).
  const onboarding = document.getElementById("onboarding-card");
  if (onboarding) {
    onboarding.hidden = models.some((m) => m.kind === "whisper" && m.installed);
  }
}

/** REMOVED view (engines radios). On 21/10: the radios/table duplicate
 * created "non-selectable zones" (greyed nemo/parakeet) that lost
 * users. The models table is now the only selection surface
 * (Automatic row on top + whisper rows),
 * cf. `refreshModels`; `engine-effective` shows the real one. The config
 * write remains `config_set` (RF-01). */

/** Compute choice (CPU/GPU, v5 addition): the same semantics as the
 * engine — written via sendConfig (full mirror), the core rebuilds
 * its bundle hot; the log records "compute:…" (the requested one) on
 * every engine mount. */
async function setCompute(choice: string): Promise<void> {
  if (config === null || config.general.compute === choice) return;
  await sendConfig();
}

/** Effectively mounted engine (source: `state_get`, i.e. the worker) —
 * the real one, not the choice; a separate line when it differs from the selection.
 * Also repaints the state badge and the engine word: called on UI
 * language change so the dynamic follows the static. */
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
    // Badge + engine word: the same source as `state://changed`, without
    // waiting for the next event (UI language change in particular).
    const badgeText = t(`state.${s.state}`);
    ($id("badge") as HTMLElement).textContent =
      badgeText === `state.${s.state}` ? s.state : badgeText;
    ($id("engine") as HTMLElement).textContent = name;
    // Transition → fake: warning ONCE per transition
    // (the core fallback is already recorded in the log; here the UI says
    // why dictation seems "fake" and how to heal it).
    if (s.engine === "fake" && lastEffectiveEngine !== null && lastEffectiveEngine !== "fake") {
      showNotice(t("engines.fellFake"));
    }
    lastEffectiveEngine = s.engine;
  } catch {
    // Unreadable: the line stays hidden (never false info).
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
  // On-the-fly hotkey capture (cf. wireHotkeyCapture) + immediate
  // "busy" marking in case of a duplicate.
  for (const id of HK_IDS) {
    wireHotkeyCapture($id(id) as HTMLInputElement);
  }

  // Simple / Advanced (presentation preference, outside configuration):
  // "mode-avance" ties/unties everything marked with the "advanced" class.
  const modeBox = $id("mode-avance") as HTMLInputElement;
  modeBox.addEventListener("change", () => {
    localStorage.setItem(MODE_KEY, modeBox.checked ? "1" : "0");
    document.body.classList.toggle("mode-avance", modeBox.checked);
  });
  // Theme: the saved preference drives data-theme; the attribute is
  // already set before the first paint (top-of-page head script).
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

  // Vocabulary & Active dictionary
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

  // UI language (v5): immediate application of texts; the
  // value is only persisted at "Save" (= consistency with the other
  // settings, RF-01). The ASR language labels follow the language.
  const uiLangSel = $id("gen-ui-language") as HTMLSelectElement;
  uiLangSel.addEventListener("change", () => {
    applyUiLanguage(uiLangSel.value);
    rebuildAsrLanguageLabels();
    // Texts built AFTER applyDom (rebuilt buttons, effective-engine
    // line, badge, vocabulary chips) follow the language: without
    // these repaints, zones stayed in the previous language
    // (inconsistency seen on 21/10).
    void refreshModels();
    void effectiveEngineLine();
    renderVocabChips();
  });

  // CPU/GPU compute: immediate write (the same semantics as the engine —
  // the core bundle is rebuilt hot).
  $id("calc-select").addEventListener("change", () => {
    const choice = ($id("calc-select") as HTMLSelectElement).value;
    void setCompute(choice);
  });

  // One-click onboarding: immediate download of the Whisper base model
  $id("btn-onboarding-download").addEventListener("click", async () => {
    void startDownload("ggml-base-q5_1.bin");
  });

  // Cancelling an in-progress download: the stop is requested from the
  // core, it happens at the next network segment (≤ 256 KiB). The
  // `models://done` (ok=false, reason "annulé") closes the bar.
  $id("btn-model-cancel").addEventListener("click", () => {
    void invoke("models_download_cancel");
  });

  // Donations: the banner buttons + the heart links of the top bar.
  wireDonate();
}

/** Dictation language labels: instead of a list frozen in French,
 * names are rendered by Intl.DisplayNames in the UI LANGUAGE
 * (de → "german" / "Deutsch" / "ドイツ語"). Real simplification:
 * the ASR value list is NEVER touched (no setting is
 * lost); if the locale is unknown, the static labels stay. */
function rebuildAsrLanguageLabels(): void {
  const sel = document.getElementById("gen-language") as HTMLSelectElement | null;
  if (sel === null) return;
  try {
    const names = new Intl.DisplayNames([localeFor()], { type: "language" });
    for (const opt of Array.from(sel.options)) {
      if (opt.value === "") continue; // "": already translated via data-i18n
      const label = names.of(opt.value);
      if (label !== undefined && label !== opt.value) opt.textContent = label;
    }
  } catch {
    // Locale unsupported by the engine: static labels kept.
  }
}

async function main(): Promise<void> {
  // Theme FIRST: this first call (synchronous, no await) paints the
  // window with the local preference; the head script of index.html
  // covers the very first paint, this one guarantees consistency
  // even if the build's CSP blocked the inline script.
  applyTheme();
  config = await invoke<ConfigJson>("config_get");
  // Display language (v5) before any dynamic text paint.
  applyUiLanguage(config.general.uiLanguage);
  applyDom();
  rebuildAsrLanguageLabels();
  const lic = await refreshLicense();
  paintConfig(config, lic);
  wireEvents();
  applyUiMode();

  // Donation banner reserved to openings of the settings: if a milestone
  // was crossed while the window was closed, it waits here — never in
  // the middle of a dictation.
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
  void refreshHistory();

  // Events (docs/05 §3 + documented additions).
  let lastDonateState = "idle";
  void listen("state://changed", async (ev) => {
    const payload = ev.payload as { state: string };
    // Donation counter: one pass into "recording" (de-duplicated via
    // the last seen state) = one speech taking by the user. The banner
    // if any paints in THIS window: never during a dictation.
    if (payload.state === "recording" && lastDonateState !== "recording") {
      if (donateTick()) showDonateBanner();
    }
    lastDonateState = payload.state;
    const badgeText = t(`state.${payload.state}`);
    ($id("badge") as HTMLElement).textContent =
      badgeText === `state.${payload.state}` ? payload.state : badgeText;
    // The engine can change without going through the configuration
    // (model installed, automatic reload): read the full state back
    // rather than deduce it from the badge alone.
    try {
      const s = await invoke<StateInfo>("state_get");
      const engineText = t(`engine.${s.engine}`);
      ($id("engine") as HTMLElement).textContent =
        engineText === `engine.${s.engine}` ? s.engine : engineText;
    } catch {
      // badge unchanged if the read fails (never false info)
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
        // Unknown size (server without content-length): animated
        // indeterminate bar instead of a frozen "0%".
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
        // Cancel requested by the user: a neutral acknowledgement,
        // not an error — the `Range` resume stays possible at any time.
        showNotice(t("models.cancelled"));
      } else {
        const clean =
          reason !== "" ? reason : t("notice.networkError");
        showNotice(t("notice.modelInterrupted", { reason: clean }));
      }
    } else {
      showNotice(t("notice.modelOk"));
      // Auto-connect: ONLY for whisper models
      // (activatable in the current build). Nemotron/Parakeet stay
      // downloaded but must NEVER be written into the config — a model
      // the worker cannot mount = a silent fake fallback
      // (inconsistency seen on 21/10). The list stays editable at
      // any time in the catalogue.
      const id = ev.payload.id;
      const info = knownModels.find((m) => m.id === id);
      if (config !== null && info !== undefined && info.kind === "whisper") {
        try {
          config.general.model = id;
          config.general.engine = "whisper";
          await invoke("config_set", { newConfig: config });
        } catch (err) {
          showNotice(String(err));
        }
      } else if (config !== null && info !== undefined) {
        showNotice(t("models.noActivation"));
      }
    }
    // Table repaint: the downloaded row turns "installed" and
    // the selection ✓ follows the auto-connect (never stale state).
    void refreshModels();
    void effectiveEngineLine();
  });
}

void main();
