//! Voxli configuration — the `%APPDATA%\Voxli\config.json` file (docs/05
//! §4). Rules:
//! - every field has a safe default (privacy first);
//! - every change bumps `schema` and triggers a tested migration;
//! - the file never contains a secret (ADR-07).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Current schema version. Every change goes through a tested migration.
/// v4: trigger mode (RF-02) + RF-36 consents.
/// v5: the UI language (`uiLanguage`, fr|en) is separated from the
/// listening language (`language`) — one drives the displayed texts, the
/// other drives ASR inference; both are independently adjustable.
pub const CURRENT_SCHEMA: u32 = 5;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("fichier de configuration illisible : {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
    #[error("schéma {found} inconnu : la migration la plus ancienne prise en charge est la 1")]
    UnknownSchema { found: u32 },
}

// ------------------------------------------------------------------ enums --

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineChoice {
    /// The best available engine, giving priority to streaming Nemotron
    /// when its model is installed (order documented in docs/05 §4).
    #[default]
    Auto,
    Whisper,
    Parakeet,
    /// Nemotron 3.5 ASR streaming via the in-process SDK.
    Nemo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PostProcessing {
    None,
    /// Deterministic local rules (level 1).
    #[default]
    Rules,
    LocalModel,
    /// Never enabled without explicit consent (RF-26).
    Cloud,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageKind {
    /// Default: memory-only history, no trace on disk (RF-33).
    #[default]
    Memory,
    Disk,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextRead {
    /// Default: no window-content reading (RF-37).
    #[default]
    None,
    AppAndTitle,
    AppTitleAndSelection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Telemetry {
    /// Default: no telemetry (ADR-10).
    #[default]
    Off,
    OptIn,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePolicy {
    #[default]
    Notify,
    Auto,
    Off,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Aggressiveness {
    Low,
    #[default]
    Balanced,
    High,
}

/// Dictation trigger mode (RF-02) — a schema mirror; conversion to the
/// core's type happens in the assembly layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerMode {
    /// (a) hold-to-talk (default, P0 compatibility).
    #[default]
    Hold,
    /// (b) toggle: one press starts, the next one stops.
    Toggle,
    /// (c) double push to lock.
    DoublePushLock,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    /// No rewriting (code, verbatim guaranteed, RF-27).
    Verbatim,
    #[default]
    Punctuated,
}

// ----------------------------------------------------------------- blocks --

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct General {
    pub language: String,
    /// UI language (`system` or one of the 15 `i18n/` codes, RF-17
    /// amended) — independent of `language` above (the ASR listening
    /// language, RF-17). Unknown values fall back to `fr` on the
    /// frontend; `system` follows `navigator.languages` (never an
    /// obscure screen).
    pub ui_language: String,
    /// Compute choice for transcriptions (`auto`|`cpu`|`gpu`|`duo`) — an
    /// additive v5 extension (serde container `default`): any unknown/empty
    /// value falls back to `auto` on the core side — never a panic nor a
    /// silent unknown mode. `auto` lets whisper.cpp use a GPU when the
    /// build embeds one (availability, not measured speed); `cpu`
    /// explicitly refuses it; `gpu` requires the build to provide one;
    /// `duo` renders the live text on the GPU and keeps the final text
    /// on CPU (transparent fallback, journaled).
    pub compute: String,
    pub engine: EngineChoice,
    pub model: String,
    pub post_processing: PostProcessing,
    pub history: HistoryCfg,
}

impl Default for General {
    fn default() -> Self {
        Self {
            language: "fr".into(),
            ui_language: "fr".into(),
            compute: "auto".into(),
            engine: EngineChoice::default(),
            model: "auto".into(),
            post_processing: PostProcessing::default(),
            history: HistoryCfg::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryCfg {
    pub enabled: bool,
    pub storage: StorageKind,
    pub retention_days: u32,
}

impl Default for HistoryCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            storage: StorageKind::Memory,
            retention_days: 30,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Hotkeys {
    pub push_to_talk: String,
    pub paste_last_transcript: String,
    pub show_last_transcript: String,
    pub use_low_level_hook: bool,
    /// Dictation trigger mode (RF-02).
    pub mode: TriggerMode,
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            push_to_talk: "Ctrl+Alt+Space".into(),
            paste_last_transcript: "Shift+Alt+Z".into(),
            show_last_transcript: "Shift+Alt+X".into(),
            use_low_level_hook: false,
            mode: TriggerMode::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AudioCfg {
    pub device: String,
    pub pre_roll_ms: u32,
    pub max_recording_seconds: u32,
    pub vad: VadCfg,
}

impl Default for AudioCfg {
    fn default() -> Self {
        Self {
            device: "default".into(),
            pre_roll_ms: 400,
            max_recording_seconds: 300,
            vad: VadCfg::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VadCfg {
    pub enabled: bool,
    pub min_speech_ms: u32,
    pub aggressiveness: Aggressiveness,
}

impl Default for VadCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            min_speech_ms: 250,
            aggressiveness: Aggressiveness::Balanced,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InsertionCfg {
    pub remember_target_at_start: bool,
    pub clipboard_hold_ms: ClipboardHoldMs,
    pub exclude_from_clipboard_history: bool,
    /// Default exclusion list (password managers). Remote Desktop is
    /// deliberately absent: insertion goes through the clipboard there.
    pub excluded_apps: Vec<String>,
}

impl Default for InsertionCfg {
    fn default() -> Self {
        Self {
            remember_target_at_start: true,
            clipboard_hold_ms: ClipboardHoldMs::default(),
            exclude_from_clipboard_history: true,
            excluded_apps: default_excluded_apps(),
        }
    }
}

/// Applications never read, never targeted (RF-29).
pub fn default_excluded_apps() -> Vec<String> {
    vec![
        "keepass.exe".into(),
        "keepassxc.exe".into(),
        "1password.exe".into(),
        "bitwarden.exe".into(),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHoldMs {
    #[serde(default = "default_hold")]
    pub default: u32,
    #[serde(default = "default_hold_slow")]
    pub slow_apps: u32,
    #[serde(default = "default_hold_rdp")]
    pub remote_desktop: u32,
}

fn default_hold() -> u32 {
    500
}
fn default_hold_slow() -> u32 {
    3000
}
fn default_hold_rdp() -> u32 {
    5000
}

impl Default for ClipboardHoldMs {
    fn default() -> Self {
        Self {
            default: 500,
            slow_apps: 3000,
            remote_desktop: 5000,
        }
    }
}

/// Explicit consents, per usage (RF-36, RF-26 level 3).
/// Every default is `false`: nothing remote without an agreement given here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConsentCfg {
    /// Transcription by a remote AI service (BYOK, P4).
    pub remote_asr: bool,
    /// Remote post-processing (RF-26 level 3).
    pub cloud_post: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Privacy {
    pub strict_local_mode: bool,
    pub context_read: ContextRead,
    pub telemetry: Telemetry,
    pub updates: UpdatePolicy,
    pub consent: ConsentCfg,
}

impl Default for Privacy {
    fn default() -> Self {
        Self {
            strict_local_mode: true,
            context_read: ContextRead::None,
            telemetry: Telemetry::Off,
            updates: UpdatePolicy::Notify,
            consent: ConsentCfg::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profiles {
    pub default: ProfileCfg,
    pub code: ProfileCfg,
    pub email: ProfileCfg,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileCfg {
    pub style: Style,
    pub dictionary: Vec<String>,
    /// Applications bound to this profile (RF-25).
    #[serde(rename = "match", default)]
    pub matches: Vec<String>,
}

impl Default for ProfileCfg {
    fn default() -> Self {
        Self {
            style: Style::Punctuated,
            dictionary: Vec::new(),
            matches: Vec::new(),
        }
    }
}

// ------------------------------------------------------------------ root ---

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub schema: u32,
    pub general: General,
    pub hotkeys: Hotkeys,
    pub audio: AudioCfg,
    pub insertion: InsertionCfg,
    pub privacy: Privacy,
    pub profiles: Profiles,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: CURRENT_SCHEMA,
            general: General::default(),
            hotkeys: Hotkeys::default(),
            audio: AudioCfg::default(),
            insertion: InsertionCfg::default(),
            privacy: Privacy::default(),
            profiles: Profiles {
                default: ProfileCfg::default(),
                code: ProfileCfg {
                    matches: vec!["Code.exe".into()],
                    style: Style::Verbatim,
                    ..Default::default()
                },
                email: ProfileCfg {
                    matches: vec!["OUTLOOK.EXE".into()],
                    style: Style::Punctuated,
                    ..Default::default()
                },
            },
        }
    }
}

impl Config {
    /// Clamps numeric values to the documented bounds — defense in depth
    /// against a hand-edited file.
    pub fn normalized(mut self) -> Self {
        self.audio.pre_roll_ms = self.audio.pre_roll_ms.clamp(100, 1000);
        self.audio.max_recording_seconds = self.audio.max_recording_seconds.clamp(30, 1800);
        self.audio.vad.min_speech_ms = self.audio.vad.min_speech_ms.clamp(100, 2000);
        self.general.history.retention_days = self.general.history.retention_days.clamp(1, 365);
        self
    }

    /// Loads with migration from any version ≥ 1.
    pub fn load(path: &std::path::Path) -> Result<Self, ConfigError> {
        let raw = std::fs::read_to_string(path)?;
        Self::from_json(&raw)
    }

    pub fn from_json(raw: &str) -> Result<Self, ConfigError> {
        let mut value: serde_json::Value = serde_json::from_str(raw)?;
        let schema = value.get("schema").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        if schema > CURRENT_SCHEMA {
            return Err(ConfigError::UnknownSchema { found: schema });
        }
        migrate(&mut value, schema)?;
        let cfg: Config = serde_json::from_value(value)?;
        Ok(cfg.normalized())
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), ConfigError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let raw = serde_json::to_string_pretty(self)?;
        std::fs::write(path, raw)?;
        Ok(())
    }

    /// Default directory: `%APPDATA%\Voxli` (Windows env); otherwise
    /// `./voxli-data`. The inheritance of the `Plume` codename (conditional
    /// fallback of 18/10) was removed on 19/10: the local data was moved
    /// once and for all to `%APPDATA%\Voxli` — no more `Plume` path in the
    /// code or in application strings.
    pub fn default_dir() -> PathBuf {
        match std::env::var("APPDATA") {
            Ok(base) if !base.is_empty() => PathBuf::from(&base).join("Voxli"),
            _ => PathBuf::from("./voxli-data"),
        }
    }

    pub fn default_path() -> PathBuf {
        Self::default_dir().join("config.json")
    }
}

/// Migration chain. Every step is tested (tests/config.rs).
fn migrate(value: &mut serde_json::Value, from: u32) -> Result<(), ConfigError> {
    let mut v = from;
    while v < CURRENT_SCHEMA {
        match v {
            // 1 → 2: "cleanup" renamed to "postProcessing" (RF-26 levels).
            1 => {
                if let Some(general) = value.get_mut("general").and_then(|g| g.as_object_mut()) {
                    if let Some(cleanup) = general.remove("cleanup") {
                        general.insert("postProcessing".into(), cleanup);
                    } else if !general.contains_key("postProcessing") {
                        general.insert("postProcessing".into(), serde_json::json!("rules"));
                    }
                }
            }
            // 2 → 3: explicit VAD (minimum duration threshold, RF-18).
            2 => {
                if let Some(audio) = value.get_mut("audio").and_then(|a| a.as_object_mut()) {
                    if !audio.contains_key("vad") {
                        audio.insert(
                            "vad".into(),
                            serde_json::json!({
                                "enabled": true,
                                "minSpeechMs": 250,
                                "aggressiveness": "balanced"
                            }),
                        );
                    }
                }
            }
            // 3 → 4: trigger mode (RF-02) + consents (RF-36).
            3 => {
                if let Some(hk) = value.get_mut("hotkeys").and_then(|h| h.as_object_mut()) {
                    if !hk.contains_key("mode") {
                        hk.insert("mode".into(), serde_json::json!("hold"));
                    }
                }
                if let Some(p) = value.get_mut("privacy").and_then(|p| p.as_object_mut()) {
                    if !p.contains_key("consent") {
                        p.insert(
                            "consent".into(),
                            serde_json::json!({"remoteAsr": false, "cloudPost": false}),
                        );
                    }
                }
            }
            // 4 → 5: the UI language is separated from the listening
            // language (`uiLanguage` appears, default `fr`); `language`
            // keeps its single role as the ASR inference language. The
            // CPU/GPU compute choice (`compute`) is additive in the same
            // way — every prior file exists, and the safe value is `auto`.
            4 => {
                if let Some(general) = value.get_mut("general").and_then(|g| g.as_object_mut()) {
                    if !general.contains_key("uiLanguage") {
                        general.insert("uiLanguage".into(), serde_json::json!("fr"));
                    }
                    if !general.contains_key("compute") {
                        general.insert("compute".into(), serde_json::json!("auto"));
                    }
                }
            }
            v => return Err(ConfigError::UnknownSchema { found: v }),
        }
        v += 1;
        if let Some(obj) = value.as_object_mut() {
            obj.insert("schema".into(), serde_json::json!(v));
        }
    }
    Ok(())
}
