/** Interface de l'application — miroir TS du schéma v5 (camelCase) :
 * `uiLanguage` (langue d'affichage, fr|en) est distinct de `language`
 * (langue d'écoute ASR, RF-17). Textes via i18n.ts. */
interface ConfigJson {
  schema: number;
  general: {
    language: string;
    uiLanguage: string;
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
import { applyDom, applyUiLanguage, t, uiLanguage } from "./i18n";

let config: ConfigJson | null = null;

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

function paintConfig(cfg: ConfigJson, lic: LicenseInfo): void {
  ($id("hk-push") as HTMLInputElement).value = cfg.hotkeys.pushToTalk;
  ($id("hk-paste") as HTMLInputElement).value = cfg.hotkeys.pasteLastTranscript;
  ($id("hk-show") as HTMLInputElement).value = cfg.hotkeys.showLastTranscript;
  ($id("hk-mode") as HTMLSelectElement).value = cfg.hotkeys.mode;
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
  ($id("gen-ui-language") as HTMLSelectElement).value =
    cfg.general.uiLanguage === "en" ? "en" : "fr";
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
  config.hotkeys.pushToTalk =
    ($id("hk-push") as HTMLInputElement).value.trim() || config.hotkeys.pushToTalk;
  config.hotkeys.pasteLastTranscript =
    ($id("hk-paste") as HTMLInputElement).value.trim() || config.hotkeys.pasteLastTranscript;
  config.hotkeys.showLastTranscript =
    ($id("hk-show") as HTMLInputElement).value.trim() || config.hotkeys.showLastTranscript;
  config.hotkeys.mode = ($id("hk-mode") as HTMLSelectElement).value;
  const preRoll = Number(($id("pre-roll") as HTMLInputElement).value);
  if (Number.isFinite(preRoll)) config.audio.preRollMs = preRoll;
  const maxRec = Number(($id("max-rec") as HTMLInputElement).value);
  if (Number.isFinite(maxRec)) config.audio.maxRecordingSeconds = maxRec;
  const retention = Number(($id("hist-retention") as HTMLInputElement).value);
  if (Number.isFinite(retention)) config.general.history.retentionDays = retention;
  config.general.language = ($id("gen-language") as HTMLSelectElement).value;
  config.general.uiLanguage = ($id("gen-ui-language") as HTMLSelectElement).value;
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
    const locale = uiLanguage() === "en" ? "en-GB" : "fr-FR";
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

async function refreshModels(): Promise<void> {
  const models: ModelInfo[] = await invoke("models_list");
  const body = $id("models-body") as HTMLElement;
  body.replaceChildren();
  for (const m of models) {
    const tr = document.createElement("tr");
    const footprint = m.sha256 ? `${m.sha256.slice(0, 16)}…` : "";
    const sizeTxt = m.sizeMb !== null ? ` ${t("models.sizeMb", { n: m.sizeMb })}` : "";
    tr.innerHTML =
      `<td>${escapeHtml(m.label)}<br /><span class="muted">` +
      `${escapeHtml(m.id)}${escapeHtml(sizeTxt)}` +
      `<br /><span class="license">${escapeHtml(m.license)}</span></span></td>` +
      `<td>${m.installed ? `<b>${escapeHtml(t("models.installed"))}</b>` : escapeHtml(t("models.notInstalled"))}</td>` +
      `<td class="mono">${escapeHtml(footprint)}${m.verified ? ` <b title="${escapeHtml(t("models.verifiedBadge"))}">✓</b>` : ""}</td>`;
    const td = document.createElement("td");
    const btn = document.createElement("button");
    btn.type = "button";
    if (m.installed) {
      btn.textContent = t("models.remove");
      btn.addEventListener("click", async () => {
        try {
          await invoke("models_remove", { id: m.id });
          void refreshModels();
        } catch (err) {
          showNotice(String(err));
        }
      });
    } else {
      btn.textContent = t("models.download");
      btn.addEventListener("click", async () => {
        try {
          await invoke("models_download", { id: m.id });
          const bar = $id("model-progress");
          // Reprise visible à zéro : la première mise à jour serveur
          // réajuste (le % résiduel de l'essai précédent n'a pas de sens).
          (document.getElementById("model-progress-fill") as HTMLElement).style.width = "0%";
          bar.hidden = false;
          ($id("model-status") as HTMLElement).textContent = t("models.downloading");
        } catch (err) {
          showNotice(String(err));
        }
      });
    }
    td.appendChild(btn);
    tr.appendChild(td);
    body.appendChild(tr);
  }
  // Règle Simple/Avancé : dès qu'UNE voix dicte, le catalogue technique
  // n'a plus besoin d'être vu au quotidien (il reste en Mode avancé) ;
  // sans moteur réel, l'écran Simple garde l'installation sous la main.
  document
    .getElementById("sec-modeles")
    ?.classList.toggle("advanced", models.some((m) => m.installed));

  // Bannière d'onboarding 1-clic : visible tant qu'aucun modèle n'est installé
  const onboarding = document.getElementById("onboarding-card");
  if (onboarding) {
    onboarding.hidden = models.some((m) => m.installed);
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
  // réglages, RF-01).
  const uiLangSel = $id("gen-ui-language") as HTMLSelectElement;
  uiLangSel.addEventListener("change", () => {
    applyUiLanguage(uiLangSel.value);
  });

  // Onboarding 1-clic : téléchargement immédiat du modèle Whisper base
  $id("btn-onboarding-download").addEventListener("click", async () => {
    try {
      await invoke("models_download", { id: "ggml-base-q5_1.bin" });
      const bar = $id("model-progress");
      (document.getElementById("model-progress-fill") as HTMLElement).style.width = "0%";
      bar.hidden = false;
      ($id("model-status") as HTMLElement).textContent = t("models.downloading");
      showNotice(t("notice.modelStarted"));
    } catch (err) {
      showNotice(String(err));
    }
  });
}

async function main(): Promise<void> {
  config = await invoke<ConfigJson>("config_get");
  // Langue d'affichage (v5) avant tout peinture de texte dynamique.
  applyUiLanguage(config.general.uiLanguage);
  applyDom();
  const lic = await refreshLicense();
  paintConfig(config, lic);
  wireEvents();
  applyUiMode();

  const state = await invoke<StateInfo>("state_get");
  ($id("badge") as HTMLElement).textContent =
    t(`state.${state.state}`) === `state.${state.state}` ? state.state : t(`state.${state.state}`);
  ($id("engine") as HTMLElement).textContent =
    t(`engine.${state.engine}`) === `engine.${state.engine}` ? state.engine : t(`engine.${state.engine}`);

  void refreshDevices();
  void refreshModels();
  void refreshHistory();

  // Événements (docs/05 §3 + additions documentées).
  void listen("state://changed", async (ev) => {
    const payload = ev.payload as { state: string };
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
      const fill = document.getElementById("model-progress-fill");
      if (fill === null) return;
      const pct =
        ev.payload.total > 0 ? (ev.payload.downloaded / ev.payload.total) * 100 : 0;
      fill.style.width = `${pct.toFixed(1)}%`;
      ($id("model-status") as HTMLElement).textContent = t("models.mbDownloaded", {
        n: Math.round(ev.payload.downloaded / (1024 * 1024)),
      });
    },
  );
  void listen<{ id: string; ok?: boolean; reason?: string }>("models://done", (ev) => {
    ($id("model-progress") as HTMLElement).hidden = true;
    ($id("model-status") as HTMLElement).textContent = "";
    if (ev.payload.ok === false) {
      const reason =
        "reason" in ev.payload && ev.payload.reason !== "" && ev.payload.reason !== undefined
          ? ev.payload.reason
          : t("notice.networkError");
      showNotice(t("notice.modelInterrupted", { reason }));
    } else {
      showNotice(t("notice.modelOk"));
    }
    void refreshModels();
  });
}

void main();
