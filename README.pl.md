<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — lokalne dyktowanie AI dla systemu Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · **Polski** · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dyktuj wszędzie w systemie Windows — naciśnij skrót, mów, a tekst się pojawia.
> **100 % na urządzeniu**: bez konta, bez chmury, bez telemetrii. Dźwięk jest
> transkrybowany przez silnik, który do Ciebie należy (whisper.cpp lub
> Nemotron 3.5 streaming), wstawiany do aktywnego pola, a schowek wraca do
> stanu, w jakim go zostawiono.

---

## Co otrzymujesz

| | |
|---|---|
| **Podgląd na żywo** | Pigułka unosi się u góry ekranu i pokazuje tekst *w trakcie mówienia* (Nemotron 3.5 streaming), a w bezczynności — spokojną kropkę. Nie przyjmuje fokusa i jest przezroczysta dla kliknięć. |
| **Silniki offline** | **whisper.cpp** (95+ języków, CPU lub GPU Vulkan) i **Nemotron 3.5 streaming 0.6B** (40 języków, tekst na żywo). Oba działają w obrębie jednego procesu aplikacji; bez serwera, bez SDK do samodzielnej instalacji — instalator dostarcza biblioteki DLL środowiska uruchomieniowego. |
| **Uczciwe obliczenia** | Réglages (Ustawienia) → *Moc obliczeniowa transkrypcji*: **Auto** korzysta z GPU, o ile jest dostępne, **CPU** wymusza jego wyłączenie, a **Duo** renderuje tekst na żywo na GPU, podczas gdy tekst końcowy powstaje na CPU. Co jest napisane, to faktycznie działa — każde zamontowanie silnika loguje `calcul:<choice>`, a dziennik przechowuje rzeczywiste opóźnienia. |
| **Nic nie ginie** | Tekst transkrypcji znajduje się w buforze rdzenia *zanim* zostanie podjęta jakakolwiek próba wstawienia; jeśli aktywna aplikacja odmówi, odbiera go okno ratunkowe. Zapis/przywracanie schowka (RF-30) zwraca to, co było wcześniej skopiowane. |
| **15 języków interfejsu** | Wykrywane automatycznie z systemu Windows albo wybierane wprost w Ustawieniach → *Język interfejsu*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Historia, słowniki, profile** | Historia szyfrowana w spoczynku (domyślnie w pamięci; opcjonalny magazyn na dysku), słowniki przypisane do dyktowań — nazwy i żargon — oraz słownictwo specyficzne dla aplikacji. |
| **Licencja** | Bezpłatne korzystanie, w zestawie weryfikacja licencji offline (Ed25519, bez wymaganej kontroli online). |

## Instalacja (ścieżka użytkownika)

1. Pobierz `Voxli_0.1.0_x64-setup.exe` (instalator NSIS w jednym pliku) ze
   strony **Releases** tego repozytorium.
2. Uruchom go: pliki lądują w `%LOCALAPPDATA%\Voxli` (instalacja dla
   pojedynczego użytkownika, WebView2 automatycznie doinstalowywany, jeśli
   go brak, deinstalator w zestawie).
3. Pierwsze uruchomienie: wybierz silnik w Ustawienia → **Modele** — pozycja
   *Whisper base (57 MB)* pobiera się jednym kliknięciem — a następnie
   przytrzymaj **Ctrl+Alt+Space** i mów. `Shift+Alt+Z` ponownie wkleja
   ostatni tekst, `Shift+Alt+X` otwiera okno ratunkowe.

**Uwaga o SmartScreen (szczerze):** instalator nie ma jeszcze podpisu
cyfrowego (certyfikat w toku), więc Windows przy pierwszym uruchomieniu
pokazuje ostrzeżenie — kliknij *Więcej informacji → Uruchom mimo to*.
Wszystko, co Voxli robi *potem*, jest weryfikowalne tutaj, jawnie.

**Pobieranie = wyraźna decyzja użytkownika.** Modele są pobierane wyłącznie
z Ustawienia → Modele, z licencją i rozmiarem pokazanymi *przed*
jakimkolwiek transferem, a SHA-256 tego, co dotarło, jest zapisywany w
`%APPDATA%\Voxli\models\manifests.json`.

## Katalog modeli (celowo ograniczony)

| Model | Co to jest | Rozmiar | Licencja |
|---|---|---|---|
| **Whisper base** (skwantyzowany) | Szybki, wielojęzyczny model domyślny, idealny na pierwsze pobranie | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (skwantyzowany) | Maksymalna dokładność, zalecane GPU | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Streaming tekstu na żywo, 40 języków | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Katalog został celowo przycięty 22/10: słabsze/redundantne warianty Whisper
(tiny/small/medium) oraz Parakeet (którego backend sherpa-onnx nie jest
jeszcze skompilowany w wydawanym buildzie) nie zaśmiecają już ekranu
pobierania. Pliki pozostawione na dysku przez wcześniejsze wersje nadal
działają dzięki **Auto** — nigdy nie są ignorowane. Pełne atrybucje:
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Prywatność w jednym akapicie

Rdzeniowy potok (przechwytywanie → VAD → wnioskowanie → wstawianie) nigdy nie
opuszcza Twojej maszyny. Jedyne zdarzenie sieciowe, jakie wykonuje Voxli, to
*pobrania modeli zainicjowane przez Ciebie* plus (opcjonalne, wydania opt-in)
weryfikacja licencji — a repozytorium zawiera `scripts/prove-no-network.ps1`,
by to zweryfikować. Tekst transkrypcji może ominąć mostek IPC wyłącznie w
dwóch udokumentowanych przypadkach (okno ratunkowe, historia otwarta przez
użytkownika). Sekrety nigdy nie trafiają do `config.json` (zamiast tego
Windows Credential Manager / DPAPI). Szczegóły:
[`docs/vie-privee.md`](docs/vie-privee.md) (po francusku).

## Języki dyktowania

- **Whisper**: 95+ języków — lista rozwijana w Ustawieniach *to właśnie*
  autorytatywna lista; `auto` (domyślnie) dokonuje detekcji przy każdej
  wypowiedzi, z zabezpieczeniem: wypowiedzi krótsze niż 1.5 s wracają do
  skonfigurowanego języka zamiast zgadywać.
- **Nemotron 3.5**: 40 języków (lista dostawcy — nie wymyślona tutaj; zobacz
  kartę modelu NVIDII, do której prowadzi link w
  [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- Język **interfejsu** (osobne ustawienie) obejmuje 15 języków wymienionych
  wyżej i nigdy nie zmienia rozpoznawania mowy.

## Budowanie ze źródeł

Wymagania wstępne: Rust (rustup) + Node 22+ + pnpm. Tylko Windows
(Win32 + WebView2).

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

### Prawdziwe silniki (funkcje opt-in)

whisper.cpp wymaga **libclang** i **CMake**; backend GPU Vulkan wymaga w
czasie kompilacji **Vulkan SDK** (`glslc` kompiluje shadery ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**Pułapka krótkiego katalogu docelowego (udokumentowana, bolesna z
doświadczenia):** zagnieżdżony MSBuild wewnątrz CMake (`vulkan-shaders-gen`)
wysypuje się po przekroczeniu ~240 znaków (MSB3501) — krótki
`CARGO_TARGET_DIR`, taki jak `C:\t`, omija ten problem całkowicie.

**Nemotron streaming (opt-in):** zainstaluj środowisko uruchomieniowe
NeMoSpeech (`%LOCALAPPDATA%\Programs\NeMoSpeech`, oficjalny instalator
NVIDII), a następnie uruchom `cargo test -p voxli-win --features nemo` —
powyższa kompilacja go podchwyci; `build.rs` dowiązuje SDK i kopiuje
biblioteki DLL środowiska uruchomieniowego obok pliku wykonywalnego.

### Pakowanie (instalator NSIS w jednym pliku)

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

## Jak to działa (mechanika, którą można poddać audytowi)

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

- **Jedno dyktowanie naraz**: naciśnięcie w trakcie nagrywania jest
  *wyraźnie odrzucane* albo kolejkowane (głębokość 1), zależnie od
  konfiguracji — nigdy nie jest po cichu gubione.
- **Kaskada wstawiania, trzy poziomy**: bezpośredni zapis wartości przez
  UIA; w przeciwnym razie schowek + Ctrl+V z zapisem i przywracaniem; w
  ostateczności okno ratunkowe, a tekst wciąż czeka w buforze rdzenia.
- **Dziennik to umowa**: każda ważna zmiana stanu — zamontowanie silnika
  (`engine_loaded`), wybór mocy obliczeniowej (`calcul:…`), częściowe
  obcięcie wypowiedzi, niepowodzenia przywracania schowka — jest
  rejestrowana bez treści transkrypcji (RF-34). Każde twierdzenie aplikacji
  znajduje pokrycie w linii, którą możesz przeczytać.

## Stan prac, szczerze

Zaimplementowane i zielone lokalnie: pełny workspace buduje się, a zestawy
testów przechodzą (`cargo test --workspace`, plus zestawy uzależnione od
feature'ów dla whisper, Nemotron streaming, SQLite/DPAPI i zdalnego
transportu). Aplikacja desktopowa realizuje przepływ dyktowania z nagrania
od początku do końca na prawdziwym dźwięku.

Jeszcze do zrobienia (śledzone, nie ukrywane): kampania pomiarów
latencji/WER w terenie na 12-aplikacyjnej macierzy wstawiania, budowa
Parakeet (backend istnieje jako feature), podpisywanie kodu.

**Linux**: w planach, bez obiecywanej daty. Warstwa silników (whisper.cpp,
GGUF, Vulkan) jest przenośna; warstwa wejściowa specyficzna dla Windows
(wstawianie przez UIA, odtwarzanie schowka, globalne skróty) najpierw
potrzebuje swojego odpowiednika pod Linuksem. Śledzone w tym samym planie —
nic nie jest ukrywane.

## Mapa dokumentacji

Studium produktu → wymagania → architektura (`docs/01`–`docs/04`), struktura
modułów (`docs/05`), plan testów (`docs/06`), plan rozwoju (`docs/07`), audyt
wewnętrzny z poprawkami (`docs/08`), roadmapa (`docs/09`), założenia
monetyzacji (`docs/10`), lista kontrolna wydania (`docs/11`), audyt UX
(`docs/12`), prywatność (`docs/vie-privee.md`), obsługiwane języki
(`docs/langues-supportees.md`). *Dokumentacja jest napisana po francusku;
komentarze w kodzie są po angielsku (pierwszymi czytelnikami są programiści
anglojęzyczni).*

## Wsparcie i sponsoring

Gdzie znaleźć dziennik i jak zgłosić przydatne zgłoszenie:
[`SUPPORT.md`](SUPPORT.md) · Zgłaszanie luk bezpieczeństwa:
[`SECURITY.md`](SECURITY.md)
· Współtworzenie: [`CONTRIBUTING.md`](CONTRIBUTING.md) (po francusku) ·
Zasady zachowania: [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Dziennik
zmian: [`CHANGELOG.md`](CHANGELOG.md).

Jeśli Voxli oszczędza Ci czas: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — serduszka w aplikacji
prowadzą do tych samych miejsc, nigdy w trakcie dyktowania.

## Licencja tego repozytorium

Kod Voxli jest **licencjonowany podwójnie** na [`MIT`](LICENSE-MIT) **LUB**
[`Apache-2.0`](LICENSE-APACHE) — wybierz dowolną z nich. Licencje stron
trzecich (modele, crates, środowiska uruchomieniowe) zachowują własne
warunki: patrz [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).
*Licencja produktu* (opcjonalna, weryfikowana offline) to osobna koncepcja
i nigdy nie warunkuje działania lokalnych funkcji.
