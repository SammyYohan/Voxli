//! VAD minimal par énergie (P0) — RF-18.
//!
//! Le silence, le bruit de clavier/ventilateur et l'audio non vocal ne
//! doivent pas produire de texte : un énoncé dont la voix détectée est
//! inférieure à `min_speech_ms` est rejeté. Le VAD Silero complet (modèle
//! ONNX) remplace ce backend en P1 — même trait, même point de couture.

use plume_core::{AudioBuffer, SpeechVerdict, VoiceActivity};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadConfig {
    /// Durée minimale de voix détectée pour transcrire (ms).
    pub min_speech_ms: u32,
    /// Seuil RMS par trame (0..1). Défaut ≈ −46 dBFS.
    pub threshold_rms: f32,
    /// Durée de trame d'analyse (ms).
    pub frame_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            min_speech_ms: 250,
            threshold_rms: 0.005,
            frame_ms: 10,
        }
    }
}

/// VAD par énergie : RMS par trame de `frame_ms`, cumul des trames au-dessus
/// du seuil.
pub struct EnergyVad {
    cfg: VadConfig,
}

impl EnergyVad {
    pub fn new(cfg: VadConfig) -> Self {
        Self { cfg }
    }

    pub fn config(&self) -> &VadConfig {
        &self.cfg
    }
}

impl Default for EnergyVad {
    fn default() -> Self {
        Self::new(VadConfig::default())
    }
}

impl VoiceActivity for EnergyVad {
    fn contains_speech(&self, audio: &AudioBuffer) -> SpeechVerdict {
        let rate = audio.sample_rate.max(1) as f32;
        let frame_len = ((self.cfg.frame_ms as f32 / 1000.0) * rate).round() as usize;
        if frame_len == 0 || audio.samples.is_empty() {
            return SpeechVerdict::Silence;
        }
        let mut voiced_ms = 0u32;
        let frame_ms = self.cfg.frame_ms as f32;
        for frame in audio.samples.chunks(frame_len) {
            let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
            if rms > self.cfg.threshold_rms {
                voiced_ms += frame_ms as u32;
            }
        }
        if voiced_ms >= self.cfg.min_speech_ms {
            SpeechVerdict::Speech
        } else if voiced_ms > 0 {
            SpeechVerdict::TooShort
        } else {
            SpeechVerdict::Silence
        }
    }
}
