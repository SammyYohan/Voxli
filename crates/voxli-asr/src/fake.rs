//! Moteur factice déterministe — tests, CI, démo sans modèle.
//!
//! Il retourne un texte stable par « empreinte » de la durée d'audio : deux
//! appels identiques produisent le même résultat. Aucun réseau, aucun modèle.

use std::time::Duration;

use voxli_core::{AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor};

#[derive(Clone)]
pub struct FakeEngine {
    pub text: String,
    pub fail: bool,
    /// Latence artificielle (tests de file d'attente, RF-05).
    pub delay: Option<Duration>,
}

impl Default for FakeEngine {
    fn default() -> Self {
        Self {
            text: "Dictée de démonstration du moteur factice.".into(),
            fail: false,
            delay: None,
        }
    }
}

impl FakeEngine {
    /// Texte fonction de la durée : reproductible pour les bancs de latence.
    pub fn from_duration(audio: &AudioBuffer) -> String {
        let ms = audio.duration().as_millis();
        format!("[moteur factice] énoncé de {ms} ms")
    }
}

impl Transcriptor for FakeEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        if self.fail {
            return Err(AsrError::NotLoaded);
        }
        Ok(())
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        _opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if let Some(d) = self.delay {
            std::thread::sleep(d);
        }
        if self.fail {
            return Err(AsrError::Inference);
        }
        Ok(Transcript {
            text: Self::from_duration(audio),
            confidence: 0.99,
            duration: audio.duration(),
            engine: EngineId::Fake,
            model: Some("fake-1".into()),
        })
    }

    fn id(&self) -> EngineId {
        EngineId::Fake
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voxli_core::SAMPLE_RATE_HZ;

    #[test]
    fn t_deterministic_and_empty_for_silence() {
        let e = FakeEngine::default();
        let silence = AudioBuffer::new(vec![0.0; SAMPLE_RATE_HZ as usize], SAMPLE_RATE_HZ);
        let a = e
            .transcribe(&silence, &TranscribeOptions::default())
            .unwrap();
        let b = e
            .transcribe(&silence, &TranscribeOptions::default())
            .unwrap();
        assert_eq!(a.text, b.text);
        assert_eq!(e.id(), EngineId::Fake);
    }

    #[test]
    fn t_failure_mode() {
        let e = FakeEngine {
            fail: true,
            ..Default::default()
        };
        let audio = AudioBuffer::new(vec![0.1; 1000], SAMPLE_RATE_HZ);
        assert!(e.transcribe(&audio, &TranscribeOptions::default()).is_err());
    }
}
