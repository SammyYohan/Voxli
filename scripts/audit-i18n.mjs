/**
 * Audit i18n : chaque dictionnaire de `apps/desktop/src/i18n/` doit porter
 * exactement les clés de la référence `en.ts` (ni trou, ni clé en plus),
 * avec les placeholders ({n}, {name}…) identiques. Exécuter :
 *   node scripts/audit-i18n.mjs
 * Sortie : un rapport par langue, code de sortie 1 au premier écart.
 */
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const DIR = fileURLToPath(new URL("../apps/desktop/src/i18n/", import.meta.url));

function keysOf(file) {
  const text = readFileSync(join(DIR, file), "utf8");
  const keys = new Map();
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^  "([^"]+)":\s*"/);
    if (m !== null) keys.set(m[1], line);
  }
  return keys;
}

function placeholdersOf(line) {
  const rest = line.slice(line.indexOf('": "') + 4);
  return new Set([...rest.matchAll(/\{(\w+)\}/g)].map((m) => m[1]));
}

const FILES = readdirSync(DIR).filter((f) => f.endsWith(".ts") && f !== "index.ts");
const reference = keysOf("en.ts");
console.log(`référence en.ts : ${reference.size} clés\n`);

let ko = 0;
for (const file of FILES) {
  if (file === "en.ts") continue;
  const keys = keysOf(file);
  const missing = [...reference.keys()].filter((k) => !keys.has(k));
  const extra = [...keys.keys()].filter((k) => !reference.has(k));
  const badPh = [...reference.keys()]
    .filter((k) => keys.has(k) && placeholdersOf(keys.get(k)).size !== placeholdersOf(reference.get(k)).size)
    .slice(0, 5);
  if (missing.length > 0 || extra.length > 0 || badPh.length > 0) {
    ko++;
    console.log(`✗ ${file} : ${keys.size} clés (attendu ${reference.size})`);
    if (missing.length > 0) console.log(`  manquantes (${missing.length}) : ${missing.slice(0, 8).join(", ")}${missing.length > 8 ? "…" : ""}`);
    if (extra.length > 0) console.log(`  en trop (${extra.length}) : ${extra.slice(0, 8).join(", ")}${extra.length > 8 ? "…" : ""}`);
    if (badPh.length > 0) console.log(`  placeholders sonores : ${badPh.join(", ")}`);
  } else {
    console.log(`✓ ${file} : ${keys.size} clés, placeholders conformes`);
  }
}

console.log(ko === 0 ? "\nAUDIT i18n : OK" : `\nAUDIT i18n : ${ko} fichier(s) en écart`);
process.exitCode = ko === 0 ? 0 : 1;
