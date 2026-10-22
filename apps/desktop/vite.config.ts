import { defineConfig } from "vite";

/**
 * Three standalone windows, no framework loader:
 * - `index.html`  → "Settings" window (shown from the tray);
 * - `rescue.html` → rescue window RF-31;
 * - `pill.html`   → state pill RF-09.
 */
export default defineConfig({
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        main: "index.html",
        rescue: "rescue.html",
        pill: "pill.html",
      },
    },
    emptyOutDir: true,
  },
});
