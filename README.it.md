<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Dettatura IA locale per Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · **Italiano** · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dettate ovunque su Windows — premi una scorciatoia, parla, il testo appare.
> **100 % sul dispositivo**: nessun account, nessun cloud, nessuna telemetria.
> L'audio viene trascritto da un motore che possiedi (whisper.cpp o Nemotron 3.5
> streaming), inserito nel campo attivo, e gli appunti vengono rimessi a posto
> come li avevi lasciati.

---

## Cosa ottieni

| | |
|---|---|
| **Anteprima in tempo reale** | La pastiglia fluttua in alto sullo schermo e mostra il testo *mentre parli* (Nemotron 3.5 streaming), poi un puntino discreto quando è inattiva. Non focalizzabile, trasparente ai clic. |
| **Motori offline** | **whisper.cpp** (95+ lingue, CPU o GPU Vulkan) e **Nemotron 3.5 streaming 0.6B** (40 lingue, testo in tempo reale). Entrambi girano in-process; nessun server, nessun SDK da installare a mano — l'installatore include le DLL di runtime. |
| **Calcolo onesto** | Réglages (Impostazioni) → *Transcription compute*: **Auto** usa la GPU quando ne è disponibile una, **CPU** la disattiva forzatamente, **Duo** esegue il testo in tempo reale sulla GPU mentre il testo finale resta sulla CPU. Ciò che è scritto è ciò che viene eseguito — ogni caricamento del motore registra `calcul:<choice>` e il registro conserva le latenze reali. |
| **Nulla va perso** | Il testo trascritto risiede nel buffer del core *prima* di qualunque tentativo di inserimento; se l'app attiva rifiuta, lo riceve la finestra di recupero. Il salvataggio/ripristino degli appunti (RF-30) restituisce ciò che avevi copiato. |
| **15 lingue di interfaccia** | Rilevate automaticamente da Windows, oppure selezionate esplicitamente in Impostazioni → *Interface language*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Cronologia, dizionari, profili** | Cronologia cifrata a riposo (in memoria per default; archivio su disco opzionale), dizionari per ogni dettatura per nomi e gergo, vocabolari specifici per applicazione. |
| **Licenza** | Libero da usare, verifica della licenza offline inclusa (Ed25519, nessun controllo online richiesto). |

## Installazione (percorso utente)

1. Scarica `Voxli_0.1.0_x64-setup.exe` (installatore NSIS a file singolo) dalla
   pagina **Releases** di questo repository.
2. Avvialo: i file finiscono in `%LOCALAPPDATA%\Voxli` (installazione per
   utente, WebView2 preparato automaticamente se assente, disinstallatore
   incluso).
3. Primo avvio: scegli il motore in Impostazioni → **Modèles** — la voce
   *Whisper base (57 MB)* si scarica con un clic — poi tieni premuto
   **Ctrl+Alt+Space** e parla. `Shift+Alt+Z` incolla di nuovo l'ultimo testo,
   `Shift+Alt+X` apre la finestra di recupero.

**Nota su SmartScreen (trasparenza):** l'installatore non è ancora firmato col
codice (certificato in attesa), quindi Windows mostra un avviso al primo
avvio — clicca *More info → Run anyway*. Tutto ciò che Voxli fa *dopo* è
verificabile qui, a visiera aperta.

**Download = azione esplicita dell'utente.** I modelli vengono scaricati solo
da Impostazioni → Modèles, con licenza e dimensione mostrate *prima* di ogni
trasferimento, e lo SHA-256 di ciò che arriva viene registrato in
`%APPDATA%\Voxli\models\manifests.json`.

## Il catalogo dei modelli (volutamente essenziale)

| Modello | Che cos'è | Dimensione | Licenza |
|---|---|---|---|
| **Whisper base** (quantizzato) | Default multilingue rapido, primo download ideale | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (quantizzato) | Accuratezza massima, GPU consigliata | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Streaming con testo in tempo reale, 40 lingue | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Il catalogo è stato ridotto deliberatamente il 22/10: le varianti Whisper più
deboli/ridondanti (tiny/small/medium) e Parakeet (il cui backend sherpa-onnx
non è ancora compilato nella build distribuita) non ingombrano più la
schermata di download. I file lasciati sul disco dalle versioni precedenti
continuano a funzionare tramite **Auto** — non vengono mai ignorati.
Attribuzioni complete: [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Privacy, in un paragrafo

La pipeline centrale (cattura → VAD → inferenza → inserimento) non lascia mai
la tua macchina. L'unico traffico di rete che Voxli genera sono *i download di
modelli che avvii tu*, più (facoltativo, build opt-in) la verifica della
licenza — e il repository include `scripts/prove-no-network.ps1` per
verificarlo. Il testo trascritto può bypassare il bridge IPC esclusivamente
nei due casi documentati (finestra di recupero, cronologia aperta
dall'utente). I segreti non risiedono mai in `config.json` (invece: Windows
Credential Manager / DPAPI). Dettagli:
[`docs/vie-privee.md`](docs/vie-privee.md) (in francese).

## Lingue di dettatura

- **Whisper**: 95+ lingue — il menu a tendina di Impostazioni *è* la lista
  autorevole; `auto` (predefinito) esegue il rilevamento a ogni dettatura con
  una guardia: gli enunciati più corti di 1.5 s ripiegano sulla lingua
  configurata invece di tirare a indovinare.
- **Nemotron 3.5**: 40 lingue (elenco del fornitore — non inventato qui; vedi
  la model card di NVIDIA collegata in
  [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- La lingua dell'**interfaccia** (un'impostazione separata) copre le 15 lingue
  sopra elencate e non influisce mai sul riconoscimento vocale.

## Compilazione dai sorgenti

Prerequisiti: Rust (rustup) + Node 22+ + pnpm. Solo Windows (Win32 + WebView2).

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

### Motori reali (funzionalità opt-in)

whisper.cpp richiede **libclang** e **CMake**; il backend GPU Vulkan ha
bisogno del **Vulkan SDK** in fase di compilazione (`glslc` compila gli shader
di ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**La trappola del short-target-dir (documentata, subita):** il MSBuild
annidato dentro CMake (`vulkan-shaders-gen`) va in errore oltre i ~240
caratteri (MSB3501) — un `CARGO_TARGET_DIR` corto come `C:\t` la aggira del
tutto.

**Nemotron streaming (opt-in):** installa il runtime NeMoSpeech
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, installatore ufficiale NVIDIA), poi
`cargo test -p voxli-win --features nemo`, e la build qui sopra lo raccoglie;
`build.rs` collega l'SDK e copia le DLL di runtime accanto all'eseguibile.

### Packaging (installatore NSIS a file singolo)

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

## Come funziona (meccanismi che puoi verificare)

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

- **Una dettatura alla volta**: una pressione durante la registrazione viene
  *rifiutata esplicitamente*, oppure messa in coda (profondità 1) a seconda
  della configurazione — mai scartata in silenzio.
- **Cascata di inserimento, tre livelli**: scrittura diretta del valore via
  UIA; altrimenti appunti + Ctrl+V con salvataggio/ripristino; altrimenti la
  finestra di recupero, col testo ancora nel buffer del core.
- **Il registro è il contratto**: ogni transizione importante — motore
  caricato (`engine_loaded`), scelta del calcolo (`calcul:…`), interruzione
  parziale, fallimenti del ripristino degli appunti — viene registrata senza
  il contenuto della trascrizione (RF-34). Ciò che l'app dichiara corrisponde
  a una riga che puoi leggere.

## Stato, in tutta onestà

Implementato e verde in locale: l'intero workspace si compila e le suite di
test girano (`cargo test --workspace`, più le suite attivabili per feature
opzionali per whisper, Nemotron streaming, SQLite/DPAPI, trasporto remoto).
L'app desktop esercita end-to-end, su audio reale, un flusso di dettatura da
audio registrato.

Resta ancora da produrre (tracciato, non nascosto): campagna di misurazione
di latenza/WER sul campo attraverso la matrice di inserimento su 12 app,
compilazione di Parakeet (il backend esiste come feature), firma del codice.

**Linux**: nella roadmap, senza data promessa. Lo strato motori
(whisper.cpp, GGUF, Vulkan) è portabile; lo strato di input specifico di
Windows (iniezione UIA, ripristino degli appunti, scorciatoie globali) ha
bisogno prima della sua controparte Linux. Tracciato nello stesso piano
del resto — nulla è nascosto.

## Mappa della documentazione

Studio di prodotto → requisiti → architettura (`docs/01`–`docs/04`),
struttura dei moduli (`docs/05`), piano dei test (`docs/06`), piano di
sviluppo (`docs/07`), audit interno con correzioni (`docs/08`),
roadmap (`docs/09`), ipotesi di monetizzazione (`docs/10`), checklist di
rilascio (`docs/11`), audit UX (`docs/12`), privacy
(`docs/vie-privee.md`), lingue supportate
(`docs/langues-supportees.md`). *La documentazione è scritta in francese; i
commenti del codice sono in inglese (i primi lettori sono sviluppatori
anglofoni).*

## Supporto e sponsorizzazione

Dove trovare il registro e come presentare una segnalazione utile:
[`SUPPORT.md`](SUPPORT.md) · Segnalazione vulnerabilità: [`SECURITY.md`](SECURITY.md)
· Contribuire: [`CONTRIBUTING.md`](CONTRIBUTING.md) (in francese) · Condotta:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Changelog:
[`CHANGELOG.md`](CHANGELOG.md).

Se Voxli ti fa risparmiare tempo: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — i cuori nell'app portano agli
stessi posti, mai durante una dettatura.

## Licenza di questo repository

Il codice di Voxli è **con doppia licenza** [`MIT`](LICENSE-MIT) **OR**
[`Apache-2.0`](LICENSE-APACHE) — scegli l'una o l'altra. Le licenze di terze
parti (modelli, crate, runtime) mantengono i loro termini: vedi
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). La *licenza del
prodotto* (facoltativa, verifica offline) è un concetto a parte e non limita
mai le funzionalità locali.
