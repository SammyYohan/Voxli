# 9 — Feuille de route post-audit

> Suite logique de `docs/08-audit-projet.md` (l'audit) et
> `docs/11-checklist-release.md` (les conditions de sortie). L'ordonnancement
> reste indicatif sauf les jalons au commanditaire [RECO] : une étape qui
> exige une décision humaine ne peut pas être « poussée » par le code.

Jalons écrits les 17–18/10/2026. **Dans tout le fil des jalons ci-dessous,
« écrit » signifie : modifié dans le dépôt mais pas encore compilé** (aucune
toolchain disponible au moment des écritures — les compilations à prouver
restent listées en M2).

---

## M1 — Correctifs d'audit (code) — écrit, à compiler

Source : `docs/08` §3 (défs P1-1…7, P2-1…4) et §5 (correctifs). Résumé des
fichiers touchés et du statut :

| Zone | Fichiers | Statut |
|---|---|---|
| Modèles : empreintes streamées, digest de dossier, comparaison de confiance, busy-libérable, archive path-traversal, taille récursive, `title`/`verified` UI | `commands.rs`, `main.ts`, `index.html` | écrit |
| Capture : tampon de conversion pré-alloué, arrêt sans alloc, `NoData` pouvant descendre au code 7 stable | `voxli-audio/capture.rs`, `voxli-core/{errors,orchestrator}.rs` | écrit |
| Orchestrateur : file RF-05 servie ou récupérée aussi après échec de `stop()` ; test de régression dédié (`t_stop_failure_then_recovery_session`) | `voxli-core`, `tests/flow.rs` | écrit |
| Warm-up ASR réels (whisper 100 ms, parakeet/nemo 150 ms silence) + refus non-16 kHz sur le chemin distant | `voxli-asr/{whisper,parakeet,nemo,remote}.rs` | écrit |
| Config v5 : `uiLanguage` séparé de `language`, migration 4→5 testée | `voxli-config` | écrit |
| i18n UI fr|en (module `i18n.ts`, `data-i18n` sur les trois fenêtres, selects séparés) | `apps/desktop/src/*` | écrit |
| Script modèle par défaut aligné `q5_1`; message raccourci exact ; tooltip tray dynamique | `scripts/fetch-model.ps1`, `strings.rs`, `main.rs` | écrit |

**Sortie de jalon M1** : la liste `cargo test` de `docs/11` §1 passe (Windows
pour les zones matérielles, Linux pour les crates pures).

## M2 — Validation mainteneur [WAITING / à planifier]

1. Compiler & exécuter le banc de tests §1 de docs/11 (les entrées restent
   toutes « à faire » tant que ce n'est pas exécuté).
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
