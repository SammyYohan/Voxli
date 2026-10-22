<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Local AI dictation for Windows</h1>

<div align="center">

**English** · [Français](README.fr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dictate anywhere on Windows — press a shortcut, speak, the text appears.
> **100 % on-device**: no account, no cloud, no telemetry. The audio is
> transcribed by an engine you own (whisper.cpp or Nemotron 3.5 streaming),
> inserted into the active field, and the clipboard is put back as you left it.

---

## What you get

| | |
|---|---|
| **Live preview** | The pill floats at the top of your screen and shows the text *while you speak* (Nemotron 3.5 streaming), then a quiet dot when idle. Non-focusable, click-transparent. |
| **Offline engines** | **whisper.cpp** (95+ languages, CPU or Vulkan GPU) and **Nemotron 3.5 streaming 0.6B** (40 languages, live text). Both run in-process; no server, no SDK you need to install yourself — the installer ships the runtime DLLs. |
| **Honest compute** | Réglages → *Transcription compute*: **Auto** uses the GPU when one is available, **CPU** forces it off, **Duo** renders the live text on the GPU while the final text stays on CPU. What is written is what runs — every engine mount logs `calcul:<choice>` and the journal keeps real latencies. |
| **Nothing lost** | The transcribed text lives in the core buffer *before* any insertion is attempted; if the active app refuses, the rescue window receives it. Clipboard save/restore (RF-30) puts back what you had copied. |
| **15 interface languages** | Auto-detected from Windows, or picked explicitly in Réglages → *Interface language*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **History, dictionaries, profiles** | Encrypted-at-rest history (default in-memory; optional disk store), per-dictation dictionaries for names and jargon, application-specific vocabularies. |
| **Licence** | Free to use, offline licence verification included (Ed25519, no online check required). |

## Install (user path)

1. Download `Voxli_0.1.0_x64-setup.exe` (single-file NSIS installer) from the
   **Releases** page of this repository.
2. Run it: files land in `%LOCALAPPDATA%\Voxli` (per-user install, WebView2
   bootstrapped automatically if missing, uninstaller provided).
3. First launch: pick your engine in Réglages → **Modèles** — the *Whisper
   base (57 Mo)* line downloads in one click — then hold **Ctrl+Alt+Space**
   and speak. `Shift+Alt+Z` re-pastes the last text, `Shift+Alt+X` opens the
   rescue window.

**SmartScreen note (honesty):** the installer is not code-signed yet
(certificate pending), so Windows shows a warning on first run — click
*More info → Run anyway*. Everything Voxli does *after* that is verifiable
here in the open.

**Downloads = explicit user action.** Models are fetched only from
Réglages → Modèles, with licence and size shown *before* any transfer, and
the SHA-256 of what arrived is recorded in `%APPDATA%\Voxli\models\manifests.json`.

## The model catalogue (kept lean on purpose)

| Model | What it is | Size | Licence |
|---|---|---|---|
| **Whisper base** (quantised) | Fast multilingual default, ideal first download | 57 Mo | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (quantised) | Maximum accuracy, GPU recommended | 574 Mo | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Live-text streaming, 40 languages | 742 Mo | OpenMDW-1.1 — NVIDIA Nemotron Speech |

The catalogue was deliberately trimmed on 22/10: weaker/redundant Whisper
variants (tiny/small/medium) and Parakeet (whose sherpa-onnx backend is not
compiled into the shipped build yet) no longer clutter the download screen.
Files left on disk by earlier versions keep working through **Auto** — they
are never ignored. Full attributions: [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Privacy, in one paragraph

The core pipeline (capture → VAD → inference → insertion) never leaves your
machine. The only network traffic Voxli performs is *model downloads you
trigger*, plus (optional, opt-in builds) licence verification — and the
repository ships `scripts/prove-no-network.ps1` to verify it. Transcribed
text can bypass the IPC bridge exclusively in the two documented cases
(rescue window, user-opened history). Secrets never live in `config.json`
(Windows Credential Manager / DPAPI instead). Details:
[`docs/vie-privee.md`](docs/vie-privee.md) (French).

## Dictation languages

- **Whisper**: 95+ languages — the Réglages dropdown *is* the authoritative
  list; `auto` (default) performs per-detection with a guard: statements
  shorter than 1.5 s fall back to the configured language instead of guessing.
- **Nemotron 3.5**: 40 languages (vendor list — not invented here; see
  NVIDIA's model card linked in [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- The **interface** language (a separate setting) covers the 15 languages
  above and never changes speech recognition.

## Build from source

Prerequisites: Rust (rustup) + Node 22+ + pnpm. Windows only (Win32 + WebView2).

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

### Real engines (opt-in features)

whisper.cpp needs **libclang** and **CMake**; the Vulkan GPU backend needs
the **Vulkan SDK** at build time (`glslc` compiles ggml's shaders):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**The short-target-dir trap (documented, suffered):** nested MSBuild inside
CMake (`vulkan-shaders-gen`) fails past ~240 characters (MSB3501) — a short
`CARGO_TARGET_DIR` like `C:\t` sidesteps it entirely.

**Nemotron streaming (opt-in):** install the NeMoSpeech runtime
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, official NVIDIA installer), then
`cargo test -p voxli-win --features nemo` and the build above picks it up;
`build.rs` links the SDK and copies the runtime DLLs next to the executable.

### Packaging (single-file NSIS installer)

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

## How it works (mechanics you can audit)

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

- **One dictation at a time**: a press during recording is *explicitly
  rejected*, or queued (depth 1) depending on configuration — never dropped
  silently.
- **Insertion cascade, three levels**: direct UIA value write; else
  clipboard + Ctrl+V with save/restore; else the rescue window with the
  text still in the core buffer.
- **The journal is the contract**: every important transition — engine
  mounted (`engine_loaded`), compute choice (`calcul:…`), partial cut-off,
  clipboard restore failures — is logged without transcription content
  (RF-34). What the app claims is matched by a line you can read.

## Status, honestly

Implemented and green locally: the full workspace builds and the test suites
run (`cargo test --workspace`, plus the feature-gated suites for whisper,
Nemotron streaming, SQLite/DPAPI, remote transport). The desktop app
develops a recorded dictation flow end-to-end on real audio.

Still to be produced (tracked, not hidden): field latency/WER measurement
campaign across the 12-app insertion matrix, Parakeet build (backend exists
as a feature), code signing.

## Documentation map

Product study → requirements → architecture (`docs/01`–`docs/04`), module
structure (`docs/05`), test plan (`docs/06`), development plan (`docs/07`),
internal audit with fixes (`docs/08`), roadmap (`docs/09`), monetization
assumptions (`docs/10`), release checklist (`docs/11`), UX audit (`docs/12`),
privacy (`docs/vie-privee.md`), supported languages
(`docs/langues-supportees.md`). *The docs are written in French; the code
comments are in English (the first readers are anglophone developers).*

## Support & sponsorship

Where to find the journal and how to file a useful report:
[`SUPPORT.md`](SUPPORT.md) · Vulnerability reporting: [`SECURITY.md`](SECURITY.md)
· Contributing: [`CONTRIBUTING.md`](CONTRIBUTING.md) (French) · Conduct:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Changelog:
[`CHANGELOG.md`](CHANGELOG.md).

If Voxli saves you time: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — the in-app hearts lead to the
same places, never during a dictation.

## Licence of this repository

The Voxli code is **dual-licensed** under [`MIT`](LICENSE-MIT) **OR**
[`Apache-2.0`](LICENSE-APACHE) — pick either. Third-party licences (models,
crates, runtimes) keep their own terms: see
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). The *product licence*
(optional, verifies offline) is a separate concept and never gates the local
features.
