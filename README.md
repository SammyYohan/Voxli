<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli Logo" />
</p>

# Voxli — dictée IA locale pour Windows

> **Voxli** est une application de dictée vocale assistée par IA 100 % locale, respectueuse de la vie privée, sans dépendance cloud et conçue pour Windows.

**Statut : P1 → P4 implémentées — en phase de test.** La conception (7 livrables) est
terminée ; les quatre phases de développement sont codées : réglages à chaud,
gestionnaire de modèles, VAD à trois niveaux, profils et dictionnaires,
historique chiffré, licences hors ligne, pastille d'état. Ce qui est **prouvé**
(compilé + tests verts) et ce qui reste **à valider sur le terrain** est listé
dans « 6. État du code » plus bas.

---

## 1. Les 7 livrables

| # | Livrable | Fichier | Statut |
|---|----------|---------|--------|
| 1 | Étude produit et concurrentielle | [`docs/01-etude-produit.md`](docs/01-etude-produit.md) | ✅ produite |
| 2 | Exigences fonctionnelles + tests d'acceptation | [`docs/02-exigences-fonctionnelles.md`](docs/02-exigences-fonctionnelles.md) | ✅ produite |
| 3 | Choix d'architecture justifiés | [`docs/03-architecture.md`](docs/03-architecture.md) | ✅ produite |
| 4 | Diagrammes et flux | [`docs/04-flux-et-diagrammes.md`](docs/04-flux-et-diagrammes.md) | ✅ produite |
| 5 | Structure des modules | [`docs/05-structure-modules.md`](docs/05-structure-modules.md) | ✅ produite |
| 6 | Plan de tests et critères de validation | [`docs/06-plan-de-tests.md`](docs/06-plan-de-tests.md) | ✅ produite |
| 7 | Plan de développement progressif (risques, dépendances) | [`docs/07-plan-de-developpement.md`](docs/07-plan-de-developpement.md) | ✅ produite |
| 8 | Audit interne du code (défs P1/P2, preuves, correctifs) | [`docs/08-audit-projet.md`](docs/08-audit-projet.md) | ✅ produite (17–18/10/2026) |
| 9 | Feuille de route post-audit (jalons M1–M5) | [`docs/09-feuille-de-route.md`](docs/09-feuille-de-route.md) | ✅ produite |
| 10 | Stratégie monétaire (fondée sur docs/01 §8) | [`docs/10-strategie-monetaire.md`](docs/10-strategie-monetaire.md) | ✅ produite |
| 11 | Checklist de release (dont procédure digests §3) | [`docs/11-checklist-release.md`](docs/11-checklist-release.md) | ✅ produite |
| — | Vie privée (grand public) | [`docs/vie-privee.md`](docs/vie-privee.md) | ✅ produite |
| — | Langues prises en charge | [`docs/langues-supportees.md`](docs/langues-supportees.md) | ✅ produite |
| — | Prototype vertical de bout en bout (code) | `crates/` + `apps/desktop/` | 🔨 **P1–P4 codés — en test** |

## 2. Comment lire ces documents

Trois niveaux d'information sont distingués partout, et étiquetés :

- **[FAIT]** — vérifié sur une source primaire (documentation éditeur, code source, API Microsoft,
  registre de paquets), avec URL et date de consultation.
- **[HYPOTHÈSE]** — croyance non testée sur l'utilisateur, le marché ou la performance. À valider
  par une expérience, jamais à présenter comme un acquis.
- **[RECO]** — recommandation d'ingénieur ou de produit, assumée comme un jugement.

Un quatrième niveau apparaît dans l'étude concurrentielle : **[SOURCE INTÉRESSÉE]** — information
provenant d'un concurrent ou d'un site d'affiliation, donc à confiance limitée.

## 3. Ce que ce dépôt ne promet pas

- Aucune performance chiffrée n'est une garantie : le plan de test (§6) décrit **comment mesurer**.
  Tout chiffre du type « temps d'insertion < 1 s » est une **cible à démontrer**, pas un résultat.
- Tauri n'est crédité d'aucun gain de mémoire ou de taille d'installation par défaut. La
  consommation dépend du modèle ASR, pas du shell applicatif ([FAIT], voir `docs/03`, ADR-02).
- Le prix, le taux de conversion et le modèle économique reposent sur des **hypothèses**
  explicitement listées (`docs/01`, §8), pas sur des données de marché validées.
- Le vocabulaire « 100 % local » est encadré : les modèles, les mises à jour et (optionnellement)
  les licences génèrent du trafic réseau. La formulation exacte et testable est dans `docs/02`
  (RF-35) et `docs/03` (ADR-07).

## 4. Décisions qui attendent le commanditaire

1. **Valider ou contester** les 3 propositions de valeur (`docs/01`, §6).
2. **Trancher le positionnement** : produit grand public à 30–60 € (achat unique) ou outil B2B
   conforme (abonnement par poste) — les deux impliquent des architectures commerciales différentes
   (`docs/07`, §5).
3. **Choisir le moteur par défaut** : Whisper `small`/`large-v3-turbo` (large couverture
   linguistique) ou Parakeet v3 (meilleur WER français annoncé, CPU-friendly, 25 langues
   européennes, licence CC-BY-4.0 avec attribution obligatoire) — voir `docs/03`, ADR-03.
4. **Nom du produit** : Confirmé sous le nom **Voxli** (workspace et 11 crates unifiés sous `voxli-*`).
5. **Autoriser le spike P0** (2 semaines) : c'est lui qui produit les seules mesures réellement
   décisives (latence, fiabilité d'insertion, RAM) avant d'engager 3 à 6 mois de développement.

## 5. Prochaine étape proposée

```
Validation des livrables
      ↓
P0 — spike vertical (2 semaines) : raccourci global → capture WASAPI → whisper.cpp → insertion
      ↓ décision go / no-go sur mesures
P1 — MVP interne (6–8 semaines) : réglages, gestionnaire de modèles, VAD, dictionnaire, historique
      ↓
P2 — confiance (4 semaines) : mode local vérifié, secrets, exclusions, tests, installeur signé
      ↓
P3 — différenciation : post-traitement contextuel, profils par application, aperçu en streaming
      ↓
P4 — commercialisation : licences, mises à jour, support, Store
```

Détail, critères de sortie et risques : [`docs/07-plan-de-developpement.md`](docs/07-plan-de-developpement.md).

## 6. État du code, P1–P4

### 6.1 Arborescence

```
crates/                        11 modules, toit Cargo workspace
├── voxli-core                 noyau pur : état, orchestrateur, types partagés
├── voxli-config               %APPDATA%\Voxli\config.json (schéma v4, migrations)
├── voxli-diagnostics          journal sans contenu (RF-34), rapport de support
├── voxli-license              licences Ed25519 hors ligne (grâce incluse)
├── voxli-storage              historique : mémoire (défaut) / SQLite chiffré DPAPI (opt-in)
├── voxli-secure               secrets : DPAPI (opt-in), jamais dans le JSON
├── voxli-hotkeys              politique des raccourcis (validation, réservées)
├── voxli-audio                mono 16 kHz f32, pré-roll 400 ms, VAD énergie à
│                              trois niveaux, liste des micros (cpal 0.18)
├── voxli-asr                  moteur factice ; whisper.cpp, Nemotron 3.5 et Parakeet ONNX
│                              (opt-in) ; transcription distante BYOK (opt-in)
├── voxli-win                  tout le `unsafe` Win32 : presse-papiers, UIA,
│                              SendInput, fenêtre active, liaison NeMo
└── voxli-text                 plan d'insertion (pur) + exécution cascade +
                               règles françaises RF-26 + dictionnaire RF-23

apps/desktop                   le seul binaire : fenêtre réglages + fenêtre
                               secours + pastille d'état, tray, raccourcis
                               globaux, gestionnaire de modèles, licence
    src-tauri/                 orchestrateur câblé, thread pipeline
    src/                       TypeScript strict (vite multi-pages : réglages,
                               secours, pastille)
```

### 6.2 Conformité à la conception

- Invariant 1 (docs/04 §3) : le texte transcrit est dans le tampon **avant**
  toute tentative d'insertion et n'est jamais perdu — testé par
  `crates/voxli-core/tests/flow.rs` (13 tests d'état, y compris panne de
  moteur → réessayer, échec d'insertion → secours, fuite de contenu).
- Invariant 2 : une seule dictée à la fois ; option **refus explicite** de RF-05
  (`RejectedBusy`) **et** file d'attente de 1 (RF-05 b, `QueuedAfterCurrent`).
- Invariant 3 : l'audio est libéré après insertion, refus ou annulation.
- Aucun texte ne passe par le pont IPC sur le chemin critique : les seules
  exceptions **documentées dans `docs/05 §3`** sont la fenêtre de secours
  (RF-31) et la lecture d'historique choisie par l'utilisateur (RF-33).
- Le resampler est maison (sinc fenêtré, 24 taps par rive) —
  cf. `voxli-audio/convert.rs`, anti-repliement testé.
- RF-16 : la détection automatique de langue est refusée pour les énoncés
  courts (< 1,5 s) — repli sur la langue configurée.
- RF-22 : le texte transcrit est conservé dans le tampon du noyau même après
  un rechargement de configuration ou une perte de micro ; « coller à
  nouveau » reste fonctionnel (`Orchestrator::restore_buffer`).

### 6.3 Ce qui est prouvé (compilé + tests verts, au 17-18/10/2026)

Tout le workspace passe (`cargo test --workspace`, exit 0) : **22 tests noyau,
11 config (schéma 4 + migrations 3→4), 16 post-traitement FR, 9 audio
(cpal 0.18)**, les suites storage/secure/licence/hotkeys/ASR/diagnostics
toutes vertes ; `cargo check -p voxli-desktop` vert **sans avertissement**,
ainsi qu'avec la feature `history-disk` (SQLite chiffré DPAPI) et
`cargo test -p voxli-asr --features remote` (6 tests). L'application de
développement démarre et consigne : `license-free` → `history-memory` →
`capture-opened:…` → `engine_loaded {"engine":"fake"}` ; les raccourcis
`Shift+Alt+Z/X` s'enregistrent et un conflit possible sur `Ctrl+Alt+Espace`
est signalé sans paniquer (RF-03). Interface : `tsc --noEmit` et `vite build`
verts (3 fenêtres : réglages, secours, pastille). **Moteur whisper.cpp réel**
(aussi au 17/10/2026) : `whisper-rs 0.16` compilé (libclang fourni par LLVM
23.1.3 via winget, CMake embarqué de VS) ; `cargo test -p voxli-asr
--features whisper` vert et `cargo build -p voxli-desktop --features
custom-protocol,real-asr` vert — la transcription voix réelle est prête, les
mesures de latence et de précision restent à produire (§6.4).
**Liaison NeMo-Speech.cpp (au 18/10/2026)** : `cargo test -p voxli-win
--features nemo` — la DLL 0.2.0 charge et répond (`nemo-speech-asr 0.2.0`) ;
`cargo test -p voxli-asr --features nemo-stream` — le GGUF réel
(`nemotron-3.5-asr-streaming-0.6b.q8_0.gguf`, 742 090 464 octets,
téléchargé) charge et l'inférence offline décode une seconde de silence,
puis le cycle streaming complet (ouverture → 4 pushes → drain → finish)
tourne sur le vrai modèle (5/5 verts) ; `vite build` + pastille partiels
(`dictation://partial`) écrite et thémée.

### 6.4 Ce qui reste à valider (écrit mais non prouvé)

- **Mesures réelles de performance et de précision** (docs/06) : le moteur
  whisper.cpp compile désormais, mais **aucun chiffre** de latence (p95),
  WER ou RAM n'est produit tant qu'une voix réelle n'a pas dicté.
- **Feature `parakeet-asr`** : sherpa-onnx (C++/CMake) jamais compilé ici ;
  code écrit d'après l'API vérifiée de la documentation.
- **Feature `nemo-stream` (P2 streaming)** : liaison SDK compilée, chaîne
  d'inférence offline et cycle streaming **testés sur le GGUF réel** (§6.3).
  Première vraie dictée (18/10/2026) : insertion réussie, mais le
  re-décodage offline coûtait 10,7 s pour 11,3 s dictées — la voie rapide
  (final du streaming réutilisé, amendement 2 ADR-02) est câblée ; reste
  à re-mesurer la latence après relâchement, la latence α du premier
  partiel, et le décodage d'un énoncé parlé via le flux (pas du silence).
- **Moteurs distants (P4)** : transcription BYOK compilée en opt-in (`remote`)
  et codée, mais **délibérément non branchée** au flux de raccourci — aucun
  envoi tant que le consentement RF-36 et l'intégration P4 ne sont pas tranchés.
- **Recette terrain** : matrice 12 applications, mesures RAM, T-01
  (raccourcis à chaud), T-02 (modes de déclenchement) — nécessite un humain,
  scripts prêts (§6.7).

### 6.5 Démarrage rapide (développement)

```powershell
# prérequis : Rust (rustup), Node 22+, pnpm — vérifier :
powershell -ExecutionPolicy Bypass -File scripts/check-env.ps1

# icônes de bundle (une seule fois ; archivées dans le dépôt)
powershell -ExecutionPolicy Bypass -File scripts/make-icons.ps1

# interface
pnpm --dir apps/desktop install
pnpm --dir apps/desktop build          # ../dist pour `frontendDist`

# tests des modules (sans microphone ni modèle)
cargo test -p voxli-core -p voxli-config -p voxli-license -p voxli-storage `
  -p voxli-diagnostics -p voxli-audio -p voxli-hotkeys -p voxli-asr `
  -p voxli-secure -p voxli-text -p voxli-win

# application complète — la feature `custom-protocol` embarque l'interface
# (`../dist`) dans le binaire : SANS elle, un simple `cargo run` reste en
# « mode dev » (devUrl localhost:5173) et chaque fenêtre webview s'ouvre
# vide faute de serveur Vite.
cargo run -p voxli-desktop --features custom-protocol
# variante développement de l'interface uniquement :
#   `pnpm --dir apps/desktop dev` + `cargo run -p voxli-desktop`
#   (aucune feature, hot reload — sinon fenêtres vides)
```

Le tray apparaît (« Voxli — dictée locale ») : clic gauche ouvre/ferme les
réglages ; `Ctrl+Alt+Espace` (appui maintenu) dicte ; `Shift+Alt+Z` recolle le
dernier texte ; `Shift+Alt+X` affiche la fenêtre de secours.

### 6.6 Moteurs réels (opt-in, toujours hors ligne)

Prérequis toolchain (coût unique, éprouvé le 17/10/2026) : `whisper-rs-sys`
(bindgen) exige **libclang**, et whisper.cpp se construit avec **CMake**.
Chemin éprouvé : LLVM via winget + le CMake embarqué de Visual Studio
(la variante « composant VS » reste possible en secours) :

```powershell
winget install --id LLVM.LLVM -e --accept-source-agreements --accept-package-agreements
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:PATH += ";C:\Program Files\Microsoft Visual Studio\18\Community\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin"

cargo test -p voxli-asr --features whisper                 # 2/2 verts
cargo build -p voxli-desktop --features custom-protocol,real-asr
```

```powershell
# Variante de secours (admin, ~1 Go) :
#   VS Installer → Modify → « C++ Clang Compiler for Windows (with libclang) »
#   (composant Microsoft.VisualStudio.Component.VC.Llvm.ClangWithLibclang)
$env:LIBCLANG_PATH = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\Llvm\x64\bin"
```

```powershell
# Historique disque chiffré + secrets Windows :
cargo test -p voxli-storage --features secure-disk   # SQLite + DPAPI (RF-33)
cargo test -p voxli-secure --features dpapi          # secrets Windows

# Modèles : le gestionnaire intégré les télécharge depuis la fenêtre
# Réglages → « Modèles » (RF-15) — empreinte SHA-256 consignée ; à la fin
# du téléchargement, le moteur reconstruit sans action supplémentaire
# (« model-installed-rebuild » au journal) ni redémarrage.
# Sans l'interface, le script reste valable :
powershell -ExecutionPolicy Bypass -File scripts/fetch-model.ps1   # modèle ggml
```

#### Accélération GPU optionnelle (Vulkan — Intel/AMD/NVIDIA)

Le build **nominal reste CPU-only** : le GPU est une opportunité (machine
qui en possède), jamais une contrainte. Prérequis au build : le **SDK
Vulkan** (l'outil `glslc` compile les shaders de ggml) ; à l'exécution,
le pilote de la carte doit exposer `vulkan-1.dll` (natif partout).

```powershell
winget install --id LunarG.VulkanSDK -e --accept-source-agreements
$env:Path += ";C:\VulkanSDK\<version>\Bin"        # glslc accessible

# Répertoire cible COURT obligatoire : la sous-compilation des shaders
# Vulkan (vulkan-shaders-gen) déclenche une erreur MSBuild MSB3501
# « chemin introuvable » au-delà de ~240 caractères (limite Windows
# MAX_PATH sur les tlogs MSBuild) — un cible-court contourne net.
$env:CARGO_TARGET_DIR = "C:\t"                    # 180 caractères de moins

cargo test -p voxli-asr --features whisper-gpu    # liaison Vulkan venue
cargo build -p voxli-desktop --features "custom-protocol,gpu"
```

Le choix MOTEUR/MACHINE est laissé à l'utilisateur dans Réglages →
Voix → « Calcul des transcriptions » : **Auto** (défaut du compilé —
GPU quand un backend est embarqué et utilisable, repli CPU interne de
ggml à l'échec d'initialisation), **CPU** (refus explicite — `use_gpu =
false`, même sur un build GPU), **GPU** (attendu, l'écran montre
l'état), **Duo** (deux contextes du même poids : les partiels d'affichage
passent sur GPU *demandé*, le texte final reste sur CPU — repli
transparent sur un seul contexte si le second refuse, journal explicite ;
RF-22/RF-34). Chaque montage de moteur consigne `calcul:<choix>` au journal ;
les latences réelles (`latency.stage=inference`) restent la preuve de
l'usage (jamais de promesse non mesurée, RF-29). Variable
d'environnement `VOXLI_COMPUTE=auto|cpu|gpu` pour tester sans changer la
config (prioritaire sur elle).

#### Partiels « vivants » (fenêtre adaptative — texte affiché pendant la prise)

La pastille écrit le texte pendant la prise (`dictation://partial`), via un
re-décodage fenêtré borné en coût — **la fenêtre s'adapte à la machine
mesurée**, sans réglage ni variable d'environnement : départ 2 s,
convergence vers ~1,5 s de décodage par tick, plancher 0,6 s, plafond 8 s
(machine rapide = prise couverte en entier). Une machine trop lente pour
tenir même le plancher coupe les partiels *en douceur* pour le reste de la
session (comportement batch, journal explicite). Réglage avancé : valeur
de départ via `VOXLI_PARTIAL_WINDOW_SEC` ; la mesure seule décide
(`latency.stage=inference` + `partiels-coupés:…` au journal).

#### Raccourcis : capture à l'appui

Dans Réglages → Raccourcis, un champ cliqué écoute le clavier : on appuie
sur les touches, la combinaison s'écrit (Ctrl/Alt/Shift/Win + lettre,
chiffre, F1–F24, Espace, Tab, Entrée, `). Une touche hors liste blanche
est expliquée (jamais muette), un doublon entre deux actions est marqué
**« occupé »** en rouge et *n'est pas enregistré* — la même règle que le
noyau (`validate_before_register`). Échap annule. Note : la combinaison
d'un raccourci *actif* reste interceptée par le système tant qu'il est
enregistré — la capture sert à choisir une combinaison libre.

#### Catalogue élargi + sélection à la ligne (style Handy)

La zone de téléchargement propose désormais, hors whisper *base* :
**tiny** (43 Mo, machines modestes), **small** (190 Mo), **medium**
(539 Mo) et **large-v3-turbo** (574 Mo, précision maximale, GPU conseillé)
— toujours ggerganov/whisper.cpp, licence affichée avant tout
téléchargement (P1-3). Les tailles affichées sont les tailles réseau
réelles (Content-Length mesuré le 21/10/2026, cohérent avec le champ
`size_mb` du catalogue). La **sélection se fait par LIGNE** : clic =
activation (ou téléchargement si absente), ligne « Automatique
(recommandé) » en tête ; le moteur reconstruit réellement son contexte
quand la voix change (comparaison label **+ fichier modèle** dans
`engine_stale`). Les radios d'engines ont été supprimées (surface de
sélection unique, plus aucune zone grisée). Nemotron/Parakeet restent
listés, téléchargeables, mais **non activables** tant que leur SDK n'est
pas compilé : la ligne l'explique sur elle-même (avis visible, jamais
l'ancien repli factice silencieux).

#### Nemotron 3.5 streaming (texte en direct dans la pastille)

Moteur **Nemotron 3.5 ASR streaming 0.6B** (GGUF q8_0, ≈ 742 Mo, 40 langues,
licence OpenMDW-1.1) via le SDK C **NeMo-Speech.cpp** in-process — le même
binary qu'une install locale de l'outil de NVIDIA, sans serveur local ni
réseau au runtime. Le SDK est **opt-in** :

```powershell
# 1) installer le runtime (installer officiel NeMoSpeech, windows x86_64
#    CPU) — emplacement par défaut %LOCALAPPDATA%\Programs\NeMoSpeech ;
#    autre emplacement → variable d'env NEMO_SPEECH_PREFIX (build.rs).
# 2) compiler + tester la liaison (build.rs lie la lib et copie les DLL
#    du SDK à côté de l'exécutable — aucun PATH à bricoler) :
cargo test -p voxli-win --features nemo          # version du SDK au journal
cargo test -p voxli-asr --features nemo-stream   # façade sûre
cargo build -p voxli-desktop `
  --features custom-protocol,real-asr,nemo-stream
# 3) modèle : Réglages → Modèles → « Nemotron 3.5 continu » (ou placer le
#    GGUF dans %APPDATA%\Voxli\models\).
```

Priorité `auto` avec nemo-stream compilé ET modèle présent : **nemo** →
whisper.cpp → Parakeet → factice (commentaire config, docs/05 §4). Sans le modèle ni la
feature, aucun changement de comportement (build par défaut vert, sans
SDK). Pendant la prise, la pastille affiche le texte en direct ; au
relâchement, la transcription vient des segments finalisés du flux
(voie rapide, aucun re-décodage — les deux routes sont au journal) et la
voie batch (tampon complet, pré-roll inclus) reste l'autorité de secours
(amendements ADR-02 dans docs/03). **Pastille v2 (RF-09)** : indicateur
permanent en haut au centre — au repos un point discret (raccourci armé,
capture non active), pendant la prise pill complète (point rouge battu,
vumètre, texte en direct, curseur clignotant) ; clic traversant, jamais
de focus. Correctif bogue notoire : la fenêtre `pill` était absente de la
surface de permissions Tauri (« capabilities ») — son webview ne recevait
ni événement ni commande ; elle est désormais listée (`core:default`).
Survol (côté noyau, clic traversant) : mini pop-up sous la pilule qui
montre le raccourci de dictée configuré, relu à chaque survol — le
survol est un rayon de 15 px autour du point (hystérésis de sortie à
20 px : pas de clignotement au bord). Adaptation
d'écran : position recalculée au moniteur principal ~2,4 s (largeur bornée
à 85 %, échelle DPI convertie) et « toujours au premier plan » réassuré
par ré-épinglage topmost ~2×/s (dégrader puis replanter : un simple
re-sert est un no-op quand la fenêtre est déjà dans la bande) ; plein
écran exclusif (jeux DirectX) recouvre n'importe quel `HWND_TOPMOST`
— limite système.

Sans `real-asr`/`parakeet-asr` (défaut), le moteur factice dicte un énoncé
reproductible — le pipeline complet reste exerçable en CI sans modèle ni
micro. Les deux features de moteur sont **mutuellement exclusives à
l'exécution** : seul le moteur compilé ET dont le modèle est présent est
retenu, jamais de repli silencieux (consigné sinon).

### 6.7 Limites assumées à ce stade

- Le VAD est un seuil d'énergie simple à trois niveaux d'agressivité
  (Silero reste une option future).
- Le post-traitement distant (RF-26 niveau 3) est codé mais non branché —
  consentement RF-36 et garde-fous en place, décision produit en attente.
- La transcription distante (BYOK) est compilable (`remote`) et testée
  unitairement, mais rien du flux de dictée ne l'appelle : branchement P4
  volontairement laissé hors ligne.
- Recette sur applications réelles (matrice 12 apps) à exécuter : scripts
  prêts, exécution manuelle requise.

### 6.8 Recette

- Matrice d'insertion : [`tests/e2e/matrice-12-apps.md`](tests/e2e/matrice-12-apps.md)
- Bancs M1/M2 : [`benchmarks/README.md`](benchmarks/README.md)
- Preuve réseau (RF-36) : [`scripts/prove-no-network.ps1`](scripts/prove-no-network.ps1)

## 7. Licences et tierces parties

- **Code du dépôt** : licence à fixer avant toute distribution (décision
  n°4 : `docs/01`).
- **Dépendances logicielles** : listées dans le `Cargo.toml` de chaque module
  et dans `apps/desktop/package.json` ; la liste complète avec textes de
  licence est à générer (`cargo license` / pnpm) avant mise en paquet.
- **Modèles de reconnaissance téléchargeables** (RF-20, attribution
  obligatoire pour certains) : voir [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).
  En pratique : l'interface affiche la provenance et la licence de chaque
  modèle avant tout téléchargement, et l'export de données inclut ce fichier.
- **Support** : [`SUPPORT.md`](SUPPORT.md) — où trouver le journal, ce que
  contient un rapport de diagnostic, comment le lire avant envoi.

---

## 8. Audit interne du code (17–18/10/2026)

Un audit complet a été exécuté sur le dépôt (46 fichiers Rust, interface,
configuration, CI, script de modèle). Définitions des défauts P1-1…7 et
P2-1…4, localisation précise, et correctifs appliqués :
[`docs/08-audit-projet.md`](docs/08-audit-projet.md). Résumé honnête :

- Tous les écarts graves connus (empreintes, verrou de téléchargement,
  extraction d'archive, allocations temps-réel, file RF-05, schéma de
  langue, warm-up ASR) ont reçu une **correction écrite** — marquée
  « écrit, non compilé » tant que la bench `docs/11` §1 n'a pas tourné.
- Deux attentes MAINTENEUR restent humaines : les **digests de
  confiance** des modèles (procédure docs/11 §3) et les **décisions
  commanditaire** (licence, dons).

## 9. Feuille de route

Jalons M1 (correctifs écrits) → M2 (validation mainteneur) →
M3 (pistes non-Windows) → M4 (monétisation au commanditaire) →
M5 (publication) : [`docs/09-feuille-de-route.md`](docs/09-feuille-de-route.md).

## 10. Monétisation

Fondée sur les hypothèses de docs/01 §8, sans en inventer de nouvelles :
[`docs/10-strategie-monetaire.md`](docs/10-strategie-monetaire.md).
**Pas de FUNDING.yml** tant que les destinations ne sont pas vérifiées.

## 11. Release

La procédure actionnable (tests à EXÉCUTER, procédure des digests,
paquet, communication) :
[`docs/11-checklist-release.md`](docs/11-checklist-release.md).

## 12. Multilingue & documents orientés utilisateurs

- Langues du produit (Whisper 95 + auto, Parakeet 25, Nemotron 40 ;
  interface en 15 langues dont détection automatique du système) :
  [`docs/langues-supportees.md`](docs/langues-supportees.md).
- La vie privée dans Voxli, écrit pour l'utilisateur :
  [`docs/vie-privee.md`](docs/vie-privee.md).
- Signaler une faille : [`SECURITY.md`](SECURITY.md). Contribuer :
  [`CONTRIBUTING.md`](CONTRIBUTING.md). Conduite :
  [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md). Historique des versions :
  [`CHANGELOG.md`](CHANGELOG.md).
