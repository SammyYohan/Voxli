# Changelog

Format [Keep a Changelog](https://keepachangelog.com/fr/1.1.0/) (consulté
2026-10-18) ; versionnement sémantique dès la première release publique.

Statut global au 19/10/2026 : **aucune version publique n'existe encore**
(la licence produit — décision n°4 — reste en attente ; voir README §7 et
docs/09 M4). Les correctifs d'audit (17–18/10) sont **compilés et testés**
depuis le 19/10/2026 : la bench de tests de docs/11 §1 est passée côté
Windows (103 tests) et la vérification croisée Linux des crates non
Windows aussi — voir docs/11 §1 pour la trace exacte, docs/09 M1 pour le
détail des correctifs d'hygiène faits au passage.

---

## [Non publié]

### Hygiène et correctifs activés par la mise en compilation (19/10/2026)

- `voxli-storage` : échec DPAPI remonté à l'appelant (la clé n'est plus
  stockée en blob vide sur échec) ; la séquence d'identifiants d'historique
  reprend après réouverture de la base au lieu de repartir à 1.
- `voxli-diagnostics` : rotation du journal **pendant l'écriture**, trois
  générations (`.log`, `.log.1`, `.log.2`) — croissance plafonnée en
  fonctionnement continu et historique conservé au démarrage ;
  `export_report` ne transmet plus aucune ligne non conforme au schéma
  d'événements, relu du disque (rédaction forcée effective, compteur de
  rejets en pied de rapport) ; doc de `redact()` alignée sur sa réalité.
- `voxli-win` : fermeture des handles (processus + jeton) dans la
  détection de fenêtre élevée ; élévation déterminée par le jeton
  (`TokenElevation`) avec repli conservateur ; presse-papiers : `GlobalFree`
  sur échec d'écriture (fuite corrigée), formats d'exclusion posés avant le
  texte, restauration tolérante format par format.
- `voxli-hotkeys` : réservation alignée sur les raccourcis système
  `Win+1..9`.
- `voxli-license` : `issue-license` refuse `--seats 0` et toute valeur non
  numérique (une faute de frappe ne produisait plus une licence illimitée) ;
  test réel d'égalité entre `PURCHASABLE_FEATURES` et `Feature::as_str()`.
- `voxli-text` : module `inserter` propre à Windows (cfg) — vérification
  croisée Linux de 8 crates OK ; réglages `hold_ms_*` / `multiline`
  documentés comme non branchés (aucun réglage fantôme).
- `voxli-asr/nemo.rs` : commentaires réalignés sur le code.
- `commands.rs` : `is_safe_model_name` refuse les noms non cadrés.

### Sécurité / intégrité (correctifs d'audit docs/08)

- Empreinte d'un modèle calculée en FLUX (plus de fichier entier en
  mémoire vide) et digest de dossier déterministe pour Parakeet (P1-1,
  P1-2).
- Comparaison effective à une empreinte de confiance (`expected_sha256`,
  statut `verified` au manifeste et à l'interface) — la valeur reste
  `None` tant que le mainteneur n'a pas publié de digest vérifié :
  rien n'est prétendu vérifié sans preuve (P1-3).
- Verrou de téléchargement rendu fiable : `catch_unwind` autour du
  thread, remise du busy dans tous les cas de sortie,
  `models://done {ok:false}` même sur panique (P1-4).
- Extraction d'archive : refus des membres absolus/`..`/backslash,
  validation post-extraction que l'arbre reste sous la destination (P1-5).
- Refus de tout taux d'échantillonnage ≠ 16 kHz sur le chemin de
  transcription distante AVANT toute requête réseau (`AsrError::Runtime`).
- Taille d'un dossier de modèle calculée récursivement (les dossiers NTFS
  n'ont pas de taille utile) (P2).

### Noyau (voxli-core)

- La file de demandes RF-05 est aussi desservie après un échec de
  `capture.stop()` : la session retombe à l'inactif SANS y laisser une
  demande en attente ; test de régression
  `t_stop_failure_then_recovery_session`.
- Nouvelle variante `CaptureError::NoData` (code stable 7) ; la capture
  n'alloue plus dans le chemin temps-réel (tampon de conversion
  pré-alloué ; callbacks I16/U8 réécrits sans allocations) (P1-6).

### Moteurs ASR

- Warm-up RÉELS (RF-19) : whisper (inférence de 100 ms de silence),
  Parakeet et Nemotron (150 ms) — l'allocation onnxruntime se fait au
  chargement, pas à la première dictée.
- Doc du loader Whisper alignée au catalogue réel (`ggml-base-q5_1.bin`
  ; résidu P2-4).

### Configuration (voxli-config)

- Schéma **v5** : `uiLanguage` (langue de l'interface, `fr`|`en`)
  séparé de `language` (langue d'écoute ASR) ; migration 4→5 testée,
  préservant une valeur `uiLanguage` déjà présente.

### Interface (apps/desktop)

- Module i18n minimal (`i18n.ts`) + marquage `data-i18n` des trois
  fenêtres ; sélecteurs UI-langue et dictée séparés ;
  état/pastille/secours localisés ; repli `fr` pour toute valeur
  inconnue.
- Badge « ✓ » de l'empreinte à l'écran Modèles quand (et seulement
  quand) le digest de confiance correspond.
- Message de raccourci conflictuel exact (l'UI édite les raccourcis ; ce
  qui échoue est l'ENREGISTREMENT OS, presque toujours un conflit) —
  constante `ERR_HOTKEY_P0` renommée `ERR_HOTKEY_TAKEN`.
- Infobulle de la barre système dynamique depuis la configuration
  (`tray_tooltip`), mise à jour au re-enregistrement à chaud (RF-01).

### Scripts

- `scripts/fetch-model.ps1` : modèle par défaut `ggml-base-q5_1.bin`
  (aligné sur le catalogue et l'onboarding, P1-7).

### Documentation

- `docs/08-audit-projet.md` (audit interne 17–18/10/2026),
  `docs/09-feuille-de-route.md`, `docs/10-strategie-monetaire.md`,
  `docs/11-checklist-release.md`, `docs/langues-supportees.md`,
  `docs/vie-privee.md`, `SECURITY.md`, `CONTRIBUTING.md`,
  `CODE_OF_CONDUCT.md`, l'actuel CHANGELOG.
- README : nouveaux renvois audit/route/monétaire/checklist (§8–§12) ;
  livrables 08 à 11 référencés dans la table.

### Ce qui manque volontairement

- Aucune `.github/FUNDING.yml` (conditions : docs/10 §5).
- Aucun changement de licence (décision n°4 en attente).
- AUCUNE promesse multi-plateforme au-delà de ce qui est codé
  (Windows-first, docs/09 M3).
