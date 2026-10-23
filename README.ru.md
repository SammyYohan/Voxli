<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — локальная ИИ-диктовка для Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · **Русский** · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Диктуйте где угодно в Windows — нажмите горячую клавишу, говорите, и текст
> появляется. **100 % на устройстве**: без аккаунта, без облака, без
> телеметрии. Аудио расшифровывается принадлежащим вам движком (whisper.cpp
> или Nemotron 3.5 streaming), текст вставляется в активное поле, а буфер
> обмена возвращается в том виде, в каком вы его оставили.

---

## Что вы получаете

| | |
|---|---|
| **Предпросмотр в реальном времени** | Плашка плавает у верхнего края экрана и показывает текст *пока вы говорите* (Nemotron 3.5 streaming), а в простое остаётся неприметная точка. Не принимает фокус ввода, прозрачна для щелчков. |
| **Автономные движки** | **whisper.cpp** (95+ языков, CPU или GPU с Vulkan) и **Nemotron 3.5 streaming 0.6B** (40 языков, текст в реальном времени). Оба работают внутри процесса: без сервера, без SDK, которое пришлось бы устанавливать самому, — установщик поставляет DLL среды выполнения. |
| **Честные вычисления** | Réglages (Настройки) → *Вычисления для транскрипции*: **Auto** использует GPU, когда он доступен, **CPU** принудительно его отключает, **Duo** рендерит текст в реальном времени на GPU, а итоговый текст остаётся на CPU. Что написано — то и работает: при каждой загрузке движка в журнал пишется `calcul:<choice>`, а журнал хранит реальные задержки. |
| **Ничего не теряется** | Распознанный текст живёт в буфере ядра *до* любой попытки вставки; если активное приложение отказывает, его получает окно спасения. Сохранение/восстановление буфера обмена (RF-30) возвращает то, что у вас было скопировано. |
| **15 языков интерфейса** | Определяются автоматически по Windows или выбираются явно в Настройках → *Язык интерфейса*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **История, словари, профили** | История хранится зашифрованной (по умолчанию в памяти; опционально на диске), словари на каждую диктовку для имён и жаргона, словари под конкретные приложения. |
| **Лицензия** | Бесплатно для использования, офлайн-проверка лицензии встроена (Ed25519, онлайн-проверка не требуется). |

## Установка (для пользователя)

1. Скачайте `Voxli_0.1.0_x64-setup.exe` (NSIS-установщик одним файлом) со
   страницы **Releases** этого репозитория.
2. Запустите его: файлы окажутся в `%LOCALAPPDATA%\Voxli` (установка на
   пользователя, WebView2 автоматически доустанавливается, если его нет,
   деинсталлятор прилагается).
3. Первый запуск: выберите движок в Настройках → **Modèles (Модели)** —
   строка *Whisper base (57 МБ)* скачивается одним щелчком, — затем
   удерживайте **Ctrl+Alt+Space** и говорите. `Shift+Alt+Z` повторно
   вставляет последний текст, `Shift+Alt+X` открывает окно спасения.

**Примечание о SmartScreen (честность):** установщик пока не имеет подписи
кода (сертификат в процессе получения), поэтому Windows при первом запуске
показывает предупреждение — нажмите *Дополнительные сведения → Выполнить в
любом случае*. Всё, что Voxli делает *после* этого, открыто и поддаётся
проверке прямо здесь.

**Скачивание = явное действие пользователя.** Модели забираются только из
Настроек → Modèles, лицензия и размер показываются *до* любой передачи, а
SHA-256 прибывшего файла записывается в
`%APPDATA%\Voxli\models\manifests.json`.

## Каталог моделей (намеренно компактный)

| Модель | Что это | Размер | Лицензия |
|---|---|---|---|
| **Whisper base** (квантованная) | Быстрая многоязычная модель по умолчанию, идеальный первый вариант для скачивания | 57 МБ | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (квантованная) | Максимальная точность, рекомендуется GPU | 574 МБ | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Потоковая передача с текстом в реальном времени, 40 языков | 742 МБ | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Каталог был сознательно сокращён 22/10: более слабые/избыточные варианты
Whisper (tiny/small/medium) и Parakeet (backend на sherpa-onnx пока не
скомпилирован в поставляемую сборку) больше не загромождают экран
скачивания. Файлы, оставшиеся на диске от прежних версий, продолжают
работать через **Auto** — их никогда не игнорируют. Полные атрибуции:
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Приватность — одним абзацем

Основной конвейер (захват → VAD → инференс → вставка) никогда не покидает
вашу машину. Единственный сетевой трафик Voxli — *скачивания моделей,
которые вы сами запускаете*, плюс (опционально, в сборках opt-in) проверка
лицензии, — и в репозитории лежит `scripts/prove-no-network.ps1`, чтобы это
доказать. Распознанный текст может миновать IPC-мост исключительно в двух
задокументированных случаях (окно спасения, открытая пользователем
история). Секреты никогда не хранятся в `config.json` (вместо них —
Windows Credential Manager / DPAPI). Подробнее:
[`docs/vie-privee.md`](docs/vie-privee.md) (на французском).

## Языки диктовки

- **Whisper**: 95+ языков — канонический список это сам выпадающий список
  в Настройках; `auto` (по умолчанию) определяет язык отдельно для каждой
  диктовки, но с защитой: фразы короче 1,5 с откатываются к настроенному
  языку, а не полагаются на угадывание.
- **Nemotron 3.5**: 40 языков (список поставщика — не придуман здесь; см.
  карточку модели NVIDIA по ссылке в
  [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- Язык **интерфейса** (отдельная настройка) охватывает 15 языков выше и
  никогда не меняет распознавание речи.

## Сборка из исходного кода

Необходимое: Rust (rustup) + Node 22+ + pnpm. Только Windows (Win32 + WebView2).

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

### Реальные движки (функции opt-in)

для whisper.cpp нужны **libclang** и **CMake**; GPU-бэкенду Vulkan нужен
**Vulkan SDK** на этапе сборки (`glslc` компилирует шейдеры ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**Ловушка короткого каталога сборки (задокументировано, пережито):**
вложенный MSBuild внутри CMake (`vulkan-shaders-gen`) падает, если путь
длиннее ~240 символов (MSB3501) — короткий `CARGO_TARGET_DIR`, например
`C:\t`, полностью обходит проблему.

**Nemotron streaming (opt-in):** установите среду выполнения NeMoSpeech
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, официальный установщик NVIDIA),
затем `cargo test -p voxli-win --features nemo` — сборка выше подхватит её
сама; `build.rs` линкует SDK и копирует DLL среды выполнения рядом с
исполняемым файлом.

### Упаковка (NSIS-установщик одним файлом)

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

## Как это работает (механику можно проверить)

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

- **Одна диктовка за раз**: нажатие во время записи либо *явно
  отклоняется*, либо ставится в очередь (глубина 1) в зависимости от
  конфигурации — но никогда не отбрасывается молча.
- **Каскад вставки, три уровня**: прямая запись значения через UIA; иначе
  — буфер обмена + Ctrl+V с сохранением/восстановлением; иначе — окно
  спасения, где текст всё ещё лежит в буфере ядра.
- **Журнал — это контракт**: каждый важный переход — загрузка движка
  (`engine_loaded`), выбор вычислений (`calcul:…`), обрыв на полуслове,
  сбои восстановления буфера обмена — логируется без содержимого
  транскрипции (RF-34). Любому заявлению приложения соответствует строка,
  которую можно прочитать.

## Статус, честно

Реализовано и зелёное локально: весь workspace собирается, наборы тестов
проходят (`cargo test --workspace`, плюс наборы, включаемые feature-флагами,
для whisper, Nemotron streaming, SQLite/DPAPI, удалённого транспорта).
Десктопное приложение прогоняет поток записанной диктовки от начала до
конца на реальном аудио.

Ещё предстоит сделать (отслеживается, не скрывается): полевая кампания
измерений задержек/WER по матрице вставки из 12 приложений, сборка
Parakeet (backend существует как feature), подпись кода.

**Linux**: в планах, без обещанной даты. Слой движков (whisper.cpp, GGUF,
Vulkan) переносим; слой ввода, специфичный для Windows (вставка через UIA,
восстановление буфера обмена, глобальные горячие клавиши), сначала должен
получить свой Linux-эквивалент. Отслеживается в том же плане — ничего не
скрыто.

## Карта документации

Изучение продукта → требования → архитектура (`docs/01`–`docs/04`),
структура модулей (`docs/05`), план тестирования (`docs/06`), план
разработки (`docs/07`), внутренний аудит с исправлениями (`docs/08`),
дорожная карта (`docs/09`), гипотезы монетизации (`docs/10`), чек-лист
релиза (`docs/11`), UX-аудит (`docs/12`), приватность
(`docs/vie-privee.md`), поддерживаемые языки
(`docs/langues-supportees.md`). *Документация написана на французском;
комментарии в коде — на английском (первые читатели — англоязычные
разработчики).*

## Поддержка и спонсорство

Где найти журнал и как подать полезный отчёт: [`SUPPORT.md`](SUPPORT.md) ·
Сообщение об уязвимостях: [`SECURITY.md`](SECURITY.md) · Участие в
разработке: [`CONTRIBUTING.md`](CONTRIBUTING.md) (на французском) · Правила
поведения: [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Список изменений:
[`CHANGELOG.md`](CHANGELOG.md).

Если Voxli экономит вам время: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — «сердечки» изнутри
приложения ведут в те же места и никогда не появляются во время диктовки.

## Лицензия этого репозитория

Код Voxli распространяется под **двойной лицензией**
[`MIT`](LICENSE-MIT) **ИЛИ** [`Apache-2.0`](LICENSE-APACHE) — на ваш выбор.
Сторонние лицензии (модели, крейты, рантаймы) сохраняют свои условия: см.
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). *Лицензия продукта*
(опциональна, проверяется офлайн) — отдельное понятие и никогда не
блокирует локальные функции.
