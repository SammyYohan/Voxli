<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Lokale KI-Diktation für Windows</h1>

<div align="center">

**Deutsch** · [English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Diktieren Sie überall unter Windows — Tastenkombination drücken, sprechen, der Text erscheint.
> **100 % auf dem Gerät**: kein Konto, keine Cloud, keine Telemetrie. Die Audiodaten werden
> von einer Engine transkribiert, die Ihnen gehört (whisper.cpp oder Nemotron 3.5 streaming),
> in das aktive Feld eingefügt, und die Zwischenablage wird so wiederhergestellt, wie Sie sie
> hinterlassen haben.

---

## Was Sie erhalten

| | |
|---|---|
| **Live-Vorschau** | Die Pastille schwebt am oberen Rand Ihres Bildschirms und zeigt den Text *während Sie sprechen* (Nemotron 3.5 streaming), danach im Ruhezustand einen dezenten Punkt. Nicht fokussierbar, klickdurchlässig. |
| **Offline-Engines** | **whisper.cpp** (95+ Sprachen, CPU oder Vulkan-GPU) und **Nemotron 3.5 streaming 0.6B** (40 Sprachen, Live-Text). Beide laufen im selben Prozess; kein Server, kein SDK, das Sie selbst installieren müssten — der Installer liefert die Runtime-DLLs mit. |
| **Ehrliche Rechenverteilung** | Réglages (Einstellungen) → *Transcription compute*: **Auto** nutzt die GPU, sobald eine verfügbar ist, **CPU** schaltet sie ab, **Duo** rendert den Live-Text auf der GPU, während der endgültige Text auf der CPU bleibt. Was dasteht, läuft auch — jede Engine-Aktivierung protokolliert `calcul:<choice>`, und das Journal zeichnet echte Latenzen auf. |
| **Nichts geht verloren** | Der transkribierte Text liegt im Kernpuffer, *bevor* irgendein Einfügen unternommen wird; verweigert die aktive App, nimmt das Rettungsfenster ihn entgegen. Sichern/Wiederherstellen der Zwischenablage (RF-30) gibt zurück, was Sie kopiert hatten. |
| **15 Oberflächensprachen** | Automatisch von Windows erkannt oder ausdrücklich gewählt unter Einstellungen → *Interface language*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Verlauf, Wörterbücher, Profile** | Verlauf mit Verschlüsselung im Ruhezustand (standardmäßig im Arbeitsspeicher; optionale Speicherung auf der Festplatte), Wörterbücher pro Diktat für Namen und Fachjargon, anwendungsspezifische Vokabulare. |
| **Lizenz** | Kostenlos nutzbar, Offline-Lizenzprüfung inklusive (Ed25519, keine Online-Prüfung erforderlich). |

## Installation (Benutzerpfad)

1. Laden Sie `Voxli_0.1.0_x64-setup.exe` (Ein-Datei-NSIS-Installer) von der
   **Releases**-Seite dieses Repositorys herunter.
2. Führen Sie die Datei aus: Sie landet in `%LOCALAPPDATA%\Voxli` (Installation pro
   Benutzer, WebView2 wird bei Fehlen automatisch eingerichtet, Deinstaller inklusive).
3. Erster Start: Wählen Sie Ihre Engine unter Einstellungen → **Modèles** — die Zeile
   *Whisper base (57 MB)* wird mit einem Klick heruntergeladen — halten Sie dann
   **Ctrl+Alt+Space** gedrückt und sprechen Sie. `Shift+Alt+Z` fügt den letzten Text
   erneut ein, `Shift+Alt+X` öffnet das Rettungsfenster.

**SmartScreen-Hinweis (ehrlich):** Der Installer ist noch nicht code-signiert
(Zertifikat ausstehend), daher zeigt Windows beim ersten Start eine Warnung —
klicken Sie auf *Weitere Informationen → Trotzdem ausführen*. Alles, was Voxli *danach* tut,
lässt sich hier offen nachprüfen.

**Downloads = ausdrückliche Benutzeraktion.** Modelle werden nur über
Einstellungen → Modèles bezogen, Lizenz und Größe werden *vor* jeder Übertragung
angezeigt, und der SHA-256 des Empfangenen wird in
`%APPDATA%\Voxli\models\manifests.json` festgehalten.

## Der Modellkatalog (absichtlich schlank gehalten)

| Modell | Was es ist | Größe | Lizenz |
|---|---|---|---|
| **Whisper base** (quantisiert) | Schnelle mehrsprachige Standardwahl, idealer erster Download | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (quantisiert) | Maximale Genauigkeit, GPU empfohlen | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Live-Text-Streaming, 40 Sprachen | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Der Katalog wurde am 22.10. absichtlich ausgedünnt: schwächere/redundante
Whisper-Varianten (tiny/small/medium) und Parakeet (dessen sherpa-onnx-Backend
noch nicht in den mitgelieferten Build kompiliert ist) überladen den
Download-Bildschirm nicht mehr. Dateien, die frühere Versionen auf der
Festplatte hinterlassen haben, funktionieren auch weiterhin über **Auto** —
sie werden nie ignoriert. Vollständige Quellenangaben:
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Datenschutz in einem Absatz

Die Kern-Pipeline (Erfassung → VAD → Inferenz → Einfügen) verlässt Ihren
Rechner nie. Der einzige Netzwerkverkehr, den Voxli erzeugt, sind *von Ihnen
ausgelöste Modell-Downloads*, dazu (optional, Opt-in-Builds) die Lizenzprüfung —
und das Repository liefert `scripts/prove-no-network.ps1` mit, um das zu
belegen. Transkribierter Text umgeht die IPC-Brücke ausschließlich in den
beiden dokumentierten Fällen (Rettungsfenster, vom Benutzer geöffneter Verlauf).
Geheimnisse liegen nie in `config.json` (stattdessen Windows Credential
Manager / DPAPI). Details: [`docs/vie-privee.md`](docs/vie-privee.md) (französisch).

## Diktiersprachen

- **Whisper**: 95+ Sprachen — die Dropdown-Liste in Einstellungen *ist* die
  maßgebliche Liste; `auto` (Standard) erkennt Sprache pro Einsatz, mit einer
  Absicherung: Äußerungen kürzer als 1,5 s fallen auf die konfigurierte Sprache
  zurück, statt zu raten.
- **Nemotron 3.5**: 40 Sprachen (Herstellerliste — nicht von uns erfunden;
  siehe die NVIDIA-Modellkarte, verlinkt in
  [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- Die **Oberflächen**sprache (eine separate Einstellung) deckt die oben
  genannten 15 Sprachen ab und ändert die Spracherkennung nie.

## Aus dem Quellcode bauen

Voraussetzungen: Rust (rustup) + Node 22+ + pnpm. Nur Windows (Win32 + WebView2).

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

### Echte Engines (Opt-in-Features)

whisper.cpp braucht **libclang** und **CMake**; das Vulkan-GPU-Backend benötigt
zur Bauzeit das **Vulkan SDK** (`glslc` kompiliert die Shader von ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**Die Falle des kurzen Target-Verzeichnisses (dokumentiert, selbst erlitten):**
Verschachteltes MSBuild innerhalb von CMake (`vulkan-shaders-gen`) scheitert ab
~240 Zeichen (MSB3501) — ein kurzer `CARGO_TARGET_DIR` wie `C:\t` umgeht das
vollständig.

**Nemotron streaming (Opt-in):** Installieren Sie die NeMoSpeech-Runtime
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, offizieller NVIDIA-Installer), danach
`cargo test -p voxli-win --features nemo`, und der obige Build nimmt sie auf;
`build.rs` linkt das SDK und kopiert die Runtime-DLLs neben die ausführbare Datei.

### Paketierung (Ein-Datei-NSIS-Installer)

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

## Funktionsweise (Mechanik, die Sie überprüfen können)

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
apps/desktop                   the only binary: settings + pill + rescue windows,
                               tray, global shortcuts, model manager
```

- **Nur ein Diktat gleichzeitig**: Ein Druck während der Aufnahme wird
  *ausdrücklich abgelehnt* oder, je nach Konfiguration, in die Warteschlange
  gestellt (Tiefe 1) — niemals stillschweigend verworfen.
- **Einfüge-Kaskade in drei Ebenen**: direktes Schreiben des UIA-Werts; sonst
  Zwischenablage + Ctrl+V mit Sichern/Wiederherstellen; sonst das
  Rettungsfenster, während der Text noch im Kernpuffer liegt.
- **Das Journal ist der Vertrag**: jeder wichtige Übergang — geladene Engine
  (`engine_loaded`), Compute-Wahl (`calcul:…`), partieller Abbruch,
  fehlgeschlagene Wiederherstellung der Zwischenablage — wird ohne
  Transkriptionsinhalt protokolliert (RF-34). Was die App behauptet,
  entspricht einer Zeile, die Sie lesen können.

## Status — ehrlich

Implementiert und lokal grün: Der gesamte Workspace baut und die Testsuiten
laufen (`cargo test --workspace`, plus die feature-abhängigen Suiten für
whisper, Nemotron streaming, SQLite/DPAPI, Remote-Transport). Die Desktop-App
entwickelt einen Diktat-Ablauf mit Aufnahme Ende-zu-Ende auf echtem Audio.

Noch offen (erfasst, nicht versteckt): Messkampagne für Feld-Latenz/WER über
die Einfüge-Matrix aus 12 Apps, Parakeet-Build (das Backend existiert als
Feature), Code-Signierung.

**Linux**: auf der Roadmap, ohne versprochenes Datum. Die Motorebene
(whisper.cpp, GGUF, Vulkan) ist portabel; die Windows-spezifische
Eingabeebene (UIA-Einfügung, Zwischenablage-Wiederherstellung, globale
Tastenkombinationen) braucht zuerst ihr Linux-Pendant. Wird im selben Plan
verfolgt — nichts bleibt verborgen.

## Dokumentationsübersicht

Produktstudie → Anforderungen → Architektur (`docs/01`–`docs/04`),
Modulstruktur (`docs/05`), Testplan (`docs/06`), Entwicklungsplan (`docs/07`),
internes Audit mit Behebungen (`docs/08`), Roadmap (`docs/09`), Annahmen zur
Monetarisierung (`docs/10`), Release-Checkliste (`docs/11`), UX-Audit
(`docs/12`), Datenschutz (`docs/vie-privee.md`), unterstützte Sprachen
(`docs/langues-supportees.md`). *Die Dokumentation ist auf Französisch
verfasst; die Codekommentare sind auf Englisch (die ersten Leser sind
englischsprachige Entwickler).*

## Support & Sponsoring

Wo Sie das Journal finden und wie Sie eine nützliche Meldung einreichen:
[`SUPPORT.md`](SUPPORT.md) · Schwachstellen melden:
[`SECURITY.md`](SECURITY.md) · Beitragen:
[`CONTRIBUTING.md`](CONTRIBUTING.md) (französisch) · Verhaltenskodex:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Changelog:
[`CHANGELOG.md`](CHANGELOG.md).

Wenn Voxli Ihnen Zeit spart: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — die Herzen in der App führen
an dieselben Orte, niemals während eines Diktats.

## Lizenz dieses Repositorys

Der Voxli-Code ist **dual lizenziert** unter [`MIT`](LICENSE-MIT) **OR**
[`Apache-2.0`](LICENSE-APACHE) — nehmen Sie eine der beiden.
Drittanbieter-Lizenzen (Modelle, Crates, Runtimes) behalten ihre eigenen
Bedingungen: siehe [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).
Die *Produktlizenz* (optional, verifiziert offline) ist ein eigenes Konzept
und schaltet die lokalen Funktionen nie frei.
