# Plume — dictée IA locale pour Windows

> **Nom de code provisoire.** `Plume` est un nom de travail, choisi pour les besoins du dépôt et
> renommable en une commande. Aucune marque n'a été déposée ni vérifiée à ce stade.

**Statut : spike P0 échafaudée.** La conception (7 livrables) est terminée ; la
spike verticale P0 est implémentée : capture WASAPI en continu → VAD →
transcription (moteur factice par défaut, whisper.cpp en option) → cascade
d'insertion → fenêtre de secours. Voir « 6. État du code, P0 » plus bas.

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
| — | Prototype vertical de bout en bout (code) | `crates/` + `apps/desktop/` | 🔨 **P0 échafaudé** |

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
4. **Confirmer le nom du produit** (`Plume` n'est qu'un nom de travail).
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

## 6. État du code, P0

### 6.1 Arborescence

```
crates/                        11 modules, toit Cargo workspace
├── plume-core                 noyau pur : état, orchestrateur, types partagés
├── plume-config               %APPDATA%\Plume\config.json (schéma v3, migrations)
├── plume-diagnostics          journal sans contenu (RF-34), rapport de support
├── plume-license              licences Ed25519 hors ligne (grâce incluse)
├── plume-storage              historique : mémoire (défaut) / SQLite (opt-in)
├── plume-secure               secrets : DPAPI (opt-in), jamais dans le JSON
├── plume-hotkeys              politique des raccourcis (validation, réservées)
├── plume-audio                mono 16 kHz f32, pré-roll 400 ms, VAD énergie
├── plume-asr                  moteur factice ; whisper.cpp (opt-in)
├── plume-win                  tout le `unsafe` Win32 : presse-papiers, UIA,
│                              SendInput, fenêtre active
└── plume-text                 plan d'insertion (pur) + exécution cascade

apps/desktop                   le seul binaire : fenêtre réglages + fenêtre secours
    src-tauri/                 orchestrateur câblé, tray, raccourcis globaux
    src/                       TypeScript strict (vite multi-pages)
```

### 6.2 Conformité à la conception

- Invariant 1 (docs/04 §3) : le texte transcrit est dans le tampon **avant**
  toute tentative d'insertion et n'est jamais perdu — testé par
  `crates/plume-core/tests/flow.rs` (13 tests d'état, y compris panne de
  moteur → réessayer, échec d'insertion → secours, fuite de contenu).
- Invariant 2 : une seule dictée à la fois. P0 applique l'option **refus
  explicite** de RF-05 (`RejectedBusy`, indication visible) ; la file d'attente
  de 1 arrive en V1 sur le même état `Transcribing { queued }`.
- Invariant 3 : l'audio est libéré après insertion, refus ou annulation.
- Aucun texte ne passe par le pont IPC sur le chemin critique : la seule
  exception est la fenêtre de secours (RF-31), texte issu du tampon interne.
- Le resampler est maison (sinc fenêtré, 24 taps par rive) —
  cf. `plume-audio/convert.rs`, anti-repliement testé.

### 6.3 Démarrage rapide (développement)

```powershell
# prérequis : Rust (rustup), Node 22+, pnpm — vérifier :
powershell -ExecutionPolicy Bypass -File scripts/check-env.ps1

# icônes de bundle (une seule fois ; archivées dans le dépôt)
powershell -ExecutionPolicy Bypass -File scripts/make-icons.ps1

# interface
pnpm --dir apps/desktop install
pnpm --dir apps/desktop build          # ../dist pour `frontendDist`

# tests des modules (sans microphone ni modèle)
cargo test -p plume-core -p plume-config -p plume-license -p plume-storage `
  -p plume-diagnostics -p plume-audio -p plume-hotkeys -p plume-asr `
  -p plume-secure -p plume-text -p plume-win

# application complète (interface déjà construite dans ../dist)
cargo run -p plume-desktop
# ou, pour itérer sur l'interface : `pnpm --dir apps/desktop dev` dans un
# second terminal, puis `cargo run -p plume-desktop` (devUrl : localhost:5173)
```

Le tray apparaît (« Plume — dictée locale ») : clic gauche ouvre/ferme les
réglages ; `Ctrl+Alt+Espace` (appui maintenu) dicte ; `Shift+Alt+Z` recolle le
dernier texte ; `Shift+Alt+X` affiche la fenêtre de secours.

### 6.4 Moteur réel (opt-in, toujours hors ligne)

Prérequis toolchain (coût unique, vérifié le 17/10/2026) : `whisper-rs-sys`
(bindgen) exige **libclang** — absent de votre VS Community 18 par défaut. Il
faut le composant « C++ Clang Compiler for Windows (with libclang) »,
puis redéfinir `LIBCLANG_PATH` vers son dossier `bin` :

```powershell
# Installer le composant VS (admin, ~1 Go) :
#   VS Installer → Modify → « C++ Clang Compiler for Windows (with libclang) »
#   (composant Microsoft.VisualStudio.Component.VC.Llvm.ClangWithLibclang)
$env:LIBCLANG_PATH = "C:\Program Files\Microsoft Visual Studio\18\Community\VC\Tools\Llvm\x64\bin"

cargo test -p plume-storage --features sqlite         # historique disque
cargo test -p plume-secure --features dpapi           # secrets Windows

powershell -ExecutionPolicy Bypass -File scripts/fetch-model.ps1   # modèle ggml
cargo build -p plume-desktop --release --features real-asr
```

Sans `real-asr` (défaut), le moteur factice dicte un énoncé reproductible —
le pipeline complet reste exerçable en CI sans modèle ni micro.

### 6.5 Limites assumées en P0

- Le VAD est un seuil d'énergie simple (Silero arrive en P1).
- Le post-traitement est identité (niveau 0) ; règles de ponctuation en V1.
- Les réglages modifiés depuis l'interface (pré-roll, durée max) ne sont lus
  qu'au démarrage ; les raccourcis ne sont pas modifiables depuis l'UI (P0).
- La langue du modèle et la sélection de périphérique audio ne sont pas
  encore exposées (`audio.device` lu en V1).
- La fenêtre de secours affiche le dernier texte uniquement en mémoire.

### 6.6 Recette

- Matrice d'insertion : [`tests/e2e/matrice-12-apps.md`](tests/e2e/matrice-12-apps.md)
- Bancs M1/M2 : [`benchmarks/README.md`](benchmarks/README.md)
- Preuve réseau (RF-36) : [`scripts/prove-no-network.ps1`](scripts/prove-no-network.ps1)

## 7. Licence

À fixer avant toute distribution (décision n°4 : `docs/01`, réutilisation du
nom ; ainsi que le choix commercial). Les dépendances utilisées sont listées
dans `Cargo.toml` de chaque module.
