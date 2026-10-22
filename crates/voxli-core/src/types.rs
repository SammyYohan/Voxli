//! Shared data types. They live here (docs/05 §6: "shared types in
//! voxli-core, never in the implementations").

use std::time::Duration;

/// Canonical pipeline sample rate: mono 16 kHz float32 (RF-12).
pub const SAMPLE_RATE_HZ: u32 = 16_000;

/// Mono audio segment at the canonical rate, ready for the VAD and the engine.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn duration(&self) -> Duration {
        if self.sample_rate == 0 {
            return Duration::ZERO;
        }
        Duration::from_secs_f64(self.samples.len() as f64 / self.sample_rate as f64)
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Normalized input level (0.0 – 1.0) for the visual indicator (RF-09).
pub type Level = f32;

/// Engine identifier, used by the logs and licensing (never content, only
/// identifiers: RF-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineId {
    WhisperCpp,
    ParakeetOnnx,
    /// Nemotron 3.5 via the NeMo-Speech.cpp SDK (in-process streaming).
    Nemo,
    Remote,
    Fake,
}

impl EngineId {
    pub fn as_str(&self) -> &'static str {
        match self {
            EngineId::WhisperCpp => "whisper.cpp",
            EngineId::ParakeetOnnx => "parakeet-onnx",
            EngineId::Nemo => "nemo",
            EngineId::Remote => "remote",
            EngineId::Fake => "fake",
        }
    }
}

/// Transcription options (RF-16, RF-24). The `initial_prompt` field carries
/// the domain-vocabulary primer; it is never logged (RF-34).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranscribeOptions {
    /// Language code (short BCP-47, e.g. `"fr"`) or `None` for automatic
    /// detection — never used on an utterance under 1.5 s (RF-16).
    pub language: Option<String>,
    /// Domain-vocabulary primer injected into the engine (RF-24).
    pub initial_prompt: Option<String>,
    /// Verbatim mode: engine output unchanged, no local rules (RF-27).
    pub verbatim: bool,
    /// Number of inference threads (RF-17). `None` = engine default.
    pub threads: Option<u8>,
}

/// Transcription result. The text is never logged: only this object carries
/// it, and it is only serialized to the consented local history and the
/// rescue window (RF-34).
#[derive(Clone, Debug, PartialEq)]
pub struct Transcript {
    pub text: String,
    pub confidence: f32,
    pub duration: Duration,
    pub engine: EngineId,
    pub model: Option<String>,
}

/// Output style requested by the profile (RF-25, RF-27).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Raw engine text, no rewriting.
    Verbatim,
    /// Level 1 local French punctuation and capitalization rules.
    #[default]
    Punctuated,
}

/// Profile insertion policy (ADR-06): preference order between levels 2 and
/// 3 of the cascade.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InsertionPolicy {
    #[default]
    ClipboardFirst,
    TypingFirst,
}

/// Application profile (RF-25).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    /// Requested output style (RF-25, RF-26).
    pub style: Style,
    /// Insertion policy (level 2/3 cascade, ADR-06).
    pub insertion: InsertionPolicy,
    /// Profile domain dictionary (RF-23): target terms and variants.
    /// Empty = no replacement.
    pub dictionary: Vec<String>,
}

/// Binds a profile to the applications that trigger it (RF-25). Matching is
/// case-insensitive on `app_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileBinding {
    /// Executable names (e.g. `Code.exe`) — case-insensitive comparison.
    pub matches: Vec<String>,
    pub profile: Profile,
}

/// Profile selection: a **pure** function (RF-25) — the first binding with a
/// pattern matching `app_id` wins, else `None` (the orchestrator's default
/// profile applies).
pub fn select_profile<'a>(bindings: &'a [ProfileBinding], app_id: &str) -> Option<&'a Profile> {
    bindings
        .iter()
        .find(|b| b.matches.iter().any(|m| m.eq_ignore_ascii_case(app_id)))
        .map(|b| &b.profile)
}

/// Dictation trigger mode (RF-02).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TriggerMode {
    /// (a) hold-to-talk: release stops (default).
    #[default]
    Hold,
    /// (b) toggle: one press starts, the next press stops.
    Toggle,
    /// (c) double push to lock: a press landing within
    /// `DOUBLE_PUSH_WINDOW_MS` of the capture start locks the current
    /// recording (the echo of the quick release of the starting press); the
    /// locked capture may run arbitrarily long, and the next press stops
    /// it. A press HELD past the window still stops the recording on
    /// release.
    DoublePushLock,
}

impl TriggerMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            TriggerMode::Hold => "hold",
            TriggerMode::Toggle => "toggle",
            TriggerMode::DoublePushLock => "double-push-lock",
        }
    }
}

/// Window for detecting a "double push" in mode (c): a press landing
/// under 400 ms after the capture start locks it (the echo of the quick
/// release of the starting press).
pub const DOUBLE_PUSH_WINDOW_MS: u64 = 400;

/// Focused-field capabilities, read via UIA (voxli-text). Read-only: the
/// orchestrator knows nothing about UIA.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldCapabilities {
    /// An editable `ValuePattern` is available (plain, non-multiline
    /// control — Windows API constraint, docs/03 §5).
    pub value_pattern_writable: bool,
    pub editable: bool,
    /// Password field (`IsPassword`) — insertion always refused (RF-29).
    pub is_password: bool,
    /// Multiline field. **Not detected today**: `focus.rs` hardcodes it to
    /// `false` (UIA constraint: `ValuePattern` only covers single-line
    /// fields, docs/03 §5) and no consumer reads it. Reserved for multiline
    /// detection (M3, non-Windows).
    pub multiline: bool,
}

/// Snapshot of the dictation target, memorized **at dictation start**, not
/// at insertion time (RF-32).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FocusSnapshot {
    /// Application identifier (executable name, e.g. `OUTLOOK.EXE`).
    pub app_id: String,
    pub title: String,
    pub caps: FieldCapabilities,
    /// Elevated (admin) window: injection impossible (UIPI) — explicit refusal.
    pub elevated: bool,
    /// `false` when the memorized window no longer exists at insert time.
    pub valid: bool,
}

impl FocusSnapshot {
    /// Sensitivity decision (RF-29): password field, excluded application,
    /// or elevated window.
    pub fn is_sensitive(&self, excluded_apps: &[String]) -> bool {
        if self.caps.is_password || self.elevated {
            return true;
        }
        excluded_apps
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&self.app_id))
    }
}

/// Levels of the insertion cascade (ADR-06, RF-28).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertLevel {
    /// 1 — UI Automation `ValuePattern.SetValue` plus read-back verification.
    ValuePattern,
    /// 2 — clipboard plus `Ctrl+V`, with save/restore (RF-30).
    Clipboard,
    /// 3 — Unicode `SendInput` typing, character by character.
    SendInput,
    /// 4 — rescue window, text pre-selected.
    Rescue,
}

impl InsertLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            InsertLevel::ValuePattern => "value-pattern",
            InsertLevel::Clipboard => "clipboard",
            InsertLevel::SendInput => "sendinput",
            InsertLevel::Rescue => "rescue",
        }
    }
}

/// Reason an insertion was refused (logged locally, no content — RF-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefuseReason {
    /// Password field or elevated window (RF-29).
    SensitiveField,
    /// Application on the exclusion list (RF-29).
    ExcludedApp,
    /// Target window closed or not found (RF-32).
    TargetUnavailable,
}

impl RefuseReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RefuseReason::SensitiveField => "sensitive-field",
            RefuseReason::ExcludedApp => "excluded-app",
            RefuseReason::TargetUnavailable => "target-unavailable",
        }
    }
}

/// Insertion plan produced by the **pure** `plan_insertion` function
/// (voxli-text): the order of levels to attempt, or a refusal reason.
#[derive(Clone, Debug, PartialEq)]
pub struct InsertPlan {
    pub levels: Vec<InsertLevel>,
    pub refuse: Option<RefuseReason>,
}

impl InsertPlan {
    pub fn refused(reason: RefuseReason) -> Self {
        Self {
            levels: Vec::new(),
            refuse: Some(reason),
        }
    }

    pub fn ordered(levels: Vec<InsertLevel>) -> Self {
        Self {
            levels,
            refuse: None,
        }
    }
}

/// Insertion report: the level retained and, when falling back, the reason
/// for each attempt (never content — RF-34).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InsertReport {
    pub level_used: Option<InsertLevel>,
    /// `(level, reason code)` for each failed level.
    pub attempts: Vec<(InsertLevel, &'static str)>,
}

/// Features gated by Free/Pro entitlements (RF-39). Never a silent
/// degradation: the queryable interface is unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    HistoryOnDisk,
    CloudPostProcessing,
    ExtraProfiles,
}

impl Feature {
    pub fn as_str(&self) -> &'static str {
        match self {
            Feature::HistoryOnDisk => "history-on-disk",
            Feature::CloudPostProcessing => "cloud-post-processing",
            Feature::ExtraProfiles => "extra-profiles",
        }
    }
}
