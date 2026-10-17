/**
 * Fenêtre de secours (RF-31) : affiche le dernier texte transcrit conservé
 * dans le tampon interne de l'orchestrateur, et permet de le replacer dans la
 * cible (« Coller à nouveau ») ou de relancer la transcription (« Réessayer »).
 * Aucun envoi réseau : tout reste dans la WebView locale.
 */
import { invoke } from "@tauri-apps/api/core";

const EMPTY_HINT =
  "Aucun texte en attente. Si une transcription vient d'échouer, l'audio est conservé : appuyez sur « Réessayer ».";

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
    hint.textContent = EMPTY_HINT;
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
  // Copie volontaire de l'utilisateur : pas de presse-papiers côté Plume
  // sans ce geste explicite (RF-30).
  text.removeAttribute("readonly");
  text.select();
  document.execCommand("copy");
  text.setAttribute("readonly", "");
});

document.getElementById("btn-close")?.addEventListener("click", () => {
  void invoke("rescue_close");
});

// Le texte peut être remplacé pendant que la fenêtre est ouverte
// (nouvelle dictée réussie) : relecture au focus.
window.addEventListener("focus", () => void refresh());
void refresh();
