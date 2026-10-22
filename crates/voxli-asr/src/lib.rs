//! `voxli-asr` — the transcription engines (ADR-03).
//!
//! Calling code knows neither whisper.cpp nor ONNX: it manipulates
//! `AudioBuffer`s and `TranscribeOptions` through the `Transcriptor` trait.
//!
//! - `FakeEngine`: deterministic, for tests and CI (no model);
//! - `WhisperCppEngine` (feature `whisper`): whisper.cpp via `whisper-rs`,
//!   anti-hallucination settings (RF-18) and vocabulary bootstrap (RF-24);
//! - `RemoteEngine` (feature `remote`): fully implemented transport (WAV
//!   PCM16 + multipart OpenAI + `ureq`); **deliberately not wired** into
//!   the dictation flow — the consent guard (RF-36) and the P4 integration
//!   remain an explicit product decision. Nothing sends without consent.

pub mod fake;
pub use fake::FakeEngine;

/// In-process streaming (utterance session): trait + fake session for
/// tests. No native dependency: always compiles.
pub mod streaming;
pub use streaming::{FakeSession, FakeStreamer, StreamUpdate, StreamingEngine, StreamingSession};

#[cfg(feature = "whisper")]
pub mod whisper;
#[cfg(feature = "whisper")]
pub use whisper::WhisperCppEngine;

/// Display partials for batch engines (feature `whisper`): windowed
/// re-decoding during capture, emitted over the streaming channel, never
/// inserted nor logged (RF-34 + ADR-02 amendment).
#[cfg(feature = "whisper")]
pub mod whisper_partial;
#[cfg(feature = "whisper")]
pub use whisper_partial::WhisperPartialStreamer;

/// Nemotron 3.5 ASR streaming (in-process C SDK) — feature `nemo-stream`.
#[cfg(feature = "nemo-stream")]
pub mod nemo;
#[cfg(feature = "nemo-stream")]
pub use nemo::NemoEngine;

#[cfg(feature = "parakeet")]
pub mod parakeet;
#[cfg(feature = "parakeet")]
pub use parakeet::ParakeetOnnxEngine;

#[cfg(feature = "remote")]
pub mod remote;
#[cfg(feature = "remote")]
pub use remote::RemoteEngine;
