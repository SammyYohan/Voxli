import { defineConfig } from "vite";

/**
 * Deux fenêtres autonomes, aucune chargeur de framework :
 * - `index.html`  → fenêtre « Réglages » (affichée depuis le tray) ;
 * - `rescue.html` → fenêtre de secours RF-31.
 */
export default defineConfig({
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        main: "index.html",
        rescue: "rescue.html",
      },
    },
    emptyOutDir: true,
  },
});
