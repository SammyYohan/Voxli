# Livrable 3 — Architecture technique

Ce document justifie chaque décision structurante sous forme d'ADR (Architecture Decision Record)
et documente les versions, licences et contraintes **réellement vérifiées** au 17/10/2026.

---

## 1. Contraintes de conception

| # | Contrainte | Origine |
|---|---|---|
| C1 | Le chemin critique (raccourci → audio → texte) ne doit jamais dépendre du rendu HTML ni d'un service distant | Latence perçue et exigence de fiabilité |
| C2 | Le mode local doit être prouvable, pas déclaratif | Positionnement produit (V1) |
| C3 | L'insertion de texte doit s'adapter à des applications hétérogènes, dont des applications qui lisent le presse-papiers lentement | Contrainte Windows réelle, documentée par la concurrence |
| C4 | Les composants doivent rester interchangeables (moteurs ASR, inséreurs, stockage) | Le marché change vite (nouveaux modèles tous les trimestres) |
| C5 | L'application ne doit jamais exiger de privilèges administrateur | Déployabilité en entreprise, réduction de surface d'attaque |
| C6 | Le produit doit rester installable et utilisable sur Windows 10 (21H2+) et 11, x64, idéalement ARM64 | Base installée |
| C7 | Compatibilité réglementaire UE (CRA) à horizon fin 2027 : suivi des vulnérabilités, mises à jour de sécurité, documentation technique | Cadre légal, `docs/01` §5.3 |
| C8 | Budget mémoire de base (hors modèle) : cible interne ≤ 120 Mo de RSS | Politique produit « outil discret toujours lancé » |

---

## 2. Vue d'ensemble

```
┌──────────────────────────────────────────────────────────────────────┐
│ Processus unique (Plume.exe)                                          │
│                                                                       │
│  ┌───────────────────┐        ┌───────────────────────────────────┐   │
│  │ Tauri / WebView2  │◄──IPC──►│ Noyau Rust (chemin critique)     │   │
│  │ (réglages,        │  (évts  │  raccourcis · capture · pipeline │   │
│  │  historique,      │   +     │  ASR · post-traitement · insertion│  │
│  │  onboarding)      │ commandes)                               │   │
│  └───────────────────┘        └───────┬───────────────┬───────────┘   │
│                                       │               │               │
│                        ┌──────────────▼──────┐  ┌─────▼───────────┐   │
│                        │ Moteurs ASR locaux  │  │ Hygiène bureau  │   │
│                        │ whisper.cpp (FFI)   │  │ UIA · clipboard │   │
│                        │ sherpa-onnx(Parakeet)│ │ SendInput       │   │
│                        └─────────────────────┘  └─────────────────┘   │
│                                                                       │
│  Stockage : JSON (config) · SQLite chiffré (historique) · DPAPI       │
└──────────────────────────────────────────────────────────────────────┘
        ▲                                        ▲
        │ optionnel, consenti, jamais par défaut │
   ┌────┴─────────────┐                  ┌───────┴──────────┐
   │ Services distants│                  │ Mises à jour     │
   │ (nettoyage cloud)│                  │ signées (minisign)│
   └──────────────────┘                  └──────────────────┘
```

Détail des flux et des frontières de confiance : `docs/04-flux-et-diagrammes.md`.

---

## 3. Décisions d'architecture

### ADR-01 — Shell applicatif : Tauri 2

- **Décision** : Tauri 2 (dernière version stable vérifiée : **2.12.4**, publiée le 14/10/2026 ;
  une branche **3.0.0-alpha.5** existe et n'est pas retenue pour un produit livrable).
- **Justification [FAIT]** : Rust côté processus principal (indispensable pour les API Win32, WASAPI,
  UIA), interface Web pour les réglages et l'historique (vélocité de développement, écosystème
  TypeScript), écosystème de plugins maintenus (raccourcis globaux, mise à jour signée, démarrage
  automatique, presse-papiers, notifications), distribution MSI/NSIS native, mise à jour intégrée
  avec vérification de signature.
- **Conséquences** : dépendance à WebView2 (Runtime Edge) : préinstallé sur Windows 11 et fourni
  avec les mises à jour de Windows 10 depuis avril 2018, éléments vérifiés dans la documentation
  Tauri/Microsoft **[FAIT]**. Un mode installateur hors ligne ajoute ~127 Mo ; un bootstrapper
  embarqué ~1,8 Mo. À trancher en §6.
- **Alternatives rejetées** :
  - *Electron* : empreinte disque/mémoire supérieure, redondance Chromium, aucune capacité Win32
    supplémentaire pour notre usage.
  - *Interface 100 % Rust (egui / iced)* : supprime la dépendance navigateur et gagnerait ~40–80 Mo
    de RSS, mais coûte un temps de développement élevé pour des écrans riches (historique,
    dictionnaires, onboarding) et impose de réécrire des composants d'interface. **Conservé en plan
    B** si WebView2 devient un problème de déploiement en entreprise.
  - *WinUI 3 / C#* : intégration Windows excellente, mais perd l'écosystème Rust pour l'inférence
    locale et les bindings `whisper-rs` / `sherpa-onnx`, et duplique les compétences.
- **Ce que Tauri **ne** garantit pas** : aucune consommation mémoire ni taille d'installation
  prédéfinie. Les chiffres « ~3 Mo » et « ~80 Mo de RSS » lus dans la presse concernent une fenêtre
  vide ; notre application embarque un webview permanent **et** un runtime d'inférence. Le budget
  mémoire est piloté par le modèle ASR, pas par Tauri (voir ADR-02 et `docs/06`).

### ADR-02 — Le webview n'est jamais dans le chemin critique

- **Décision** : le cycle dictée est exécuté entièrement dans le noyau Rust. Le webview ne reçoit que
  des événements d'état (démarré, en transcription, inséré, erreur) et ne fournit que des commandes
  de configuration. Il n'y a **aucun** passage audio ou texte par le pont IPC sur le chemin critique
  de l'insertion.
- **Justification** : un webview peut être suspendu par Windows (fenêtre masquée, économie
  d'énergie), rechargé après une panne, ou simplement lent ; aucun de ces états ne doit pouvoir
  empêcher une dictée. Corollaire (v2) : la pastille est une **fenêtre native dédiée** (Win32
  topmost, non activable, clic traversant) dont le rendu est un webview séparé — toute panne de
  ce webview ne peut pas affecter le cycle (T-17/T-21 inchangés). Elle est **permanente** :
  au repos un point discret en haut au centre (écoute armée — le raccourci est prêt, la capture
  n'est PAS active), pendant la prise le visuel complet (état + vumètre + texte en direct). Au
  survol (détecté côté noyau — le clic traverse la fenêtre), la légende du raccourci de dictée
  glisse sous la pilule ; le déclencheur est un rayon de 15 px autour du point (hystérésis de
  sortie à 20 px), pas le rect de la fenêtre. Un thread de maintenance re-sert position
  (moniteur principal, largeur
  bornée à 85 %, DPI géré) et l'attribut « toujours au premier plan » — que Windows peut
  abandonner au changement d'affichage ou à l'ouverture d'une fenêtre plein écran fenêtrée.
  **Limite assumée** : le plein écran *exclusif* (jeux DirectX) recouvre tout `HWND_TOPMOST` ;
  Windows n'offre aucune garantie de plan au-dessus d'un échange exclusif GPU.
- **Conséquence testable** : le test T-17 (reprise pendant transcription) et T-21 (panique moteur)
  s'exécutent avec le webview tué volontairement ; la dictée doit continuer de fonctionner.
- **Amendement (P2 streaming, 2026)** : la règle « aucun passage de texte dicté par l'IPC » reçoit
  une exception unique et bornée — les **partiels streaming** (`dictation://partial`). Le principe
  initial vise l'**insertion** : l'IPC n'en fait jamais partie. Les partiels sont une seconde
  voie **d'affichage uniquement** : sortis du moteur streaming pendant la prise, poussés à la
  pastille, jamais écrits (journal, disque, historique). Si le webview meurt au milieu d'une
  dictée, l'affichage des partiels disparaît et l'insertion reste intacte (conséquence testable
  inchangée, T-17/T-21). Table IPC : docs/05 §3 (addition documentée, exceptions d'exfiltration 3).
- **Amendement 2 (voie rapide, 2026)** : les **segments finalisés** du streaming — ceux que
  l'utilisateur a déjà vus s'afficher, à l'exclusion des transitoires — peuvent devenir la
  transcription insérée (Amendement 1, dernier paragraphe). Motif mesuré : l'ancien re-décodage
  offline coûtait ~0,95× la durée parlée en attente après relâchement (11,3 s dictés → 10,7 s).
  La voie batch redevient l'autorité si la session n'a rien produit (silence, échec ou reprise
  inbox). Limite assumée : le pré-roll (~0,4 s d'audio antérieur au bouton) n'a pas transité par
  le flux ; un moteur univoque (whisper) reste disponible. Les deux routes sont journalisées
  sans contenu (RF-34) : `transcription-via-stream` / `transcription-via-batch`.

### ADR-03 — Abstraction moteur ASR et stratégie bi-moteur

- **Décision** : une interface `TranscriptionEngine` unique, avec deux implémentations locales et une
  implémentation distante facultative :
  1. **whisper.cpp** via `whisper-rs` (**0.16.0**, bindings sous Unlicense, moteur MIT) — couverture
     linguistique large, écosystème mature, quantification (CPU), Vulkan/CUDA en option.
  2. **sherpa-onnx** (ONNX Runtime) avec le modèle **NVIDIA Parakeet TDT 0.6B v3** — 25 langues
     européennes dont le français, licence **CC-BY-4.0**, ponctuation et casse automatiques, et
     surtout exécution CPU optimisée.
  3. **Moteur distant** (facultatif, jamais par défaut) derrière la même interface, avec un garde-fou
     de consentement dans l'appelant, pas dans le moteur.
- **Justification** : les mesures publiées (protocole explicite, corpus FLEURS/MLS/Common Voice,
  source intéressée donc **[FAIT-INTÉRESSÉ]**) donnent un WER français de **5,39 %** à Parakeet v3
  contre **7,97 %** à `large-v3-turbo`, et positionnent Parakeet comme un modèle optimisé CPU.
  Les mesures publiques de whisper.cpp sur CPU bas de gamme (Ryzen 5 3600XT : `small` int8 à
  **1,7–2,0 s par phrase courte**) montrent qu'un moteur Whisper seul rendrait le produit inutilisable
  sur une part importante du parc. **Conclusion : le moteur par défaut doit être choisi par langue et
  par matériel, pas par dogme.**
- **Conséquences** :
  - L'**attribution NVIDIA est obligatoire** (écran « À propos / Licences »), CC-BY-4.0 : c'est une
    obligation de licence, pas une politesse.
  - Le surcoût d'intégration est réel (deux runtimes d'inférence à empaqueter, tester et suivre en
    vulnérabilités). C'est le principal coût caché de cette ADR ; il est accepté pour la qualité
    française et la vitesse CPU.
  - Le code d'appel ne connaît ni whisper.cpp ni ONNX : il manipule des `AudioBuffer` et des
    `TranscribeOptions`.
- **Alternative rejetée** : `faster-whisper` (CTranslate2) — excellentes performances, mais impose un
  runtime Python (ou un sidecar Python), ce qui aggrave la taille, le démarrage, la surface
  d'attaque et la maintenance. Rejeté pour un produit grand public.

### ADR-04 — Capture audio : WASAPI en mode partagé via `cpal`, avec pré-roll

- **Décision** : capture WASAPI **mode partagé** via `cpal` (**0.18.2** vérifié), thread temps réel,
  tampon circulaire **pré-roll de 400 ms**, conversion immédiate en mono 16 kHz float32 avec
  ré-échantillonnage filtré (rubato ou implémentation équivalente, à figer), suivie d'un
  aiguillage VAD.
- **Justification** : le mode partagé n'exclusivise pas le micro (l'utilisateur garde ses autres
  applications) ; c'est le mode attendu par défaut dans Windows. Le pré-roll répond à RF-06 (ne pas
  tronquer le premier mot) : c'est un détail d'implémentation que plusieurs outils concurrents ont
  identifié comme critique **[FAIT]** (README de whisper-local). Le format natif du périphérique
  doit être traité comme inconnu (48 kHz stéréo flottant le plus souvent, mais aussi 16 kHz mono sur
  des micros Bluetooth en profil mains libres).
- **Conséquences** : le callback audio ne fait **aucune allocation** ; les données sont poussées dans
  une file sans verrou vers le thread de traitement ; les métriques (trames reçues, discontinuités)
  sont exposées pour les tests RF-07.
- **Points d'attention documentés** : changement de périphérique par défaut (suivi via
  `IMMNotificationClient` ou équivalent), casques Bluetooth (bascule en profil HFP → qualité
  dégradée à l'ouverture du micro, latence d'établissement), micro déjà utilisé en mode exclusif par
  une autre application.

### ADR-05 — Raccourcis globaux : `RegisterHotKey` d'abord, crochet clavier en option

- **Décision** : utiliser le plugin `tauri-plugin-global-shortcut` (**2.4.1** stable vérifié),
  adossé à la crate `global-hotkey` (**0.8.0**), qui repose sur `RegisterHotKey`. Détection de
  relâchement par scrutation `GetAsyncKeyState` toutes les 50 ms. Un mode « crochet clavier bas
  niveau » (`WH_KEYBOARD_LL`) est proposé en option pour les combinaisons impossibles à enregistrer,
  et désactivable par politique (voir risque T7 : ressemblance avec un enregistreur de frappe du
  point de vue des EDR).
- **Justification [FAIT]** : le code source de `global-hotkey` montre `RegisterHotKey` + une boucle de
  scrutation à 50 ms pour émettre l'événement « relâché » — d'où une **latence de relâchement allant
  jusqu'à ~50 ms**, acceptable pour le push-to-talk, et une consommation CPU corrigée en 0.8.0
  (l'absence de temporisation brûlait un cœur pendant toute la durée d'un appui). `RegisterHotKey`
  échoue si la combinaison est déjà enregistrée : c'est la base de RF-03.
- **Conséquences** : les combinaisons utilisant une touche morte du système (certains `Win+…`
  réservés) ne sont pas enregistrables — l'interface doit guider vers des choix viables. Un
  changement de raccourci implique désenregistrement + enregistrement à chaud (RF-01).

### ADR-06 — Insertion du texte : cascade explicite, mesurée, avec hygiène du presse-papiers

- **Décision** : quatre niveaux, dans cet ordre (détail RF-28) :
  1. **UI Automation `ValuePattern.SetValue`** — uniquement si le contrôle focalisé est simple,
     éditable, non sensible, et si l'on peut reconstruire le contenu (les contrôles multilignes
     n'exposent pas `ValuePattern`, et `TextPattern` **ne permet pas d'écrire** : contrainte de
     l'API Microsoft **[FAIT]**).
  2. **Presse-papiers + `Ctrl+V`** avec sauvegarde/restauration du contenu (texte et formats riches),
     et marquage du contenu pour l'**exclure de l'historique du presse-papiers Windows** quand c'est
     possible.
  3. **`SendInput` en Unicode**, caractère par caractère, sans toucher au presse-papiers.
  4. **Fenêtre de secours** native avec le texte pré-sélectionné.
- **Justification** : aucune méthode unique ne couvre Windows. Le presse-papiers est le plus
  universel mais le plus invasif et le plus fragile (applications lentes, contenu écrasé) ; c'est
  précisément pourquoi le concurrent leader publie des maintiens de 3 s et 5 s **[FAIT]**. La frappe
  Unicode est plus lente et se comporte mal avec certaines entrées (IME, jeux, terminaux avec
  rémanence) mais ne détruit rien. Le niveau 4 garantit qu'aucun texte n'est perdu.
- **Conséquences** : profil par application (RF-25) qui fixe l'ordre des niveaux et les temporisations ;
  journalisation locale du niveau utilisé (sans contenu) pour mesurer le taux de repli (H3) ;
  impossible de contourner l'isolation UIPI sans élever le processus, donc **les fenêtres élevées sont
  hors périmètre d'insertion** — limite documentée, identique chez toute la concurrence **[FAIT]**.

### ADR-07 — Stockage : séparation stricte configuration / secrets / contenu

- **Décision** :
  - **Configuration** : fichier JSON versionné (schéma explicite, migration à chaque version),
    lisible et exportable, dans `%APPDATA%\Plume\`.
  - **Secrets** (clés d'API des services distants, jeton d'activation) : **Windows Credential
    Manager** ou protection DPAPI par utilisateur (`CryptProtectData`) — jamais dans le JSON, jamais
    dans le stockage du webview (`localStorage`).
  - **Historique** (optionnel) : SQLite local, base chiffrée au niveau des champs sensibles ou du
    fichier, liée au compte Windows ; rétention configurable ; purge à la désinstallation.
  - **Modèles** : répertoire configurable (`%LOCALAPPDATA%\Plume\models\` par défaut), avec empreinte
    SHA-256 vérifiée à l'installation et au chargement.
- **Justification** : le stockage du webview est un vecteur d'exfiltration et de fuite (scripts,
  extensions), et un fichier JSON unique contenant à la fois secrets et contenu rend impossible toute
  promesse de confidentialité crédible. Les trois catégories ont des durées de vie et des niveaux de
  sensibilité différents.
- **Conséquences** : RF-33 et RF-34 deviennent testables (recherche de chaînes sentinelles dans
  l'ensemble des fichiers de données et journaux).

### ADR-08 — Licence locale signée, sans secret embarqué

- **Décision** : licences au format JSON signé **Ed25519**, vérifiées hors ligne avec une clé
  **publique** embarquée. Aucune clé privée dans le dépôt ni dans l'artefact. Période de grâce hors
  ligne de 30 jours après activation. Activation en ligne facultative pour la limitation du nombre
  de postes (RF-38).
- **Justification** : une clé privée embarquée est une faille (extractible et réutilisable pour
  signer n'importe quelle licence). La vérification hors ligne est nécessaire au positionnement
  « fonctionne sans réseau ».
- **Conséquences** : la révocation à distance n'est pas possible sans contact réseau — assumé, avec
  des lots de licences différents par campagne et une détection de partage anormal côté activation.

### ADR-09 — Mises à jour signées, canaux séparés

- **Décision** : `tauri-plugin-updater` avec signature minisign, canal `stable` et canal `beta`,
  manifeste hébergé sur infrastructure maîtrisée, **déploiement progressif** (10 % → 50 % → 100 %) et
  possibilité de rétrograder en republiant l'artefact précédent.
- **Justification** : exigence CRA (mises à jour de sécurité pendant la durée de support annoncée)
  **[FAIT]**, et nécessité de limiter le risque d'une mise à jour défectueuse sur une base installée.
- **Conséquences** : la vérification de signature est côté client ; la clé de signature privée doit
  être stockée hors du dépôt (secret CI) et sauvegardée ; une release sans artefact signé n'est pas
  distribuable.

### ADR-10 — Télémétrie : aucune par défaut

- **Décision** : pas de télémétrie activée par défaut. Un envoi **opt-in**, sans contenu (durées,
  modèle, succès/échec d'insertion, code d'erreur), avec identifiant anonyme rotatif. Les mesures de
  recette (latence, WER, repli d'insertion) sont d'abord collectées en interne.
- **Justification** : le positionnement V1 (confidentialité vérifiable) serait incohérent avec une
  télémétrie par défaut. Sans télémétrie, la qualité produit doit être pilotée par des mesures en
  recette et par les tests H3/H6 auprès d'utilisateurs volontaires.

### ADR-11 — Inférence in-process, avec isolation par tests, sidecar en option

- **Décision (MVP)** : le moteur d'inférence vit **dans le processus de l'application**, dans un
  thread dédié à priorité réduite, protégé par capture de panique et un chien de garde qui recharge le
  moteur (RF-21). L'interface `TranscriptionEngine` permet de déplacer l'implémentation dans un
  processus séparé plus tard sans changer les appelants.
- **Justification** : un sidecar ajoute une copie mémoire de l'audio, un protocole IPC, une gestion de
  cycle de vie et une latence supplémentaire — coût élevé pour un bénéfice d'isolation qui peut être
  obtenu par la capture de panique pour la majorité des pannes. Le risque résiduel (crash natif d'une
  bibliothèque C/C++) est réel et justifie le chien de garde et le journal de crash local.
- **Décision de réexamen** : si `sherpa-onnx` et `whisper-rs` cohabitent mal (conflits de symboles,
  conflits de runtime OpenMP), séparer les deux moteurs en deux sidecars, un par famille.

### ADR-12 — Modèle de threads et priorités

| Thread | Responsabilité | Règles |
|---|---|---|
| Thread principal (UI/Tauri) | Fenêtre, zone de notification, événements | Ne bloque jamais ; aucune attente d'inférence |
| Thread temps réel audio | Callback `cpal` | Aucune allocation, aucun verrou long, aucune journalisation |
| Thread pipeline | VAD, assemblage, appel moteur, post-traitement | Priorité sous la normale ; annulable |
| Thread injection | UIA/saisie/presse-papiers | Priorité normale ; doit pouvoir travailler quand le pipeline tourne |
| Thread maintenance | Téléchargement de modèles, mise à jour, purge | I/O en basse priorité |

---

## 4. Versions et licences vérifiées (17/10/2026)

| Composant | Version vérifiée | Licence | Usage | Vérification |
|---|---|---|---|---|
| `tauri` | **2.12.4** stable (3.0.0-alpha.5 en préversion) | Apache-2.0 / MIT | Shell applicatif | crates.io, 17/10/2026 |
| `tauri-plugin-global-shortcut` | **2.4.1** stable | Apache-2.0 / MIT | Raccourcis globaux | crates.io, 17/10/2026 |
| `global-hotkey` (crate sous-jacent) | **0.8.0** | Apache-2.0 / MIT | `RegisterHotKey` + relâchement | Dépôt tauri-apps, source lue |
| `whisper-rs` | **0.16.0** | Unlicense (bindings) | FFI whisper.cpp | crates.io, 17/10/2026 |
| whisper.cpp | MIT (branche principale) | MIT | Moteur ASR | Dépôt ggml-org, LICENSE |
| Modèles Whisper (ggml) | tiny/base/small/medium/large-v3-turbo | MIT (poids OpenAI) | Modèles ASR | Dépôt ggml-org |
| `parakeet-tdt-0.6b-v3` | 600 M paramètres | **CC-BY-4.0 → attribution obligatoire** | ASR européen (25 langues, dont l'allemand, l'espagnol, le français, l'italien, le portugais, le néerlandais…) | Fiche modèle NVIDIA |
| sherpa-onnx / bindings Rust | à figer à l'implémentation | Apache-2.0 (à confirmer) | Exécution ONNX de Parakeet | À vérifier avant P1 |
| `cpal` | **0.18.2** | Apache-2.0 (à confirmer) | Capture WASAPI | crates.io, 17/10/2026 |
| Ré-échantillonneur (`rubato` ou équivalent) | à figer | MIT / Apache-2.0 | 16 kHz mono | À vérifier avant P1 |
| VAD (Silero, modèle ONNX) | v5 | MIT (à confirmer) | Détection de voix | À vérifier avant P1 |
| SQLite (via `rusqlite`) | à figer | Domaine public (SQLite) / MIT (bindings) | Historique | À vérifier avant P1 |
| Cryptographie licence (`ed25519-dalek`) | à figer | BSD-3-Clause (à confirmer) | Vérification de licence | À vérifier avant P1 |
| Stockage de secrets (`keyring` ou `windows`) | à figer | MIT / Apache-2.0 | Credential Manager / DPAPI | À vérifier avant P1 |
| Windows on ARM (cible `aarch64-pc-windows-msvc`) | — | — | Support Snapdragon | À tester en P3 |

**Règle projet** : toute ligne marquée « à vérifier » doit être fermée (version + licence + lien
source) **avant** d'entrer dans une release distribuée. Une licence non vérifiée est un risque
juridique, pas un détail.

---

## 5. Contraintes Windows structurantes (faits vérifiés)

| Contrainte | Conséquence de conception | Source |
|---|---|---|
| `ValuePattern` ne s'applique pas aux contrôles multilignes ; `TextPattern` **ne permet pas** d'écrire | L'insertion générique par UI Automation est impossible : les niveaux 2 et 3 sont indispensables | Documentation Microsoft UI Automation |
| UIPI interdit à un processus non élevé d'envoyer des entrées à une fenêtre élevée | Les applications en mode administrateur sont hors périmètre ; le produit n'exigera jamais l'élévation | Documentation Microsoft |
| `RegisterHotKey` échoue si la combinaison appartient déjà à un autre processus | Gestion explicite du conflit (RF-03) | Documentation Microsoft + écosystème |
| Un crochet clavier global + injection d'entrées ressemble au comportement d'un enregistreur de frappe | Risque de blocage par EDR/antivirus en entreprise ; politique de signature, documentation et mode sans crochet | **[RECO]** (analyse de risque) |
| Les applications lentes à lire le presse-papiers (Office, messageries, bureau à distance) exigent de maintenir le contenu | Temporisations par profil d'application, restauration différée, avertissement | Guide du concurrent principal |
| WebView2 est un composant système sur Windows 11 et sur Windows 10 à jour | Installateur léger par défaut (téléchargement du bootstrapper) | Documentation Tauri |
| Le CRA impose des obligations de sécurité et de mise à jour sur les logiciels installés localement | Durée de support publiée, politique de vulnérabilités, mises à jour signées | Commission européenne |

---

## 6. Distribution, installation et mise à jour

| Sujet | Décision | Justification / coût |
|---|---|---|
| Formats | **NSIS (`.exe`)** par défaut + **MSI** pour l'entreprise | Le MSI est exigé dans beaucoup d'environnements gérés (GPO/Intune) |
| WebView2 | **Bootstrapper téléchargé** par défaut ; option « installateur hors ligne » (+ ~127 Mo) et « bootstrapper embarqué » (+ ~1,8 Mo) | Taille vs. autonomie ; à trancher selon la part de parc sans WebView2 **[FAIT : les trois modes existent côté Tauri]** |
| Architectures | x64 en priorité ; ARM64 conditionné au support de Parakeet/ONNX | Éviter d'annoncer un support non testé |
| Signature de code | **Obligatoire** (certificat OV/EV ou service de signature géré) | Sans signature : alertes SmartScreen, blocages EDR, refus en entreprise. Coût à vérifier au moment du lancement (revoir les tarifs 2026) |
| Boutique | Dépôt Microsoft Store envisagé en second canal (mise à jour gérée, découverte) | Contraintes de certification et rare facturation ; frais de compte développeur à vérifier |
| Mises à jour | Canal stable + canal beta, déploiement progressif, signature minisign | ADR-09 |
| Désinstallation | Suppression du binaire, des modèles (option), de l'historique et **des réglages** | RGPD : la désinstallation doit laisser un système propre |
| Base de registre | Usage minimal (`HKCU` uniquement) : démarrage automatique, association éventuelle | Pas de privilèges, désinstallation propre |
| Durée de support | Publiée par version majeure (ex. 24 mois de mises à jour fonctionnelles + sécurité) | Contrainte CRA et modèle économique (§`docs/07`) |

---

## 7. Ce que l'architecture refuse explicitement

| Refus | Raison |
|---|---|
| Appeler le cloud sans consentement, même en cas d'erreur locale | Détruirait la proposition de valeur V1 |
| Fonctionner en service Windows (session 0) | Les raccourcis globaux et l'insertion exigent la session interactive de l'utilisateur |
| Enregistrer l'audio par défaut | Minimisation des données ; l'audio n'est conservé que pour la récupération d'erreur, et détruit après |
| Stocker des secrets dans la configuration ou le webview | ADR-07 |
| Exiger l'élévation | C5 |
| Faire transiter audio ou texte par le pont IPC du webview | ADR-02 |
| Dépendre d'un GPU pour fonctionner | T10 ; le GPU est une accélération optionnelle détectée |
| Utiliser `faster-whisper`/Python | ADR-03 |
| Envoyer le contenu dans les journaux | RF-34 |

---

## 8. Points à revérifier avant chaque phase

1. **P1** : sherpa-onnx, rubato, Silero VAD, rusqlite, ed25519-dalek, keyring — versions et licences.
2. **P1** : comportement réel du plugin de raccourci sur Windows 10 21H2 (chemin `RegisterHotKey`).
3. **P2** : périmètre exact du CRA pour un éditeur solo (CE, documentation technique, gestion des
   vulnérabilités) — avis juridique recommandé.
4. **P2** : coût et procédure d'un certificat de signature (OV/EV ou service géré) et réputation
   SmartScreen.
5. **P3** : WER français réel des deux moteurs sur le corpus de recette (H4) — aucune décision de
   communication avant cette mesure.
6. **P3** : comportement sur Snapdragon X (ARM64) et sur un PC sans NPU ni GPU.
