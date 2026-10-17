//! Moteur distant (squelette) — ADR-03 point 3, feature `remote`.
//!
//! **Jamais par défaut.** Le garde-fou de consentement vit dans l'appelant
//! (garde de consentement de l'application, RF-36) : ce moteur ne vérifie pas
//! lui-même le consentement — il est CONÇU pour être instancié uniquement
//! après un accord explicite, par utilisation. En P0 ce type n'existe qu'à
//! titre d'interface : l'implémentation HTTP arrive en P4 (BYOK, docs/07 §5.3).

use std::time::Duration;

use plume_core::{AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor};

#[derive(Clone)]
pub struct RemoteEngine {
    /// Fournisseur choisi par l'utilisateur (journalisé, jamais le contenu).
    pub provider: String,
    /// Clé API de l'utilisateur (BYOK) — stockée via plume-secure, jamais en
    /// clair dans la configuration (ADR-07).
    pub credentials_key: String,
}

impl Transcriptor for RemoteEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        Ok(())
    }

    fn transcribe(
        &self,
        _audio: &AudioBuffer,
        _opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        // Aucun envoi n'est implémenté en P0 : refus propre et explicite.
        let _ = Duration::ZERO;
        Err(AsrError::ConsentMissing)
    }

    fn id(&self) -> EngineId {
        EngineId::Remote
    }
}
