//! IPC commands — webview ⇄ core surface (docs/05 §3).
//!
//! Principle: the webview only **observes** and **configures**. It never
//! sees audio; it triggers no arbitrary text insertion.
//!
//! Commands (IPC table, updated per docs/05 §3):
//! - P0: `configGet` / `configSet` / `stateGet` / `rescueLast` /
//!   `rescuePasteAgain` / `rescueRetry` / `rescueClose` / `cancelDictation` /
//!   `diagnosticsExport`;
//! - P1: `devicesList` (RF-08), `modelsList` / `modelsDownload` /
//!   `modelsDownloadCancel` / `modelsRemove` (RF-20), `historyQuery` /
//!   `historyDelete` / `historyPurge` (RF-33), `licenseImport` /
//!   `licenseStatus` (RF-38), `dataExport`, `dictionariesImport` /
//!   `dictionariesExport` (RF-23), `dictionaryPreview` (RF-37);
//! - extra events documented in the table: `notice://notice`,
//!   `meter://level`, `models://progress`, `state://changed` (worker.rs).
//!
//! Guard T-34: no command returns dictated text, except the two
//! documented exceptions (RF-31 rescue window, RF-33 encrypted data export
//! — that is an explicit user action).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, Wry};

use voxli_config::Config;
use voxli_diagnostics::{Event, Logger};
use voxli_license::Entitlement as LicenseEntitlement;

use crate::strings;
use crate::worker::WorkerMsg;

/// Shared state: main thread ⇄ webview ⇄ pipeline thread. Every field is
/// `Send + Sync`; access goes through `Mutex` with poison recovery
/// (never panic on a poisoned lock).
pub struct UiShared {
    /// Pipeline thread input queue (dictation events).
    pub tx: Sender<WorkerMsg>,
    /// Durable configuration held in memory; the disk is its mirror.
    pub config: Mutex<Config>,
    pub config_path: PathBuf,
    /// Shared logger: pipeline thread + commands.
    pub logger: Arc<Logger>,
    /// "Rescue text" buffer shown in the RF-31 window; documented
    /// exception: mirror of the orchestrator's internal buffer (RF-22).
    pub last: Mutex<Option<String>>,
    /// Current state name (state machine docs/04 §3), never content.
    pub state: Mutex<String>,
    /// Loaded engine id (metadata, RF-34).
    pub engine: Mutex<String>,
    /// Verified license (RF-38): nothing durable in memory without the
    /// Ed25519 signature. Reloaded at startup, updated by import.
    pub license: Mutex<LicenseInfo>,
    /// History store (RF-33): shared between the worker (write at the
    /// end of a session) and the IPC commands (read/delete).
    pub history: Mutex<Option<Box<dyn voxli_storage::HistoryStore>>>,
    /// Model download in progress (a single one at a time, RF-20).
    pub download_busy: Mutex<bool>,
    pub download_cancel: Arc<AtomicBool>,
}

/// License state as seen by the UI (RF-38/RF-39) — never keys, never
/// any secret: holder + features + expiration.
#[derive(Clone, Debug, Serialize)]
pub struct LicenseInfo {
    /// `free` | `active` | `grace` | `expired`.
    pub state: String,
    pub licensee: String,
    pub features: Vec<String>,
    pub expires: Option<String>,
    /// Expiration date in seconds (drives the "days remaining" display).
    pub expires_at_secs: Option<u64>,
}

impl LicenseInfo {
    pub fn free() -> Self {
        Self {
            state: "free".into(),
            licensee: "gratuit".into(),
            features: Vec::new(),
            expires: None,
            expires_at_secs: None,
        }
    }
}

/// Panic-free helper to access a mutex's contents.
pub(crate) fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ------------------------------------------------------------ license ----

/// Verifies the license file (`license.txt`) against the embedded public
/// key; offline grace period of 30 days (RF-38/ADR-08).
pub fn license_from_file(path: &std::path::Path) -> LicenseInfo {
    let Some(key) = voxli_license::embedded_public_key() else {
        return LicenseInfo::free();
    };
    let Ok(blob) = std::fs::read_to_string(path) else {
        return LicenseInfo::free();
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    match voxli_license::verify(
        &blob,
        &key,
        voxli_license::PRODUCT,
        std::time::SystemTime::now(),
        grace,
    ) {
        Ok(ent) => license_info_from(ent),
        Err(_) => LicenseInfo::free(),
    }
}

fn license_info_from(ent: LicenseEntitlement) -> LicenseInfo {
    use voxli_core::{Entitlement as _, Feature};
    let mut features = Vec::new();
    for f in [
        Feature::HistoryOnDisk,
        Feature::CloudPostProcessing,
        Feature::ExtraProfiles,
    ] {
        if ent.feature(f) {
            features.push(f.as_str().to_string());
        }
    }
    // `verify` accepts an expired license within the grace window (RF-38):
    // flag it — "grace", not "active".
    let now = now_secs();
    let state = match ent.expires_at_secs() {
        Some(e) if e <= now => "grace".to_string(),
        _ => "active".to_string(),
    };
    LicenseInfo {
        state,
        licensee: ent.licensee().to_string(),
        features,
        expires: None,
        expires_at_secs: ent.expires_at_secs(),
    }
}

#[derive(Serialize)]
pub struct LicenseStatus {
    pub license: LicenseInfo,
    /// Expected file path (helps manual import).
    pub path: String,
}

#[tauri::command]
pub fn license_status(shared: State<'_, UiShared>) -> LicenseStatus {
    LicenseStatus {
        license: lock_or_recover(&shared.license).clone(),
        path: license_path().display().to_string(),
    }
}

fn license_path() -> PathBuf {
    Config::default_dir().join("license.txt")
}

/// Imports a signed license blob (full text, pasted or picked file).
/// Verification is local, offline, Ed25519 (ADR-08); the blob is
/// persisted **only if valid**.
#[tauri::command]
pub fn license_import(shared: State<'_, UiShared>, blob: String) -> Result<LicenseInfo, String> {
    let Some(key) = voxli_license::embedded_public_key() else {
        return Err(
            "Aucune clé publique embarquée dans ce binaire — import impossible.".to_string(),
        );
    };
    let grace = std::time::Duration::from_secs(30 * 86_400);
    let ent = voxli_license::verify(
        &blob,
        &key,
        voxli_license::PRODUCT,
        std::time::SystemTime::now(),
        grace,
    )
    .map_err(|e| format!("Licence refusée : {e}"))?;
    let info = license_info_from(ent);
    std::fs::write(license_path(), blob.trim())
        .map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    *lock_or_recover(&shared.license) = info.clone();
    // Entitlements take effect **immediately**: the worker re-reads the
    // license on every rebuild — send it the signal, without reloading
    // the engine (it sees `engine_stale` unchanged).
    let _ = shared.tx.send(WorkerMsg::ConfigChanged(Box::new(
        lock_or_recover(&shared.config).clone(),
    )));
    Ok(info)
}

// ------------------------------------------------------------ config ----

#[tauri::command]
pub fn config_get(shared: State<'_, UiShared>) -> Config {
    lock_or_recover(&shared.config).clone()
}

/// Configuration write: full validation on the core side, zero trust in
/// the webview. RF-01: hotkeys are **re-registered hot** (registration
/// failure logged, never panic); the pipeline thread receives
/// `ConfigChanged` and rebuilds its orchestrator (RF-22 buffer preserved)
/// — the change is live, no restart required.
#[tauri::command]
pub fn config_set(
    app: AppHandle<Wry>,
    shared: State<'_, UiShared>,
    new_config: Config,
) -> Result<(), String> {
    let normalized = new_config.normalized();
    // Hotkey palette: syntactic validation + internal conflicts
    // (two identical hotkeys are forbidden).
    let combos = [
        normalized.hotkeys.push_to_talk.clone(),
        normalized.hotkeys.paste_last_transcript.clone(),
        normalized.hotkeys.show_last_transcript.clone(),
    ];
    if combos.iter().any(|c| c.trim().is_empty()) {
        return Err("Un raccourci ne peut pas être vide.".to_string());
    }
    for (i, a) in combos.iter().enumerate() {
        for b in combos.iter().skip(i + 1) {
            if a.eq_ignore_ascii_case(b) {
                return Err(
                    "Conflit de raccourcis : deux actions utilisent la même combinaison."
                        .to_string(),
                );
            }
        }
    }
    // P2 (audit): the model name remains a PLAIN name within the
    // `%APPDATA%\Voxli\models` folder — the webview cannot make the
    // engine read a file outside the data folder.
    if !normalized.general.model.is_empty()
        && normalized.general.model != "auto"
        && !is_safe_model_name(&normalized.general.model)
    {
        return Err(
            "Nom de modèle invalide : un nom de fichier simple est attendu (ex. ggml-base-q5_1.bin)."
                .to_string(),
        );
    }
    normalized
        .save(&shared.config_path)
        .map_err(|e| format!("{} : {e}", strings::ERR_CONFIG_SAVE))?;
    *lock_or_recover(&shared.config) = normalized.clone();

    // RF-01: re-registration of global hotkeys on the main thread
    // (tauri commands run there — global-shortcut plugin OK).
    crate::re_register_shortcuts(&app, &normalized);

    // The pipeline re-builds orchestrator, engine, capture, history.
    let _ = shared
        .tx
        .send(WorkerMsg::ConfigChanged(Box::new(normalized)));
    Ok(())
}

// ------------------------------------------------------------- state ----

#[derive(Serialize)]
pub struct StateInfo {
    /// State-machine state name (docs/04 §3).
    pub state: String,
    /// Loaded engine id (metadata RF-34).
    pub engine: String,
}

#[tauri::command]
pub fn state_get(shared: State<'_, UiShared>) -> StateInfo {
    StateInfo {
        state: lock_or_recover(&shared.state).clone(),
        engine: lock_or_recover(&shared.engine).clone(),
    }
}

// --------------------------------------------------- devices RF-08 ---

#[derive(Serialize)]
pub struct DeviceInfo {
    /// Stable cpal id (persisted in `audio.device`).
    pub id: String,
    pub name: String,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

#[tauri::command]
pub fn devices_list() -> Vec<DeviceInfo> {
    voxli_audio::list_input_devices()
        .into_iter()
        .map(|d| DeviceInfo {
            id: d.id,
            name: d.name,
            is_default: d.is_default,
        })
        .collect()
}

// ------------------------------------------------------ models RF-20 ---

#[derive(Clone, Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub label: String,
    /// "whisper" | "nemo" (kinds of the current catalog).
    pub kind: String,
    /// Estimated size (MB) — indicative.
    #[serde(rename = "sizeMb")]
    pub size_mb: Option<u64>,
    /// License of the downloaded model — displayed BEFORE any action.
    pub license: String,
    pub installed: bool,
    #[serde(rename = "sizeBytes")]
    pub size_bytes: Option<u64>,
    pub sha256: Option<String>,
    /// True if the manifest digest EQUALS the catalog's trusted reference
    /// (P1-3): always false while `expected_sha256` stays `None` (no
    /// published digest → nothing is claimed verified).
    pub verified: bool,
}

/// Catalog: provenance and license are visible in the UI before any
/// download (attribution CC-BY-4.0 / OpenMDW, THIRD-PARTY-LICENSES.md).
struct ModelEntry {
    id: &'static str,
    label: &'static str,
    /// "whisper" | "nemo". (Parakeet removed from the catalog on 22/10 —
    /// the distributed build does not compile sherpa-onnx and therefore
    /// cannot enable it; the entry and its license line come back
    /// together as soon as a `parakeet-asr` build is produced.)
    kind: &'static str,
    /// Estimated size (MB) — indicative.
    size_mb: Option<u64>,
    /// License of the downloaded model — displayed BEFORE any action.
    license: &'static str,
    /// SHA-256 hex digest (lowercase) from a TRUSTED source: release
    /// manifest signed by the maintainer, then compared post-download.
    /// **Nothing is invented here** — until a human review brings the
    /// digest in (procedure docs/11-checklist-release.md §3), the value
    /// stays `None`: the computed digest is logged alone, with
    /// `verified: false` (P1-3, docs/08-audit).
    expected_sha256: Option<&'static str>,
}

const MODEL_CATALOG: &[ModelEntry] = &[
    ModelEntry {
        id: "ggml-base-q5_1.bin",
        label: "Whisper base (multilingue, quantisé — 57 Mo)",
        kind: "whisper",
        size_mb: Some(57),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "ggml-large-v3-turbo-q5_0.bin",
        label: "Whisper large-v3-turbo (précision maximale, GPU conseillé — 574 Mo)",
        kind: "whisper",
        size_mb: Some(574),
        license: "MIT — ggerganov/whisper.cpp",
        expected_sha256: None,
    },
    ModelEntry {
        id: "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
        label: "Nemotron 3.5 continu (40 langues, texte en direct — 742 Mo)",
        kind: "nemo",
        size_mb: Some(742),
        license: "OpenMDW-1.1 — NVIDIA Nemotron Speech",
        expected_sha256: None,
    },
];

fn hash_of_manifest(id: &str) -> Option<String> {
    let manifests = Config::default_dir().join("models").join("manifests.json");
    let raw = std::fs::read_to_string(manifests).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v.get("entries")
        .and_then(|e| e.get(id))
        .and_then(|m| m.get("sha256"))
        .and_then(|h| h.as_str())
        .map(String::from)
}

#[tauri::command]
pub fn models_list() -> Vec<ModelInfo> {
    let models_dir = Config::default_dir().join("models");
    MODEL_CATALOG
        .iter()
        .map(|entry| {
            let id = entry.id;
            let kind = entry.kind;
            let path = match kind {
                "parakeet" => models_dir.join(id).join("tokens.txt"),
                _ => models_dir.join(id),
            };
            let installed = match kind {
                "parakeet" => {
                    let dir = models_dir.join(id);
                    dir.join("encoder.int8.onnx").is_file() || dir.join("encoder.onnx").is_file()
                }
                _ => path.is_file(),
            };
            // Size: for Parakeet, the real sum of the folder's files
            // (an NTFS directory entry size is worthless) — P2, audit §3.
            let size_bytes = if installed && kind == "parakeet" {
                Some(dir_size_bytes(&models_dir.join(id)))
            } else if installed {
                std::fs::metadata(&path).ok().map(|m| m.len())
            } else {
                None
            };
            // Verified status (P1-3): actual comparison at the list
            // level too — the user sees the real match.
            let digest = hash_of_manifest(id);
            let verified = match (&entry.expected_sha256, digest.as_deref()) {
                (Some(expected), Some(actual)) => expected.eq_ignore_ascii_case(actual),
                _ => false,
            };
            ModelInfo {
                id: id.to_string(),
                label: entry.label.to_string(),
                kind: kind.to_string(),
                size_mb: entry.size_mb,
                license: entry.license.to_string(),
                installed,
                size_bytes,
                sha256: digest,
                verified,
            }
        })
        .collect()
}

/// Recursive sum of a folder's file sizes (0 if nonexistent).
fn dir_size_bytes(dir: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                total += dir_size_bytes(&p);
            } else if let Ok(m) = e.metadata() {
                total += m.len();
            }
        }
    }
    total
}

/// Declared engine status for the "Voix" screen: compiled into THIS
/// build (feature) and actually usable on THIS machine (model present
/// on disk). No promise without verification.
#[derive(Serialize)]
pub struct EngineInfo {
    /// Config id — values of `general.engine` (serde lowercase).
    pub id: String,
    /// The engine feature is compiled into this build.
    pub compiled: bool,
    /// Compiled AND ready to dictate here (model installed) — else
    /// greyed out in the UI.
    pub usable: bool,
}

/// Resolves the whisper model when the config says "auto" (or empty):
/// the first model present on disk, in this preference order — base
/// (best default), small, medium, large-v3-turbo, tiny (ids kept
/// verbatim from earlier distributions so on-disk leftovers are never
/// ignored), else the catalog default (base). Shared by `engines_list`
/// and `worker::model_path`: ONE source of truth, the "auto" choice
/// never depends on who asks (the 21/10 inconsistency — engine
/// "loaded" but table "not installed" — came from the twin resolutions).
///
/// Guard: an EXPLICIT config id is honored ONLY if it is a whisper-kind
/// entry of the catalog. A nemo/parakeet id (poisoned in the past by
/// the old guard-less auto-plug, code 2 in the journal) is NEVER handed
/// to the whisper loader — falls back to the auto resolution.
pub(crate) fn resolve_whisper_model(cfg: &Config, models_dir: &std::path::Path) -> String {
    let explicit = cfg.general.model.as_str();
    let explicit_whisper = !explicit.is_empty()
        && explicit != "auto"
        && MODEL_CATALOG
            .iter()
            .any(|e| e.id == explicit && e.kind == "whisper");
    if explicit_whisper {
        return explicit.to_string();
    }
    for id in [
        "ggml-base-q5_1.bin",
        "ggml-small-q5_1.bin",
        "ggml-medium-q5_0.bin",
        "ggml-large-v3-turbo-q5_0.bin",
        "ggml-tiny-q8_0.bin",
    ] {
        if models_dir.join(id).is_file() {
            return id.to_string();
        }
    }
    "ggml-base-q5_1.bin".to_string()
}

/// Candidate engine list. Selection is written via `config_set`
/// (`general.engine`) and the worker rebuilds its bundle live
/// ("engine_stale", RF-20) — a restart is never required.
#[tauri::command]
pub fn engines_list(shared: State<'_, UiShared>) -> Vec<EngineInfo> {
    let cfg = lock_or_recover(&shared.config).clone();
    let models_dir = Config::default_dir().join("models");

    // whisper.cpp — the model from the user setting, or the best file
    // present when `model` is "auto" (SAME resolution as the worker).
    let whisper_compiled = cfg!(feature = "real-asr");
    let whisper_model = resolve_whisper_model(&cfg, &models_dir);
    let whisper_usable = whisper_compiled && models_dir.join(&whisper_model).is_file();

    // Nemotron streaming — the SDK must be compiled (feature
    // `nemo-stream`) and the GGUF must be on disk.
    let nemo_compiled = cfg!(feature = "nemo-stream");
    let nemo_usable = nemo_compiled
        && models_dir
            .join("nemotron-3.5-asr-streaming-0.6b.q8_0.gguf")
            .is_file();

    // "auto" always resolves to something (synthetic at the end of the
    // chain, never mute): usable by construction.
    // Parakeet absent from the list: the catalog no longer offers it
    // (removed 22/10 — sherpa-onnx not compiled in the distributed
    // build, see ModelEntry and THIRD-PARTY-LICENSES.md §1).
    vec![
        EngineInfo {
            id: "auto".into(),
            compiled: true,
            usable: true,
        },
        EngineInfo {
            id: "whisper".into(),
            compiled: whisper_compiled,
            usable: whisper_usable,
        },
        EngineInfo {
            id: "nemo".into(),
            compiled: nemo_compiled,
            usable: nemo_usable,
        },
    ]
}

#[tauri::command]
pub fn models_remove(id: String, shared: State<'_, UiShared>) -> Result<(), String> {
    let models_dir = Config::default_dir().join("models");
    if !MODEL_CATALOG.iter().any(|e| e.id == id) {
        return Err("Modèle inconnu.".to_string());
    }
    let path = models_dir.join(&id);
    let ok = if path.is_file() {
        std::fs::remove_file(&path)
    } else if path.is_dir() {
        std::fs::remove_dir_all(&path)
    } else {
        Ok(())
    };
    // Removal changes the effective engine at the next rebuild (RF-15) —
    // it must be visible in the journal, else a model's disappearance
    // stays undemonstrable (UX finding of 19/10: model gone, no trace).
    match &ok {
        Ok(()) => shared.logger.log(Event::DeviceEvent {
            kind: format!("modèle-retiré:{id}"),
        }),
        Err(_) => shared.logger.log(Event::DeviceEvent {
            kind: format!("modèle-retrait-échec:{id}"),
        }),
    }
    ok.map_err(|e| format!("{} : {e}", strings::ERR_WRITE))
}

#[tauri::command]
pub fn models_download_cancel(shared: State<'_, UiShared>) -> Result<(), String> {
    shared.download_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Downloads one catalog model: ONE explicit user action (RF-36), one at
/// a time, `Range` resume supported, SHA-256 computed and logged into
/// `manifests.json` (RF-20). The connection is Voxli's ONLY network
/// path — only from this screen, never at startup nor in the background.
#[tauri::command]
pub fn models_download(
    app: AppHandle<Wry>,
    shared: State<'_, UiShared>,
    id: String,
) -> Result<(), String> {
    let Some(entry) = MODEL_CATALOG.iter().find(|e| e.id == id) else {
        return Err("Modèle inconnu.".to_string());
    };
    {
        let mut busy = lock_or_recover(&shared.download_busy);
        if *busy {
            return Err("Un téléchargement est déjà en cours.".to_string());
        }
        *busy = true;
    }
    shared.download_cancel.store(false, Ordering::SeqCst);

    let handle = app.clone();
    let cancel = Arc::clone(&shared.download_cancel);
    let id_owned = id.clone();
    let url = model_url(entry.id, entry.kind);
    let kind = entry.kind;
    let expected = entry.expected_sha256;
    let spawned = std::thread::Builder::new()
        .name("voxli-model-download".into())
        .spawn(move || {
            // P1-4 (audit): even a thread panic must release the
            // `download_busy` lock — without the reset, every further
            // download would be refused as "already in progress" until
            // a restart.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                download_model_thread(&handle, &id_owned, &url, kind, expected, &cancel)
            }));
            let reason = match result {
                Ok(Ok(())) => None,
                Ok(Err(reason)) => Some(reason),
                Err(_panic) => {
                    handle
                        .state::<UiShared>()
                        .logger
                        .log(Event::Error { module: "models".into(), code: 7 });
                    Some("erreur interne : téléchargement reconduit (l'application peut être relancée)".to_string())
                }
            };
            if let Some(reason) = reason {
                // Reason logged (RF-34: network metadata, never
                // dictated content) — the first draft's silent failure
                // made diagnosis impossible.
                handle.state::<UiShared>().logger.log(Event::DeviceEvent {
                    kind: format!(
                        "téléchargement-échec:{}",
                        reason.chars().take(96).collect::<String>()
                    ),
                });
                // Success is emitted by the thread; a failure (cancel,
                // network, extraction) must also close the progress
                // on the UI side — never a stuck bar.
                let _ = handle.emit(
                    "models://done",
                    serde_json::json!({ "id": id_owned, "ok": false, "reason": reason }),
                );
            }
            let shared = handle.state::<UiShared>();
            *lock_or_recover(&shared.download_busy) = false;
        });
    if let Err(e) = spawned {
        // P1-4 (audit): the spawn failed BEFORE the thread ran — the
        // lock taken above would never have been released. Release it
        // and surface the error to the UI.
        *lock_or_recover(&shared.download_busy) = false;
        return Err(format!("{} : {e}", strings::ERR_WORKER));
    }
    Ok(())
}

fn model_url(id: &str, kind: &str) -> String {
    match kind {
        "parakeet" => format!(
            "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/{id}.tar.bz2"
        ),
        "nemo" => format!(
            "https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b/resolve/main/{id}"
        ),
        _ => format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{id}"),
    }
}

const DOWNLOAD_CHUNK: usize = 256 * 1024;

fn download_model_thread(
    app: &AppHandle<Wry>,
    id: &str,
    url: &str,
    kind: &str,
    expected: Option<&'static str>,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let shared = app.state::<UiShared>();
    let models_dir = Config::default_dir().join("models");
    std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
    // Single partial under `{id}.part` for all formats: a contiguous
    // prefix of the server stream, it allows a resume without re-down-
    // loading everything. The fanciful `.part.bz2` name of the first
    // draft is cleaned up (orphan file seen in real testing, 192 MB of
    // no use).
    let tmp = models_dir.join(format!("{id}.part"));
    let legacy = models_dir.join(format!("{id}.part.bz2"));
    if legacy.is_file() {
        let _ = std::fs::remove_file(&legacy);
    }
    shared.logger.log(Event::DeviceEvent {
        kind: "téléchargement-démarré".into(),
    });

    // Deadline by INACTIVITY (30 s without a byte), not a global cap: a
    // long download on a slow line must be able to finish (RF-20); a
    // dead connection must fail early and resume later (the first
    // draft's global 600 s timeout cut large downloads short).
    let agent: ureq::Agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(20))
        .timeout_read(std::time::Duration::from_secs(30))
        .build();

    // AUTOMATIC resume (UX audit 19/10: a connection reset mid-transfer
    // failed the download — the user had to click again, sometimes in a
    // loop, never finishing). Each attempt resumes from the existing
    // `.part` (`Range` header); network errors retry with growing
    // backoff up to MAX_TENTATIVES; cancellation remains instant at all
    // times.
    const MAX_TENTATIVES: u32 = 8;
    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        if attempt > 1 {
            // Cancelable backoff: 1, 2, 4, 8, 15, 30 then 60 s.
            let wait = [1u64, 2, 4, 8, 15, 30, 60][((attempt - 2) as usize).min(6)];
            shared.logger.log(Event::DeviceEvent {
                kind: format!("téléchargement-reprise-auto:essai={attempt},attente={wait}s"),
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(wait);
            while std::time::Instant::now() < deadline {
                if cancel.load(Ordering::SeqCst) {
                    return Err("annulé".into());
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
        match transfer_attempt(app, id, url, cancel, &agent, &tmp) {
            Attempt::Complete => break,
            Attempt::AlreadyComplete => {
                return finalize_download(app, id, kind, expected, true);
            }
            Attempt::Fatal(e) => return Err(e),
            Attempt::Retryable(detail) => {
                shared.logger.log(Event::DeviceEvent {
                    kind: format!(
                        "téléchargement-réseau-interrompu:essai={attempt},{}",
                        detail.chars().take(96).collect::<String>()
                    ),
                });
                if attempt >= MAX_TENTATIVES {
                    let partio = std::fs::metadata(&tmp).map(|m| m.len()).unwrap_or(0);
                    return Err(format!(
                        "réseau : {detail} ({attempt} tentatives ; les {partio} o déjà reçus sont conservés, un nouvel essai reprendra)"
                    ));
                }
            }
        }
    }
    shared.logger.log(Event::DeviceEvent {
        kind: "téléchargement-fini".into(),
    });
    finalize_download(app, id, kind, expected, false)
}

/// Outcome of one transfer attempt (see `download_model_thread`).
enum Attempt {
    /// Server stream fully consumed: ready for finalization.
    Complete,
    /// The `.part` is already complete: the server refuses the range (416).
    AlreadyComplete,
    /// Network error: the `.part` is kept, the next attempt will resume.
    Retryable(String),
    /// Fatal error (cancel, disk): immediate abort.
    Fatal(String),
}

/// ONE transfer attempt: connection with a `Range` set at the current
/// `.part` size, chunked writes, progress emitted. No resume here —
/// that is the caller loop's job.
fn transfer_attempt(
    app: &AppHandle<Wry>,
    id: &str,
    url: &str,
    cancel: &AtomicBool,
    agent: &ureq::Agent,
    tmp: &std::path::Path,
) -> Attempt {
    let shared = app.state::<UiShared>();
    let start_from = std::fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    // P2 (audit): an already-complete `.part` gets a 416 (range beyond
    // content) — this is NOT a network failure: the whole file is already
    // legitimately there. Jump straight to finalization; otherwise the
    // fully received partial never finished ("network: 416" in a loop).
    let resp = match agent
        .get(url)
        .set("Range", &format!("bytes={start_from}-"))
        .call()
    {
        Ok(r) => r,
        Err(ureq::Error::Status(416, _)) if start_from > 0 => {
            shared.logger.log(Event::DeviceEvent {
                kind: "téléchargement-reprise-déjà-complète".into(),
            });
            return Attempt::AlreadyComplete;
        }
        Err(e) => return Attempt::Retryable(format!("réseau : {e}")),
    };
    // Server code: 206 = the range request is granted (resume); 200 =
    // the ENTIRE content was sent regardless of the range — restart from
    // zero MANDATORILY, else the file would be corrupted (kept prefix +
    // full stream). Without this test, no resume was safe.
    let content_len = resp
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    let (from, expected_total) = if start_from > 0 && resp.status() == 206 {
        (start_from, content_len + start_from)
    } else if start_from > 0 {
        shared.logger.log(Event::DeviceEvent {
            kind: "téléchargement-reprise-refusée".into(),
        });
        (0u64, content_len)
    } else {
        (0u64, content_len)
    };

    let mut file = if from > 0 {
        match std::fs::OpenOptions::new().append(true).open(tmp) {
            Ok(f) => f,
            Err(e) => return Attempt::Fatal(e.to_string()),
        }
    } else {
        match std::fs::File::create(tmp) {
            Ok(f) => f,
            Err(e) => return Attempt::Fatal(e.to_string()),
        }
    };
    let mut reader = resp.into_reader();
    let mut buf = vec![0u8; DOWNLOAD_CHUNK];
    let mut downloaded = from;
    let mut next_emit = downloaded + 512 * 1024;
    loop {
        if cancel.load(Ordering::SeqCst) {
            return Attempt::Fatal("annulé".into());
        }
        use std::io::Read;
        let n = match reader.read(&mut buf) {
            Ok(0) => return Attempt::Complete,
            Ok(n) => n,
            Err(e) => return Attempt::Retryable(format!("réseau : {e}")),
        };
        use std::io::Write;
        if let Err(e) = file.write_all(&buf[..n]) {
            return Attempt::Fatal(e.to_string());
        }
        downloaded += n as u64;
        if expected_total > 0 && downloaded >= next_emit {
            next_emit = downloaded + 512 * 1024;
            let _ = app.emit(
                "models://progress",
                serde_json::json!({
                    "id": id,
                    "downloaded": downloaded,
                    "total": expected_total,
                }),
            );
        }
    }
}

/// Shared finalization (after a download OR after a full-resume 416):
/// validated archive extraction, renaming, streamed digest, match against
/// the trusted reference, manifest, signal to the worker — the engine
/// reloads without a restart (RF-15).
fn finalize_download(
    app: &AppHandle<Wry>,
    id: &str,
    kind: &str,
    expected: Option<&'static str>,
    resumed_complete: bool,
) -> Result<(), String> {
    let shared = app.state::<UiShared>();
    let models_dir = Config::default_dir().join("models");
    let is_archive = kind == "parakeet";
    let dest = models_dir.join(id);
    let tmp = models_dir.join(format!("{id}.part"));

    if is_archive {
        // P1-5 (audit): the ARCHIVE ENTRIES are validated BEFORE
        // extraction — an absolute member or "../" must never escape the
        // target folder, whatever the system tar's policy is.
        validate_archive_members(&tmp)?;
        // The target directory must exist BEFORE `tar -C`: the first
        // draft's omission made every extraction fail (defect observed
        // in real testing; the complete archive stayed unusable).
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        let out_str = dest.display().to_string();
        let input = tmp.display().to_string();
        let status = std::process::Command::new("tar")
            .args(["-xjf", &input, "-C", &out_str])
            .status()
            .map_err(|e| format!("extraction : {e}"))?;
        // The partial/archived file is removed only if extraction
        // succeeded: on failure, the already-received bytes stay
        // reusable (RF-20). EXCEPTION: if the file was already complete
        // (416 resume), a broken archive would fail identically in a
        // loop — delete it to restart from a clean download.
        if !status.success() {
            if resumed_complete {
                let _ = std::fs::remove_file(&tmp);
            }
            return Err("extraction de l'archive impossible (tar requis)".to_string());
        }
        let _ = std::fs::remove_file(&tmp);
        // The archive contains a folder of the same name: bring the
        // files back up to the chosen folder (dest). Silent fix if
        // already in the right shape.
        if !dest.join("tokens.txt").is_file() {
            if let Ok(entries) = std::fs::read_dir(&dest) {
                for e in entries.flatten() {
                    let inner = e.path();
                    if inner.join("tokens.txt").is_file() {
                        for f in [
                            "encoder.int8.onnx",
                            "decoder.int8.onnx",
                            "joiner.int8.onnx",
                            "tokens.txt",
                            "encoder.onnx",
                            "decoder.onnx",
                            "joiner.onnx",
                        ] {
                            let src = inner.join(f);
                            if src.is_file() {
                                let _ = std::fs::rename(&src, dest.join(f));
                            }
                        }
                        break;
                    }
                }
            }
        }
        // P1-5 (audit, post-extraction barrier): the extracted tree must
        // stay canonically under `dest` — catches what names alone do
        // not see (symlinks, exotic encodings).
        ensure_tree_within(&dest)?;
    } else {
        std::fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    }

    // Digest computed IN STREAM (P1-2 audit: never the whole model in
    // RAM) — deterministic folder digest for Parakeet (P1-1 audit: NEVER
    // read a directory as a file), file otherwise.
    let sha = if is_archive {
        directory_digest(&dest)?
    } else {
        streamed_sha256(&dest)?
    };
    // P1-3 (audit): match against the trusted reference when it exists;
    // a mismatch is a blocking failure (file deleted), not a warning
    // buried in the logs.
    let verified = match expected {
        Some(expected) => {
            if !sha.eq_ignore_ascii_case(expected) {
                shared.logger.log(Event::Error {
                    module: "models".into(),
                    code: 8,
                });
                if dest.is_dir() {
                    let _ = std::fs::remove_dir_all(&dest);
                } else {
                    let _ = std::fs::remove_file(&dest);
                }
                return Err(format!(
                    "empreinte SHA-256 incohérente avec la référence de confiance ({sha} ≠ {expected})."
                ));
            }
            shared.logger.log(Event::DeviceEvent {
                kind: "empreinte-vérifiée".into(),
            });
            true
        }
        None => {
            // No trusted digest published for this model; the computed
            // digest stays LOGGED (user-verifiable, RF-20) — it is not
            // presented as a guarantee of authenticity (procedure
            // docs/11 §3).
            shared.logger.log(Event::DeviceEvent {
                kind: "empreinte-consignée".into(),
            });
            false
        }
    };
    let manifests_path = models_dir.join("manifests.json");
    let mut root = serde_json::json!({
        "entries": serde_json::Value::Object(
            std::fs::read_to_string(&manifests_path)
                .ok()
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r).ok())
                .map(|v| {
                    v.get("entries").cloned().unwrap_or(serde_json::json!({}))
                })
                .unwrap_or(serde_json::json!({}))
                .as_object()
                .cloned()
                .unwrap_or_default(),
        )
    });
    if let Some(obj) = root.get_mut("entries").and_then(|e| e.as_object_mut()) {
        let size = if is_archive {
            dir_size_bytes(&dest)
        } else {
            std::fs::metadata(&dest).ok().map(|m| m.len()).unwrap_or(0)
        };
        obj.insert(
            id.to_string(),
            serde_json::json!({
                "sha256": sha,
                "verified": verified,
                "sizeBytes": size,
                "downloadedAt": format!("epoch-{}", now_secs()),
            }),
        );
    }
    let _ = std::fs::write(
        &manifests_path,
        serde_json::to_string_pretty(&root).unwrap_or_default(),
    );
    let _ = app.emit("models://done", serde_json::json!({ "id": id, "ok": true }));
    // The worker re-evaluates the loaded engine: if the freshly
    // installed model makes it stale (fake → real), it reloads without
    // user action (RF-15), and without touching an ongoing dictation.
    let _ = app.state::<UiShared>().tx.send(WorkerMsg::ModelInstalled);
    Ok(())
}

// ------------------------------------------------ streamed digests -----

/// SHA-256 digest of a file IN STREAM (P1-2 audit): 1 MiB chunk reads —
/// a ~708 MB GGUF must not double the process's memory to be digested
/// (the old code read the whole file).
fn streamed_sha256(path: &std::path::Path) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;
    const CHUNK: usize = 1024 * 1024;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_digest(&hasher.finalize()))
}

/// Folder-digest format prefix (a future evolution = a new prefix; never
/// the same name for another formula).
const DIR_DIGEST_V1: &str = "voxli-dir-digest-v1";

/// Deterministic digest of a model folder (Parakeet — P1-1 audit):
/// files sorted by relative path (`/` separator), each file hashed in
/// stream, path and size participating in the digest — never an
/// `fs::read` of a directory. The value is NOT a file SHA-256 digest:
/// it is a format signed by its prefix and logged as such in the
/// manifest (`voxli-dir-digest-v1:…`).
fn directory_digest(dir: &std::path::Path) -> Result<String, String> {
    use sha2::Digest;
    use std::io::Read;

    fn collect(
        dir: &std::path::Path,
        rel: &str,
        out: &mut Vec<(String, std::path::PathBuf, u64)>,
    ) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let child = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if p.is_dir() {
                collect(&p, &child, out)?;
            } else if p.is_file() {
                let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                out.push((child, p, len));
            }
        }
        Ok(())
    }

    let mut files: Vec<(String, std::path::PathBuf, u64)> = Vec::new();
    collect(dir, "", &mut files)?;
    if files.is_empty() {
        return Err("dossier de modèle vide : empreinte impossible".to_string());
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = sha2::Sha256::new();
    // Header: the path list — the digest reacts to an added, removed or
    // renamed file, not only to content.
    hasher.update(DIR_DIGEST_V1.as_bytes());
    hasher.update(b"\n");
    let names: Vec<&str> = files.iter().map(|(r, _, _)| r.as_str()).collect();
    hasher.update(names.join("\n").as_bytes());
    hasher.update(b"\n");
    const CHUNK: usize = 1024 * 1024;
    let mut buf = vec![0u8; CHUNK];
    for (rel, path, len) in &files {
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        hasher.update(rel.as_bytes());
        hasher.update(b":");
        hasher.update(len.to_le_bytes());
        hasher.update(b"\n");
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
    }
    Ok(format!(
        "{DIR_DIGEST_V1}:{}",
        hex_digest(&hasher.finalize())
    ))
}

/// Raw SHA-256 digest → lowercase hex.
fn hex_digest(digest: &[u8]) -> String {
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

// ----------------------------------------- extraction validation ---

/// P1-5 (audit): the ARCHIVE ENTRY names are listed (`tar -tjf`) and
/// validated BEFORE extraction — never a blind trust in the system
/// tar's policy (bsdtar protects by default, older GNU tar does not).
fn validate_archive_members(archive: &std::path::Path) -> Result<(), String> {
    let out = std::process::Command::new("tar")
        .args(["-tjf", &archive.display().to_string()])
        .output()
        .map_err(|e| format!("extraction : lecture de l'archive impossible ({e})"))?;
    if !out.status.success() {
        return Err("extraction : archive illisible (tar -t impossible)".to_string());
    }
    let listing = String::from_utf8_lossy(&out.stdout);
    for line in listing.lines() {
        if !archive_entry_name_safe(line) {
            return Err(format!(
                "extraction refusée : entrée d'archive suspecte « {} »",
                line.chars().take(72).collect::<String>()
            ));
        }
    }
    Ok(())
}

/// Sane archive entry name: relative, no "..", no Windows
/// separator, no ":" (drive prefix), no NUL.
fn archive_entry_name_safe(name: &str) -> bool {
    if name.trim().is_empty() {
        return false;
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return false;
    }
    if name.contains('\\') || name.contains(':') || name.contains('\0') {
        return false;
    }
    name.split('/').all(|c| c != "..")
}

/// P1-5 (audit, post-extraction barrier): every extracted file must
/// resolve canonically INSIDE the target folder.
fn ensure_tree_within(root: &std::path::Path) -> Result<(), String> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| format!("extraction : {e}"))?;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| e.to_string())?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let canon = path
                .canonicalize()
                .map_err(|e| format!("extraction : {e}"))?;
            if !canon.starts_with(&root_canon) {
                return Err(format!(
                    "extraction refusée : sortie du dossier cible ({})",
                    canon.display()
                ));
            }
        }
    }
    Ok(())
}

/// P2 (audit): the configured model name is a PLAIN name relative to the
/// models folder — never a path (no `/`, `\`, `..`, `:`, no invisible
/// name starting with a dot). Prevents a modified config from making
/// the engine read a file outside `%APPDATA%\Voxli\models`.
fn is_safe_model_name(name: &str) -> bool {
    // Edge spaces are not a legitimate variant of a name: " .hidden.bin"
    // (leading space) hides a hidden file, and an accidental margin
    // would point elsewhere than the intended model.
    if name != name.trim() {
        return false;
    }
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.starts_with('-')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && !name.split('/').any(|c| c == "..")
}

// --------------------------------------------------- history RF-33 ---

#[derive(Serialize)]
pub struct HistoryItem {
    pub id: u64,
    pub text: String,
    pub engine: String,
    #[serde(rename = "createdMs")]
    pub created_ms: u64,
}

fn item_of(e: voxli_storage::Entry) -> HistoryItem {
    HistoryItem {
        id: e.id,
        text: e.text,
        engine: e.engine,
        created_ms: e.created_ms,
    }
}

/// Bounded read (RF-33): the `n` most recent entries. The encrypted
/// store decodes for the current user — the IPC returns transcript
/// history: this is the authorized, **contained** exception (user-chosen
/// content, deletable at any moment; documented docs/05 §3).
#[tauri::command]
pub fn history_query(shared: State<'_, UiShared>, n: Option<u32>) -> Vec<HistoryItem> {
    let guard = lock_or_recover(&shared.history);
    let Some(store) = guard.as_ref() else {
        return Vec::new();
    };
    let limit = n.unwrap_or(50).clamp(1, 200) as usize;
    store.recent(limit).into_iter().map(item_of).collect()
}

#[tauri::command]
pub fn history_delete(shared: State<'_, UiShared>, id: u64) -> Result<bool, String> {
    let mut guard = lock_or_recover(&shared.history);
    let Some(store) = guard.as_mut() else {
        return Ok(false);
    };
    Ok(store.delete(id))
}

#[tauri::command]
pub fn history_purge(shared: State<'_, UiShared>) -> Result<(), String> {
    let mut guard = lock_or_recover(&shared.history);
    if let Some(store) = guard.as_mut() {
        store.purge();
    }
    Ok(())
}

// ------------------------------------------------- dictionary RF-23 ---

/// Exports a profile's dictionary (CSV `target|variant…`) to the webview
/// (download via an `<a download>` tag).
#[tauri::command]
pub fn dictionaries_export(shared: State<'_, UiShared>, profile: String) -> Result<String, String> {
    let cfg = lock_or_recover(&shared.config);
    let entries: Vec<String> = match profile.as_str() {
        "code" => cfg.profiles.code.dictionary.clone(),
        "email" => cfg.profiles.email.dictionary.clone(),
        "default" => cfg.profiles.default.dictionary.clone(),
        other => {
            return Err(format!("profile inconnu : {other}"));
        }
    };
    Ok(entries.join("\n"))
}

/// Imports a dictionary CSV (one entry per line: `target|v1|v2`).
#[tauri::command]
pub fn dictionaries_import(
    shared: State<'_, UiShared>,
    profile: String,
    csv: String,
) -> Result<usize, String> {
    let mut cfg = lock_or_recover(&shared.config).clone();
    let cleaned: Vec<String> = csv
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect();
    let count = cleaned.len();
    match profile.as_str() {
        "code" => cfg.profiles.code.dictionary = cleaned,
        "email" => cfg.profiles.email.dictionary = cleaned,
        "default" => cfg.profiles.default.dictionary = cleaned,
        other => return Err(format!("profile inconnu : {other}")),
    }
    cfg.save(&shared.config_path)
        .map_err(|e| format!("{} : {e}", strings::ERR_CONFIG_SAVE))?;
    *lock_or_recover(&shared.config) = cfg.clone();
    let _ = shared.tx.send(WorkerMsg::ConfigChanged(Box::new(cfg)));
    Ok(count)
}

// --------------------------------------------------- RF-37 context ---

/// Context preview: foreground app + window title, NEVER the content of
/// the field (RF-37). Explicit refusal outside the allowed mode.
#[tauri::command]
pub fn context_preview(shared: State<'_, UiShared>) -> Result<Option<(String, String)>, String> {
    let mode = lock_or_recover(&shared.config).privacy.context_read;
    if matches!(mode, voxli_config::ContextRead::None) {
        return Err("contexte désactivé (mode strict, RF-37)".into());
    }
    #[cfg(target_os = "windows")]
    {
        let excluded = lock_or_recover(&shared.config)
            .insertion
            .excluded_apps
            .clone();
        let reader = voxli_text::WindowsFocusReader::new(excluded);
        Ok(voxli_core::FocusReader::snapshot(&reader).map(|s| (s.app_id, s.title)))
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(None)
    }
}

// ---------------------------------------------------- data export ---

/// User data export (RF-33: ownership): copies the configuration, the
/// dictionaries and the diagnostic report into a provided folder — never
/// dictated text nor API key in the export. Default destination:
/// `Desktop\Voxli\voxli-export-…`.
#[tauri::command]
pub fn data_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    use std::fs;
    let dest = destination
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(|u| PathBuf::from(u).join("Desktop"))
                .unwrap_or_else(std::env::temp_dir)
        })
        .join("Voxli");
    fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
    let dir = Config::default_dir();
    let stamp = format!("voxli-export-epoch-{}", now_secs());
    let out = dest.join(&stamp);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // Configuration (readable, secret-free).
    if let Ok(raw) = fs::read_to_string(dir.join("config.json")) {
        fs::write(out.join("config.json"), raw).map_err(|e| e.to_string())?;
    }
    // Digests of installed models (manifests).
    if let Ok(raw) = fs::read_to_string(dir.join("models").join("manifests.json")) {
        fs::create_dir_all(out.join("models")).ok();
        fs::write(out.join("models").join("manifests.json"), raw).map_err(|e| e.to_string())?;
    }
    // Support report.
    let log_file = dir.join("logs").join("voxli.log");
    let raw = fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = voxli_diagnostics::export_report(&lines, shared.logger.uptime());
    fs::write(out.join("rapport-diagnostic.txt"), report).map_err(|e| e.to_string())?;
    Ok(out.display().to_string())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ------------------------------------------------------------ rescue ---

/// Current text of the rescue buffer; `null` if none.
#[tauri::command]
pub fn rescue_last(shared: State<'_, UiShared>) -> Option<String> {
    lock_or_recover(&shared.last).clone()
}

/// "Paste again" (RF-31) — re-inserts the internal buffer's text.
/// Documented exception: arbitrary text must not enter through IPC.
#[tauri::command]
pub fn rescue_paste_again(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::PasteAgain));
    Ok(())
}

/// "Retry" after an engine failure (RF-21): the orchestrator restarts
/// the pipeline with the audio it kept.
#[tauri::command]
pub fn rescue_retry(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::Retry));
    Ok(())
}

#[tauri::command]
pub fn rescue_close(app: AppHandle<Wry>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("rescue") {
        let _ = w.close();
    }
    Ok(())
}

// ----------------------------------------------------- dictation ----

/// Cancels an ongoing dictation or resets the recovery state (the held
/// audio is released — invariant 3).
#[tauri::command]
pub fn cancel_dictation(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(voxli_core::InputEvent::Cancel));
    Ok(())
}

// --------------------------------------------------- diagnostics RF-34 ----

/// Exports the support report (redacted by construction). Destination:
/// provided path, else `Desktop\rapport-diagnostic-voxli.txt`.
#[tauri::command]
pub fn diagnostics_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    let log_file = voxli_config::Config::default_dir()
        .join("logs")
        .join("voxli.log");
    let raw = std::fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = voxli_diagnostics::export_report(&lines, shared.logger.uptime());
    let path = destination.map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|u| PathBuf::from(u).join("Desktop"))
            .unwrap_or_else(std::env::temp_dir)
            .join("rapport-diagnostic-voxli.txt")
    });
    std::fs::write(&path, report).map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    Ok(path.display().to_string())
}

// ------------------------------------------------------------ tests ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_safe_model_names() {
        assert!(is_safe_model_name("ggml-base-q5_1.bin"));
        assert!(is_safe_model_name("model-1.gguf"));
        assert!(!is_safe_model_name(""));
        assert!(!is_safe_model_name("."));
        assert!(!is_safe_model_name(" .hidden.bin"));
        assert!(!is_safe_model_name("../escape.bin"));
        assert!(!is_safe_model_name("sub/dir/model.bin"));
        assert!(!is_safe_model_name("sub\\dir\\model.bin"));
        assert!(!is_safe_model_name("C:evil.bin"));
        let nul = "\u{0}";
        assert!(!is_safe_model_name(nul));
    }

    #[test]
    fn t_archive_entry_names() {
        assert!(archive_entry_name_safe("tokens.txt"));
        assert!(archive_entry_name_safe("model-1.0/encoder.int8.onnx"));
        assert!(archive_entry_name_safe("a/b/c/d.txt"));
        assert!(!archive_entry_name_safe(""));
        assert!(!archive_entry_name_safe("/etc/passwd"));
        assert!(!archive_entry_name_safe("../evil.txt"));
        assert!(!archive_entry_name_safe("model/../../evil.txt"));
        assert!(!archive_entry_name_safe("C:/evil.txt"));
        assert!(!archive_entry_name_safe("a\\b.txt"));
    }

    #[test]
    fn t_streamed_hash_matches_reference() {
        // Parity: the streamed hash must equal exactly the SHA-256
        // of a whole-file read (sha2 in one pass, as the reference).
        let dir = std::env::temp_dir().join("voxli-commands-test-hash");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("sample.bin");
        let data: Vec<u8> = (0..250_000u32).map(|i| (i % 251) as u8).collect();
        std::fs::write(&file, &data).unwrap();
        let streamed = streamed_sha256(&file).unwrap();
        let reference = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(&data);
            h.finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(streamed, reference);
        // Determinism.
        assert_eq!(streamed_sha256(&file).unwrap(), streamed);
    }

    #[test]
    fn t_directory_digest_is_stable_and_responsive() {
        let dir = std::env::temp_dir().join("voxli-commands-test-dir");
        let model = dir.join("sherpa-faux");
        let _ = std::fs::remove_dir_all(&model);
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("tokens.txt"), b"ab cd").unwrap();
        std::fs::write(model.join("encoder.int8.onnx"), b"esch").unwrap();
        let d1 = directory_digest(&model).unwrap();
        assert!(d1.starts_with("voxli-dir-digest-v1:"));
        // Stability (same content → same digest, order independent of the OS).
        assert_eq!(directory_digest(&model).unwrap(), d1);
        // Sensitivity: changing one byte of a file changes the digest.
        std::fs::write(model.join("tokens.txt"), b"ab ce").unwrap();
        assert_ne!(directory_digest(&model).unwrap(), d1);
        // Sensitivity: MOVING a file changes the relative paths, hence
        // the digest (covers the P1-1 rename case).
        std::fs::create_dir_all(model.join("sous")).unwrap();
        std::fs::rename(
            model.join("encoder.int8.onnx"),
            model.join("sous/encoder.int8.onnx"),
        )
        .unwrap();
        assert_ne!(directory_digest(&model).unwrap(), d1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t_dir_size_sums_files() {
        let dir = std::env::temp_dir().join("voxli-commands-test-size");
        let model = dir.join("parakeet-faux");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&model).unwrap();
        std::fs::write(model.join("a"), vec![0u8; 1000]).unwrap();
        std::fs::write(model.join("b"), vec![0u8; 500]).unwrap();
        std::fs::create_dir_all(model.join("sub")).unwrap();
        std::fs::write(model.join("sub/c"), vec![0u8; 100]).unwrap();
        assert_eq!(dir_size_bytes(&model), 1600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
