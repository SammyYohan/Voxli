//! Machine à états de la session de dictée (docs/04 §3).
//!
//! ```text
//! Inactif → Enregistrement → Finalisation → Analyse → Transcription
//!   → PostTraitement → Insertion → Inséré | Secours | ErreurRécupérable
//! ```

use std::time::Duration;

use crate::types::FocusSnapshot;

/// Marqueur de demande en file (RF-05 : file d'attente de 1).
#[derive(Clone, Debug, PartialEq)]
pub struct PendingMarker {
    pub target: FocusSnapshot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryReason {
    /// Erreur moteur : audio conservé, « réessayer » disponible (RF-20, RF-21).
    EngineError,
    /// Échec d'insertion des niveaux 1–3 : fenêtre de secours (RF-28, RF-31).
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

/// État observable de la session. L'interface ne reçoit que ces états et des
/// horodatages — jamais de texte (ADR-02, RF-34).
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

    /// La cible mémorisée au démarrage de la prise, quand l'état en porte une.
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
