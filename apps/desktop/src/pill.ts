/** Pastille RF-09 v2 — indicateur permanent en haut au centre de l'écran.
 * La fenêtre vit du démarrage à l'extinction : au repos elle montre le
 * seul point d'écoute armée, pendant la prise le visuel complet (état,
 * vumètre, texte en direct). Le survol est détecté côté noyau (la fenêtre
 * est traversée par le clic) : à l'entrée, la légende montre le raccourci
 * de dictée configuré — relu à chaque survol, il peut changer à chaud.
 * Aucun contenu dicté n'est journalisé ni conservé (RF-34) ; les
 * hypothèses streaming sont transitoires (amendement ADR-02) et
 * s'effacent dès la fin de la prise.
 * Langue de présentation : `general.uiLanguage` (v5, repli fr). */

import { applyUiLanguage, t } from "./i18n";

/** Noms des touches localisés (libellés de voxli-hotkeys côté noyau). */
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

  // Langue de l'interface (v5) : lue de la configuration (la pastille
  // s'ouvre APRÈS le chargement par l'assemblage) — repli fr silencieux.
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

  /* Jauge de voix (RF-09 v2) : balistiques de vumètre — remplissage vif,
   * vidange douce (chasse plus lente), rendu via requestAnimationFrame
   * pour une animation continue entre les échantillons (~10 Hz). La boucle
   * s'arrête seule quand la voix se tait : aucun coût hors prise. */
  let target = 0;
  let shown = 0;
  let raf = 0;

  /** Pose la largeur et l'intensité (variable CSS --lvl pour opacité/halo). */
  function applyLevel(v: number): void {
    const fill = document.getElementById("pill-meter");
    if (fill === null) return;
    const leveled = Math.max(0, Math.min(1, v));
    fill.style.width = `${(leveled * 100).toFixed(1)}%`;
    fill.style.setProperty("--lvl", leveled.toFixed(3));
  }

  function step(): void {
    // Attaque rapide (la voix monte), chute douce (la jauge se vide).
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

  // Hypothèse transitoire du streaming (amendement ADR-02) : la ligne
  // s'écrit en direct. Uniquement de l'affichage.
  void listen<{ text: string; atMs: number }>("dictation://partial", (ev) => {
    const el = document.getElementById("partial");
    if (el === null) return;
    el.textContent = ev.payload.text;
    el.classList.add("live");
    document.body.classList.add("has-partial");
  });

  // Survol (détection noyau, fenêtre traversée au clic) : la légende
  // montre le raccourci courant, relu à chaque entrée (RF-01 : changeable
  // à chaud).
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
    if (label !== null) {
      const key = `state.${state}`;
      const rendered = t(key);
      // Clé inconnue → le nom brut d'état reste lisible (jamais blanc).
      label.textContent = rendered === key ? state : rendered;
    }
  }
})();
