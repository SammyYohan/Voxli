/**
 * i18n audit: every dictionary in `apps/desktop/src/i18n/` must carry
 * exactly the keys of the `en.ts` reference (no hole, no extra key),
 * with identical placeholders ({n}, {name}…). Run:
 *   node scripts/audit-i18n.mjs
 * Output: one report per language, exit code 1 at the first mismatch.
 *
 * Level 2 (references): keys cited by the application must
 * EXIST in the reference — `data-i18n[-html|-title|-placeholder]`
 * attributes of the three windows (*.html at the root of apps/desktop)
 * and literal `t("key")` calls in the TS. A made-up key silently
 * falls into the fallback chain (current→en→fr): it is an
 * invisible-to-the-eye translation hole, hence hunted here.
 */
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { join } from "node:path";

const APP = fileURLToPath(new URL("../apps/desktop/", import.meta.url));
const DIR = join(APP, "src/i18n/");

function keysOf(file) {
  const text = readFileSync(join(DIR, file), "utf8");
  const keys = new Map();
  for (const line of text.split(/\r?\n/)) {
    const m = line.match(/^[\t ]*"([^"]+)":\s*"/);
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

// ---- level 2: application references vs the reference ----------------
const used = new Set();
// 1) data-i18n* attributes of the HTML pages (index, pill, rescue).
for (const page of ["index.html", "pill.html", "rescue.html"]) {
  const html = readFileSync(join(APP, page), "utf8");
  for (const m of html.matchAll(/data-i18n(?:-html|-title|-placeholder)?="([^"]+)"/g)) {
    used.add(m[1]);
  }
}
// 2) literal t("…") calls in the TS (the dynamic templates
//    `t(\`x.${v}\`)` are excluded: their bases are covered by the
//    fixed keys composing the family, checked above).
function collectTsCalls(dir) {
  for (const f of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, f.name);
    if (f.isDirectory()) {
      collectTsCalls(p);
    } else if (f.name.endsWith(".ts") && f.name !== "index.ts") {
      const code = readFileSync(p, "utf8");
      for (const m of code.matchAll(/\bt\("([^"]+)"\)/g)) {
        used.add(m[1]);
      }
    }
  }
}
collectTsCalls(join(APP, "src"));
const dangling = [...used].filter((k) => !reference.has(k)).sort();
if (dangling.length > 0) {
  ko++;
  console.log(`✗ références pendantes (${dangling.length}) : ${dangling.join(", ")}`);
} else {
  console.log(`✓ références HTML+TS : ${used.size} clés citées, toutes définies`);
}

console.log(ko === 0 ? "\nAUDIT i18n : OK" : `\nAUDIT i18n : ${ko} fichier(s) en écart`);
process.exitCode = ko === 0 ? 0 : 1;
