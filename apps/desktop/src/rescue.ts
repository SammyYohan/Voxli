/**
 * Rescue window (RF-31): shows the last transcribed text kept
 * in the internal buffer of the orchestrator, and allows putting it back into
 * the target ("Paste again") or re-running transcription ("Retry").
 * No network send: everything stays in the local WebView.
 * Presentation language: `general.uiLanguage` (v5, fr fallback).
 */
import { invoke } from "@tauri-apps/api/core";
import { applyUiLanguage, t } from "./i18n";

const text = document.getElementById("text") as HTMLTextAreaElement;
const hint = document.getElementById("empty-hint") as HTMLParagraphElement;

async function refresh(): Promise<void> {
  const last = await invoke<string | null>("rescue_last");
  if (last !== null && last.length > 0) {
    text.value = last;
    text.hidden = false;
    hint.hidden = true;
    text.select();
  } else {
    text.value = "";
    text.hidden = true;
    hint.textContent = t("rescue.emptyHint");
    hint.hidden = false;
  }
}

document.getElementById("btn-retry")?.addEventListener("click", () => {
  void invoke("rescue_retry");
  void refresh();
});

document.getElementById("btn-paste")?.addEventListener("click", () => {
  void invoke("rescue_paste_again");
});

document.getElementById("btn-copy")?.addEventListener("click", () => {
  // Voluntary copy by the user: no clipboard on the Voxli side
  // without this explicit gesture (RF-30).
  text.removeAttribute("readonly");
  text.select();
  document.execCommand("copy");
  text.setAttribute("readonly", "");
});

document.getElementById("btn-close")?.addEventListener("click", () => {
  void invoke("rescue_close");
});

// UI language (v5): read from the configuration, fr fallback.
try {
  void (async () => {
    const cfg = await invoke<{ general: { uiLanguage: string } }>("config_get");
    applyUiLanguage(cfg.general.uiLanguage);
  })();
} catch {
  applyUiLanguage("fr");
}

// The text can be replaced while the window is open
// (new successful dictation): re-read on focus.
window.addEventListener("focus", () => void refresh());
void refresh();
