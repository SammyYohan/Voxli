# Recherche moteurs ASR open source & installation via Hugging Face

*Date : 19 octobre 2026 — recherche web à froid (post-M1.5). Contexte : dictée
française locale sur CPU portable (i5-10210U, pas de GPU), l'application doit
rester simple à installer pour l'utilisateur final.*

## 1. Panorama des moteurs open source actuels

Sources croisées : Gladia « Best open-source speech-to-text models in 2026 »,
Northflank « Best open source STT model in 2026 (benchmarks) », AssemblyAI,
OpenWhispr « Parakeet vs Whisper vs Nemotron », docs officielles HF
(nvidia/parakeet-tdt-0.6b-v3, nvidia/nemotron-3.5-asr-streaming-0.6b,
onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4), sherpa-onnx 1.3
(prétrained models), issue sherpa-onnx #2918.

| Moteur | Taille | Français | Streaming réel | CPU portable | Licence | Verdict Voxli |
| --- | --- | --- | --- | --- | --- | --- |
| **Nemotron 3.5 ASR Streaming 0.6B** (NVIDIA) | ~600 Mo (int8 ONNX) | ✅ fr-FR/fr-CA (40 locales) | ✅ natif (chunks 80-1120 ms, latence < 100 ms) | ✅ (cache-aware FastConformer) | ouverte (open-weight NVIDIA) | **Cible n°1 pour les partiels** |
| **Parakeet TDT 0.6B v3** (NVIDIA) | ~670-680 Mo int8 | ✅ (25-26 langues européennes) | ❌ pseudo-streaming seulement (re-décodage, issue sherpa-onnx #2918) | ✅ RTFx > 2000 en batch | CC-BY-4.0 | Batch rapide optionnel (M2+) |
| **Whisper** (ggml, q5_1) | 70 Mo (base) → 500 Mo+ | ✅ 99+ langues | ❌ (batch + fenêtrage maison) | ⚠️ ~2,5× temps réel sur i5 | MIT | **Socle actuel** : écosystème, robustesse, filtre hallucination RF-18 |
| **Qwen3-ASR 0.6B / 1.7B** (Alibaba, janv. 2026) | 0,6-1,7 Md | ✅ (30-52 langues) | ✅ (chunked) | ⚠️ écosystème jeune | ouverte | À surveiller, pas d'export sherpa-onnx établi |
| **Moonshine** (Useful Sensors) | 245 M | ❌ anglais | ✅ | ✅ edge | MIT/CC | Hors périmètre (pas de fr) |
| **Canary-Qwen 2.5B** (NVIDIA) | 2,5 Md | ❌ (en/es/de/fr pour Canary-180m-flash seulement) | ❌ batch | ❌ lourd | ouverte | Non |
| **Vosk** | 50 Mo-1,8 Go | ✅ | ✅ | ✅ | Apache 2.0 | Génération précédente, WER dépassé |

Chiffres d'accuracy (Open ASR Leaderboard, fin 2026) : Parakeet v3 ≈ 6,34 % WER
moyen contre ≈ 7,4 % pour Whisper large-v3 — et Parakeet est 4× plus petit.
Nemotron 3.5 Streaming : FLEURS fr ≈ 12,6 % WER (correct, pas exceptionnel ;
la ponctuation/casse natives sont un vrai plus pour la dictée).

### Verdict pour Voxli

1. **Socle inchangé** : whisper.cpp + ggml base q5_1 reste le batch par défaut
   (fiable, léger, écosystème, filtre RF-18 déjà en place).
2. **Changement de cible pour les partiels d'affichage** : abandonner l'idée
   d'un streaming parakeet (pseudo-streaming seulement) au profit de
   **Nemotron 3.5 ASR Streaming via sherpa-onnx** — export officiel
   `sherpa-onnx-nemotron-3.5-asr-streaming-0.6b-560ms-int8-2026-06-11`
   (encoder/decoder/joiner int8 + tokens.txt, chunk 560 ms, prompt de langue
   auto ou explicite). C'est le seul combo open source **vrai streaming +
   français + CPU portable + moteur natif Rust disponible** (sherpa-onnx a des
   bindings et un démo Rust officiel).
3. Parakeet v3 int8 reste pertinent comme moteur batch rapide alternatif si
   whisper s'avère trop lent sur les machines cibles.
4. Qwen3-ASR à réévaluer dans 6-12 mois (écosystème + exports).

## 2. Téléchargement depuis Hugging Face : pratiques robustes

Vérifié contre la doc officielle HF (« Downloading models », protocole Xet)
et les forums Hub :

- URL canonique d'un fichier : `https://huggingface.co/{dépôt}/resolve/main/{fichier}`
  → redirection 302 vers un CDN (`cdn-lfs-us-1.hf.co`, `cas-bridge.xethub.hf.co`,
  `us.aws.cdn.hf.co`…). Les clients doivent suivre les redirections ; les
  environnements filtrés doivent allowlister `huggingface.co` **et** les
  domaines CDN — un `ReadTimeout` en plein transfert signifie en général que
  le hub a répondu mais que le CDN est instable (exactement le symptôme
  observé chez l'utilisateur le 19/10 : connexion réinitialisée à ~50 %).
- Le CDN accepte `Range` / `206 Partial Content` → la reprise par plage déjà
  implémentée dans Voxli est la bonne approche ; **la rendre automatique**
  (backoff, `.part` conservé) est le correctif appliqué le 19/10 dans
  `commands.rs` (`download_model_thread`/`transfer_attempt`).
- Le protocole Xet (nouveaux dépôts) signe des plages d'octets ; pour les
  fichiers LFS classiques l'URL `resolve` reste la voie simple et supportée.
- Taille et empreinte fiables : en-têtes `X-Linked-Size` / `X-Linked-Etag`
  (ETag = SHA-256 pour les LFS) sur la réponse `resolve` — utilisables pour
  afficher la taille avant téléchargement et confronter l'empreinte après
  (Voxli le fait déjà via sa référence de confiance).
- Repli réseau : un miroir (`hf-mirror.com`) peut servir d'endpoint secondaire
  configurable ; à considérer seulement si les coupures se reproduisent malgré
  la reprise automatique.

## 3. Système d'installation HF « intuitif et simple » — design proposé

Principe : **un seul bouton, zéro connaissance requise**. Ce qui existe déjà
couvre 80 % du chemin (catalogue UI, barre de progression, reprise Range,
vérification empreinte, `ModelInstalled` → recharge du moteur à chaud,
`engine=auto` qui choisit le meilleur moteur disponible).

Compléments proposés (par ordre de valeur) :

1. **Bouton « Installer le moteur recommandé »** en tête de la page Modèles :
   selon la langue d'interface (`ui_language`) et `engine=auto`, il enchaîne
   tout seul le téléchargement du bundle recommandé puis l'activation —
   l'utilisateur n'a qu'UN clic, jamais de choix de fichier.
2. **Bundles multi-fichiers déclarés dans le catalogue** : le catalogue actuel
   liste des fichiers uniques ou une archive ; généraliser aux dépôts HF
   multi-fichiers (Nemotron sherpa = encoder/decoder/joiner/tokens) en
   déclarant, par bundle, la liste `{url, taille, sha256}` — la boucle de
   téléchargement itère, l'archive existante reste un cas particulier.
3. **Cartes moteur claires** : pour chaque moteur — langues, taille, licence,
   WER approximatif en français, état (compilé / installé / téléchargeable /
   indisponible sur cette build), et un seul bouton d'action. Éviter les
   radios qui proposent un moteur non compilé sans explication.
4. **Messages d'échec honnêtes et actionnables** : « connexion interrompue à
   50 % — reprise automatique dans 4 s (essai 3/8) » au lieu d'une erreur
   brute (liens avec les événements `téléchargement-reprise-auto` ajoutés au
   journal le 19/10).
5. **Option avancée** (repliée sous « Avancé ») : champ URL personnalisée pour
   un miroir ou un GGUF arbitraire compatible whisper.cpp.

## 4. Suites (propositions, hors périmètre immédiat)

- M2 (machine outillée) : compiler `sherpa-onnx` + feature `nemo-stream`,
  intégrer Nemotron 3.5 Streaming 560 ms int8 comme `streaming` du bundle,
  avec le GGUF déjà téléchargé (707 Mo) utilisable ensuite comme batch nemo
  si le SDK NeMo est outillé — sinon le garder en simple téléchargement
  inutilisé et le signaler dans l'UI.
- Réévaluer Qwen3-ASR quand un export sherpa-onnx officiel paraîtra.
