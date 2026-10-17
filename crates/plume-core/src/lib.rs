//! `plume-core` — le noyau de Plume.
//!
//! Rôle (docs/05 §2.1) : la logique métier pure, testable sans Windows, sans
//! audio, sans GPU. Ce crate ne dépend d'aucun autre crate interne : les
//! implémentations (whisper, ONNX, UIA, DPAPI) sont injectées via les traits
//! définis ici.
//!
//! Invariants portés par l'orchestrateur (docs/04 §3) :
//! 1. aucune transition ne perd le texte transcrit (RF-20, RF-22) ;
//! 2. une seule session active ; une seconde demande est mise en file (1) ou
//!    refusée explicitement (RF-05) ;
//! 3. l'audio est libéré après insertion ou après expiration du délai de
//!    récupération.

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
