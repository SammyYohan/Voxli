//! Kernel ports (traits) — the testable seam points (docs/05 §1):
//! "implementations are injected into the kernel through the traits defined
//! in voxli-core".

use std::time::Duration;

use crate::errors::{AsrError, CaptureError, InsertError, PostError};
use crate::types::{
    AudioBuffer, EngineId, FocusSnapshot, InsertPlan, InsertReport, Level, Profile,
    TranscribeOptions, Transcript,
};

/// Audio capture (RF-06, RF-07). Starting includes the pre-roll: audio
/// preceding the press is restituted by `stop`.
pub trait Capture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<AudioBuffer, CaptureError>;
    /// Normalized input level 0..1 for the visual indicator (RF-09).
    fn level(&self) -> Level;
    /// `true` when the device reported disconnection since the capture
    /// started (RF-10).
    fn device_lost(&self) -> bool {
        false
    }
}

/// VAD verdict (RF-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechVerdict {
    /// Usable speech.
    Speech,
    /// Speech, but shorter than the threshold (`min_speech_ms`).
    TooShort,
    /// Silence or non-vocal noise.
    Silence,
}

pub trait VoiceActivity {
    fn contains_speech(&self, audio: &AudioBuffer) -> SpeechVerdict;
}

/// Transcription engine (ADR-03). The caller (pipeline) holds the consent
/// for any remote engine — the guardrail lives in the caller, never in the
/// engine.
pub trait Transcriptor: Send + Sync {
    /// Background warm-up (RF-19). May be a no-op.
    fn warmup(&self) -> Result<(), AsrError>;
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError>;
    fn id(&self) -> EngineId;
}

/// Local post-processing (RF-26). Level 0: identity. The remote engine does
/// not implement this trait — it goes through a dedicated `Transcriptor`.
pub trait PostProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError>;
}

/// Reads the dictation target (RF-32). Returns `None` when no usable
/// focused field is detected.
pub trait FocusReader {
    fn snapshot(&self) -> Option<FocusSnapshot>;
}

/// Text insertion (RF-28 through RF-32). `plan` is a pure function of
/// `(field capabilities, profile) → level order`: testable off-Windows.
pub trait Inserter {
    fn plan(&self, target: &FocusSnapshot, profile: &Profile) -> InsertPlan;
    /// `insert` receives only the plan and the text; on total failure, the
    /// text stays in the orchestrator's internal buffer (RF-22).
    fn insert(&self, text: &str, plan: &InsertPlan) -> Result<InsertReport, InsertError>;
}

/// Free/Pro entitlements (RF-38, RF-39).
pub trait Entitlement {
    fn feature(&self, f: crate::types::Feature) -> bool;
}

/// Injectable clock to test timings (the RF-11 maximum duration) without
/// real time.
pub trait Clock {
    fn now(&self) -> Duration;
}

/// Measured latency stage (docs/06 §4). Only the duration is logged, never
/// the content (RF-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    ReleaseDetect,
    Vad,
    Inference,
    PostProcess,
    Insert,
}

impl Stage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::ReleaseDetect => "release-detect",
            Stage::Vad => "vad",
            Stage::Inference => "inference",
            Stage::PostProcess => "post-process",
            Stage::Insert => "insert",
        }
    }
}
