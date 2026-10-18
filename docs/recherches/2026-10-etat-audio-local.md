# État des outils audio locaux (GRATUITS/open source) — octobre 2026

> **Collecte : 2026-10-17.** Sources primaires uniquement : releases/tags GitHub + API GitHub, fiches modèles Hugging Face + API HF, page Vosk d'Alpha Cephei, API crates.io. Aucun benchmark local exécuté. Chaque affirmation cite sa source ; ce qui n'a pas pu être confirmé sur une source primaire est marqué **[NON VÉRIFIÉ]** et repris en fin de document (§9).
>
> **Lecture Plume** : cible P1 = dictée push-to-talk locale, Windows x64 CPU uniquement, hors-ligne, énoncés de 3 à 30 s, budget mémoire moteur ≈ 210 MB. Ces cibles servent de filtre de lecture — ce ne sont pas des résultats mesurés ici.

---

## 1. Périmètre

- **Couvert** : ASR (reconnaissance vocale) exécutable localement, VAD (détection d'activité vocale), diarization (§5, court), chemins d'intégration Rust (§6).
- **Hors périmètre** : TTS (sauf note de licence), diarization approfondie, LLM, services cloud.
- **Règle de licence** : on distingue partout **licence du code** et **licence des poids**. Les poids non-OSI (CC-BY-NC, licence « gemma », tag HF `license:other`) sont signalés ⚠️.
- Les chiffres GPU (RTFx, leaderboard Open ASR) sont signalés comme tels ; ce document ne les retient que comme *ordre de grandeur*, pas comme preuve de perf CPU.

## 2. Tableau synthèse (ASR local)

| Moteur | Licence code | Licence poids | FR ? | RAM / emprise | Streaming ? | Intégration Rust | Force / précaution |
|---|---|---|---|---|---|---|---|
| [whisper.cpp v1.9.5](https://github.com/ggml-org/whisper.cpp) (6 oct. 2026) | MIT | Poids Whisper (OpenAI) : MIT | **oui** (multilingue Whisper) | README : tiny ≈ 273 MB → large ≈ 3.9 GB en RAM ; [large-v3-turbo-q5_0 = 547 MB](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md) sur disque | non natif (batch par énoncé) ; ex. `stream` [NON VÉRIFIÉ] | [whisper-rs 0.16.0](https://crates.io/crates/whisper-rs) | maturité, quantization, **Silero VAD intégré**, CI Windows x64/arm64. Précaution : RAM croît vite au-delà de `small` |
| [sherpa-onnx v1.13.8](https://github.com/k2-fsa/sherpa-onnx) (10 sep. 2026) | Apache-2.0 | dépend du modèle (Parakeet cc-by-4.0, Moonshine MIT, etc.) | **oui selon modèle** (Parakeet v3 `fr`, canary-1b-v2, zipformer streaming FR) | variable ; [Parakeet v2 int8 ≈ 661 MB de fichiers](https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8) | **oui** pour certains modèles (zipformer FR streaming, Moonshine streaming) | [crate officielle `sherpa-onnx` 1.13.8](https://crates.io/crates/sherpa-onnx) (⚠️ `sherpa-rs` community = déprécié) | un runtime ONNX, 12 bindings dont Rust, Windows x64/arm64. Précaution : qualité très variable selon modèle, écosystème dense à trier |
| [Parakeet TDT 0.6B v3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) (NVIDIA, via sherpa-onnx) | NeMo/code : Apache-2.0 | **cc-by-4.0** | **oui** — [FLEURS fr_fr WER 5.15](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3) | [q8_0.gguf = 714 MB](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/tree/main) (disque) | non (TDT = offline) | crate `sherpa-onnx` | meilleur couple qualité FR / vitesse (RTFx 3380 — GPU, [carte v2](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2)). Précaution : **preuves CPU Windows absentes** [NON VÉRIFIÉ] |
| [Moonshine](https://huggingface.co/moonshine-ai/moonshine-tiny) (tiny/base/streaming) | MIT | tiny/base(en) : MIT ; **variantes officielles multilingues : `license:other` ⚠️** | pas de variante officielle FR ; [finetune communautaire](https://huggingface.co/Cornebidouil/moonshine-tiny-fr) [NON VÉRIFIÉ] | tiny 27 M → base 61 M params ; streaming small 123 M | **oui** (famille streaming du 2026-01-06) | via sherpa-onnx (poids int8 [csukuangfj](https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-tiny-en-int8)) | latence très basse, streaming natif. Précaution : **pas de `fr` officiel à ce jour** |
| [faster-whisper](https://github.com/SYSTRAN/faster-whisper) | MIT | poids Whisper : MIT | **oui** | [small int8 ≈ 1477 MB (bench README)](https://github.com/SYSTRAN/faster-whisper) | non | **aucun chemin Rust maintenu** [NON VÉRIFIÉ] | vitesse CPU solide via CTranslate2. Précaution : Python-only |
| [Vosk](https://alphacephei.com/vosk/models) (Alpha Cephei) | [Apache-2.0](https://github.com/alphacep/vosk-api) | modèles fr officiels : Apache-2.0 (modèles LINTO etc. listés : AGPL ⚠️) | **oui** (modèles dédiés fr) | [small-fr = 41 MB](https://alphacephei.com/vosk/models) | **oui** (Kaldi) | bindings C/Python ; crate Rust tierce [NON VÉRIFIÉ] | ultra-léger, robuste offline. Précaution : [WER fr modestes : 23.95 (CommonVoice), 19.30 (MTEDX)](https://alphacephei.com/vosk/models) |
| [Canary-1B-v2](https://huggingface.co/nvidia/canary-1b-v2) (NVIDIA, via sherpa-onnx) | NeMo : Apache-2.0 | **cc-by-4.0** | **oui** (25 langues UE incl. `fr`) | ≈ 1 B params (taille fichiers non capturée) [NON VÉRIFIÉ] | non | crate `sherpa-onnx` ([v1.13.8 mentionne Canary](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8)) | FR multilingue ; perf CPU [NON VÉRIFIÉ]. ⚠️ [canary-1b original = cc-by-nc-4.0, non-OSI](https://huggingface.co/nvidia/canary-1b) |
| [Qwen3-ASR 0.6B / 1.7B](https://huggingface.co/Qwen) (janv./juin 2026) | Apache-2.0 | Apache-2.0 | oui (31 langues) | 0.6–1.7 B params — trop gros pour budget P1 | non | conversions [GGUF](https://huggingface.co/ggml-org) + runtime `transcribe.cpp` [NON VÉRIFIÉ pour Rust] | récent, licences propres. Précaution : très jeune, pas de chiffres FR CPU |
| [Voxtral](https://huggingface.co/mistralai/Voxtral-Mini-3B-2507) (Mistral) | Apache-2.0 ([Mini-4B-Realtime-2602](https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602) aussi) | Apache-2.0 ; ⚠️ [Voxtral-4B-TTS-2603 = cc-by-nc-4.0 (TTS)](https://huggingface.co/mistralai) | oui | 3–24 B params → hors budget CPU P1 | Mini Realtime : à étudier | — | qualité potentielle, médiatisation forte. Précaution : taille incompatible Plume ; TTS non-OSI |

## 3. Détail par moteur

### 3.1 whisper.cpp (ggml-org)

- **Dernière version vérifiée : [v1.9.5, publiée le 2026-10-06](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.9.5)** — release de maintenance passée à [ggml v0.26.0](https://github.com/ggml-org/ggml/releases/tag/v0.26.0). Repo : https://github.com/ggml-org/whisper.cpp — licence MIT.
- La v1.9.5 embarque des correctifs concrets pour ce projet : [compression VAD — rejet des modèles autres que 4 couches encoder (#4064)](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.9.5), `abort_callback` sur détection de langue (#4077), contrôle `ttype` whisper/parakeet (#4091). La [v1.9.4](https://github.com/ggml-org/whisper.cpp/releases) avait corrigé le décodage Parakeet TDT ([#4017](https://github.com/ggml-org/whisper.cpp/pull/4017)).
- **CI Windows couverte** : MSVC, MinGW, ARM64 (workflows CI visibles sur le dépôt ; l'upstream ggml a activé « Windows ARM64 + MSVC cl.exe »).
- **Poids** : les poids Whisper d'OpenAI (tiny → large-v3-turbo) sont distribués **MIT** (cartes OpenAI, licences indiquées sur la fiche du modèle correspondant). Script officiel : `models/download-ggml-model.sh` — cf. [models/README.md](https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md).
- **Emprise disque** (models/README.md) : tiny 75 MiB → large 2.9 GiB ; **large-v3-turbo-q5_0 : 547 MiB** ; section quantization dédiée (Q5_0/Q5_1/Q8_0 etc.).
- **RAM à l'exécution** (tableau du README) : tiny ≈ 273 MB, base ≈ 388 MB, small ≈ 852 MB, medium ≈ 2.1 GB, large ≈ 3.9 GB.
- **Silero VAD intégré en première classe** : `models/download-vad-model.sh silero-v6.2.0` (poids hébergés via ggml-org/whisper-vad) + options dédiées : `--vad-threshold`, `--vad-min-speech-duration-ms`, `--vad-max-speech-duration-s`, `--vad-speech-pad-ms`, `--vad-samples-overlap` (README du dépôt). C'est un argument fort pour un pipeline push-to-talk → utterance → ASR.
- **Pas de streaming au noyau** : whisper.cpp décode des fenêtres complètes (batch). L'exemple `examples/stream` fait du fenêtrage (~5 s) mais son adéquation à une latence push-to-talk **[NON VÉRIFIÉ]**. En pratique Plume segmente par énoncé (3–30 s), ce que whisper.cpp fait très bien hors-ligne.
- **Limites documentaires** : pas de chiffres WER FR ni de bench RTF CPU publiés dans le dépôt [NON VÉRIFIÉ] ; la RAM à l'exécution d'un modèle q5 quantifié (ex. turbo-q5_0 ≈ 547 MB disque) n'est pas documentée [NON VÉRIFIÉ].
- Bindings : liste officielle des bindings dans le README, dont **Rust : [whisper-rs](https://crates.io/crates/whisper-rs)**.

### 3.2 sherpa-onnx (k2-fsa)

- **Dernière version vérifiée : [v1.13.8, 2026-09-10](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8)** — [repo](https://github.com/k2-fsa/sherpa-onnx), licence Apache-2.0. Notes de v1.13.8 : corrections d'hallucinations silencieuses (Moonshine v2, Qwen3-ASR, Canary, FunASR-Nano), support Intel NPU via OpenVINO, paramétrage de fenêtre du pipeline pyannote (diarization).
- [v1.13.5](https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.5) : montée à **onnxruntime 1.27.1**, export du modèle streaming `nemotron-3.5-asr-streaming`, export Moonshine → QNN, et un exemple Flutter listant **Parakeet TDT v2 (en) / v3 (25 langues UE)**, Qwen3 ASR 0.6B, **et un modèle zipformer streaming FR**.
- **Plateformes** : Windows x64 **et arm64** documentés dans les releases ; bindings dans **12 langages** dont C/C++, Rust, Python, C#, Java/Kotlin, Swift…
- **Modèles hébergés à jour** par le mainteneur core (csukuangfj) sur HF, prêts à l'emploi : cf. §3.2.1, §3.2.3.
- Lire la doc modèles en ligne (dossier `docs/` du dépôt) avant tout choix : les « sherpa-onnx-… » identiques sous deux auteurs (csukuangfj vs csukuangfj2, quantized vs int8 vs original) prêtent à confusion ; ce document cite systématiquement l'URL exacte du dépôt de poids.

#### 3.2.1 Parakeet TDT 0.6B v2 / v3 (NVIDIA)

- **v2** (en) : https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2 — poids **cc-by-4.0**. **RTFx 3380** annoncé sur la carte (chiffre HuggingFace Open ASR leaderboard, batch GPU) — *chiffre GPU*.
- **v3** (25 langues européennes incl. **fr**) : https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 — poids **cc-by-4.0**, publiée 2025-08-14, papier lié arXiv 2509.14128. **WER FLEURS fr_fr = 5.15** (carte NVIDIA). ≈ 0.6 B params fp32.
- Fichiers vérifiés sur l'arbre HF de v3 : `model.safetensors` 2.51 GB, `.nemo` 2.51 GB, **`parakeet-tdt-0.6b-v3.q8_0.gguf` 714 MB** (https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/tree/main).
- **Version « prête sherpa-onnx »** (par le mainteneur core) : https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8 — `encoder.int8.onnx` 652 MB + `decoder.int8.onnx` 7.2 MB + joiner ≈ 1.7 MB.
- ⚠️ **Les cartes NVIDIA sont orientées inference GPU/CUDA ; le chemin CPU x64 Windows existe (sherpa-onnx décode ces modèles en CPU multi-thread) mais aucune perf WER/RTF FR CPU n'est publiée** [NON VÉRIFIÉ] → à bencher (cf. §7).
- Runtime parallèle C++ du projet NeMo (« Speech.cpp ») mentionne aussi le `.gguf` q8_0 — signal de portabilité CPU, sans chiffres FR [NON VÉRIFIÉ].

#### 3.2.2 Canary (NVIDIA)

- **canary-1b-v2** : https://huggingface.co/nvidia/canary-1b-v2 — **cc-by-4.0**, publié ~2025-08-04, 25 langues européennes dont **fr**. Pris en charge par sherpa-onnx (release v1.13.8).
- ⚠️ **canary-1b (original) est cc-by-nc-4.0 → non-OSI** : https://huggingface.co/nvidia/canary-1b — à ne pas utiliser si exigence OSI stricte.
- canary-180m-flash, canary-1b-flash, canary-qwen-2.5b : **cc-by-4.0** (org NVIDIA).
- Aucun WER FR sur CPU capturé [NON VÉRIFIÉ].

#### 3.2.3 Moonshine (moonshine-ai)

- **tiny / base (en)** : MIT — 27 M / 61 M params (oct. 2024) : https://huggingface.co/moonshine-ai/moonshine-tiny, https://huggingface.co/moonshine-ai/moonshine-base
- **Famille streaming (2026-01-06)** : Tiny 34 M / Small 123 M / Medium 245 M, WER LibriSpeech `clean` 4.49 / 2.49 / 2.08 — https://huggingface.co/moonshine-ai/moonshine-streaming-small (licence MIT sur la carte).
- **`moonshine-streaming-tiny-es`** (espagnol) : MIT, 27.0 M params, **WER FLEURS es = 6.34** — https://huggingface.co/moonshine-ai/moonshine-streaming-tiny-es
- ⚠️ **Aucune variante FR officielle à ce jour** (recherche API HF du 2026-10-17). Les variantes officielles multilingues (es/ja/ar/uk/ko/zh/vi) sont taggées **`license:other`** (pas MIT) — texte de licence exact non capturé [NON VÉRIFIÉ].
- **Finetunes FR communautaires** (non officiels) : https://huggingface.co/Cornebidouil/moonshine-tiny-fr (MIT, créée 2026-01-27, entraînée sur `facebook/multilingual_librispeech`) et port ONNX https://huggingface.co/onnx-community/moonshine-tiny-fr-ONNX (MIT). **Aucun WER publié [NON VÉRIFIÉ] → qualité inconnue.**
- Prêt sherpa-onnx : https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-tiny-en-int8 (en).
- Conclusion pour Plume : profil latence/streaming excellent, **mais pas de `fr` officiel documenté** → écarté en l'état (cf. §7).

#### 3.2.4 Qwen3-ASR (Alibaba) et noveltés 2025–2026

- **Qwen3-ASR-0.6B / 1.7B** (+ variantes `-hf` compatibles Transformers) : **Apache-2.0**, 31 langues dont fr, publiés 2026-01-28 (originaux) et 2026-06-26 (variantes -hf) — org https://huggingface.co/Qwen
- Conversions **GGUF** disponibles (org ggml-org, unslothai) et runtime C++ `transcribe.cpp` (handy-computer) — chemin Rust non établi [NON VÉRIFIÉ].
- **Gemma-3n (Google)** : tag de licence `license:gemma` — **non-OSI** (exigence Google « confirm required »). Recherche : https://huggingface.co/models?search=gemma-3n (fiche exemple : https://huggingface.co/google/gemma-3n-E4B). ⚠️ hors périmètre si exigence OSI stricte.

### 3.3 faster-whisper (SYSTRAN)

- https://github.com/SYSTRAN/faster-whisper — MIT, repose sur CTranslate2 (MIT).
- **Bench publié dans le README** : avec le modèle `small` quantifié **int8** — ~13 minutes d'audio transcrit en **1 min 42 s** (i7-12700K, 8 threads), **≈ 1477 MB de RAM**. Bon indicateur ordinateur de bureau, *hors* budget P1 (≈ 210 MB).
- **Aucun binding Rust maintenu** trouvé pour faster-whisper/CTranslate2 (recherche crates.io) [NON VÉRIFIÉ].
- Intérêt pour Plume : référence de vitesse CPU Whisper int8 ; mais l'intégration passerait par un process séparé Python (coût mémoire + IPC) → non priorisé.

### 3.4 Vosk (Alpha Cephei)

- Page modèles officielle : https://alphacephei.com/vosk/models — repo : https://github.com/alphacep/vosk-api (Apache-2.0).
- **vosk-model-small-fr-0.22 : 41 MB, Apache-2.0** — WER **23.95** (CommonVoice `test`) et **19.30** (MTEDX) ; **vosk-model-fr-0.22 : 1.4 GB, Apache-2.0**.
- **Streaming natif Kaldi** (le VAD/le décodage incrémental est le cœur du projet), très léger — le small-fr tient largement dans le budget P1 en taille disque (RAM réelle à vérifier au bench).
- ⚠️ Dans la liste des modèles fr, plusieurs contributeurs tiers (LINTO, daanzu) sont sous **AGPL** — vérifier la licence de chaque modèle téléchargé : l'AGPL est un poison pour une intégration appli fermée.
- Qualité FR nettement en dessous des modèles 2025–2026 (WER ~20-24 % vs 5 % pour Parakeet v3 sur FLEURS) — à considérer uniquement comme solution de secours (fallback).

### 3.5 Voxtral (Mistral) — mention

- [Voxtral-Mini-3B-2507](https://huggingface.co/mistralai/Voxtral-Mini-3B-2507), [Voxtral-Small-24B-2507](https://huggingface.co/mistralai/Voxtral-Small-24B-2507) : Apache-2.0 (juil. 2025).
- **Voxtral-Mini-4B-Realtime-2602** (2026-01-21) : Apache-2.0, > 2 M de téléchargements — https://huggingface.co/mistralai/Voxtral-Mini-4B-Realtime-2602 — piste « streaming » la plus médiatisée, mais 4B params → **hors budget CPU P1**.
- ⚠️ **Voxtral-4B-TTS-2603 est cc-by-nc-4.0 (non-OSI)** — TTS, pas ASR ; à retenir uniquement si un jour Plume veut du TTS « non-OSI » pour des raisons pratiques.

## 4. VAD

### 4.1 Silero VAD — recommandé

- Dépôt : https://github.com/SNakers4/silero-vad — **MIT**, poids ONNX. **Dernier tag vérifié : v6.2.3** (⚠️ les versions longtemps citées « v4/v5 » sont antérieures ; la famille actuelle est v6.x).
- Claim README : **< 1 ms par chunk audio** (ONNX) ; implémentations de référence Python/JS listées.
- **Intégration directe dans whisper.cpp** : cf. §3.1 (modèle `silero-v6.2.0` téléchargé via whisper.cpp ; cf. poids hébergés ggml-org/whisper-vad). La v1.9.5 durcit même la validation du modèle VAD (#4064) — signal d'une intégration entretenue.
- **Intégration via sherpa-onnx** : le modèle silero VAD est intégré aux exemples/docs sherpa-onnx → accessible depuis Rust.
- Pour un usage Rust maison (sans sherpa-onnx ni whisper.cpp) : charger le ONNX via **ort** (§6) — chemin simple et léger.

### 4.2 VAD WebRTC (py-webrtcvad) — repli léger

- https://github.com/wiseman/py-webrtcvad — wrapper Python/ C du VAD de WebRTC (Google). **≈ 2500 étoiles** ; dernier push 2024-07 ; **licence : fichier LICENSE non standard (GitHub rapporte NOASSERTION)** [⚠️].
- Très léger CPU mais décision par frame ±30 ms, sans modèle neuronal — sensibilité/robustesse inférieure à Silero (aperçu classique du projet, *non vérifié par bench ici*).
- Pour Rust, il faut embarquer le code C WebRTC (ou une crate tierce) — pas de chemin crate officiel vérifié [NON VÉRIFIÉ].

**Verdict VAD** : Silero (via whisper.cpp natif, sherpa-onnx ou ort) est le choix rationnel ; webrtcvad reste l'option de repli ultra-légère au cas où la dépendance ONNX devait poser problème.

## 5. Diarization (court — décision P3)

- **pyannote/speaker-diarization-3.1** : MIT — https://huggingface.co/pyannote/speaker-diarization-3.1
- **pyannote/speaker-diarization-community-1** (2025-04) : cc-by-4.0 — modèle « consolidé » plus rapide revendiqué. **pyannote/speaker-diarization-precision** (2025-09) : licence non capturée [NON VÉRIFIÉ]. Org : https://huggingface.co/pyannote
- sherpa-onnx v1.13.8 ajoute un paramètre de fenêtre au pipeline pyannote → chemin d'exécution possible côté CPU, mais l'empilement (VAD → embeddings → clustering) reste lourd, orienté PyTorch/GPU dans son implémentation de référence.
- **Recommandé P3**, hors enveloppe P1 : coût mémoire et complexité non justifiés pour une dictée mono-locutrice.

## 6. Chemins Rust d'intégration

| Crate | Version / màj | URL | Remarques |
|---|---|---|---|
| `whisper-rs` | **0.16.0** (màj 2026-03-12, ≈ 1.6 M téléchargements) | https://crates.io/crates/whisper-rs | binding whisper.cpp ; crate système `whisper-rs-sys` ; **repo déplacé sur Codeberg** (`git.codeberg.org/tazz4843/whisper-rs`, cf. métadonnées crates.io) ; licence crate « Unlicense » (API crates.io) |
| `sherpa-onnx` (officielle) | **1.13.8** (màj 2026-09-11) | https://crates.io/crates/sherpa-onnx | binding officiel k2-fsa, Apache-2.0, suit les releases sherpa-onnx |
| `sherpa-rs` (thewh1teagle) | **0.6.8** (dernière release 2025-10-05) | https://github.com/thewh1teagle/sherpa-rs | **DÉPRÉCIÉ** — le README renvoie à la crate officielle `sherpa-onnx` |
| `ort` | **2.0.0-rc.14** (màj 2026-10-12) | https://crates.io/crates/ort | binding ONNX Runtime (v1.31), maintenu par pyke.io — pour pipelines VAD/ASR maison (Silero, Parakeet int8, Moonshine ONNX) |
| ONNX Runtime | 1.31 | https://github.com/microsoft/onnxruntime | **licence MIT** (API GitHub vérifiée le 2026-10-17) |

- **Stratégie** : `whisper-rs` si Plume garde whisper.cpp ; `sherpa-onnx` (crate officielle) si Plume bascule sur Parakeet/Qwen3/Canary/Moonshine ; `ort` pour un chemin « à la main » (Silero VAD + modèle ONNX) minimale en dépendances.
- **Aucun chemin Rust maintenu pour faster-whisper** [NON VÉRIFIÉ] — écarter.
- ⚠️ ne plus utiliser `sherpa-rs` community (déprécié) dans du code neuf.

## 7. Recommandation pour Plume P1

Trois options pesées contre les cibles P1 (FR, CPU x64, ≈ 210 MB, énoncés 3–30 s, offline) :

1. **Option retenue — garder whisper.cpp (quantized) + Silero VAD intégré**
   - Tout est en place : binding Rust maintenu (`whisper-rs` 0.16.0), VAD Silero première classe (flags `--vad-*`), quantizations officielles, licences **100 % MIT** (code + poids), CI Windows x64/arm64 vérifiée.
   - Choix du modèle selon budget RAM réel à mesurer : **large-v3-turbo-q5_0 (547 MB disque)** pour la qualité FR si le budget le permet, sinon `small`/`base` quantized (tiny ≈ 273 MB / base ≈ 388 MB / small ≈ 852 MB en RAM selon README — à re-mesurer en quantifié).
   - Risques indiqués au §9 (RAM réelle quantifiée, pas de WER FR public).
2. **Option challenger — Parakeet TDT v3 via sherpa-onnx (crate officielle)**
   - Meilleure qualité FR publiée au regard des licences propres (cc-by-4.0) : **WER FLEURS fr 5.15**, poids q8_0 de 714 MB, variantes int8 prêtes pour sherpa-onnx.
   - Mais toutes les preuves de publication sont GPU → **mesurer (bench) avant toute bascule** : WER (CommonVoice fr + FLEURS fr), RTF x64 multi-thread, RAM pic, latence premier mot, sur la plateforme Plume.
   - S'organiser en ticket **P2** (bench A/B), ne pas déplacer P1 pour une hypothèse non mesurée.
3. **Option écartée — Moonshine tiny FR**
   - Pas de variante FR officielle et le seul finetune (Cornebidouil, MIT) n'a **aucun WER publié** [NON VÉRIFIÉ]. Surveiller à horizon trimestriel (la famille streaming + port ONNX évolue vite).

**Note licences** : l'option retenue respecte l'exigence OSI côté poids (MIT) ; le challenger Parakeet (cc-by-4.0) est OSI-compatible y compris pour un usage commercial. En revanche, **ne jamais s'appuyer** sur les poids ⚠️ recensés en §3 : canary-1b (cc-by-nc-4.0), Voxtral-4B-TTS (cc-by-nc-4.0), gemma-3n (`license:gemma`), variantes multilingues officielles Moonshine (`license:other`), modèles Vosk LINTO/daanzu (AGPL).

## 8. Sources

**whisper.cpp / ggml**
- https://github.com/ggml-org/whisper.cpp
- https://github.com/ggml-org/whisper.cpp/releases/tag/v1.9.5 (publiée 2026-10-06)
- https://github.com/ggml-org/ggml/releases/tag/v0.26.0
- https://github.com/ggml-org/whisper.cpp/blob/master/models/README.md (tailles, quantization, RAM)
- https://github.com/ggml-org/whisper.cpp/pull/4017 (fix Parakeet TDT, v1.9.4)
- (changelog v1.9.5 : #4064 VAD, #4077 abort_callback, #4091 parakeet ttype)

**sherpa-onnx / k2-fsa**
- https://github.com/k2-fsa/sherpa-onnx
- https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.8 (2026-09-10)
- https://github.com/k2-fsa/sherpa-onnx/releases/tag/v1.13.5 (onnxruntime 1.27.1, zipformer FR streaming, Parakeet v2/v3, Qwen3, Moonshine QNN)

**Hugging Face — poids et licences**
- https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2 (cc-by-4.0, RTFx 3380)
- https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3 (cc-by-4.0, 25 langues, FLEURS fr 5.15)
- https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3/tree/main (fichiers : safetensors 2.51 GB, q8_0.gguf 714 MB)
- https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v2-int8 (encoder int8 652 MB, decoder 7.2 MB, joiner 1.7 MB)
- https://huggingface.co/nvidia/canary-1b-v2 (cc-by-4.0) ; https://huggingface.co/nvidia/canary-1b (cc-by-nc-4.0 ⚠️)
- https://huggingface.co/moonshine-ai/moonshine-tiny ; …/moonshine-base ; …/moonshine-streaming-small ; …/moonshine-streaming-tiny-es (MIT)
- https://huggingface.co/Cornebidouil/moonshine-tiny-fr ; https://huggingface.co/onnx-community/moonshine-tiny-fr-ONNX
- https://huggingface.co/csukuangfj/sherpa-onnx-moonshine-tiny-en-int8
- https://huggingface.co/Qwen (Qwen3-ASR-0.6B / 1.7B / -hf, Apache-2.0)
- https://huggingface.co/mistralai/Voxtral-Mini-3B-2507 ; …/Voxtral-Small-24B-2507 ; …/Voxtral-Mini-4B-Realtime-2602 (Apache-2.0) ; Voxtral-4B-TTS-2603 (cc-by-nc-4.0 ⚠️)
- https://huggingface.co/models?search=gemma-3n ; https://huggingface.co/google/gemma-3n-E4B (`license:gemma` ⚠️)
- https://huggingface.co/pyannote/speaker-diarization-3.1 (MIT) ; …/speaker-diarization-community-1 (cc-by-4.0) ; …/speaker-diarization-precision
- API HF (listing) : https://huggingface.co/api/models?search=moonshine&limit=100 (confirme les tags `license:other` des variantes multilingues + l'existence du finetune fr)

**Vosk**
- https://alphacephei.com/vosk/models (small-fr-0.22 41 MB, WER 23.95 / 19.30 ; fr-0.22 1.4 GB)
- https://github.com/alphacep/vosk-api

**VAD / diarization**
- https://github.com/SNakers4/silero-vad (MIT, tag v6.2.3)
- https://github.com/wiseman/py-webrtcvad (NOASSERTION ⚠️, dernier push 2024-07)
- https://github.com/pyannote/pyannote-audio

**faster-whisper**
- https://github.com/SYSTRAN/faster-whisper (bench small int8, RAM 1477 MB)

**Rust / runtime**
- https://crates.io/crates/whisper-rs (0.16.0, 2026-03-12) + https://crates.io/api/v1/crates/whisper-rs
- https://crates.io/crates/sherpa-onnx (1.13.8, 2026-09-11) + https://crates.io/api/v1/crates/sherpa-onnx
- https://crates.io/crates/ort (2.0.0-rc.14, 2026-10-12)
- https://github.com/thewh1teagle/sherpa-rs (0.6.8, déprécié)
- https://github.com/microsoft/onnxruntime (MIT)

## 9. Points non vérifiés

1. **WER/RTF FR sur CPU x64 Windows pour Parakeet v3 (int8/q8_0)** — les cartes NVIDIA sont orientées GPU ; sherpa-onnx permet le décodage CPU mais aucun chiffre FR CPU n'a été trouvé → bench A/B à conduire (P2).
2. **RAM réelle d'exécution des modèles whisper.cpp quantisés** (ex. large-v3-turbo-q5_0 : 547 MB est une taille disque) — le tableau RAM du README couvre les modèles fp32 classiques.
3. **WER FR et RTF publiés pour whisper.cpp** — absents du dépôt (le README ne publie pas de chiffres FR).
4. **Le streaming `examples/stream` de whisper.cpp** (fenêtrage ~5 s) — existence et adéquation à la latence push-to-talk non revérifiées.
5. **Aucun chemin Rust maintenu pour faster-whisper / CTranslate2** — recherche crates.io non exhaustive, à confirmer si option réétudiée.
6. **Qualité du finetune FR Moonshine** (`Cornebidouil/moonshine-tiny-fr`) — aucun WER publié ; **texte exact des licences `license:other` des variantes multilingues Moonshine** non capturé.
7. **WER FR CPU pour canary-1b-v2 et Qwen3-ASR** — non capturés (cartes lues pour les licences).
8. **Perf Windows ARM64** — les CI ggml/sherpa couvrent les builds arm64 ; les perfs ne sont pas documentées.
9. **Licence de `pyannote/speaker-diarization-precision`** — non capturée.
10. **RAM à l'exécution de vosk-model-small-fr-0.22** (41 MB disque) — non mesurée.

---

*Document généré pour Plume — `docs/recherches/` — 2026-10-17. Méthode : collecte de sources primaires (GitHub/HF/crates.io) sans benchmark local ; toute mise à jour doit re-vérifier les versions citées (moteurs et crates) avant usage.*
