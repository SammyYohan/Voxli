//! Types d'erreurs du noyau. Un type par module (convention docs/05 §6) ;
//! aucun `unwrap()` sur un chemin utilisateur en production.

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
    /// Le consentement n'a pas été vérifié côté appelant : l'appel distant est
    /// refusé par construction (ADR-03, RF-36).
    #[error("consentement absent : fonction distante désactivée")]
    ConsentMissing,
    /// Message diagnostique d'un moteur in-process (SDK) : le détail du SDK
    /// est conservé pour le journal, jamais pour l'interface.
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
