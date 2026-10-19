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

### Audit UX/UI (résolu le 19/10/2026, code seul — compilation locale requise)

- **Partiels réellement visibles dans la pastille** : le worker émettait
  `s.snapshot()` sur la voie `dictation://partial`, mais le streamer
  partiel whisper renvoyait toujours `""` — le texte drainé était jeté ;
  la pastille ne montrait qu'un curseur sans texte ni animation.
  `snapshot()` renvoie désormais le dernier re-décodage conservé
  (`last_text`), la pill ne pose plus `has-partial` sur un texte vide et
  le texte entre par une animation (fondu + glissement 160 ms).
- **Anti-hallucination ancré en tête de texte** (RF-18) : le filtre
  `contains` effaçait des dictées légitimes (« bye-bye » en fin de
  message, etc.). Désormais ancré début de texte, avec résidu non verbal
  uniquement — le contenu dicté au milieu d'une phrase n'est plus
  amputé.
- **Deux-points compacts préservés** (RF-27) : `12:00`, `3:2` et autres
  formats horaires ne sont plus reformatés en `12 : 00` par la
  typographie fine française ; l'insécable reste appliquée partout
  ailleurs (`mot :`, `50 %`).
- **L'option « auto » retombe sur la langue d'affichage** (RF-16) : en
  détection automatique, le repli sous 1,5 s s'avérait vide (détection
  aléatoire sur deux mots) ; il retombe désormais sur `uiLanguage` —
  jamais `None`.
- **Sélection moteur toujours visible en mode Simple** : seule la table
  du catalogue passe en « avancé » ; les radios du moteur (avec leur
  état réel) restent visibles.
- **Téléchargements pilotables** : bouton Annuler (branché sur
  `models_download_cancel`, acquittement neutre « annulé »), statut avec
  le NOM du modèle et le pourcentage, bande indéterminée quand la taille
  serveur est inconnue (au lieu d'un « 0 % » figé), un seul télécharge-
  ment à la fois avec un message clair.
- **Thème appliqué au démarrage** : la CSP du build bloquait le script
  inline de thème (flash clair en sombre, « système » ignoré) — hachage
  SHA-256 du script ajouté au CSP, `applyTheme()` appelé en tête de
  `main()` (double filet), l'onglet « système » suit désormais
  `prefers-color-scheme` (re-render sans recharger).
- **Boutons lisibles en thème sombre** : encre sombre sur accent vif
  (ratio ≈ 8:1, WCAG AA) ; `secondary` garde son encre grisée.
- **Fenêtre partielle réglable sans toucher au schéma v5** :
  `VOXLI_PARTIAL_WINDOW_SEC` (avancé, borné 2..8 s, défaut 8 s).

### Fonctionnalités commanditées : partiels en direct + sélection moteur (19/10/2026)

- **Texte en direct pendant la prise, quel que soit le moteur** : les
  moteurs batch offline (whisper.cpp) produisent dorénavant des partiels
  d'affichage par re-décodage fenêtré du tampon (≤ 8 s), via la voie
  streaming existante — pur affichage, jamais inséré ni journalisé
  (RF-34) ; auto-désactivation si la machine est trop lente (la
  finalisation batch reste l'autorité du texte inséré).
- **Écran Voix : sélection du moteur** : chaque moteur est listé avec son
  état réel (compilé ; utilisable = modèle présent sur le disque), le
  moteur sélectionné est visible, la sélection s'applique immédiatement
  via `configSet` (le pipeline reconstruit son bundle sans redémarrage) ;
  ligne « moteur effectif » alimentée par `stateGet`. i18n fr/EN.
- **Purge du nom de code `Plume`** : répertoire de données toujours et
  uniquement `%APPDATA%\Voxli` (données locales déplacées une fois pour
  toutes depuis `%APPDATA%\Plume` : config, modèles, historique,
  journaux) ; en-têtes de clé et de licence retagués `voxli-*` (le
  compat de l'ancien format est retiré — aucun blob émis sous ce
  format) ; clé localStorage de thème héritée retirée.
- **Thème sombre au noir profond** : fond à `#000000` ; nouvelle
  variable `--surface` pour les champs (réglages, dictionnaires,
  vocabulaire, fenêtre de secours) qui étaient en `#fff` codé en dur —
  blanc sur blanc en sombre, désormais lisibles.
- Bench repassée après changements : fmt-check, clippy `-D warnings`,
  `cargo test --workspace`, `tsc --noEmit`, `vite build` — tous EXIT=0
  (docs/09 M1.5 pour le détail).

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
- **Icône de l'application corrigée dans l'exécutable** (le « coin
  zoomé » rapporté sur l'icône fenêtre/barre des tâches) : l'exe ne
  contenait qu'**une seule image** (un PNG 256 px) tombée du mono-icône
  `assets/logo.ico` copié par le script d'icônes, et la regénération du
  19/10 n'est jamais redevenue active dans l'exe (out-dirs de build
  `voxli-desktop-*` non invalidés — les icônes ne déclenchent pas de
  re-run de `build.rs`). Purge des out-dirs + rebuild : **6 images
  embarquées** (16-256 BMP), rendu vérifié via le décodeur du shell
  (SHDefExtractIcon) à 16/32/48 px — logo complet et lisible.

### Scripts

- `scripts/fetch-model.ps1` : modèle par défaut `ggml-base-q5_1.bin`
  (aligné sur le catalogue et l'onboarding, P1-7).
- `scripts/make-icons.ps1` réécrit : il ne copie plus le mono-icône
  `assets/logo.ico` (un seul PNG 256 px) sur `icons/icon.ico` — cause
  directe de l'icône exe incomplète. Il génère désormais un ICO
  multi-tailles **16/24/32/48/64/256 encodé BMP 32bpp + masque AND**
  (format canonique décodé à l'identique par rc.exe/le shell), écrit
  de façon identique dans `assets/logo.ico` et
  `apps/desktop/src-tauri/icons/icon.ico`, plus les PNG 32/128/256 ;
  en-tête : repasser par ce script à chaque changement de logo
  (ne pas regénérer `icon.ico` via `pnpm tauri icon`).

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
