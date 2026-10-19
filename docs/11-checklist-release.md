# 11 — Checklist de release

> Documents de pilotage du dépôt : voir le README, table des livrables. Ce
> document est actionnable : chaque case est soit **cochable**, soit porte un
> nom de responsable. Il s'applique à LA PREMIÈRE publication et à chaque
> release suivante. Il complète `docs/08-audit-projet.md` (correctifs) et
> `docs/09-feuille-de-route.md` (ordonnancement).

Niveaux utilisés (convention du dépôt) : **[FAIT]** vérifié sur source primaire,
**[RECO]** recommandation d'ingénierie assumée, **[À FAIRE]** maintenance
restante au jour d'écriture.

---

## 1. Ce qui doit être VRAI avant d'envisager une release

Chaque item se **démontre par une commande réellement exécutée et sa sortie
conservée** (aucun statut « vert » n'est déclaré sur parole — règle
transcrite de l'audit, §7).

> **Exécution du 19/10/2026** (machine Windows 11, rustc/cargo 1.99.1,
> Node via pnpm). Les items cochés le sont avec les commandes ci-dessous ;
> les non cochés restent des entrées de jalon M2/machine outillée.

- [x] `cargo fmt --all --check` passe sans diff [FAIT : 19/10/2026, EXIT=0]
- [x] `cargo clippy --workspace --all-targets -- -D warnings` sans erreur
      [FAIT : 19/10/2026, EXIT=0, zéro avertissement ; (`voxli-win` et les
      features matérielles s'exécutent sous Windows ; les crates pures ont aussi
      été vérifiées en cible Linux le même jour :
      `cargo check --target x86_64-unknown-linux-gnu` sur core, config,
      license, storage, diagnostics, hotkeys, text, win — EXIT=0, ce qui
      étend le job CI `tests-purs-linux` §5)]
- [x] `cargo test --workspace` vert — 103 tests, 0 échec [FAIT :
      19/10/2026, EXIT=0 ; compris :

  | Suite | Contenu couvert |
  |---|---|
  | `voxli-core/tests/flow.rs` | machine d'états complète (25 scénarios, y compris `t_stop_failure_then_recovery_session`) |
  | `voxli-config/tests/config.rs` | schéma v5 (`uiLanguage`) et migrations 1→5 |
  | `voxli-asr` | WAV 16 kHz (refus hors contrat), multipart, fake |
  | `commands.rs` (module `#[cfg(test)]`) | catalogue, noms sûrs, arborescence d'extraction, digests |
  | `voxli-audio`, `voxli-vad`, `voxli-hotkeys`, `voxli-license`, `voxli-diagnostics`, `voxli-core` | suites unitaires existantes |
  | `voxli-storage --features sqlite` | 8 tests (reprise d'id, DPAPI en échec remonté) |

      Avancée 19/10/2026 : la partie whisper-rs du chemin C est désormais
      faisable sur la machine de test — cmake 4.4.4 + ninja 1.13.2
      installés (winget), build `--features custom-protocol,real-asr`
      EXIT=0, moteur `whisper.cpp` chargé au runtime. Restent à la machine
      outillée : sherpa-onnx (parakeet) et le SDK NeMo pour la liste
      `--all-features` complète, résultat à consigner ici AVANT toute
      release.]
- [x] Interface TypeScript sans erreur — **[FAIT : 19/10/2026]** via les
      scripts réels du `package.json` (`check` = `tsc --noEmit`, EXIT=0) et
      `build` = `vite build` (EXIT=0). La formulation antérieure citait un
      script `lint` qui n'existe pas dans le dépôt — le script réel est
      `check` [corrigé ici, la convention §1 s'applique : commande exécutée
      réellement, sortie conservée].
- [ ] Chemin Windows du paquet : `cargo tauri build` avec la feature
      `custom-protocol` **une fois**, installé puis lancé sur une machine
      proprette (aucune fenêtre vide, hotkey enregistrée, modèle
      installable) [À FAIRE — entrée M2 ; le binaire release seul a été
      construit : `cargo build --release -p voxli-desktop`, EXIT=0,
      19/10/2026, mais l'installateur NSIS et le test machine proprette
      restent à exécuter par le mainteneur]

**Interdiction transversale** : aucun changement de licence appliqué sans la
décision humaine n°4 (`docs/01`) ; aucune URL de don sans destination vérifiée ;
aucun digest inventé.

## 2. Cohérence documentaire et de produit

- [ ] `README` (statut, §6), `CHANGELOG.md`, `SUPPORT.md` et le numéro de
      version (tauri.conf + Cargo.toml + package.json) disent la même chose.
- [ ] `docs/02` : vérifier que chaque RF marquée « implémentée » est
      effectivement couverte par une commande/executable observable de la
      release (aucune RF « à faire » n'est présentée comme done).
- [ ] Écrans copiés-collés cohérents avec les messages réels : la langue de
      l'interface (`uiLanguage`) et de dictée (`language`) sont séparées
      (schéma v5) — les deux sélecteurs sont présents.
- [ ] `THIRD-PARTY-LICENSES.md` mis à jour si une dépendance ou un modèle
      nouveau entre dans le paquet (attribution CC-BY-4.0 requise pour
      Parakeet).

## 3. Empreintes de confiance des modèles (procédure mainteneur)

> Référencée par l'audit P1-3 (`docs/08` §P1-3) et par le code
> (`commands.rs`, champ `expected_sha256`). La mécanique de comparaison est
> **écrite** ; jusqu'à l'exécution de cette procédure, aucune empreinte de
> confiance n'existe et l'UI doit rester honnête (`verified: false`).

### 3.1 Obtenir un digest qui a le droit de s'appeler « vérifié »

1. **Source primaire** : télécharger le fichier DEPUIS les URL du catalogue
   (`commands.rs`, `MODEL_CATALOG`) — et uniquement celles-ci — sur une
   machine outillée, par HTTPS (curl -L ou le script `scripts/fetch-model.ps1`
   auquel on passe explicitement `-Model`).
2. **Calcul en double lecture** : calculer `sha256sum` (ou
   `Get-FileHash -Algorithm SHA256`) **deux fois** — deux passes ne doivent
   jamais diverger ; en cas d'écart : téléchargement corrompu/redirigé,
   on recommence.
3. **Revue humaine** : une seconde personne (le commanditaire) refait au
   moins l'étape 1 sur un autre canal (autre réseau, autre outil). Le digest
   publié est celui des DEUX exécutions convergeantes. Aucun digest n'est
   validé par la personne qui l'a calculé seul [RECO].
4. **Traçabilité** : consigner au journal de release : date, URL, taille
   octale exacte, digest, l'identité des deux exécutants.
5. **Publication** : le digest entre dans `MODEL_CATALOG`
   (`expected_sha256: Some("…")`, hex minuscule) ET dans un fichier
   `docs/releases/digests-<version>.md` portant le même contenu. Les deux
   sources doivent rester identiques — cette cohérence est testée par la
   checklist §2.
6. **UI** : une fois en place, l'interface affiche « ✓ » sur la ligne du
   modèle (champ `verified` de `ModelInfo`) — si ce badge est visible, le
   digest A VÉRITABLEMENT été revu humainement (ce ci-dessus [FAIT] après
   validation, jamais avant).

### 3.2 Étendue

Modèles concernés aujourd'hui : `ggml-base-q5_1.bin`,
`ggml-small-q5_1.bin`, `nemotron-3.5-asr-streaming-0.6b.q8_0.gguf`,
`sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8` (pour Parakeet, le digest porte
sur le fichier modèle regroupé, pas sur le répertoire : convention du champ
`expected_sha256`). Ceux qui ne peuvent fournir ce niveau de preuve restent
à `None` — c'est un choix assumé : ne pas prétendre.

> Rappel P1-7 : le défaut du script PS est `ggml-base-q5_1.bin` —
> aligné sur le catalogue ; l'onboarding 1-clic installe le même id.

## 4. Paquet Windows

- [ ] Version cohérente (`tauri.conf.json` > `cargo.toml` > `package.json`).
- [ ] Icone/URI Relations : assets logo.svg/logo.png archivés ; binaire
      `custom-protocol` unique ; la DLL SDK NeMoSpeech est copiée par
      `build.rs` au bon emplacement (conditions du README §6b).
- [ ] Installeur : [RECO] NSIS par Tauri par défaut ; noter l'absence de
      signature de code (c'est un risque connu au premier déploiement :
      SmartScreen alarmera — le SUPPORT.md devant le dire honnêtement tant
      qu'un certificat n'existe pas [À FAIRE : décision de budget
      commanditaire]).
- [ ] `%APPDATA%\Voxli` : première exécution crée `config.json` au schéma
      courant (v5 au moment de la rédaction) et JAMAIS autre chose ; le
      chemin Plume hérité n'est utilisé que si Voxli n'existe pas déjà
      (code `default_dir`).
- [ ] Désinstallation : rien de tenace hors `%APPDATA%\Voxli` (aucune clé de
      registre d'état, aucun service).

## 5. CI au jour de la publication

- [ ] Le workflow GitHub Actions passe sur Linux (crates pures : core,
      config, license, hotkeys, diagnostics) — job ajouté en
      CI, à VÉRIFIER lors du premier push (marqué « à confirmer »).
- [ ] Un job Windows (msvc) s'ajoute quand un runner est disponible :
      compilation + tests unitaires sans matériel (mêmes suites que §1)
      [À FAIRE — mainteneur].
- [ ] Le job CI n'exécute AUCUNE connexion réseau vers les fournisseurs ;
      les tests réseaux-existent en unitaires hors ligne.

## 6. Publication

- [ ] CHANGELOG.md versionné (Keep a Changelog) + tag git.
- [ ] Note de release : ce qui est prouvé (tests réussis à ce moment —
      commandes + horodatage), ce qui reste à valider terrain, et la liste
      des dépendances tierces avec texte de licence.
- [ ] Ne JAMAIS publier une formulation « audité par un tiers » — l'audit
      `docs/08` est un travail interne, la formulation publique reste sobre.
- [ ] Après publication : ouvrir une issue « retour d'incident » pour
      éplucher les journaux `Event::Error` anonymisés des premières
      installations [RECO].

## 7. Rappel des garde-fous

1. **Vie privée** : revoir `docs/vie-privee.md` à chaque écran nouveau —
   l'absence d'envoi réseau est une propriété testable, pas un slogan.
2. **Licence produit** : décision pendante (n°4). Tant que la décision
   n'est pas prise : aucune release publique.
3. **Dons** : aucune URL de don tant que les destinations ne sont pas
   vérifiées par le commanditaire ; le FICHIER FUNDING.yml ne doit pas exister
   avant (conditions dans `docs/10` §5).
