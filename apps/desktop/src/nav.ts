/** Navigation latérale des Réglages : une page à la fois, sans rechargement.
 * Présentation seulement — aucun état produit, aucune configuration. Les
 * sections gardent leurs identifiants ; main.ts n'a rien à savoir de la page
 * affichée. */
const PAGES = ["home", "voice", "keys", "vocab", "history", "advanced"] as const;
type Page = (typeof PAGES)[number];

const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>("button[data-nav]"));

function show(page: Page): void {
  document.body.dataset.page = page;
  for (const b of buttons) {
    const on = b.dataset.nav === page;
    b.classList.toggle("on", on);
    if (on) b.setAttribute("aria-current", "page");
    else b.removeAttribute("aria-current");
  }
  window.scrollTo({ top: 0 });
}

for (const b of buttons) {
  b.addEventListener("click", () => show(b.dataset.nav as Page));
}

/** Flèches haut/bas dans la barre latérale (clavier Windows). */
document.querySelector("nav.side")?.addEventListener("keydown", (e) => {
  const k = (e as KeyboardEvent).key;
  if (k !== "ArrowDown" && k !== "ArrowUp") return;
  const i = buttons.findIndex((b) => b === document.activeElement);
  if (i < 0) return;
  e.preventDefault();
  const next = buttons[(i + (k === "ArrowDown" ? 1 : buttons.length - 1)) % buttons.length];
  next.focus();
  show(next.dataset.nav as Page);
});
