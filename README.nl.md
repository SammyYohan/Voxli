<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Lokaal AI-dicteren voor Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · **Nederlands** · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dicteer overal op Windows — druk op een sneltoets, spreek, en de tekst verschijnt.
> **100 % op het apparaat zelf**: geen account, geen cloud, geen telemetrie. De audio
> wordt getranscribeerd door een engine die van jou is (whisper.cpp of Nemotron 3.5 streaming),
> ingevoegd in het actieve veld, en het klembord wordt teruggezet zoals je het achterliet.

---

## Wat je krijgt

| | |
|---|---|
| **Live voorbeeld** | De pastille zweeft bovenin je scherm en toont de tekst *terwijl je spreekt* (Nemotron 3.5 streaming), en daarna een rustig stipje wanneer ze inactief is. Vangt geen focus op en laat muisklikken door. |
| **Offline engines** | **whisper.cpp** (95+ talen, CPU of Vulkan-GPU) en **Nemotron 3.5 streaming 0.6B** (40 talen, live-tekst). Beide draaien in-process; geen server, geen SDK die je zelf moet installeren — de installer levert de runtime-DLL's mee. |
| **Eerlijke rekenkracht** | Réglages (Instellingen) → *Transcriptie-berekening*: **Auto** gebruikt de GPU zodra er een beschikbaar is, **CPU** schakelt die geforceerd uit, **Duo** rendert de live-tekst op de GPU terwijl de eindtekst op de CPU blijft. Wat er geschreven staat, is wat er draait — elke engine-mount logt `calcul:<choice>` en het logboek bewaart de werkelijke latenties. |
| **Niets gaat verloren** | De getranscribeerde tekst staat in de core-buffer *vóórdat* er een invoegpoging wordt gedaan; weigert de actieve app dan, dan vangt het noodvenster de tekst op. Klembord opslaan/herstellen (RF-30) zet terug wat je had gekopieerd. |
| **15 interfacetalen** | Automatisch van Windows afgelezen, of expliciet gekozen in Instellingen → *Interfacetaal*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Geschiedenis, woordenboeken, profielen** | Geschiedenis versleuteld in rust (standaard in het geheugen; optionele opslag op schijf), woordenboeken per dicteersessie voor namen en jargon, toepassingsspecifieke vocabulaires. |
| **Licentie** | Vrij te gebruiken, offline licentieverificatie inbegrepen (Ed25519, geen onlinecontrole nodig). |

## Installatie (gebruikerspad)

1. Download `Voxli_0.1.0_x64-setup.exe` (NSIS-installer in één bestand) van de
   **Releases**-pagina van deze repository.
2. Voer het bestand uit: de bestanden belanden in `%LOCALAPPDATA%\Voxli` (installatie per
   gebruiker, WebView2 wordt automatisch gebootstrapt als die ontbreekt, uninstaller
   bijgeleverd).
3. Bij de eerste start: kies je engine in Instellingen → **Modellen** — de regel
   *Whisper base (57 MB)* downloadt met één klik — houd daarna **Ctrl+Alt+Space**
   ingedrukt en spreek. `Shift+Alt+Z` plakt de laatste tekst opnieuw, `Shift+Alt+X` opent
   het noodvenster.

**SmartScreen-opmerking (eerlijkheid):** de installer is nog niet code-ondertekend
(certificaat in behandeling), dus Windows toont bij de eerste start een waarschuwing —
klik op *Meer info → Toch uitvoeren*. Alles wat Voxli *daarna* doet, is hier openbaar
verifieerbaar.

**Downloads = expliciete actie van de gebruiker.** Modellen worden alleen opgehaald via
Instellingen → Modellen, met licentie en grootte getoond *vóór* elke overdracht, en de
SHA-256 van wat is aangekomen wordt geregistreerd in `%APPDATA%\Voxli\models\manifests.json`.

## De modelcatalogus (met opzet compact gehouden)

| Model | Wat het is | Grootte | Licentie |
|---|---|---|---|
| **Whisper base** (gekwantiseerd) | Snel meertalig standaardmodel, ideaal als eerste download | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (gekwantiseerd) | Maximale nauwkeurigheid, GPU aanbevolen | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Streaming met live-tekst, 40 talen | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

De catalogus is op 22/10 bewust ingekrompen: zwakkere/redundante Whisper-varianten
(tiny/small/medium) en Parakeet (waarvan de sherpa-onnx-backend nog niet in de meegeleverde
build is gecompileerd) rommelen het downloadscherm niet langer vol. Bestanden die eerdere
versies op schijf achterlieten, blijven werken via **Auto** — ze worden nooit genegeerd.
Volledige toeschrijvingen: [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Privacy, in één alinea

De kernpijplijn (capture → VAD → inference → insertion) verlaat je machine niet. Het enige
netwerkverkeer dat Voxli uitvoert, zijn *modeldownloads die jij start*, plus (optioneel,
opt-in-builds) licentieverificatie — en de repository levert `scripts/prove-no-network.ps1`
mee om dat te verifiëren. Getranscribeerde tekst kan de IPC-brug uitsluitend in de twee
gedocumenteerde gevallen omzeilen (noodvenster, door de gebruiker geopende geschiedenis).
Geheimen staan nooit in `config.json` (Windows Credential Manager / DPAPI in plaats
daarvan). Details: [`docs/vie-privee.md`](docs/vie-privee.md) (Frans).

## Dicteertalen

- **Whisper**: 95+ talen — de keuzelijst in Instellingen *is* de gezaghebbende lijst;
  `auto` (standaard) detecteert per dictaat met een vangnet: uitingen korter dan 1.5 s
  vallen terug op de geconfigureerde taal in plaats van te gokken.
- **Nemotron 3.5**: 40 talen (leverancierslijst — hier niet verzonnen; zie de modelkaart
  van NVIDIA waarnaar [`docs/langues-supportees.md`](docs/langues-supportees.md) verwijst).
- De taal van de **interface** (een afzonderlijke instelling) omvat de 15 talen hierboven
  en verandert nooit de spraakherkenning.

## Bouwen vanuit de broncode

Vereisten: Rust (rustup) + Node 22+ + pnpm. Alleen Windows (Win32 + WebView2).

```powershell
powershell -ExecutionPolicy Bypass -File scripts/check-env.ps1

# interface → ../dist (embedded into the binary by `custom-protocol`)
pnpm --dir apps/desktop install
pnpm --dir apps/desktop build

# unit tests (no microphone, no model needed)
cargo test -p voxli-core -p voxli-config -p voxli-license -p voxli-storage `
  -p voxli-diagnostics -p voxli-audio -p voxli-hotkeys -p voxli-asr `
  -p voxli-secure -p voxli-text -p voxli-win

# run the app (default build drives a deterministic fake engine — the
# full pipeline is exercisable without any model)
cargo run -p voxli-desktop --features custom-protocol
```

### Echte engines (opt-in-functies)

whisper.cpp heeft **libclang** en **CMake** nodig; de Vulkan-GPU-backend heeft bij de
build de **Vulkan SDK** nodig (`glslc` compileert de shaders van ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**De valkuil van de korte target-map (gedocumenteerd, zelf ondervonden):** geneste
MSBuild binnen CMake (`vulkan-shaders-gen`) faalt boven ~240 tekens (MSB3501) — een
korte `CARGO_TARGET_DIR` zoals `C:\t` omzeilt dat volledig.

**Nemotron streaming (opt-in):** installeer de NeMoSpeech-runtime
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, officiële installer van NVIDIA), voer daarna
`cargo test -p voxli-win --features nemo` uit en de build hierboven pikt die mee;
`build.rs` linkt de SDK en kopieert de runtime-DLL's naast het uitvoerbare bestand.

### Verpakking (NSIS-installer in één bestand)

```powershell
$env:CARGO_TARGET_DIR = "C:\t"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"

# 1) exe first via plain cargo (reliable)
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"

# 2) stage runtime DLLs (MSVC runtimes + NeMoSpeech SDK) — gitignored resource folder
New-Item -ItemType Directory -Force apps/desktop/src-tauri/dist-dlls | Out-Null
Copy-Item "$env:LOCALAPPDATA\Programs\NeMoSpeech\bin\*.dll" apps/desktop/src-tauri/dist-dlls/

# 3) then tauri build on the warm cache — whisper is already fresh there
pnpm --dir apps/desktop tauri build --features custom-protocol,nemo-stream,gpu
# → bundle/nsis/Voxli_0.1.0_x64-setup.exe
```

## Hoe het werkt (een mechaniek dat je kunt natrekken)

```
crates/                       11 modules under one Cargo workspace
├── voxli-core                pure kernel: orchestrator, state machine, types
├── voxli-config              %APPDATA%\Voxli\config.json (schema v5, migrations)
├── voxli-diagnostics         content-less journal (RF-34), support report
├── voxli-license             offline Ed25519 licence verification
├── voxli-storage             history: memory (default) / encrypted SQLite (opt-in)
├── voxli-secure              secrets: DPAPI (opt-in), never in the JSON
├── voxli-hotkeys             shortcut policy (validation, reserved combos)
├── voxli-audio               16 kHz mono f32, 400 ms pre-roll, 3-level VAD
├── voxli-asr                 engines: fake / whisper.cpp (+Vulkan) / Nemotron
│                             streaming / Parakeet ONNX (opt-in) / remote BYOK
├── voxli-win                 all Win32 unsafe: clipboard hygiene, UIA,
│                             SendInput, foreground window, NeMo link
└── voxli-text                insertion plan (pure) + cascade execution
apps/desktop                  the only binary: settings + pill + rescue windows,
                              tray, global shortcuts, model manager
```

- **Eén dicteersessie tegelijk**: een toetsdruk tijdens de opname wordt
  *expliciet geweigerd*, of juist in de wachtrij geplaatst (diepte 1), afhankelijk
  van de configuratie — nooit stilletjes weggegooid.
- **Invoegcascade in drie niveaus**: de waarde rechtstreeks schrijven via UIA; anders
  klembord + Ctrl+V met opslaan/herstel; anders het noodvenster, met de tekst nog
  altijd in de core-buffer.
- **Het logboek is het contract**: elke belangrijke overgang — engine geladen
  (`engine_loaded`), berekeningskeuze (`calcul:…`), gedeeltelijke afkap, mislukte
  klembordherstel — wordt gelogd zonder inhoud van de transcriptie (RF-34). Wat de
  app beweert, wordt onderbouwd door een regel die je kunt lezen.

## Status, in alle eerlijkheid

Lokaal geïmplementeerd en groen: de volledige workspace bouwt en de testsuites draaien
(`cargo test --workspace`, plus de suites achter feature-gates voor whisper, Nemotron
streaming, SQLite/DPAPI, remote transport). De desktop-app ontwikkelt een opgenomen
dicteerflow end-to-end op echte audio.

Nog te leveren (opgevolgd, niet verstopt): de veldcampagne voor latentie- en
WER-metingen over de invoegmatrix van 12 apps, de Parakeet-build (de backend bestaat
als feature), codeondertekening.

## Documentatiemap

Productstudie → vereisten → architectuur (`docs/01`–`docs/04`), modulestructuur
(`docs/05`), testplan (`docs/06`), ontwikkelingsplan (`docs/07`), interne audit met
fixes (`docs/08`), roadmap (`docs/09`), aannames voor het verdienmodel (`docs/10`),
releasechecklist (`docs/11`), UX-audit (`docs/12`), privacy
(`docs/vie-privee.md`), ondersteunde talen (`docs/langues-supportees.md`). *De
documentatie is in het Frans geschreven; de codecommentaren zijn in het Engels (de
eerste lezers zijn Engelstalige ontwikkelaars).*

## Ondersteuning & sponsoring

Waar je het logboek vindt en hoe je een bruikbare melding indient:
[`SUPPORT.md`](SUPPORT.md) · Kwetsbaarheden melden: [`SECURITY.md`](SECURITY.md)
· Bijdragen: [`CONTRIBUTING.md`](CONTRIBUTING.md) (Frans) · Gedragscode:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Changelog:
[`CHANGELOG.md`](CHANGELOG.md).

Als Voxli je tijd bespaart: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — de hartjes in de app leiden naar
dezelfde plekken, nooit tijdens het dicteren.

## Licentie van deze repository

De Voxli-code is **dubbel gelicenseerd** onder [`MIT`](LICENSE-MIT) **OR**
[`Apache-2.0`](LICENSE-APACHE) — kies één van beide. Licenties van derden (modellen,
crates, runtimes) behouden hun eigen voorwaarden: zie
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). De *productlicentie*
(optioneel, offline te verifiëren) is een apart concept en blokkeert nooit de
lokale functies.
