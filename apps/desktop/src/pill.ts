/** Pill RF-09 v2 — permanent indicator at the top center of the screen.
 * The window lives from startup to shutdown: at rest it shows the
 * armed listening point alone, during a take the full visual (state,
 * level meter, live text). Hover is detected on the core side (the window
 * is click-through): on entry, the legend shows the configured
 * dictation hotkey — re-read on every hover, it can change at runtime.
 * No dictated content is logged or kept (RF-34); the
 * streaming partials are transient (ADR-02 amendment) and
 * vanish as soon as the take ends.
 * Presentation language: `general.uiLanguage` (v5, fr fallback). */

import { applyUiLanguage, t } from "./i18n";

/** Localized key names (labels of the core-side voxli-hotkeys). */
function keyName(raw: string): string {
  return raw
    .split("+")
    .map((k) => k.trim())
    .filter((k) => k.length > 0)
    .map((k) => t(`key.${k}`))
    .join(" + ");
}

void (async function init() {
  const listen = await import("@tauri-apps/api/event").then((m) => m.listen);
  const { invoke } = await import("@tauri-apps/api/core");

  // UI language (v5): read from the configuration (the pill
  // opens AFTER being loaded by the assembly) — silent fr fallback.
  try {
    const cfg = await invoke<{ general: { uiLanguage: string } }>("config_get");
    applyUiLanguage(cfg.general.uiLanguage);
  } catch {
    applyUiLanguage("fr");
  }

  window.setTimeout(() => {
    void (async () => {
      const s = await invoke<{ state: string }>("state_get");
      applyState(s.state);
    })();
  }, 0);

  void listen("state://changed", (ev) => {
    const payload = ev.payload as { state: string };
    applyState(payload.state);
  });

  /* Voice gauge (RF-09 v2): meter ballistics — fast fill,
   * gentle drain (slower chase), rendering via requestAnimationFrame
   * for continuous animation between samples (~10 Hz). The loop
   * stops by itself when the voice goes quiet: no cost outside a take. */
  let target = 0;
  let shown = 0;
  let raf = 0;

  /** Sets the width and the intensity (CSS variable --lvl for opacity/glow). */
  function applyLevel(v: number): void {
    const fill = document.getElementById("pill-meter");
    if (fill === null) return;
    const leveled = Math.max(0, Math.min(1, v));
    fill.style.width = `${(leveled * 100).toFixed(1)}%`;
    fill.style.setProperty("--lvl", leveled.toFixed(3));
  }

  function step(): void {
    // Fast attack (the voice rises), gentle fall (the gauge drains).
    const chase = target > shown ? 0.5 : 0.14;
    shown += (target - shown) * chase;
    if (target < 0.004 && shown < 0.004) {
      shown = 0;
      raf = 0;
    } else {
      raf = requestAnimationFrame(step);
    }
    applyLevel(shown);
  }

  void listen<{ level: number }>("meter://level", (ev) => {
    target =
      document.body.dataset.state === "recording"
        ? Math.max(0, Math.min(1, ev.payload.level))
        : 0.0;
    if (raf === 0) raf = requestAnimationFrame(step);
  });

  // Transient streaming partial (ADR-02 amendment): the line
  // writes live. Display only. An empty stream (a filtered
  // partial, "anti-hallucination" silence) does NOT set has-partial:
  // otherwise the label disappears behind an orphaned cursor.
  void listen<{ text: string; atMs: number }>("dictation://partial", (ev) => {
    const el = document.getElementById("partial");
    if (el === null) return;
    const text = ev.payload.text;
    el.textContent = text;
    el.classList.toggle("live", text !== "");
    document.body.classList.toggle("has-partial", text !== "");
  });

  // Hover (core-side detection, click-through window): the legend
  // shows the current hotkey, re-read on every entry (RF-01: changeable
  // at runtime).
  void listen<boolean>("pill://hover", (ev) => {
    if (ev.payload) {
      document.body.classList.add("hover");
      void (async () => {
        const cfg = await invoke<{ hotkeys: { pushToTalk: string } }>("config_get");
        const el = document.getElementById("keys");
        if (el !== null) el.textContent = keyName(cfg.hotkeys.pushToTalk);
      })();
    } else {
      document.body.classList.remove("hover");
    }
  });

  /** Clears the transient line (end of take — RF-34: nothing remains). */
  function clearPartial(): void {
    const el = document.getElementById("partial");
    if (el !== null) {
      el.textContent = "";
      el.classList.remove("live");
    }
    document.body.classList.remove("has-partial");
  }

  /** The whole visual follows the data-state attribute (CSS = single truth). */
  function applyState(state: string): void {
    document.body.dataset.state = state;
    if (state !== "recording") {
      // Partials only make sense during a take: they are
      // cleared as soon as the engine takes over.
      clearPartial();
    }
    const label = document.getElementById("label");
    if (label !== null) {
      const key = `state.${state}`;
      const rendered = t(key);
      // Unknown key → the raw state name stays readable (never blank).
      label.textContent = rendered === key ? state : rendered;
    }
  }
})();
