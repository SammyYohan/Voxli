/** Interface de l'application — miroir TS de la configuration (schema v3). */
interface ConfigJson {
  schema: number;
  general: {
    language: string;
    engine: string;
    model: string;
    postProcessing: string;
  };
  hotkeys: {
    pushToTalk: string;
    pasteLastTranscript: string;
    showLastTranscript: string;
  };
  audio: { preRollMs: number; maxRecordingSeconds: number };
  privacy: { strictLocalMode: boolean; contextRead: string; telemetry: string };
}

interface StateInfo {
  state: string;
  engine: string;
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

const fr_context: Record<string, string> = {
  none: "aucun (défaut)",
  app_and_title: "application + titre",
  app_title_and_selection: "application + titre + sélection",
};

const fr_engine: Record<string, string> = {
  fake: "moteur factice (démo)",
  "whisper.cpp": "whisper.cpp — 100 % local",
  remote: "moteur distant (non utilisé)",
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

function paintConfig(cfg: ConfigJson): void {
  ($id("hk-push") as HTMLElement).textContent = cfg.hotkeys.pushToTalk;
  ($id("hk-paste") as HTMLElement).textContent = cfg.hotkeys.pasteLastTranscript;
  ($id("hk-show") as HTMLElement).textContent = cfg.hotkeys.showLastTranscript;
  ($id("pre-roll") as HTMLInputElement).value = String(cfg.audio.preRollMs);
  ($id("max-rec") as HTMLInputElement).value = String(cfg.audio.maxRecordingSeconds);
  ($id("pv-strict") as HTMLElement).textContent =
    cfg.privacy.strictLocalMode ? "actif — rien ne sort de cette machine" : "inactif";
  ($id("pv-context") as HTMLElement).textContent =
    fr_context[cfg.privacy.contextRead] ?? cfg.privacy.contextRead;
  ($id("pv-telemetry") as HTMLElement).textContent =
    cfg.privacy.telemetry === "off" ? "désactivée (défaut)" : cfg.privacy.telemetry;
}

async function refreshState(): Promise<void> {
  const info = await invoke<StateInfo>("state_get");
  ($id("badge") as HTMLElement).textContent = fr_state[info.state] ?? info.state;
  const engine = fr_engine[info.engine] ?? info.engine;
  ($id("engine") as HTMLElement).textContent =
    info.engine === "" ? "démarrage…" : engine;
}

async function main(): Promise<void> {
  // Configuration initiale puis rendu.
  config = await invoke<ConfigJson>("config_get");
  paintConfig(config);

  $id("form-audio").addEventListener("submit", async (ev) => {
    ev.preventDefault();
    if (config === null) return;
    const clone: ConfigJson = structuredClone(config);
    const pre = Number(($id("pre-roll") as HTMLInputElement).value);
    const max = Number(($id("max-rec") as HTMLInputElement).value);
    if (Number.isFinite(pre) && pre >= 100 && pre <= 1000) clone.audio.preRollMs = pre;
    if (Number.isFinite(max) && max >= 30 && max <= 1800) clone.audio.maxRecordingSeconds = max;
    try {
      await invoke("config_set", { newConfig: clone });
      config = await invoke<ConfigJson>("config_get");
      paintConfig(config);
      ($id("audio-result") as HTMLElement).textContent = "✔ enregistré";
    } catch (err) {
      ($id("audio-result") as HTMLElement).textContent = String(err);
    }
  });

  $id("btn-export").addEventListener("click", async () => {
    try {
      const path = await invoke<string>("diagnostics_export", { destination: null });
      ($id("export-result") as HTMLElement).textContent = `rapport écrit : ${path}`;
    } catch (err) {
      ($id("export-result") as HTMLElement).textContent = String(err);
    }
  });

  // Observateurs noyau → UI (aucun contenu dans ces événements).
  await listen<Notice>("notice://notice", (ev) => {
    showNotice(ev.payload.message);
  });
  await listen<{ state: string }>("state://changed", () => {
    void refreshState();
  });

  const tick = window.setInterval(() => void refreshState().catch(() => undefined), 2000);
  window.addEventListener("unload", () => window.clearInterval(tick));
  void refreshState();
}

void main();
