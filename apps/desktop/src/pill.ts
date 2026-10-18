/** Pastille RF-09 v2 — indicateur permanent en haut au centre de l'écran.
 * La fenêtre vit du démarrage à l'extinction : au repos elle montre le
 * seul point d'écoute armée, pendant la prise le visuel complet (état,
 * vumètre, texte en direct). Aucun contenu dicté n'est journalisé ni
 * conservé (RF-34) ; les hypothèses streaming sont transitoires
 * (amendement ADR-02) et s'effacent dès la fin de la prise. */

const fr_state: Record<string, string> = {
  idle: "prête",
  recording: "enregistrement…",
  finalizing: "finalisation…",
  transcribing: "transcription…",
  "post-processing": "préparation…",
  inserting: "insertion…",
  recovery: "incident",
};

void (async function init() {
  const state = await import("@tauri-apps/api/event").then((m) => m.listen);
  const { invoke } = await import("@tauri-apps/api/core");

  window.setTimeout(() => {
    void (async () => {
      const s = await invoke<{ state: string }>("state_get");
      applyState(s.state);
    })();
  }, 0);

  void state("state://changed", (ev) => {
    const payload = ev.payload as { state: string };
    applyState(payload.state);
  });

  void state<{ level: number }>("meter://level", (ev) => {
    const fill = document.getElementById("pill-meter");
    if (fill === null) return;
    const leveled =
      document.body.dataset.state === "recording"
        ? Math.max(0, Math.min(1, ev.payload.level))
        : 0.0;
    fill.style.width = `${(leveled * 100).toFixed(1)}%`;
  });

  // Hypothèse transitoire du streaming (amendement ADR-02) : la ligne
  // s'écrit en direct. Uniquement de l'affichage.
  void state<{ text: string; atMs: number }>("dictation://partial", (ev) => {
    const el = document.getElementById("partial");
    if (el === null) return;
    el.textContent = ev.payload.text;
    el.classList.add("live");
    document.body.classList.add("has-partial");
  });

  /** Efface la ligne transitoire (fin de prise — RF-34 : rien ne reste). */
  function clearPartial(): void {
    const el = document.getElementById("partial");
    if (el !== null) {
      el.textContent = "";
      el.classList.remove("live");
    }
    document.body.classList.remove("has-partial");
  }

  /** Le visuel entier suit l'attribut data-state (CSS = vérité unique). */
  function applyState(state: string): void {
    document.body.dataset.state = state;
    if (state !== "recording") {
      // Les partiels n'ont de sens que pendant la prise : ils sont
      // effacés dès que le moteur prend la main.
      clearPartial();
    }
    const label = document.getElementById("label");
    if (label !== null) label.textContent = fr_state[state] ?? state;
  }
})();
