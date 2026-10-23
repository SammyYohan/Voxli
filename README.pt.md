<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Ditado com IA local para Windows</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · **Português** · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · [Türkçe](README.tr.md)

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Dite em qualquer lugar no Windows — pressione um atalho, fale, o texto aparece.
> **100 % no dispositivo**: sem conta, sem nuvem, sem telemetria. O áudio é
> transcrito por um mecanismo que você possui (whisper.cpp ou Nemotron 3.5 streaming),
> inserido no campo ativo, e a área de transferência volta como você a deixou.

---

## O que você recebe

| | |
|---|---|
| **Pré-visualização em tempo real** | A pastilha flutua no topo da tela e mostra o texto *enquanto você fala* (Nemotron 3.5 streaming), depois um ponto discreto quando ociosa. Não pode receber o foco e é transparente ao clique. |
| **Mecanismos offline** | **whisper.cpp** (95+ idiomas, CPU ou GPU Vulkan) e **Nemotron 3.5 streaming 0.6B** (40 idiomas, texto ao vivo). Ambos rodam no próprio processo; sem servidor, sem SDK que você precise instalar por conta própria — o instalador já traz as DLLs de runtime. |
| **Cálculo honesto** | Réglages (Configurações) → *Transcription compute*: **Auto** usa a GPU quando alguma está disponível, **CPU** desliga a GPU à força, **Duo** renderiza o texto ao vivo na GPU enquanto o texto final permanece na CPU. O que está escrito é o que roda — cada montagem de mecanismo registra `calcul:<choice>` e o journal guarda as latências reais. |
| **Nada se perde** | O texto transcrito fica no buffer central *antes* de qualquer tentativa de inserção; se o aplicativo ativo recusar, a janela de resgate o recebe. O salvar/restaurar da área de transferência (RF-30) devolve o que você tinha copiado. |
| **15 idiomas de interface** | Detectado automaticamente a partir do Windows, ou escolhido explicitamente em Configurações → *Interface language*: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Histórico, dicionários, perfis** | Histórico criptografado em repouso (em memória por padrão; armazenamento em disco opcional), dicionários por ditado para nomes e jargões, vocabulários específicos por aplicativo. |
| **Licença** | Uso gratuito, verificação de licença offline incluída (Ed25519, nenhuma verificação online necessária). |

## Instalação (caminho do usuário)

1. Baixe o `Voxli_0.1.0_x64-setup.exe` (instalador NSIS de arquivo único) na
   página de **Releases** deste repositório.
2. Execute-o: os arquivos ficam em `%LOCALAPPDATA%\Voxli` (instalação por
   usuário, WebView2 providenciado automaticamente se ausente, desinstalador incluído).
3. Primeira execução: escolha o seu mecanismo em Configurações → **Modèles** — a linha
   *Whisper base (57 MB)* baixa com um clique — depois mantenha **Ctrl+Alt+Space**
   pressionado e fale. `Shift+Alt+Z` cola novamente o último texto, `Shift+Alt+X` abre a
   janela de resgate.

**Nota sobre o SmartScreen (honestidade):** o instalador ainda não está assinado por
código (certificado pendente), então o Windows mostra um aviso na primeira
execução — clique em *More info → Run anyway*. Tudo o que o Voxli faz *depois*
disso é verificável aqui, de forma aberta.

**Downloads = ação explícita do usuário.** Os modelos só são obtidos de
Configurações → Modèles, com a licença e o tamanho exibidos *antes* de qualquer
transferência, e o SHA-256 do que chegou é registrado em `%APPDATA%\Voxli\models\manifests.json`.

## O catálogo de modelos (mantido enxuto de propósito)

| Modelo | O que é | Tamanho | Licença |
|---|---|---|---|
| **Whisper base** (quantizado) | Padrão multilíngue rápido, ideal como primeiro download | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (quantizado) | Precisão máxima, GPU recomendada | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Streaming de texto ao vivo, 40 idiomas | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

O catálogo foi deliberadamente enxugado em 22/10: variantes Whisper mais fracas/redundantes
(tiny/small/medium) e Parakeet (cujo backend sherpa-onnx ainda não está compilado
na build distribuída) não poluem mais a tela de download. Arquivos deixados no
disco por versões anteriores continuam funcionando via **Auto** — nunca são
ignorados. Atribuições completas: [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Privacidade, em um parágrafo

O pipeline central (captura → VAD → inferência → inserção) nunca sai da sua
máquina. O único tráfego de rede que o Voxli executa são *downloads de modelo
iniciados por você*, mais (opcional, builds opt-in) a verificação de licença — e o
repositório inclui `scripts/prove-no-network.ps1` para verificar isso. O texto
transcrito pode contornar a ponte IPC exclusivamente nos dois casos documentados
(janela de resgate, histórico aberto pelo usuário). Segredos nunca ficam no
`config.json` (em vez disso, Windows Credential Manager / DPAPI). Detalhes:
[`docs/vie-privee.md`](docs/vie-privee.md) (em francês).

## Idiomas de ditado

- **Whisper**: 95+ idiomas — a lista suspensa do Configurações *é* a lista
  autoritativa; `auto` (padrão) faz a detecção a cada ditado com uma proteção:
  enunciados com menos de 1,5 s recaem sobre o idioma configurado, em vez de adivinhar.
- **Nemotron 3.5**: 40 idiomas (lista do fornecedor — não inventada aqui; veja
  o model card da NVIDIA vinculado em [`docs/langues-supportees.md`](docs/langues-supportees.md)).
- O idioma da **interface** (uma configuração separada) cobre os 15 idiomas
  acima e nunca altera o reconhecimento de fala.

## Compilar a partir do código-fonte

Pré-requisitos: Rust (rustup) + Node 22+ + pnpm. Apenas Windows (Win32 + WebView2).

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

### Mecanismos reais (recursos opt-in)

O whisper.cpp precisa de **libclang** e **CMake**; o backend de GPU Vulkan precisa
do **Vulkan SDK** no momento da build (`glslc` compila os shaders do ggml):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**A armadilha do diretório de destinos curto (documentada, e sofrida):** o MSBuild
aninhado dentro do CMake (`vulkan-shaders-gen`) falha ao passar de ~240 caracteres
(MSB3501) — um `CARGO_TARGET_DIR` curto como `C:\t` contorna o problema por completo.

**Nemotron streaming (opt-in):** instale o runtime NeMoSpeech
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, instalador oficial da NVIDIA), depois
`cargo test -p voxli-win --features nemo` e a build acima o reconhece;
o `build.rs` vincula o SDK e copia as DLLs de runtime junto ao executável.

### Empacotamento (instalador NSIS de arquivo único)

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

## Como funciona (mecânica que você pode auditar)

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

- **Um ditado por vez**: um pressionamento durante a gravação é *recusado
  explicitamente*, ou entra na fila (profundidade 1) dependendo da configuração —
  nunca descartado em silêncio.
- **Cascata de inserção, três níveis**: escrita direta de valor via UIA; caso
  contrário, área de transferência + Ctrl+V com salvar/restaurar; caso
  contrário, a janela de resgate, com o texto ainda no buffer central.
- **O journal é o contrato**: cada transição importante — mecanismo montado
  (`engine_loaded`), escolha de cálculo (`calcul:…`), corte parcial, falhas de
  restauração da área de transferência — é registrada sem conteúdo de
  transcrição (RF-34). O que o app afirma encontra uma linha que você pode ler.

## Status, com honestidade

Implementado e verde localmente: o workspace completo compila e as suítes de
testes rodam (`cargo test --workspace`, além das suítes condicionadas por
feature para whisper, Nemotron streaming, SQLite/DPAPI e transporte remoto).
O app desktop desenvolve um fluxo de ditado gravado de ponta a ponta com áudio real.

Ainda a produzir (rastreado, não escondido): campanha de medição de latência/WER
em campo pela matriz de inserção de 12 apps, build do Parakeet (o backend existe
como feature) e assinatura de código.

## Mapa da documentação

Estudo de produto → requisitos → arquitetura (`docs/01`–`docs/04`), estrutura
de módulos (`docs/05`), plano de testes (`docs/06`), plano de desenvolvimento
(`docs/07`), auditoria interna com correções (`docs/08`), roadmap (`docs/09`),
hipóteses de monetização (`docs/10`), checklist de release (`docs/11`), auditoria
de UX (`docs/12`), privacidade (`docs/vie-privee.md`), idiomas suportados
(`docs/langues-supportees.md`). *Os documentos estão escritos em francês; os
comentários do código estão em inglês (os primeiros leitores são desenvolvedores
anglófonos).*

## Suporte e patrocínio

Onde encontrar o journal e como abrir um relatório útil:
[`SUPPORT.md`](SUPPORT.md) · Relato de vulnerabilidades: [`SECURITY.md`](SECURITY.md)
· Como contribuir: [`CONTRIBUTING.md`](CONTRIBUTING.md) (em francês) · Conduta:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Changelog:
[`CHANGELOG.md`](CHANGELOG.md).

Se o Voxli economiza o seu tempo: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — os corações no app levam aos
mesmos lugares, nunca durante um ditado.

## Licença deste repositório

O código do Voxli é **dual-licenciado** sob [`MIT`](LICENSE-MIT) **OU**
[`Apache-2.0`](LICENSE-APACHE) — escolha um deles. As licenças de terceiros (modelos,
crates, runtimes) mantêm seus próprios termos: veja
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). A *licença do produto*
(opcional, verificada offline) é um conceito separado e nunca bloqueia as
funcionalidades locais.
