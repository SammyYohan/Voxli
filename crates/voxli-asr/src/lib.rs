//! `voxli-asr` — les moteurs de transcription (ADR-03).
//!
//! Le code d'appel ne connaît ni whisper.cpp ni ONNX : il manipule des
//! `AudioBuffer` et des `TranscribeOptions` via le trait `Transcriptor`.
//!
//! - `FakeEngine` : déterministe, pour les tests et la CI (aucun modèle) ;
//! - `WhisperCppEngine` (feature `whisper`) : whisper.cpp via `whisper-rs`,
//!   paramètres anti-hallucination (RF-18) et amorce de vocabulaire (RF-24) ;
//! - `RemoteEngine` (feature `remote`) : squelette — jamais activé sans le
//!   garde-fou de consentement **côté appelant** (RF-36).

pub mod fake;
pub use fake::FakeEngine;

/// Streaming in-process (session d'énoncé) : trait + session factice pour
/// les tests. Sans dépendance native : compile toujours.
pub mod streaming;
pub use streaming::{FakeSession, FakeStreamer, StreamUpdate, StreamingEngine, StreamingSession};

#[cfg(feature = "whisper")]
pub mod whisper;
#[cfg(feature = "whisper")]
pub use whisper::WhisperCppEngine;

/// Partiels d'affichage pour moteurs batch (feature `whisper`) : re-décodage
/// fenêtré pendant la prise, émis par la voie streaming, jamais inséré ni
/// journalisé (RF-34 + amendement ADR-02).
#[cfg(feature = "whisper")]
pub mod whisper_partial;
#[cfg(feature = "whisper")]
pub use whisper_partial::WhisperPartialStreamer;

/// Nemotron 3.5 ASR streaming (SDK C in-process) — feature `nemo-stream`.
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
