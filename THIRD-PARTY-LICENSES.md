# Tierces parties — licences et attributions

Ce fichier récapitule ce que Plume charge ou télécharge qui n'est pas son
propre code, avec la licence applicable et le lien primaire. Il est recopié
dans l'export de données (`dataExport`) pour que l'utilisateur garde la
traçabilité avec lui. Avant toute distribution, la liste exhaustive des
crates (y compris transitives) doit être générée et vérifiée
(`cargo license`, `pnpm licenses`) — cette page ne remplace pas ce travail.

## 1. Modèles de reconnaissance téléchargeables (RF-20)

Le téléchargement est **une action explicite de l'utilisateur**, déclenchée
depuis la fenêtre Réglages → « Modèles ». La licence est affichée dans
l'interface **avant** tout téléchargement ; l'empreinte SHA-256 consommée est
consignée dans `%APPDATA%\Plume\models\manifests.json`.

| Modèle | Source primaire | Licence | Attribution |
|---|---|---|---|
| `ggml-base-q5_0.bin`, `ggml-small-q5_0.bin` (Whisper quantisés) | `huggingface.co/ggerganov/whisper.cpp` | **MIT** (dépôt whisper.cpp) | citation du dépôt suffit, pas d'exigence formelle |
| `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8` (Parakeet TDT v3, 25 langues européennes) | `github.com/k2-fsa/sherpa-onnx/releases` (archive `asr-models`) | **CC-BY-4.0** (modèle NeMo de NVIDIA) | **Attribution obligatoire** : crédits « Modèle Parakeet de NVIDIA, converti par sherpa-onnx, sous CC-BY-4.0 » dans « À propos » à la distribution |

Ces URL sont celles consultées et validées lors de l'étude
(`docs/01`, `docs/03` ADR-03). Un changement d'URL ou de licence en amont doit
être répercuté ici et dans `crates/…/commands.rs` (`MODEL_CATALOG`).

## 2. Dépendances logicielles notables (code de construction)

| Crate / paquet | Rôle | Licence |
|---|---|---|
| tauri, tauri-plugin-global-shortcut | coquille applicative, raccourcis | MIT / Apache-2.0 |
| cpal | capture WASAPI | MIT / Apache-2.0 |
| rusqlite (bundled SQLite) | historique disque | MIT (SQLite : domaine public) |
| ureq, sha2 | téléchargement + empreinte | MIT / Apache-2.0 |
| ed25519-dalek | signature de licence | BSD-3 / Apache-2.0 |
| whisper-rs / sherpa-onnx-sys | liaisons moteurs (opt-in) | MIT whisper-rs ; Apache-2.0 sherpa-onnx |
| TypeScript, Vite, esbuild (pnpm) | chaîne de build | Apache-2.0 / MIT |

## 3. Ce qui n'est PAS issu de tierces parties

- Le resampler et le VAD d'énergie sont du code maison (`plume-audio`).
- Le moteur factice (`fake`) est un générateur d'énoncés de recette, à nous.
- Aucune police, icône ou ressource distante n'est embarquée dans les
  fenêtres (CSP `docs/05 §3`) — les icônes du bundle sont produites par le
  script `scripts/make-icons.ps1` (à nous).
