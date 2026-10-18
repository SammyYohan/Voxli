# Audit de projet Voxli — 18/10/2026

> Document exigé par la phase 1 du prompt maître (« Discover and audit »).
> Distinction stricte entre **confirmé** (lu et vérifié sur le code) et
> **suspecté** (happé par lecture, non reproduit ici).
>
> **Cadre de cet audit** : exécuté hors Windows, sans toolchain Rust
> disponible (Google Cloud Shell) — **aucun `cargo test`/`cargo build` n'a
> été lancé**. Tout micro-bilan « compilé+vert » ci-dessous renvoie à la
> session de l'auteur du 17–18/10/2026 (README §6.3), **pas à une validation
> de cet audit**. Chaque correctif appliqué ici est réputé « écrit, non
> compilé » tant qu'un `cargo test --workspace` n'est pas relancé.
>
> Document livré en français, conformément à la convention des docs 01–07.

## 1. Méthode

- Lecture intégrale des fichiers concernés : `commands.rs` (940 l.), `main.rs`
  (670 l.), `worker.rs` (sections moteur/langue/événements, 1741 l.),
  `orchestrator.rs` (776 l.), `state.rs`, `traits.rs`, `types.rs`, `errors.rs`,
  `flow.rs` (1081 l.), `capture.rs` (467 l.), `vad.rs`, `convert.rs`, les cinq
  moteurs ASR, `config/lib.rs` (502 l.) + tests, `scripts/fetch-model.ps1`,
  `.github/workflows/ci.yml`, interface (3 pages + 3 scripts TS).
- Statut git : **le dépôt n'est pas un dépôt git** (extraction d'archive) —
  aucune fusion utilisateur en risque, mais aucun historique pour tracer les
  modifications (voir §6, recommandation d'initialiser git avant tout suivi).

## 2. Carte d'architecture vérifiée

```
apps/desktop/src-tauri (assemblage Tauri 2)
├── main.rs        fenêtres, tray, raccourcis globaux (plugin), pastille
├── commands.rs    23 commandes IPC + gestionnaire de modèles RF-20
└── worker.rs      thread pipeline, sélection moteur, pilote streaming
crates/
├── voxli-core     machine à états, orchestrateur strictement synchrone,
│                  tampon RF-22 (texte retenu avant insertion, testé)
├── voxli-audio    cpal 0.18.2 (Cargo.lock), anneau pré-roll 400 ms,
│                  VAD énergie 3 seuils (0.010/0.005/0.002 RMS, worker.rs)
├── voxli-asr      fake (défaut) / whisper.cpp / Parakeet ONNX /
│                  Nemotron streaming / distant BYOK (opt-ins)
├── voxli-text     plan d'insertion (pur) + exécution Windows
├── voxli-win      FFI Win32 + SDK NeMo-Speech.cpp ( liaison build.rs)
├── voxli-config   schéma v4 au moment de l'audit — évolution v5
│                  (`uiLanguage`, migration 4→5) écrite dans la passe de
│                  correctifs (§5)
└── storage/secure/license/diagnostics/hotkeys (cf. docs/05 existant)
```

## 3. Défauts confirmés (avec preuve)

### P1-1 — Parakeet : l'empreinte est calculée sur un RÉPERTOIRE (échec garanti)
- **Fichier** : `apps/desktop/src-tauri/src/commands.rs:643-644`.
- **Raisonnement** : `download_model_thread` lit `dest` avec
  `std::fs::read(&dest)` puis `sha256_of(&bytes)`. Pour `kind == "parakeet"`,
  `dest` est un **dossier** créé à la ligne 598 et rempli par `tar` —
  `fs::read` sur un dossier renvoie une erreur OS. Résultat : une
  installation Parakeet **réussie est déclarée échouée** à l'interface
  (`models://done {ok:false}`) et l'empreinte n'est jamais consignée dans
  `manifests.json` (`models_list` renvoie `sha256: null`, UI privée du contrôle).
- Les seuls cas traités par type sont dans `models_list` (l. 365-380) et
  `models_remove` (l. 407-410) ; l'étape d'empreinte a oublié le type.

### P1-2 — Empreinte non streamée : le modèle ENTIER est chargé en RAM
- **Fichier** : `commands.rs:686-695` (`sha256_of` prend `&[u8]` complet),
  alimenté par `std::fs::read(&dest)` (l. 643).
- **Impact** : rame temporaire ≈ taille du modèle — jusqu'à ~708 Mo pour le
  GGUF Nemotron (181 Mo whisper) doublée en pic mémoire sur un thread de fond.
- **Fix appliqué** : hachage en flux (`BufReader` 1 Mio) + digeste de dossier
  déterministe pour Parakeet.

### P1-3 — Empreinte calculée mais JAMAIS comparée à une référence de confiance
- **Fichiers** : `commands.rs:663-677` (consignation seule) ;
  `crates/voxli-asr/src/whisper.rs:4-6` (doc affirmant « vérifiée à
  l'installation » — faux) ; `docs/03` (verbe « vérification ») ;
  `apps/desktop/index.html:296-299` (copie « Chaque fichier est vérifié
  après réception ») ; `docs/04:203` (nœud « Empreinte SHA-256 valide ? »
  du diagramme, non implémenté).
- **Impact** : un téléchargement corrompu ou redirigé est accepté. Le hash
  recalcule une égalité avec lui-même (« preuve » non authentifiée).
- **Position d'ingénierie** : ne pas **inventer** les références de confiance
  (les fournisseurs ne publient pas de digest dans les URL ci-dessus). Fix
  appliqué : support de `expected_sha256` dans le catalogue (aucune valeur
  inventée — restée `None` tant qu'un digest vérifié n'est pas publié par le
  mainteneur), comparaison effective quand renseigné, statut `verified` au
  manifeste et dans l'UI. Procédure mainteneur documentée dans
  `docs/11-checklist-release.md` §3.1.

### P1-4 — Verrou de téléchargement bloqué pour toujours si `spawn` échoue
- **Fichier** : `commands.rs:442-448` + `479`.
- Le busy est posé `true` avant `thread::Builder::spawn(...)` ; sa remise à
  `false` n'existe qu'au fond du thread. Si `spawn` échoue, l'erreur remonte
  **sans jamais libérer** le verrou → tout téléchargement ultérieur répond
  « déjà en cours » jusqu'à redémarrage. Un panic à l'intérieur du thread
  avait le même effet (jamais traité).
- **Fix appliqué** : `catch_unwind` + remise du busy dans tous les issues, et
  émission d'un `models://done {ok:false}` même en cas de panic.

### P1-5 — Extraction d'archive sans validation de chemins (path traversal)
- **Fichier** : `commands.rs:598-638`.
- `tar -xjf` est invoqué **sans contrôle des entrées** de l'archive : un
  membre `../../…` ou absolu est extrait où system `tar` le dirige (bsdtar
  sécurise par défaut, GNU ancien non — jamais de garantie écrite).
- **Fix appliqué** : pré-lecture `tar -tjf` refusant tout nom absolu/`..`/
  backslash/drive/NUL ; post-extraction, validation que l'arbre canonique
  reste sous `dest` ; échec ⇒ message utilisateur explicite.

### P1-6 — Allocation dans le chemin temps réel audio (I16/U8)
- **Fichier** : `crates/voxli-audio/src/capture.rs:302-324`.
- Les callbacks I16 et U8 allouent un `Vec<f32>` PAR TRANCHE (contrat RF-07 :
  « le callback n'alloue pas », doc l. 6-7). Le chemin F32 et le tap streaming
  sont propres (scratch préalloué l. 56, 91).
- **Fix appliqué** : scratch de conversion préalloué dans `Shared` (même
  schéma que `tap_scratch`), zéro allocation ensuite.

### P1-7 — Fichier téléchargé par script ≠ attendu par le moteur
- **Fichier** : `scripts/fetch-model.ps1:12` (défaut `ggml-base-q5_0.bin`)
  contre `worker.rs:1052-1060` + catalogue `commands.rs:317-346`
  (`ggml-base-q5_1.bin`). Un utilisateur qui suit le script obtient un
  fichier **jamais retenu** par `engine=auto/model=auto` — sans avertissement
  (seul `check-env.ps1:52-55` scanne `ggml-*.bin` sans vérifier le nom exact).
- **Fix appliqué** : défaut du script aligné sur `ggml-base-q5_1.bin` + note
  explicite.

### P2-1 — File d'attente RF-05 non servie sur l'échec d'arrêt de capture
- **Fichier** : `crates/voxli-core/src/orchestrator.rs:500-515`.
- L'échec de `capture.stop()` retombe Idle **sans `serve_queue()`** — une
  demande mise en file pendant un incident reste en attente jusqu'au prochain
  événement. Toutes les autres issues la servent (l. 544, 598, 709, 729).
- **Fix appliqué** + test de régression ajouté (`flow.rs`, capture en échec
  d'arrêt + demande en file).

### P2-2 — `config.general.model` rejoints sans validation (séquence `..`)
- **Fichier** : `worker.rs:1052-1060` ; écrit depuis le webview via
  `config_set` (`commands.rs:211-252`) sans contrôle.
- Un `config.json` écrit à la main par une extension compromise du webview
  peut pointer whisper sur un fichier hors du dossier modèles.
- **Fix appliqué** : validation stricte en commande (`is_safe_model_name`)
  et repli défensif côté worker (nom invalide ⇒ défaut + consignation).

### P2-3 — Audio vide erroné (pathologique) étiqueté `DeviceLost`
- **Fichier** : `crates/voxli-audio/src/capture.rs:364-366`. Un conversion
  vide est décrite comme la perte du périphérique alors qu'il est encore
  vivant ; un variant dédié `CaptureError::NoData` (code 7) est plus honnête
  au journal RF-34.
- **Fix appliqué** (errors.rs + orchestrator.rs + capture.rs).

### P2-4 — Résidu de nommage `q5_0` versus `q5_1`
- `crates/voxli-asr/src/whisper.rs:39` (doc d'exemple `q5_0`) ; aligné.

### P3 — Observations sans correctif ici (seuils/du type)
- `convert.rs:97,109` : la fenêtre Hann et la normalisation re-codent 24 taps
  (« _taps » ignoré) — zéro défaut fonctionnel, au piège de maintenance si le
  nombre de taps change ; testable qu'à la relance des tests audio.
- Whispr « threads » (`TranscribeOptions.threads`, types.rs:75) jamais
  transmis ; filé dans whisper.rs ici (parakeet/nemo s'en servent déjà pour
  leurs threads en dur côté config).
- `PendingMarker` (state.rs:14) est un type dormant : porté par l'état
  mais jamais rempli (orchestrator.rs:629) — candidat à une implémentation
  réelle de RF-05 ou à un retrait.
- `PostError` avalé silencieusement (orchestrator.rs:634-637, niveau 0
  identité assumé) — le diagnostic RF-26 gagnerait un événement dédié.
- messages perimés : `strings.rs:49-50` (raccourcis modifiables désormais via
  l'UI) — corrigé ; commentaire `main.rs:335-336` (« masquée au lancement »)
  contradictoire avec `tauri.conf.json` (`visible: true`) — pour fenêtre
  « qualité » ; tray tooltip hardcodé « Ctrl+Alt+Espace » remplacé par la
  valeur configurée.
- `remote.rs:120` : nom `encode_wav_16k_mono` mais débit transmis tel quel ;
  garde-fou 16 kHz ajouté.

## 4. Écarts contre le prompt maître (Phase 3/4/5/7/8)

| Écart | Réponse ici |
|---|---|
| i18n interface absente (chaînes FR en dur TS+HTML, `fr_state` dupliqué et déjà divergent main.ts/pill.ts) | couche `i18n.ts` (clés + fr/en), placeholders `data-i18n`, langue d'interface **séparée** de la langue de dictée (schéma v5 + migration testée) |
| langue de dictée fruit de `general.language` que l'UI partageait | idem, v5 |
| programmation de langues/capacités par moteur non exposée par code | documenté en `docs/langues-supportees.md` (jamais affirmé : la liste par moteur attend sa mise en cohérence ; l'API par-code attendra sa validation) |
| Linux uniquement stubs (`worker.rs:84-103`) + CI ✗ Linux | CI Linux ajoutée pour les crates pures — marquée « à vérifier » ; la séparation plateforme effective reste un jalon (docs/09 M3) |
| repo ouvert : SECURITY.md / CONTRIBUTING.md / CODE_OF_CONDUCT.md / CHANGELOG.md absents ; FUNDING absent | les 4 fichiers créés ; FUNDING **non créé** volontairement (aucune destination vérifiée fournie — règle « never fabricate payment links ») |
| empreinte de confiance | §P1-3 |
| licence de dépôt pas tranchée (`license.workspace = "Proprietary"`) | décision humaine requise ; NON modifiée — analyse au doc 10 |
| docs de stratégies/roadmap libérées du prompt absentes | docs/09, 10, 11 créées |

## 5. Correctifs appliqués dans cette passe (fichiers)

| Zone | Fichiers |
|---|---|
| gestionnaire de modèles | apps/desktop/src-tauri/src/commands.rs |
| orchestrateur (file RF-05) | crates/voxli-core/src/{orchestrator.rs,errors.rs} + tests/flow.rs |
| capture temps réel | crates/voxli-audio/src/capture.rs |
| Moteurs ASR (warm-up réels) | crates/voxli-asr/src/{whisper,parakeet,nemo,remote}.rs |
| config (schéma v5 : langue d'interface) | crates/voxli-config/src/lib.rs + tests |
| interface (i18n, langue d'UI) | apps/desktop/{index,rescue,pill}.html + src/{main,pill,rescue,i18n}.ts |
| arborescence | apps/desktop/src-tauri/src/{main.rs,strings.rs}, worker.rs |
| script modèle | scripts/fetch-model.ps1 |
| CI | .github/workflows/ci.yml (job pures Linux) |
| documentation | docs/08-audit-projet.md (ce doc), 09, 10, 11, PRIVACY/LANGUES, SECURITY/CONTRIBUTING/CODE_OF_CONDUCT/CHANGELOG à la racine, README §12 |

## 6. Ce qui reste — en clair

1. **Rien n'a été compilé ici** : `cargo fmt` + `cargo test --workspace`
   + `cargo check -p voxli-desktop --features custom-protocol,real-asr`
   doivent être relancés avant toute affirmation de validité. La CI GitHub
   (windows-latest) est l'arbitre recommandé.
2. Attente mainteneur : **digests de confiance** à publier à côté du
   catalogue (procédure docs/11 §3) — la mécanique de comparaison existe
   désormais, la source de confiance non.
3. Parité Linux (moteur, insertion, raccourcis) : worker.rs est déjà
   segmenté (`FocusStub`/`InserterStub`/`FakeCapture`), mais le vrai travail
   (ALSA/PipeWire, X11/Wayland, insertion) reste au Jalon 4 — documenté dans
   docs/09.
4. Décisions humaines en attente : choix de licence, périmètre payant et
   prix, éventuels liens de don, dépôt des digests des modèles, publication
   release. Rien n'est publié automatiquement.
5. « Nom `Plume` » : héritage (`Config::default_dir` garde le repli
   `Plume` quand le dossier existe déjà) maintenu pour compatibilité des
   données — ne pas supprimer sous peine de perdre des profils de données.

Vérifier chaque référence de preuve ici par rapport aux numéros de ligne
post-modification (les lignes se déplaceront avec les correctifs).
