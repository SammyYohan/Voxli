//! `plume-asr` — les moteurs de transcription (ADR-03).
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

#[cfg(feature = "whisper")]
pub mod whisper;
#[cfg(feature = "whisper")]
pub use whisper::WhisperCppEngine;

#[cfg(feature = "parakeet")]
pub mod parakeet;
#[cfg(feature = "parakeet")]
pub use parakeet::ParakeetOnnxEngine;

#[cfg(feature = "remote")]
pub mod remote;
#[cfg(feature = "remote")]
pub use remote::RemoteEngine;
