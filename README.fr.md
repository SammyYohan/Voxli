<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli" />
</p>

<h1 align="center">Voxli — dictée IA locale pour Windows</h1>

<div align="center">

[English](README.md) · **Français** · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dictez partout sous Windows — un raccourci, la voix, le texte apparaît.
> **100 % local** : pas de compte, pas de cloud, pas de télémétrie. L'audio est
> transcrit par un moteur que vous possédez (whisper.cpp ou Nemotron 3.5
> continu), inséré dans le champ actif, et le presse-papiers est remis
> comme vous l'aviez laissé.

---

## Ce que vous obtenez

| | |
|---|---|
| **Texte en direct** | La pastille flotte en haut de l'écran et écrit *pendant que vous parlez* (Nemotron 3.5 continu), point discret au repos. Non focalisable, transparente aux clics. |
| **Moteurs hors ligne** | **whisper.cpp** (95+ langues, CPU ou GPU Vulkan) et **Nemotron 3.5 ASR streaming 0.6B** (40 langues, texte en direct). Les deux tournent in-process ; l'installateur embarque les DLL d'exécution nécessaires. |
| **Calcul honnête** | Réglages → *Calcul des transcriptions* : **Auto** choisit le GPU quand un accélérateur est disponible, **CPU** le refuse explicitement, **Duo** écrit le texte en direct sur le GPU et garde le final sur CPU. Ce qui est écrit est ce qui tourne : chaque montage consigne `calcul:<choix>` et le journal garde les latences réelles. |
| **Rien ne se perd** | Le texte transcrit vit dans le tampon du noyau *avant* toute insertion ; si l'application refuse, la fenêtre de secours le reçoit. La sauvegarde/restauration du presse-papiers (RF-30) remet ce que vous aviez copié. |
| **15 langues d'interface** | Détectées automatiquement, ou choisies dans Réglages → *Langue de l'interface* : fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Historique, dictionnaires, profils** | Historique chiffré (défaut en mémoire ; disque chiffré en option), dictionnaires de dictée pour noms et jargon, vocabulaires par application. |
| **Licence** | Gratuit à l'usage, vérification de licence hors ligne (Ed25519, aucun contrôle en ligne requis). |

## Installation (utilisateur)

1. Téléchargez `Voxli_0.1.0_x64-setup.exe` (installateur NSIS en un seul
   fichier) depuis la page **Releases** de ce dépôt.
2. Lancez-le : fichiers dans `%LOCALAPPDATA%\Voxli` (installation par
   utilisateur, WebView2 installé automatiquement s'il manque,
   désinstalleur fourni).
3. Premier démarrage : choisissez votre moteur dans Réglages →
   **Modèles** — *Whisper base (57 Mo)* se télécharge en un clic — puis
   maintenez **Ctrl+Alt+Espace** et parlez. `Shift+Alt+Z` recolle le dernier
   texte, `Shift+Alt+X` ouvre la fenêtre de secours.

**Note SmartScreen (honnêteté) :** l'installateur n'est pas signé de code
(certificat à venir) — Windows alerte au premier lancement, clic
*Plus d'infos → Exécuter quand même*. Tout ce que fait Voxli *ensuite* est
vérifiable ici, en ouvert.

**Le téléchargement est votre action explicite.** Les modèles ne partent
que de Réglages → Modèles, avec licence et taille affichées *avant* tout
transfert, et l'empreinte SHA-256 du fichier reçu est consignée dans
`%APPDATA%\Voxli\models\manifests.json`.

## Catalogue de modèles (épuré exprès)

| Modèle | Rôle | Taille | Licence |
|---|---|---|---|
| **Whisper base** (quantisé) | Défaut multilingue rapide, premier téléchargement idéal | 57 Mo | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (quantisé) | Précision maximale, GPU conseillé | 574 Mo | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Texte en direct, 40 langues | 742 Mo | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Catalogue épuré le 22/10 : les échelons Whisper faibles/redondants
(tiny/small/medium) et Parakeet (sherpa-onnx non compilé dans le build
distribué) n'encombrent plus l'écran de téléchargement. Les fichiers laissés
par des versions antérieures continuent de fonctionner via **Auto** — jamais
ignorés. Attributions complètes : [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Vie privée, en un paragraphe

Le pipeline (capture → VAD → inférence → insertion) ne quitte jamais votre
machine. Le seul trafic réseau de Voxli est le *téléchargement de modèles
que vous déclenchez* et — variante opt-in — la vérification de licence ; le
dépôt fournit `scripts/prove-no-network.ps1` pour le vérifier. Aucun secret
dans `config.json` (Credential Manager / DPAPI). Détails :
[`docs/vie-privee.md`](docs/vie-privee.md).

## Langues de dictée

- **Whisper** : 95+ langues — la liste déroulante de Réglages EST la
  liste autoritaire ; `auto` (défaut) détecte avec un garde-fou : un énoncé
  plus court que 1,5 s retombe sur la langue configurée au lieu de deviner.
- **Nemotron 3.5** : 40 langues (liste constructeur — jamais inventée ici,
  fiche modèle liée depuis [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- La **langue d'interface** (réglage distinct) couvre les 15 langues
  ci-dessus et ne change jamais la reconnaissance.

## Construire depuis les sources

Prérequis : Rust (rustup) + Node 22+ + pnpm. Windows uniquement.

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-env.ps1

# interface → ../dist (embarquée par `custom-protocol`)
pnpm --dir apps/desktop install
pnpm --dir apps/desktop build

# tests unitaires (ni micro, ni modèle)
cargo test -p voxli-core -p voxli-config -p voxli-license -p voxli-storage `
  -p voxli-diagnostics -p voxli-audio -p voxli-hotkeys -p voxli-asr `
  -p voxli-secure -p voxli-text -p voxli-win

# application (le moteur factice exerce tout le pipeline sans modèle)
cargo run -p voxli-desktop --features custom-protocol
```

### Moteurs réels (features opt-in)

whisper.cpp exige **libclang** et **CMake** ; le backend GPU Vulkan exige le
**SDK Vulkan** au build (`glslc` compile les shaders de ggml) :

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # chemin COURT — voir le piège

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**Le piège du répertoire cible court (documenté, subi) :** le MSBuild
imbriqué de CMake (`vulkan-shaders-gen`) casse au-delà de ~240 caractères
(MSB3501) — un `CARGO_TARGET_DIR` court comme `C:\t` l'évinte.

**Nemotron continu (opt-in)** : installer le runtime NeMoSpeech
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, installateur officiel NVIDIA), puis
`cargo test -p voxli-win --features nemo` ; `build.rs` lie le SDK et copie
les DLL à côté de l'exécutable.

### Empaquetage (installateur NSIS un fichier)

```powershell
$env:CARGO_TARGET_DIR = "C:\t"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"

# 1) l'exécutable d'abord via cargo standard (fiable)
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"

# 2) mise en scène des DLL (runtimes MSVC + SDK NeMoSpeech) — dossier gitignore
New-Item -ItemType Directory -Force apps/desktop/src-tauri/dist-dlls | Out-Null
Copy-Item "$env:LOCALAPPDATA\Programs\NeMoSpeech\bin\*.dll" apps/desktop/src-tauri/dist-dlls/

# 3) puis tauri build sur le cache chaud — whisper est déjà frais
pnpm --dir apps/desktop tauri build --features custom-protocol,nemo-stream,gpu
# → bundle/nsis/Voxli_0.1.0_x64-setup.exe
```

## Mécanique auditable

```
crates/                       11 modules sous un Cargo workspace
├── voxli-core                noyau pur : orchestrateur, machine à états, types
├── voxli-config              %APPDATA%\Voxli\config.json (schéma v5, migrations)
├── voxli-diagnostics         journal sans contenu (RF-34), rapport de support
├── voxli-license             licences Ed25519 hors ligne
├── voxli-storage             historique : mémoire (défaut) / SQLite chiffré (opt-in)
├── voxli-secure              secrets : DPAPI (opt-in), jamais dans le JSON
├── voxli-hotkeys             politique des raccourcis (validation, réservés)
├── voxli-audio               mono 16 kHz f32, pré-roll 400 ms, VAD 3 niveaux
├── voxli-asr                 moteurs : factice / whisper.cpp (+Vulkan) / Nemotron
│                             continu / Parakeet ONNX (opt-in) / distant BYOK
├── voxli-win                 tout le unsafe Win32 : hygiène presse-papiers, UIA,
│                             SendInput, fenêtre au premier plan, lien NeMo
└── voxli-text                plan d'insertion (pur) + exécution cascade
apps/desktop                  le seul binaire : réglages + pastille + secours,
                              tray, raccourcis globaux, gestionnaire de modèles
```

- **Une dictée à la fois** : un appui pendant la prise est *refusé
  explicitement* ou mis en file (profondeur 1) selon la config — jamais
  abandonné en silence.
- **Cascade d'insertion, trois niveaux** : écriture UIA directe ; sinon
  presse-papiers + Ctrl+V avec sauvegarde/restauration ; sinon fenêtre de
  secours, le texte restant au noyau.
- **Le journal est le contrat** : `engine_loaded`, `calcul:…`, coupure des
  partiels, échecs de restauration — tout se lit, sans contenu transcrit
  (RF-34). Ce que l'app raconte est vérifiable ligne à ligne.

## État, honnêtement

Implémenté et vert en local : le workspace entier compile, les suites de
tests tournent (`cargo test --workspace` + suites à features pour whisper,
Nemotron continu, SQLite/DPAPI, transport distant). L'application de bureau
mène une dictée réelle de bout en bout.

Reste à produire (suivi, pas caché) : campagne de mesure latence/WER sur la
matrice 12 apps, build Parakeet, signature de code.

## Carte de la documentation

L'étude produit → exigences → architecture (`docs/01`–`docs/04`), structure
des modules (`docs/05`), plan de tests (`docs/06`), plan de développement
(`docs/07`), audit interne avec correctifs (`docs/08`), feuille de route
(`docs/09`), monétisation (`docs/10`), checklist de release (`docs/11`),
audit UX (`docs/12`), vie privée (`docs/vie-privee.md`), langues prises en
charge (`docs/langues-supportees.md`). *Les commentaires du code sont en
anglais (premier public visé) ; la documentation produit est en français.*

## Support & soutiens

Où trouver le journal et comment faire un rapport utile :
[`SUPPORT.md`](SUPPORT.md) · Failles : [`SECURITY.md`](SECURITY.md) ·
Contribuer : [`CONTRIBUTING.md`](CONTRIBUTING.md) · Conduite :
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Versions :
[`CHANGELOG.md`](CHANGELOG.md).

Si Voxli vous fait gagner du temps :
[**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — les cœurs de l'application
mènent aux mêmes liens, jamais pendant une dictée.

## Licence du dépôt

Le code de Voxli est **sous double licence** ([`MIT`](LICENSE-MIT) **OU**
[`Apache-2.0`](LICENSE-APACHE)) — au choix. Les licences tierces (modèles,
crates, runtimes) gardent leurs termes propres :
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). La *licence du
produit* (optionnelle, vérifiée hors ligne) est un autre concept et
n'éteint jamais les fonctionnalités locales.
