//! `voxli-core` — the Voxli kernel.
//!
//! Role (docs/05 §2.1): pure business logic, testable without Windows,
//! audio, or a GPU. This crate depends on no other internal crate: the
//! implementations (whisper, ONNX, UIA, DPAPI) are injected through the
//! traits defined here.
//!
//! Invariants carried by the orchestrator (docs/04 §3):
//! 1. no transition loses the transcribed text (RF-20, RF-22);
//! 2. one active session at a time; a second request is queued (depth 1) or
//!    explicitly refused (RF-05);
//! 3. audio is released on insertion, explicit refusal, retry or
//!    cancellation — the core keeps no timer of its own.

pub mod errors;
pub mod orchestrator;
pub mod state;
pub mod traits;
pub mod types;

pub use errors::{AsrError, CaptureError, InsertError, PostError};
pub use orchestrator::{
    asr_code, capture_code, CoreEvent, Deps, InputEvent, Orchestrator, OrchestratorCfg,
    SteadyClock, TestClock,
};
pub use state::{DictationState, RecoveryReason};
pub use traits::{
    Capture, Clock, Entitlement, FocusReader, Inserter, PostProcessor, SpeechVerdict, Stage,
    Transcriptor, VoiceActivity,
};
pub use types::*;
