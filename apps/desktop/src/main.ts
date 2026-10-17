/** Interface de l'application — miroir TS du schéma v4 (camelCase). */
interface ConfigJson {
  schema: number;
  general: {
    language: string;
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

const fr_state: Record<string, string> = {
  idle: "prête",
  recording: "enregistrement…",
  finalizing: "finalisation de l'enregistrement…",
  transcribing: "transcription…",
  "post-processing": "post-traitement…",
  inserting: "insertion dans l'application…",
  recovery: "incident — réessayer possible",
};

const fr_engine: Record<string, string> = {
  fake: "moteur factice (démo)",
  "whisper.cpp": "whisper.cpp — 100 % local",
  parakeet: "Parakeet — 100 % local",
};

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
  // Langue : garder une option libre si le fichier contient autre chose.
  const langInput = ($id("gen-language") as HTMLSelectElement);
  if (![...langInput.options].some((o) => o.value === cfg.general.language)) {
    const opt = document.createElement("option");
    opt.value = cfg.general.language;
    opt.textContent = cfg.general.language;
    langInput.appendChild(opt);
  }
  langInput.value = cfg.general.language;
  // RF-39 : profils multiples = fonctionnalité Pro — avertissement clair.
  const lock = $id("profiles-lock");
  const pro =
    (lic.state === "active" || lic.state === "grace") &&
    lic.features.includes("extra-profiles");
  lock.textContent = pro
    ? ""
    : "Profils multiples : fonctionnalité Pro. Sans licence, seul le profil " +
    "par défaut s'applique — vos liaisons restent enregistrées ici.";
  lock.hidden = pro;
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
    ($id("save-result") as HTMLElement).textContent = "enregistré — appliqué à chaud";
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
    const body = $id("history-body") as HTMLElement;
    body.replaceChildren();
    for (const it of items) {
      const tr = document.createElement("tr");
      const date = new Date(it.createdMs).toLocaleString("fr-FR");
      tr.innerHTML =
        `<td>${escapeHtml(date)}</td>` +
        `<td>${escapeHtml(it.engine)}</td>` +
        `<td class="wrap">${escapeHtml(it.text)}</td>`;
      const btn = document.createElement("button");
      btn.type = "button";
      btn.textContent = "supprimer";
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
      tr.innerHTML = '<td colspan="4" class="muted">aucune entrée</td>';
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
    line.textContent = `Licence active — titulaire : ${lic.licensee}.`;
  } else if (lic.state === "grace") {
    const remaining = lic.expiresAtSecs ?? 0;
    const days = Math.max(0, Math.ceil((remaining - Date.now() / 1000) / 86_400));
    line.textContent =
      `Licence expirée — période de grâce hors ligne : encore ${days} jour(s) ` +
      "de fonctionnalités Pro, puis retour à l'édition gratuite.";
  } else {
    line.textContent =
      "Édition gratuite — tout le local fonctionne ; les fonctions Pro " +
      "(historique chiffré, profils multiples) restent masquées.";
  }
  const list = $id("license-features") as HTMLElement;
  list.replaceChildren();
  if (lic.features.length === 0) {
    const li = document.createElement("li");
    li.textContent = "aucune fonctionnalité Pro";
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
    tr.innerHTML =
      `<td>${escapeHtml(m.label)}<br /><span class="muted">` +
      `${escapeHtml(m.id)}${m.sizeMb !== null ? ` — ${m.sizeMb} Mo env.` : ""}` +
      `<br /><span class="license">${escapeHtml(m.license)}</span></span></td>` +
      `<td>${m.installed ? "<b>installé</b>" : "non installé"}</td>` +
      `<td class="mono">${escapeHtml(footprint)}</td>`;
    const td = document.createElement("td");
    const btn = document.createElement("button");
    btn.type = "button";
    if (m.installed) {
      btn.textContent = "retirer";
      btn.addEventListener("click", async () => {
        try {
          await invoke("models_remove", { id: m.id });
          void refreshModels();
        } catch (err) {
          showNotice(String(err));
        }
      });
    } else {
      btn.textContent = "télécharger";
      btn.addEventListener("click", async () => {
        try {
          await invoke("models_download", { id: m.id });
          const bar = $id("model-progress");
          bar.hidden = false;
          ($id("model-status") as HTMLElement).textContent = "téléchargement…";
        } catch (err) {
          showNotice(String(err));
        }
      });
    }
    td.appendChild(btn);
    tr.appendChild(td);
    body.appendChild(tr);
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
  def.textContent = "Micro par défaut du système";
  select.appendChild(def);
  for (const d of devices) {
    const opt = document.createElement("option");
    opt.value = d.id;
    opt.textContent = d.isDefault ? `${d.name} (défaut)` : d.name;
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
    showNotice(`Dictionnaire importé : ${n} entrée(s).`);
  } catch (err) {
    showNotice(String(err));
  }
}

function wireEvents(): void {
  $id("btn-save-general").addEventListener("click", () => {
    void sendConfig();
  });
  $id("btn-devices").addEventListener("click", () => {
    void refreshDevices();
  });
  $id("btn-history-purge").addEventListener("click", async () => {
    try {
      await invoke("history_purge");
      ($id("history-result") as HTMLElement).textContent = "historique purgé";
      void refreshHistory();
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("btn-license-import").addEventListener("click", async () => {
    const blob = ($id("license-blob") as HTMLTextAreaElement).value;
    try {
      await invoke("license_import", { blob });
      ($id("license-result") as HTMLElement).textContent = "licence importée et vérifiée.";
      await refreshLicense();
    } catch (err) {
      showNotice(String(err));
    }
  });
  $id("btn-context-preview").addEventListener("click", async () => {
    try {
      const res = (await invoke("context_preview")) as [string, string] | null;
      const out = $id("context-result") as HTMLElement;
      out.textContent = res === null ? "aucun contexte lu" : `${res[0]} — ${res[1]}`;
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
    downloadTextFile("plume-dictionnaire-code.csv", csv);
  });
  $id("dict-email-export").addEventListener("click", async () => {
    const csv = (await invoke("dictionaries_export", { profile: "email" })) as string;
    downloadTextFile("plume-dictionnaire-email.csv", csv);
  });
}

async function main(): Promise<void> {
  config = await invoke<ConfigJson>("config_get");
  const lic = await refreshLicense();
  paintConfig(config, lic);
  wireEvents();

  const state = await invoke<StateInfo>("state_get");
  ($id("badge") as HTMLElement).textContent = fr_state[state.state] ?? state.state;
  ($id("engine") as HTMLElement).textContent = fr_engine[state.engine] ?? state.engine;

  void refreshDevices();
  void refreshModels();
  void refreshHistory();

  // Événements (docs/05 §3 + additions documentées).
  void listen("state://changed", (ev) => {
    const payload = ev.payload as { state: string };
    ($id("badge") as HTMLElement).textContent =
      fr_state[payload.state] ?? payload.state;
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
      ($id("model-status") as HTMLElement).textContent =
        `${Math.round(ev.payload.downloaded / (1024 * 1024))} Mo téléchargés`;
    },
  );
  void listen<{ id: string; ok?: boolean; reason?: string }>("models://done", (ev) => {
    ($id("model-progress") as HTMLElement).hidden = true;
    ($id("model-status") as HTMLElement).textContent = "";
    if (ev.payload.ok === false) {
      showNotice(
        `Téléchargement interrompu (${"reason" in ev.payload && ev.payload.reason !== "" && ev.payload.reason !== undefined ? ev.payload.reason : "erreur réseau"}). Vous pouvez recommencer : il reprendra là où il s'était arrêté.`,
      );
    }
    void refreshModels();
  });
}

void main();
