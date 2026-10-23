<p align="center">
  <img src="assets/logo.svg" width="120" height="120" alt="Voxli logo" />
</p>

<h1 align="center">Voxli — Windows için yerel yapay zekâ diktesi</h1>

<div align="center">

[English](README.md) · [Français](README.fr.md) · [Español](README.es.md) · [Português](README.pt.md) · [Deutsch](README.de.md) · [Italiano](README.it.md) · [Nederlands](README.nl.md) · [Русский](README.ru.md) · [Polski](README.pl.md) · **Türkçe**

[![CI](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml/badge.svg)](https://github.com/SammyYohan/Voxli/actions/workflows/ci.yml)

</div>

> Windows'ta her yerde dikte edin — bir kısayola basın, konuşun, metin belirir.
> **%100 cihazda**: hesap yok, bulut yok, telemetri yok. Ses,
> size ait bir motorla (whisper.cpp veya Nemotron 3.5 streaming) yazıya
> dökülür, etkin alana eklenir ve pano, bıraktığınız haliyle geri getirilir.

---

## Neler kazanıyorsunuz

| | |
|---|---|
| **Canlı önizleme** | Hap, ekranınızın en üstünde yüzer ve *konuşurken* metni gösterir (Nemotron 3.5 streaming); boştayken ise sade bir noktaya döner. Odak alamaz, tıklamalar içinden geçer. |
| **Çevrimdışı motorlar** | **whisper.cpp** (95+ dil, CPU veya Vulkan GPU) ve **Nemotron 3.5 streaming 0.6B** (40 dil, canlı metin). İkisi de uygulamanın kendi sürecinde çalışır; sunucu yok, kendiniz kurmanız gereken bir SDK yok — çalışma zamanı DLL'lerini yükleyici beraberinde getirir. |
| **Dürüst hesaplama** | Réglages (Ayarlar) → *Transkripsiyon hesaplaması*: **Auto** bir GPU varsa onu kullanır, **CPU** onu zorla devre dışı bırakır, **Duo** canlı metni GPU'da oluşturur, son metin ise CPU'da kalır. Yazılan, gerçekte çalışandır — her motor yükleme işleminde `calcul:<choice>` günlüğe yazılır ve günlük, gerçek gecikme sürelerini saklar. |
| **Hiçbir şey kaybolmaz** | Yazıya dökülen metin, herhangi bir ekleme denenmeden *önce* çekirdek arabelleğinde durur; etkin uygulama kabul etmezse kurtarma penceresi onu alır. Pano kaydet/geri getir (RF-30), kopyaladığınızı olduğu gibi geri getirir. |
| **15 arayüz dili** | Windows'tan otomatik algılanır ya da Ayarlar → *Arayüz dili*'nden açıkça seçilir: fr · en · es · pt · de · it · nl · ru · pl · tr · ar (RTL) · hi · zh · ja · ko. |
| **Geçmiş, sözlükler, profiller** | Beklemede şifrelenen geçmiş (varsayılan olarak bellek içi; isteğe bağlı disk deposu), adlar ve jargon için dikte başına sözlükler, uygulamaya özgü kelime dağarcıkları. |
| **Lisans** | Kullanımı ücretsiz, çevrimdışı lisans doğrulaması dahildir (Ed25519, çevrimiçi kontrol gerekmez). |

## Kurulum (kullanıcı yolu)

1. Bu deponun **Releases** sayfasından `Voxli_0.1.0_x64-setup.exe` (tek dosyalık
   NSIS yükleyicisi) dosyasını indirin.
2. Çalıştırın: dosyalar `%LOCALAPPDATA%\Voxli` içine yerleşir (kullanıcı başına
   kurulum, WebView2 eksikse otomatik olarak kurulur, kaldırıcı dahildir).
3. İlk açılışta motorunuzu Ayarlar → **Modeller** içinden seçin —
   *Whisper base (57 MB)* satırı tek tıkla iner — sonra **Ctrl+Alt+Space**'i
   basılı tutup konuşun. `Shift+Alt+Z` son metni yeniden yapıştırır,
   `Shift+Alt+X` kurtarma penceresini açar.

**SmartScreen notu (dürüstlük):** Yükleyici henüz kod imzalı değil (sertifika
beklemede); bu yüzden Windows ilk açılışta bir uyarı gösterir —
*Daha fazla bilgi → Yine de çalıştır*'ı tıklayın. Voxli'nin *o andan sonra*
yaptıklarının tümü burada, açıkta doğrulanabilir.

**İndirme = açık kullanıcı eylemi.** Modeller yalnızca Ayarlar →
Modeller'den alınır; lisans ve boyut *herhangi bir aktarım öncesinde*
gösterilir ve gelen dosyanın SHA-256 değeri
`%APPDATA%\Voxli\models\manifests.json` içine kaydedilir.

## Model kataloğu (kasten sade tutulmuştur)

| Model | Ne olduğu | Boyut | Lisans |
|---|---|---|---|
| **Whisper base** (niceleştirilmiş) | Hızlı, çok dilli varsayılan; ilk indirme için ideal | 57 MB | MIT — ggerganov/whisper.cpp |
| **Whisper large-v3-turbo** (niceleştirilmiş) | En yüksek doğruluk; GPU önerilir | 574 MB | MIT — ggerganov/whisper.cpp |
| **Nemotron 3.5 ASR streaming** (q8_0) | Canlı metin akışı, 40 dil | 742 MB | OpenMDW-1.1 — NVIDIA Nemotron Speech |

Katalog, 22/10'da bilinçli olarak budandı: zayıf/gereksiz Whisper varyantları
(tiny/small/medium) ve Parakeet (sherpa-onnx arka ucu henüz dağıtılan
derlemeye derlenmediği için) artık indirme ekranını bulandırmıyor. Önceki
sürümlerce diskte bırakılmış dosyalar **Auto** üzerinden çalışmaya devam eder
— asla yok sayılmazlar. Eksiksiz atıflar:
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md).

## Gizlilik, tek paragrafta

Çekirdek hattı (yakalama → VAD → çıkarım → ekleme) makinenizden asla çıkmaz.
Voxli'nin yaptığı tek ağ trafiği, *tetiklediğiniz model indirmeleri* ve
(isteğe bağlı, opt-in derlemelerde) lisans doğrulamasıdır — bunu doğrulamak
için depo `scripts/prove-no-network.ps1` betiğini beraberinde getirir.
Yazıya dökülen metin, IPC köprüsünü yalnızca belgelenmiş iki durumda aşabilir
(kurtarma penceresi, kullanıcının açtığı geçmiş). Gizli hiçbir bilgi
`config.json` içinde saklanmaz (bunun yerine Windows Credential Manager /
DPAPI kullanılır). Ayrıntılar: [`docs/vie-privee.md`](docs/vie-privee.md)
(Fransızca).

## Dikte dilleri

- **Whisper**: 95+ dil — Ayarlar'daki açılır liste *yetkili listenin ta
  kendisidir*; `auto` (varsayılan) her diktede algılama yapar, bir koruma
  şartıyla: 1,5 s'den kısa ifadeler, dil tahmini yapmak yerine
  yapılandırılmış dile geri düşer.
- **Nemotron 3.5**: 40 dil (satıcı listesi — burada uydurulmadı; NVIDIA'nın
  model kartına [`docs/langues-supportees.md`](docs/langues-supportees.md)
  içindeki bağlantıdan bakın).
- **Arayüz** dili (ayrı bir ayar) yukarıdaki 15 dili kapsar ve konuşma
  tanımasını asla değiştirmez.

## Kaynaktan derleme

Önkoşullar: Rust (rustup) + Node 22+ + pnpm. Yalnızca Windows (Win32 + WebView2).

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

### Gerçek motorlar (opt-in özellikler)

whisper.cpp için **libclang** ve **CMake** gerekir; Vulkan GPU arka ucu,
derleme sırasında **Vulkan SDK** ister (`glslc`, ggml'nin shader'larını
derler):

```powershell
winget install --id LLVM.LLVM -e
winget install --id LunarG.VulkanSDK -e
$env:LIBCLANG_PATH = "C:\Program Files\LLVM\bin"
$env:VULKAN_SDK = "C:\VulkanSDK\<version>"
$env:CARGO_TARGET_DIR = "C:\t"        # SHORT path — see the trap below

cargo test -p voxli-asr --features whisper-gpu
cargo build --release -p voxli-desktop --features "custom-protocol,nemo-stream,gpu"
```

**Kısa hedef dizin (short-target-dir) tuzağı (belgelenmiş, yaşanmıştır):**
CMake içindeki iç içe MSBuild (`vulkan-shaders-gen`), ~240 karakteri aşınca
başarısız olur (MSB3501) — `C:\t` gibi kısa bir `CARGO_TARGET_DIR` bu tuzağı
bütünüyle atlatır.

**Nemotron streaming (opt-in):** NeMoSpeech çalışma zamanını kurun
(`%LOCALAPPDATA%\Programs\NeMoSpeech`, NVIDIA'nın resmî yükleyicisi), sonra
`cargo test -p voxli-win --features nemo` çalıştırın — yukarıdaki derleme onu
kendiliğinden devreye alır; `build.rs`, SDK'yı bağlar ve çalışma zamanı
DLL'lerini yürütülebilir dosyanın yanına kopyalar.

### Paketleme (tek dosyalık NSIS yükleyicisi)

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

## Nasıl çalışır (denetleyebileceğiniz mekanizmalar)

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

- **Aynı anda tek dikte**: kayıt sırasında gelen bir basış, yapılandırmaya
  göre *açıkça reddedilir* ya da kuyruğa alınır (derinlik 1) — asla sessizce
  yok sayılmaz.
- **Üç seviyeli ekleme zinciri**: doğrudan UIA değer yazımı; olmazsa
  pano + Ctrl+V, kaydet/geri getir ile birlikte; olmazsa kurtarma penceresi,
  metin çekirdek arabelleğinde duruyor.
- **Günlük, sözleşmedir**: önemli her geçiş — motor yüklendi
  (`engine_loaded`), hesaplama seçimi (`calcul:…`), kısmî kesilme, pano geri
  getirme arızaları — transkripsiyon içeriği olmadan günlüğe işlenir (RF-34).
  Uygulamanın iddia ettikleri, okuyabileceğiniz bir satırla eşleşir.

## Durum, dürüstçe

Yerelde uygulanmış durumda ve yeşil: tüm workspace derlenir ve test takımları
çalışır (`cargo test --workspace`, ayrıca whisper, Nemotron streaming,
SQLite/DPAPI ve uzak aktarım için özellik bayrağına bağlı takımlar). Masaüstü
uygulaması, kaydedilmiş bir dikte akışını gerçek ses üzerinde uçtan uca
geliştiriyor.

Henüz üretilmesi gerekenler (takip altında, gizli değil): 12 uygulamalı
ekleme matrisi üzerinden saha gecikme/WER ölçüm kampanyası, Parakeet
derlemesi (arka uç bir özellik olarak mevcut), kod imzalama.

**Linux**: yol haritasında, tarih sözü vermeden. Motor katmanı
(whisper.cpp, GGUF, Vulkan) taşınabilirdir; Windows'a özgü giriş katmanı
(UIA ile alan ekleme, panoyu geri yükleme, genel kısayollar) önce kendi
Linux karşılığını gerektiriyor. Geri kalanıyla aynı planda izleniyor —
hiçbir şey gizlenmiyor.

## Belgelendirme haritası

Ürün çalışması → gereksinimler → mimari (`docs/01`–`docs/04`), modül yapısı
(`docs/05`), test planı (`docs/06`), geliştirme planı (`docs/07`), iç denetim
ve düzeltmeler (`docs/08`), yol haritası (`docs/09`), para kazanma
varsayımları (`docs/10`), sürüm kontrol listesi (`docs/11`), UX denetimi
(`docs/12`), gizlilik (`docs/vie-privee.md`), desteklenen diller
(`docs/langues-supportees.md`). *Belgeler Fransızca yazıldı; kod yorumları
İngilizcedir (ilk okurlar, İngilizce konuşan geliştiricilerdir).*

## Destek ve sponsorluk

Günlüğü nerede bulacağınız ve işe yarar bir raporu nasıl göndereceğiniz:
[`SUPPORT.md`](SUPPORT.md) · Güvenlik açığı bildirimleri:
[`SECURITY.md`](SECURITY.md) · Katkıda bulunma:
[`CONTRIBUTING.md`](CONTRIBUTING.md) (Fransızca) · Davranış kuralları:
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) · Değişiklik günlüğü:
[`CHANGELOG.md`](CHANGELOG.md).

Voxli size zaman kazandırıyorsa: [**Ko-fi**](https://ko-fi.com/sammyyohan) ·
[**PayPal.me**](https://paypal.me/sammyyohan) — uygulama içindeki kalpler
aynı yerlere götürür, asla bir dikte sırasında değil.

## Bu deponun lisansı

Voxli kodu [`MIT`](LICENSE-MIT) **VEYA** [`Apache-2.0`](LICENSE-APACHE)
altında **çift lisanslıdır** — dilediğinizi seçin. Üçüncü taraf lisanslar
(modeller, crates, çalışma zamanları) kendi koşullarını korur: bkz.
[`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). *Ürün lisansı*
(isteğe bağlı, çevrimdışı doğrulama) ayrı bir kavramdır ve yerel özelliklerin
kullanımını asla kısıtlamaz.
