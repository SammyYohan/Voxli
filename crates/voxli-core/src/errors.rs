//! Kernel error types. One type per module (docs/05 §6 convention); no
//! `unwrap()` on user-facing paths in production.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("aucun périphérique d'entrée disponible")]
    NoDevice,
    #[error("le micro est occupé par une autre application en mode exclusif")]
    DeviceBusy,
    #[error("périphérique déconnecté pendant la capture")]
    DeviceLost,
    #[error("échec de démarrage du flux audio")]
    StreamStart,
    #[error("aucune capture en cours")]
    NotStarted,
    #[error("impossible d'ouvrir un flux dans ce format d'échantillon")]
    UnsupportedFormat,
    /// Capture ended without a single converted sample: neither device loss
    /// (the device stays alive) nor silence — a pathological state reported
    /// as-is to the log (P2-3, audit).
    #[error("prise vide : aucun échantillon reçu du périphérique")]
    NoData,
}

#[derive(Debug, Error)]
pub enum AsrError {
    #[error("moteur non chargé")]
    NotLoaded,
    #[error("fichier de modèle introuvable ou illisible : {0}")]
    ModelMissing(String),
    #[error("échec d'inférence du moteur")]
    Inference,
    #[error("transcription annulée")]
    Cancelled,
    #[error("mémoire insuffisante pour l'inférence")]
    OutOfMemory,
    /// Consent was not verified on the caller's side: the remote call is
    /// refused by construction (ADR-03, RF-36).
    #[error("consentement absent : fonction distante désactivée")]
    ConsentMissing,
    /// Diagnostic message from an in-process (SDK) engine: the SDK detail is
    /// kept for the log, never for the UI.
    #[error("échec du moteur in-process : {0}")]
    Runtime(String),
}

#[derive(Debug, Error)]
pub enum PostError {
    #[error("échec des règles locales de post-traitement")]
    RuleFailure,
}

#[derive(Debug, Error)]
pub enum InsertError {
    #[error("insertion refusée : champ sensible (mot de passe ou fenêtre élevée)")]
    RefusedSensitive,
    #[error("insertion refusée : application exclue")]
    RefusedExcluded,
    #[error("insertion impossible : cible indisponible")]
    TargetUnavailable,
    #[error("toutes les méthodes d'insertion ont échoué : {0}")]
    AllLevelsFailed(String),
}
