//! Ports (traits) du noyau — les points de couture testables (docs/05 §1) :
//! « les implémentations sont injectées dans le noyau via les traits définis
//! dans plume-core ».

use std::time::Duration;

use crate::errors::{AsrError, CaptureError, InsertError, PostError};
use crate::types::{
    AudioBuffer, EngineId, FocusSnapshot, InsertPlan, InsertReport, Level, Profile,
    TranscribeOptions, Transcript,
};

/// Capture audio (RF-06, RF-07). Le démarrage inclut le pré-roll : l'audio
/// antérieur à l'appui est restitué par `stop`.
pub trait Capture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError>;
    fn stop(&mut self) -> Result<AudioBuffer, CaptureError>;
    /// Niveau d'entrée normalisé 0..1 pour l'indicateur visuel (RF-09).
    fn level(&self) -> Level;
    /// `true` si le périphérique a signalé une déconnexion depuis le début de
    /// la prise (RF-10).
    fn device_lost(&self) -> bool {
        false
    }
}

/// Verdict du VAD (RF-18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeechVerdict {
    /// Voix exploitable.
    Speech,
    /// De la voix, mais une durée inférieure au seuil (`min_speech_ms`).
    TooShort,
    /// Silence ou bruit non vocal.
    Silence,
}

pub trait VoiceActivity {
    fn contains_speech(&self, audio: &AudioBuffer) -> SpeechVerdict;
}

/// Moteur de transcription (ADR-03). L'appelant (pipeline) garde le
/// consentement pour tout moteur distant — le garde-fou est côté appelant,
/// jamais dans le moteur.
pub trait Transcriptor: Send + Sync {
    /// Préchargement en tâche de fond (RF-19). Peut ne rien faire.
    fn warmup(&self) -> Result<(), AsrError>;
    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError>;
    fn id(&self) -> EngineId;
}

/// Post-traitement local (RF-26). Niveau 0 : identité. Le moteur distant
/// n'implémente pas ce trait — il passe par un `Transcriptor` dédié.
pub trait PostProcessor {
    fn process(&self, text: &str, profile: &Profile) -> Result<String, PostError>;
}

/// Lecture de la cible de dictée (RF-32). Retourne `None` si aucun champ
/// focalisé exploitable n'est détecté.
pub trait FocusReader {
    fn snapshot(&self) -> Option<FocusSnapshot>;
}

/// Insertion du texte (RF-28 à RF-32). `plan` est une fonction pure de
/// `(capacités du champ, profil) → ordre des niveaux` : testable hors Windows.
pub trait Inserter {
    fn plan(&self, target: &FocusSnapshot, profile: &Profile) -> InsertPlan;
    /// `insert` ne reçoit que le plan et le texte ; en cas d'échec total, le
    /// texte reste dans le tampon interne de l'orchestrateur (RF-22).
    fn insert(&self, text: &str, plan: &InsertPlan) -> Result<InsertReport, InsertError>;
}

/// Droits Free/Pro (RF-38, RF-39).
pub trait Entitlement {
    fn feature(&self, f: crate::types::Feature) -> bool;
}

/// Horloge injectable pour tester les temporisations (durée maximale de
/// sécurité RF-11) sans temps réel.
pub trait Clock {
    fn now(&self) -> Duration;
}

/// Tranche de latence mesurée (docs/06 §4). Seule la durée est journalisée,
/// jamais le contenu (RF-34).
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

/// Durée utile au calcul de latence.
pub fn duration_since(start: Duration, now: Duration) -> Duration {
    now.checked_sub(start).unwrap_or(Duration::ZERO)
}
