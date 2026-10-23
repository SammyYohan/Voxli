<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Dictado con IA local para Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · **Español** · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dicta donde quieras en Windows — pulsa un atajo, habla y el texto aparece.
> **100 % en tu dispositivo**: sin cuenta, sin nube, sin telemetría. El audio
> lo transcribe un motor que es tuyo (whisper.cpp o Nemotron 3.5 streaming),
> se inserta en el campo activo y el portapapeles queda como lo dejaste.

---

## Lo que obtienes

| | |
|---|---|
| **Vista previa en vivo** | La pastilla flota en la parte superior de la pantalla y muestra el texto *mientras hablas* (Nemotron 3.5 streaming), y un punto discreto cuando está inactiva. No recibe el foco y es transparente a los clics. |
| **Motores sin conexión** | **whisper.cpp** (95+ idiomas, CPU o GPU Vulkan) y **Nemotron 3.5 streaming 0.6B** (40 idiomas, texto en vivo). Ambos se ejecutan en el propio proceso; sin servidor y sin SDK que tengas que instalar por tu cuenta — el instalador incluye las DLL de runtime. |
| **Cómputo honesto** | Réglages (Ajustes) → *Cómputo de transcripción*: **Auto** usa la GPU cuando hay una disponible, **CPU** la desactiva, **Duo** renderiza el texto en vivo en la GPU mientras el texto final se queda en la CPU. Lo que está escrito es lo que se ejecuta — cada montaje de motor registra `calcul:<choice>` y el diario guarda latencias reales. |
| **Nada se pierde** | El texto transcrito vive en el búfer del núcleo *antes* de cualquier intento de inserción; si la aplicación activa lo rechaza, lo recibe la ventana de rescate. El guardado/restauración del portapapeles (RF-30) devuelve lo que tenías copiado. |
| **15 idiomas de interfaz** | Detectados automáticamente desde Windows, o elegidos explícitamente en Ajustes → *Idioma de la interfaz*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Historial, diccionarios, perfiles** | Historial cifrado en reposo (en memoria por defecto; almacén en disco opcional), diccionarios de dictado para nombres y jerga, vocabularios específicos por aplicación. |
| **Licencia** | Libre de usar, con verificación de licencia sin conexión incluida (Ed25519, sin comprobación en línea). |

## Instalar (ruta del usuario)

1. Descarga `Voxli_0.1.0_x64-setup.exe` (instalador NSIS de un solo archivo) desde la
   página **Releases** de este repositorio.
2. Ejecútalo: los archivos se instalan en `%LOCALAPPDATA%\Voxli` (instalación por usuario,
   WebView2 se instala automáticamente si falta, desinstalador incluido).
3. Primer arranque: elige tu motor en Ajustes → **Modelos** — la línea *Whisper
   base (57 MB)* se descarga con un clic — luego mantén pulsado **Ctrl+Alt+Space**
   y habla. `Shift+Alt+Z` vuelve a pegar el último texto, `Shift+Alt+X` abre la
   ventana de rescate.

**Nota sobre SmartScreen (sinceridad):** el instalador aún no está firmado con código
(certificado pendiente), por lo que Windows muestra una advertencia en la primera ejecución — haz clic en
*Más información → Ejecutar de todas formas*. Todo lo que Voxli hace *después* de eso
es verificable aquí, a la vista de todos.

**Descargar = acción explícita del usuario.** Los modelos solo se obtienen desde
Ajustes → Modelos, con la licencia y el tamaño visibles *antes* de cualquier transferencia, y el
SHA-256 de lo recibido se registra en `%APPDATA%\Voxli\models\manifests.json`.

## El catálogo de modelos (recortado a propósito)

| Modelo | Qué es | Tamaño | Licencia |
|---|---|---|---|
| **Whisper base** (cuantizado) | Rápido y multilingüe por defecto, primera descarga ideal | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (cuantizado) | Precisión máxima, GPU recomendada | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Streaming con texto en vivo, 40 idiomas | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

El catálogo se recortó deliberadamente el 22/10: las variantes Whisper más débiles o
redundantes (tiny/small/medium) y Parakeet (cuyo backend sherpa-onnx aún no está
compilado en la build que se distribuye) ya no saturan la pantalla de descargas. Los archivos que
versiones anteriores dejaron en el disco siguen funcionando mediante **Auto** — nunca
se ignoran. Atribuciones completas: [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Privacidad, en un párrafo

El pipeline central (captura → VAD → inferencia → inserción) nunca sale de tu
máquina. El único tráfico de red que Voxli realiza son las *descargas de modelos que tú
inicias*, más (opcional, en compilaciones opt-in) la verificación de licencia — y el
repositorio incluye `scripts/prove-no-network.ps1` para verificarlo. El texto transcrito
puede eludir el puente IPC únicamente en los dos casos documentados
(ventana de rescate, historial abierto por el usuario). Los secretos nunca viven en `config.json`
(en su lugar, Administrador de credenciales de Windows / DPAPI). Detalles:
[`docs/vie-privee.md`](docs/vie-privee.md) (en francés).

## Idiomas de dictado

- **Whisper**: 95+ idiomas — el desplegable de Ajustes *es* la lista
  autoritativa; `auto` (por defecto) hace una detección en cada uso con una salvaguarda: los enunciados
  de menos de 1,5 s vuelven al idioma configurado en lugar de adivinar.
- **Nemotron 3.5**: 40 idiomas (lista del fabricante — no la inventamos aquí; ver la
  ficha del modelo de NVIDIA, enlazada en [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- El idioma de **interfaz** (una opción aparte) cubre los 15 idiomas
  anteriores y nunca cambia el reconocimiento de voz.

## Compilar desde el código fuente

Requisitos previos: Rust (rustup) + Node 22+ + pnpm. Solo para Windows (Win32 + WebView2).

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

### Motores reales (features opt-in)

whisper.cpp necesita **libclang** y **CMake**; el backend GPU Vulkan necesita
el **Vulkan SDK** en tiempo de compilación (`glslc` compila los shaders de ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**La trampa del directorio de destino corto (documentada, sufrida):** el MSBuild anidado dentro de
CMake (`vulkan-shaders-gen`) falla pasados ~240 caracteres (MSB3501) — un
`CARGO_TARGET_DIR` corto como `C:\t` lo esquiva por completo.

**Nemotron streaming (opt-in):** instala el runtime de NeMoSpeech
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, instalador oficial de NVIDIA), luego
`cargo test -p voxli-win --features nemo` y la compilación de arriba lo toma en cuenta;
`build.rs` enlaza el SDK y copia las DLL del runtime junto al ejecutable.

### Empaquetado (instalador NSIS de un solo archivo)

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

## Cómo funciona (mecánica que puedes auditar)

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

- **Un dictado a la vez**: una pulsación durante la grabación se *rechaza
  explícitamente*, o se encola (profundidad 1) según la configuración — nunca
  se descarta en silencio.
- **Cascada de inserción, tres niveles**: escritura directa del valor mediante UIA; si no,
  portapapeles + Ctrl+V con guardado/restauración; si no, la ventana de rescate con el
  texto aún en el búfer del núcleo.
- **El diario es el contrato**: cada transición importante — motor
  montado (`engine_loaded`), elección de cómputo (`calcul:…`), corte parcial,
  fallos al restaurar el portapapeles — se registra sin contenido de
  transcripción (RF-34). Lo que la aplicación afirma viene avalado por una línea que puedes leer.

## Estado, con honestidad

Implementado y en verde en local: el workspace completo compila y las suites de pruebas
se ejecutan (`cargo test --workspace`, además de las suites condicionadas por features para whisper,
Nemotron streaming, SQLite/DPAPI, transporte remoto). La aplicación de escritorio
recorre de extremo a extremo, con audio real, un flujo de dictado grabado.

Queda por producir (con seguimiento, no oculto): campaña de medición de latencia/WER en campo
sobre la matriz de inserción de 12 aplicaciones, compilación de Parakeet (el backend existe
como feature), firma de código.

## Mapa de la documentación

Estudio del producto → requisitos → arquitectura (`docs/01`–`docs/04`), estructura de
módulos (`docs/05`), plan de pruebas (`docs/06`), plan de desarrollo (`docs/07`),
auditoría interna con correcciones (`docs/08`), hoja de ruta (`docs/09`), hipótesis de
monetización (`docs/10`), checklist de lanzamiento (`docs/11`), auditoría de UX (`docs/12`),
privacidad (`docs/vie-privee.md`), idiomas soportados
(`docs/langues-supportees.md`). *La documentación está escrita en francés; los comentarios
del código están en inglés (los primeros lectores son desarrolladores angloparlantes).*

## Soporte y patrocinio

Dónde encontrar el diario y cómo presentar un informe útil:
[`SUPPORT.md`](SUPPORT.md) · Notificar vulnerabilidades: [`SECURITY.md`](SECURITY.md)
· Contribuir: [`CONTRIBUTING.md`](CONTRIBUTING.md) (francés) · Conducta:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Historial de cambios:
[`CHANGELOG.md`](CHANGELOG.md).

Si Voxli te ahorra tiempo: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — los corazones dentro de la aplicación llevan
a los mismos sitios, nunca durante un dictado.

## Licencia de este repositorio

El código de Voxli está publicado con **doble licencia** bajo [`MIT`](LICENSE-MIT) **O**
[`Apache-2.0`](LICENSE-APACHE) — elige la que prefieras. Las licencias de terceros (modelos,
crates, runtimes) conservan sus términos propios: ver
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). La *licencia del producto*
(opcional, se verifica sin conexión) es un concepto aparte y nunca condiciona las
funciones locales.
