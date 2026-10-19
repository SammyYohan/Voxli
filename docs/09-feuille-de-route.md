# 9 — Feuille de route post-audit

> Suite logique de `docs/08-audit-projet.md` (l'audit) et
> `docs/11-checklist-release.md` (les conditions de sortie). L'ordonnancement
> reste indicatif sauf les jalons au commanditaire [RECO] : une étape qui
> exige une décision humaine ne peut pas être « poussée » par le code.

Jalons écrits les 17–18/10/2026. **Mise à jour du 19/10/2026** : les
correctifs du jalon M1 ci-dessous sont **compilés et testés** (la bench de
`docs/11` §1 a été exécutée — preuves et statuts case par case dans
`docs/11` §1). L'ancienne convention « écrit = non encore compilé » ne
décrit plus que les jalons ultérieurs non démarrés.

---

## M1 — Correctifs d'audit (code) — compilé et testé (19/10/2026)

Source : `docs/08` §3 (défs P1-1…7, P2-1…4) et §5 (correctifs). Résumé des
fichiers touchés et du statut :

| Zone | Fichiers | Statut |
|---|---|---|
| Modèles : empreintes streamées, digest de dossier, comparaison de confiance, busy-libérable, archive path-traversal, taille récursive, `title`/`verified` UI | `commands.rs`, `main.ts`, `index.html` | compilé + testé |
| Capture : tampon de conversion pré-alloué, arrêt sans alloc, `NoData` pouvant descendre au code 7 stable | `voxli-audio/capture.rs`, `voxli-core/{errors,orchestrator}.rs` | compilé + testé |
| Orchestrateur : file RF-05 servie ou récupérée aussi après échec de `stop()` ; test de régression dédié (`t_stop_failure_then_recovery_session`) | `voxli-core`, `tests/flow.rs` | compilé + testé |
| Warm-up ASR réels (whisper 100 ms, parakeet/nemo 150 ms silence) + refus non-16 kHz sur le chemin distant | `voxli-asr/{whisper,parakeet,nemo,remote}.rs` | compilé + testé (`fake`) ; **build `real-asr` vérifié le 19/10/2026**
(cmake 4.4.4 + ninja 1.13.2 installés via winget ; EXIT=0 en 9 min 32 s,
moteur `whisper.cpp` chargé à l'exécution avec `ggml-base-q5_1.bin`) ;
restent `parakeet` (sherpa-onnx) et `nemo` (SDK absent) |
| Config v5 : `uiLanguage` séparé de `language`, migration 4→5 testée | `voxli-config` | compilé + testé |
| i18n UI fr|en (module `i18n.ts`, `data-i18n` sur les trois fenêtres, selects séparés) | `apps/desktop/src/*` | compilé + testé (tsc + vite) |
| Script modèle par défaut aligné `q5_1`; message raccourci exact ; tooltip tray dynamique | `scripts/fetch-model.ps1`, `strings.rs`, `main.rs` | compilé + testé |

**Correctifs d'hygiène supplémentaires découverts à la compilation/test
(19/10/2026)** :

- `voxli-storage` : échec DPAPI remonté (plus de blob vide enregistré) ;
  la séquence d'identifiants reprend après réouverture de la base
  (`MAX(id)`) — 8 tests sqlite passants.
- `voxli-diagnostics` : rotation **pendant l'écriture** à 3 générations
  (`.log`, `.log.1`, `.log.2`) — le journal ne croît plus sans borne entre
  réouvertures, aucun contenu d'historique n'est écrasé au démarrage ;
  `export_report` filtre **structurellement** les lignes relues du disque
  (rédaction forcée effective, rejets comptabilisés) ; doc de `redact()`
  réelle (utilitaire public, limites documentées).
- `voxli-win` : fuite de handles fermée (processus + jeton) ; élévation
  lue sur le jeton (`TokenElevation`) avec repli conservateur ;
  presse-papiers : `GlobalFree` obligatoire sur échec d'écriture,
  restauration tolérante format par format.
- `voxli-hotkeys` : réservation alignée sur `Win+1..9` (test mis à jour) ;
  docs honnêtes (l'enregistrement réel appartient au plugin).
- `voxli-text` / `voxli-core` : module `inserter` cfg-gated pour Windows
  (cible Linux vérifiée : `cargo check --target x86_64-unknown-linux-gnu`
  sur 8 crates, EXIT=0) ; retrait de `duration_since` (mort) ; réglages
  `hold_ms_*`/`multiline` documentés comme non branchés (plus tard dans
  M2/M3).
- `voxli-license` : `--seats 0` ou non numérique refusé explicitement par
  `issue-license` (plus de licence « illimitée » par faute de frappe) ;
  test d'égalité réel `PURCHASABLE_FEATURES` ↔ `Feature::as_str()`.
- `commands.rs` : `is_safe_model_name` refuse désormais les noms non
  cadrés (espaces en tête/fin).
- `voxli-asr/nemo.rs` : commentaires réalignés sur le code (échec de
  warm-up remonté ; fin de flux après 151 drains secs consécutifs, et non
  « deux »).

**Sortie de jalon M1** : la bench de `docs/11` §1 est passée le
19/10/2026 — fmt-check, clippy `--workspace --all-targets -- -D warnings`
(0 warning), `cargo test --workspace` (103 tests, EXIT=0) et
`cargo test -p voxli-storage --features sqlite` (8 tests, EXIT=0),
`tsc --noEmit` + `vite build` (EXIT=0), `cargo build --release -p
voxli-desktop` (EXIT=0). Restent des entrées de `docs/11` §1 **hors
machine** : `cargo test --workspace --all-features` (partiellement débloqué : le
chemin whisper.cpp/`real-asr` est construit et vérifié le 19/10/2026 ;
restent sherpa-onnx — parakeet, le `.part` est resté interrompu —
et le SDK NeMo), `cargo tauri build` + installation sur
machine proprette, et la procédure digests §3 (humaine).

## M1.5 — Commanditaire UX : partiels cross-engine + sélection moteur [FAIT, 19/10/2026]

Demande du commanditaire (19/10/2026) : du texte en direct pendant la prise
avec n'importe quel moteur, et une sélection de moteur complète à l'écran
(moteur actif visible, activation/désactivation). Implémentation :

- `voxli-asr/whisper_partial.rs` (feature `whisper`) — partiels par
  re-décodage fenêtré (≤ 8 s) du tampon pendant la prise, émis par la voie
  streaming existante : jamais insérés ni journalisés (RF-34 + exception 3
  de docs/05 §3.1). Auto-désactivation de la session si un re-décodage
  dépasse 2,5 s (machine lente : la finalisation batch reste l'autorité
  du texte inséré). Défauts bornés : ≥ 1,2 s d'audio neuf par tick,
  ≥ 1,5 s entre deux re-décodages.
- Worker (bundle whisper) : `streaming: Some(...)`monté automatiquement —
  l'affichage direct s'active dès qu'un moteur batch est chargé, sans
  réglage utilisateur ; dégradation en douceur sur machine lente.
- Écran Voix : commande `enginesList` (état par moteur : compilé = feature
  du build ; utilisable = modèle présent sur le disque) + sélection radio
  de `general.engine` via `configSet` — **appliquée en live** par le
  pipeline (rebuild du bundle, RF-20 ; jamais de redémarrage) ; ligne
  « moteur effectif » alimentée par `stateGet`. i18n fr/EN.
- Nouveaux tests : 2 unitaires `whisper_partial` (throttle, fenêtre bornée,
  auto-off) — suite `voxli-asr` : 5 tests vert.

- Purge du nom de code `Plume` (demande commanditaire, 19/10) : répertoire
  de données sans exception `%APPDATA%\Voxli` (l'héritage conditionnel du
  18/10 est retiré) ; données locales déplacées une bonne fois pour toutes
  `%APPDATA%\Plume` → `%APPDATA%\Voxli` (config, 856 Mo de modèles,
  historique, journaux) ; en-têtes de clé/blob de licence retagués
  `voxli-*` (aucun blob émis sous l'ancien format — le compat est
  retiré) ; clé localStorage de thème héritée retirée ; README nettoyé ;
  variables CSS issues du renommage mécanique (`--Voxli`) rebaptisées
  `--accent`/`--accent-soft`.
- Thème sombre : noir profond `#000000` (était `#1d1b24`, demande
  commanditaire) et nouvelles variables `--surface` / `--chip` — les
  zones de texte (réglages, dictionnaires, vocabulaire, fenêtre de
  secours) étaient en `background:#fff` codé en dur : texte clair sur
  fond blanc en sombre ; désormais surface dédiée, bordure et couleur
  de texte explicites.

Bench exécutée après les changements (19/10/2026) : fmt-check EXIT=0 ;
clippy `--workspace --all-targets -- -D warnings` EXIT=0 ;
`cargo test --workspace` EXIT=0 ; `cargo check -p voxli-desktop`
`custom-protocol,real-asr` EXIT=0 ; `tsc --noEmit` + `vite build` EXIT=0 ;
`cargo build --release -p voxli-desktop --features
custom-protocol,real-asr` EXIT=0 (7 min 25 s). Lancement du binaire
release vérifié : journal et `engine_loaded` whisper.cpp dans
`%APPDATA%\Voxli\logs\voxli.log`, `%APPDATA%\Plume` non recréé par la
nouvelle version (preuve du retrait de l'héritage).

## M2 — Validation mainteneur [WAITING / à planifier]

1. ~~Compiler & exécuter le banc de tests §1 de docs/11~~ **fait à l'essentiel
   le 19/10/2026** : la bench a tourné côté Windows (103 tests, clippy
   `-D warnings`, fmt-check, release build) + vérification croisée Linux des
   8 crates non-Windows. Restent les entrées non exécutables ici :
   `--all-features` sur machine outillée (cmake/ninja), `cargo tauri build`
   et le test d'installation machine proprette — listées [À FAIRE] dans
   `docs/11` §1 avec empreinte.
2. Procédure digests de confiance (docs/11 §3) pour les quatre modèles.
3. Relecture de l'audit par le commanditaire ; arbitrage des « décisions
   en attente » (n°1 licence, n°4 licence code, dons).

## M3 — Plateformes — pistes non-Windows [À FAIRE]

État actuel : l'application cible Windows (décision produit). Les extraits
`#[cfg(not(target_os = "windows"))]` de `worker.rs` (l. 84-103) laissent le
pipeline démarrer sans moteur réel hors Windows — aucun « fallback
silencieux » n'est affiché : c'est assumé et documenté. Pour une prétention
croisée réelle, il faudra :

- insertion hors Windows : le niveau `SendInput` (API Windows) doit être
  remplacé par des plans d'insertion natifs (macOS : voix électronique et
  Linux : X11/Wayland) — chantier séparé, évalué avant d'être promis ;
- hotkeys globales hors Windows (Enigo/rdev sont candidats, jamais
  présentés comme acquis sans banc de test réel) ;
- secours d'insertion repliés proprement hors niveaux disponibles.

Aucune de ces briques n'est écrite au 18/10/2026 — niant tout communiqué
« multi-plateforme » (règle du dépôt).

## M4 — Monétisation et licences [WAITING commanditaire]

Voir `docs/10-strategie-monetaire.md`. Le code expose déjà les crochets
(`license_status`, features `extra-profiles`, `history-on-disk`,
`cloud-post`) — aucun prix ni template de licence ne sort du dépôt tant que
les décisions restantes sont humaines et non tranchées.

## M5 — Publication

Gouvernée par `docs/11-checklist-release.md` (toutes cases cochées avec
preuves). Rien n'est publié tant que la licence produit n'est pas choisie.

---

### Ce que ce document ne promet pas

- Aucune date de release n'est datée : les sprints M2-M4 dépendent du
  commanditaire.
- Rien dans ce dépôt ne prétend « audité par un tiers » : l'audit interne
  est un artefact produit, sa formulation publique restera sobre.
