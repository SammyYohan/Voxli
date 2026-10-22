//! Dictation session state machine (docs/04 §3).
//!
//! ```text
//! Idle → Recording → Finalizing → VAD → Transcribing
//!   → PostProcessing → Inserting → Inserted | Rescue | RecoverableError
//! ```

use std::time::Duration;

use crate::types::FocusSnapshot;

/// Marker of a queued request (RF-05: queue depth 1).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingMarker {
    pub target: FocusSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReason {
    /// Engine error: audio kept, "retry" available (RF-20, RF-21).
    EngineError,
    /// Insertion levels 1–3 failed: rescue window (RF-28, RF-31).
    InsertFailed,
}

impl RecoveryReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecoveryReason::EngineError => "engine-error",
            RecoveryReason::InsertFailed => "insert-failed",
        }
    }
}

/// Observable session state. The UI receives only these states and
/// timestamps — never text (ADR-02, RF-34).
#[derive(Clone, Debug, PartialEq)]
pub enum DictationState {
    Idle,
    Recording {
        target: FocusSnapshot,
        started_at: Duration,
    },
    Finalizing {
        target: FocusSnapshot,
    },
    Transcribing {
        target: FocusSnapshot,
        queued: Option<PendingMarker>,
    },
    PostProcessing {
        target: FocusSnapshot,
    },
    Inserting {
        target: FocusSnapshot,
    },
    Recovery {
        reason: RecoveryReason,
    },
}

impl DictationState {
    pub fn name(&self) -> &'static str {
        match self {
            DictationState::Idle => "idle",
            DictationState::Recording { .. } => "recording",
            DictationState::Finalizing { .. } => "finalizing",
            DictationState::Transcribing { .. } => "transcribing",
            DictationState::PostProcessing { .. } => "post-processing",
            DictationState::Inserting { .. } => "inserting",
            DictationState::Recovery { .. } => "recovery",
        }
    }

    /// The target memorized at capture start, when the state carries one.
    pub fn target(&self) -> Option<&FocusSnapshot> {
        match self {
            DictationState::Recording { target, .. }
            | DictationState::Finalizing { target }
            | DictationState::Transcribing { target, .. }
            | DictationState::PostProcessing { target }
            | DictationState::Inserting { target } => Some(target),
            DictationState::Idle | DictationState::Recovery { .. } => None,
        }
    }
}
