# Livrable 5 — Structure des modules

Principe directeur : **modules profonds**. Une petite interface stable, une implémentation cachée,
un point de couture (seam) testable par module. Aucun module ne connaît les détails d'un autre
(le pipeline ne sait pas qu'il parle à whisper.cpp ; l'inséreur ne sait pas qu'il y a un moteur ASR).

---

## 1. Arborescence du dépôt

```
Plume/
├── Cargo.toml                     # workspace Rust
├── crates/
│   ├── plume-core/                # types partagés, machine à états, orchestrateur, ports
│   ├── plume-config/              # schéma de configuration, migrations, import/export
│   ├── plume-audio/               # capture WASAPI, pré-roll, conversion 16 kHz mono, VAD
│   ├── plume-asr/                 # trait Transcriptor + moteurs (whisper, parakeet, distant)
│   ├── plume-text/                # insertion : UIA, presse-papiers, SendInput, profils
│   ├── plume-hotkeys/             # raccourcis globaux, conflits, reconfiguration à chaud
│   ├── plume-storage/             # historique chiffré, purge, rétention
│   ├── plume-secure/              # DPAPI / Credential Manager, chiffrement local
│   ├── plume-license/             # vérification Ed25519, droits, période de grâce
│   ├── plume-diagnostics/         # journaux rédigés, mesures locales, export de rapport
│   └── plume-win/                 # interop Win32 isolée (fenêtres, privilèges, presse-papiers bas niveau)
├── apps/
│   └── desktop/
│       ├── src-tauri/             # application Tauri : commandes IPC, cycle de vie, tray
│       └── src/                   # interface TypeScript (réglages, historique, onboarding)
├── docs/                          # ces livrables
└── tests/
    ├── fixtures/audio/            # corpus de recette (silence, bruit, FR métier)
    └── e2e/                       # scénarios de recette instrumentés
```

**Règle de dépendance** : `plume-core` ne dépend d'aucun autre crate interne. Les implémentations
(whisper, ONNX, UIA, DPAPI) sont injectées dans le noyau via les traits définis dans `plume-core`.
`apps/desktop` est le seul endroit qui assemble le tout.

---

## 2. Responsabilités et interfaces

### 2.1 `plume-core` — orchestrateur et machine à états

Rôle : la logique métier pure, testable sans Windows, sans audio, sans GPU. C'est le module le plus
profond du projet.

```rust
pub enum DictationState {
    Idle,
    Recording { target: FocusSnapshot, started: Instant },
    Finalizing { target: FocusSnapshot },
    Transcribing { target: FocusSnapshot, queued: Option<PendingRequest> },
    PostProcessing { target: FocusSnapshot },
    Inserting { target: FocusSnapshot },
    Recovery { transcript: Transcript, reason: InsertFailure },
}

pub struct FocusSnapshot { /* handle opaque + app id + titre + capacités */ }

pub trait Capture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<AudioBuffer, CaptureError>;
    fn level(&self) -> Level; // pour l'indicateur visuel
}

pub trait VoiceActivity {
    fn contains_speech(&self, audio: &AudioBuffer) -> SpeechVerdict;
}

pub trait Transcriptor: Send + Sync {
    fn warmup(&self) -> Result<(), AsrError>;
    fn transcribe(&self, audio: &AudioBuffer, opts: &TranscribeOptions) -> Result<Transcript, AsrError>;
    fn id(&self) -> EngineId; // pour les journaux et la licence
}

pub trait PostProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError>;
}

pub trait Inserter {
    fn plan(&self, target: &FocusSnapshot, profile: &Profile) -> InsertPlan;
    fn insert(&self, text: &str, plan: &InsertPlan) -> Result<InsertReport, InsertError>;
}

pub trait Entitlement {
    fn feature(&self, f: Feature) -> bool; // limites Free/Pro, jamais de dégradation silencieuse
}
```

Points de couture testables : tous les traits ci-dessus sont remplaçables par des doubles de test,
ce qui permet de couvrir **toutes** les branches de la machine à états (succès, silence, erreur
moteur, échec d'insertion, focus perdu, appui pendant transcription) sans matériel ni Windows.

### 2.2 `plume-audio`

Interface publique : `Capture` (+ l'énumération des périphériques et un test de niveau).
Implémentation cachée : cpal/WASAPI, tampon circulaire de pré-roll de 400 ms, mixage mono,
ré-échantillonnage filtré vers 16 kHz f32, rééchantillonnage paresseux si le périphérique est déjà
à 16 kHz (chemin rapide), comptage des discontinuités.
Testable : par injection d'un `AudioSource` synthétique (fichiers WAV de recette) — aucun micro requis
en CI.

### 2.3 `plume-asr`

Interface publique : `Transcriptor`, `TranscribeOptions` (langue, amorce de vocabulaire, verbatim,
budget de threads), `Transcript` (texte, segments, confiance, durée, moteur, modèle).
Implémentations cachées :
- `WhisperCppEngine` (FFI `whisper-rs`) : chargement du modèle, paramètres anti-hallucination,
  amorce par `initial_prompt`, quantification choisie selon le matériel.
- `ParakeetOnnxEngine` (sherpa-onnx) : chargement ONNX, langue automatique ou forcée, threads intra-op.
- `RemoteEngine` (facultatif) : HTTP, avec vérification du consentement **côté appelant**.
Testable : `FakeEngine` déterministe + corpus WAV → texte attendu, sans GPU ni modèle lourd en CI
(un petit modèle `tiny` est utilisé pour un test d'intégration « fumée »).

### 2.4 `plume-text`

Interface publique : `Inserter`, `InsertPlan`, `InsertReport`, `FocusSnapshot` (lecture seule des
capacités : sensible ? éditable ? multiligne ?).
Implémentation cachée : UI Automation, gestion du presse-papiers (sauvegarde multi-format,
marquage anti-historique, restauration différée), `SendInput` Unicode, fenêtre de secours native,
profils par application.
Testable : le plan d'insertion (`plan()`) est une **fonction pure** de `(capacités du champ, profil)
→ ordre des niveaux` : entièrement testable hors Windows. L'exécution réelle est couverte par la
recette manuelle instrumentée (matrice d'applications).

### 2.5 `plume-hotkeys`

Interface publique : `HotkeyRegistry::rebind(binding) -> Result<(), HotkeyError>`, flux d'événements
`Pressed`/`Released`, détection de conflit, mode « sans crochet ».
Implémentation cachée : plugin global-shortcut, temporisation de relâchement, crochet optionnel.
Testable : la politique de résolution de conflit et la normalisation des combinaisons sont pures.

### 2.6 `plume-storage`, `plume-secure`, `plume-license`

- `plume-storage` : historique (activer/désactiver, rétention, suppression unitaire et totale, export),
  schéma versionné, requêtes bornées. Interface volontairement minimale : `append`, `recent`, `delete`,
  `purge`.
- `plume-secure` : `SecretStore` (get/set/delete) sur Credential Manager ou DPAPI ; `Protect`/`Unprotect`
  pour chiffrer des blobs liés à l'utilisateur. Un `SecretStore` en mémoire sert de double de test.
- `plume-license` : `verify(license_blob, public_key) -> Result<Entitlement, LicenseError>` —
  fonction **pure** sur des octets, testable avec un jeu de licences valides, expirées, altérées,
  d'un autre produit.

### 2.7 `plume-diagnostics`

Interface publique : `log_event(Event)` (structure, sans contenu) ; `redact()` ; `export_report()`
(archive de diagnostic destinée au support, avec **rédaction forcée** et avertissement avant envoi).
C'est le module qui rend RF-34 vérifiable : un test de fuite par chaînes sentinelles s'exécute à
chaque build.

---

## 3. Surface IPC entre le noyau et l'interface

Principe : le webview **observe** et **configure**. Il ne déclenche jamais une insertion et ne voit
jamais l'audio.

### 3.1 Événements (noyau → UI), émis par le thread pipeline (`worker.rs`)

| Sens | Message | Charge utile | Remarque |
|---|---|---|---|
| Noyau → UI | `state://changed` | état de la machine à états, horodatage | Aucun contenu de texte |
| Noyau → UI | `meter://level` | niveau normalisé (échantillonné 1/2 tick, ~100 ms) | Passif : jamais enregistré, jamais transmis |
| Noyau → UI | `session://finished` | durée, moteur, niveau d'insertion, succès | Aucun contenu |
| Noyau → UI | `notice://notice` | `kind` + message FR neutre (`strings.rs`) | `nothing`, `busy`, `inserted`, `refused`, `max-warning`, `locked`, `queued`, `retarget`, `hotkey` — **addition documentée à cette table** ; aucun événement ne contient de texte dicté (T-34) |
| Noyau → UI | `dictation://partial` | hypothèse transitoire du streaming (`text`, horodatage) | **Addition documentée à cette table** (amendement ADR-02, P2 streaming) : affichage en direct pendant la prise uniquement, jamais journalisé ni conservé (RF-34), effacé à la fin de prise — exception 3 ci-dessous |
| Noyau → UI | `models://progress` / `models://done` | `id`, octets reçus / total | **Seule** voie réseau consentie : téléchargement de modèles (RF-20, RF-36) |

** Exceptions documentées d'exfiltration de contenu du noyau (les seules) :
1. **Fenêtre de secours** (RF-31) — texte du tampon interne, jamais par le canal d'événements.
2. **Historique** (RF-33) — lecture choisie par l'utilisateur, supprimable à tout instant
   (`history.delete` / `history.purge`) ; chiffré au repos (DPAPI).
3. **Affichage partiel streaming** (P2, amendements ADR-02) — hypothèses transitoires du moteur
   Nemotron pendant la prise (`dictation://partial`) : pur affichage pastille, **jamais** écrites
   (journal, disque, historique). Le texte inséré sort **soit** des segments finalisés du flux
   (voie rapide — Amendement 2, aucun re-décodage offline), **soit** de la voie batch sur le
   tampon complet (pré-roll inclus) ; jamais d'un partiel transitoire.

### 3.2 Commandes (UI → noyau), fichier `commands.rs`

| Sens | Commande | Charge utile | Remarque |
|---|---|---|---|
| UI → Noyau | `configGet` / `configSet` | configuration complète | Validation + clamp côté noyau ; `configSet` sauvegarde, **re-registre les raccourcis à chaud** (RF-01) et pousse `ConfigChanged` au pipeline (§2.6) |
| UI → Noyau | `stateGet` | `{ state, engine }` | Métadonnées, aucun contenu |
| UI → Noyau | `rescueLast` / `rescuePasteAgain` / `rescueRetry` / `rescueClose` | — | Exception RF-31 ; « coller » et « réessayer » ne peuvent insérer QUE le texte du tampon interne |
| UI → Noyau | `cancelDictation` | — | Annulation (RF-09) |
| UI → Noyau | `diagnosticsExport` | chemin de destination optionnel | Rédige le rapport (ni dictée, ni presse-papiers) |
| UI → Noyau | `devicesList` | — | Liste des micros (RF-08 : identifiant cpal stable) |
| UI → Noyau | `modelsList` / `modelsDownload` / `modelsDownloadCancel` / `modelsRemove` | `id` | Catalogue + SHA-256 consigné (RF-20) ; un seul téléchargement à la fois |
| UI → Noyau | `historyQuery` / `historyDelete` / `historyPurge` | `n`, `id` | Lecture bornée ; contenu affiché car l'UI est locale sans ressource distante (§3.1 exception 2) |
| UI → Noyau | `licenseStatus` / `licenseImport` | blob de licence | Vérification locale Ed25519 hors ligne (ADR-08) ; persisté seulement si valide |
| UI → Noyau | `dictionariesImport` / `dictionariesExport` | `profile`, CSV | Dictionnaire utilisateur RF-23 (cible|variantes) ; poussé au pipeline |
| UI → Noyau | `contextPreview` | — | RF-37 : app + titre uniquement, refus explicite en `contextRead: none`, **jamais le contenu du champ** |
| UI → Noyau | `dataExport` | destination optionnelle | Configuration + dictionnaires + empreintes de modèles ; **jamais** de dictée ni de secret |

**Interdits** : aucune commande ne permet d'insérer du texte arbitraire dans une application, d'accéder à
l'audio, de lire le presse-papiers, ou d'ouvrir une voie réseau non consignée ici. Toute commande future
doit être ajoutée à cette table **et** à la surface de permissions Tauri (capabilities), avec le minimum
de droits.

---

## 4. Configuration (schéma)

Fichier `%APPDATA%\Plume\config.json`, versionné, lisible, exportable. Extrait :

```jsonc
{
  "schema": 4,
  "general": {
    "language": "fr",
    "engine": "auto",                 // auto | whisper | parakeet | nemo ;
                                      // auto : nemo (modèle présent) → whisper → parakeet → fake
    "model": "auto",
    "postProcessing": "rules",        // none | rules | local_model
    "history": { "enabled": true, "storage": "memory", "retentionDays": 30 }
  },
  "hotkeys": {
    "pushToTalk": "Ctrl+Alt+Space",
    "pasteLastTranscript": "Shift+Alt+Z",
    "showLastTranscript": "Shift+Alt+X",
    "useLowLevelHook": false,
    "mode": "hold"                    // hold | toggle | double-push-lock (RF-02)
  },
  "audio": {
    "device": "default",
    "preRollMs": 400,
    "maxRecordingSeconds": 300,
    "vad": { "enabled": true, "minSpeechMs": 250, "aggressiveness": "balanced" }
  },
  "insertion": {
    "rememberTargetAtStart": true,
    "clipboardHoldMs": { "default": 500, "slowApps": 3000, "remoteDesktop": 5000 },
    "excludeFromClipboardHistory": true,
    "excludedApps": ["keepass.exe", "keepassxc.exe", "1password.exe", "bitwarden.exe"]
  },
  "privacy": {
    "strictLocalMode": true,
    "contextRead": "none",            // none | app_and_title | app_title_and_selection (RF-37)
    "telemetry": "off",
    "updates": "notify",
    "consent": { "remoteAsr": false, "cloudPost": false } // RF-36 : tout défaut = false
  },
  "profiles": {
    "default": { "style": "punctuated", "dictionary": [] },
    "code":    { "match": ["Code.exe"], "style": "verbatim", "dictionary": [] },
    "email":   { "match": ["OUTLOOK.EXE"], "style": "punctuated", "dictionary": [] }
  }
}
```

**Règles** : chaque champ a une valeur par défaut sûre (privacy d'abord) ; toute écriture passe par le
noyau, jamais directement par le webview ; toute évolution incrémente `schema` et déclenche une
migration testée.

---

## 5. Arborescence des données

```
%APPDATA%\Plume\
├── config.json                  # réglages (aucun secret)
├── history.db                   # historique (optionnel, chiffré, lié au compte)
├── dictionaries\                # dictionnaires importés/exportés (.csv, .json)
└── logs\                        # journaux rédigés, rotation, purge automatique

%LOCALAPPDATA%\Plume\
├── models\                      # modèles téléchargés (taille annoncée dans l'UI)
│   ├── ggml-small-q5_0.bin
│   ├── parakeet-tdt-0.6b-v3-int8\
│   └── manifests.json           # empreintes, dates, provenance
└── cache\                       # audio temporaire de récupération d'erreur (chiffré, purgé)
```

**Secrets** : jamais dans ces dossiers. Windows Credential Manager (clés d'API) ou blobs DPAPI.

---

## 6. Conventions de développement

| Sujet | Convention |
|---|---|
| Types partagés | Dans `plume-core`, jamais dans les implémentations |
| Erreurs | Un type d'erreur par module, `thiserror` ; aucun `unwrap()` sur un chemin utilisateur en production, `expect()` réservé aux invariants documentés |
| Journalisation | Via `plume-diagnostics` uniquement ; jamais `println!` |
| Chaînes visibles | En français, externalisées (aucune chaîne d'interface en dur dans le code Rust) |
| Interop Win32 | Concentrée dans `plume-win` ; les autres modules n'appellent jamais l'API Windows directement |
| Tests | Un module sans test de son interface publique n'est pas considéré comme terminé |
| Benchmarks | Un banc de mesure par module critique (audio, asr, insertion) exécutable en local |
| Sécurité | `unsafe` interdit hors `plume-win` et FFI moteurs, chaque bloc documenté et justifié |
| Dépendances | Toute nouvelle dépendance exige : licence + mainteneur + dernière publication + poids du binaire |

---

## 7. Ce qui n'est pas dans cette structure (et pourquoi)

| Absent | Raison |
|---|---|
| Module « notifications » dédié | Fusionné dans le noyau et `plume-win` tant que le volume de code le justifie |
| Module « cloud » dédié | Le moteur distant est un `Transcriptor` parmi d'autres, derrière la garde de consentement (`plume-core`) |
| Service Windows / mode session 0 | Refusé (ADR-07 / §7 de `docs/03`) |
| Sidecar Python | Refusé (ADR-03) |
| Moteur « à la demande » téléchargeable | Séduisant mais complexe (signature de code au chargement) ; réévalué après P2 |
