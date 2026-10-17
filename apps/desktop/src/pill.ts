/** Pastille RF-09 — la fenêtre ne porte JAMAIS de texte dicté
 * (ADR-02, RF-34) : nom d'état + niveau instantané. */

const fr_state: Record<string, string> = {
  idle: "prête",
  recording: "enregistrement…",
  finalizing: "finalisation…",
  transcribing: "transcription…",
  "post-processing": "préparation…",
  inserting: "insertion…",
  recovery: "incident",
};

/** Couleur d'état — le visuel remplace le texte (muet / discret). */
const dot_color: Record<string, string> = {
  idle: "#4b5563",
  recording: "#b91c1c",
  finalizing: "#a16207",
  transcribing: "#a16207",
  "post-processing": "#a16207",
  inserting: "#166534",
  recovery: "#a16207",
};

void (async function init() {
  const state = await import("@tauri-apps/api/event").then((m) => m.listen);
  const { invoke } = await import("@tauri-apps/api/core");

  let current = "recording";
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
    const leveled = current === "recording" ? Math.max(0, Math.min(1, ev.payload.level)) : 0.0;
    fill.style.width = `${(leveled * 100).toFixed(1)}%`;
  });

  function applyState(state: string): void {
    current = state;
    if (state === "idle") {
      // La fenêtre est masquée depuis le côté noyau (pill::PillCtl) — ici
      // on ne fait que tenir le visuel à jour.
      return;
    }
    const label = document.getElementById("label");
    const dot = document.getElementById("dot");
    if (label !== null) label.textContent = fr_state[state] ?? state;
    if (dot !== null) dot.style.background = dot_color[state] ?? "#a16207";
  }
})();
