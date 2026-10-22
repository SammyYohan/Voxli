//! Minimal energy-based VAD (P0) — RF-18.
//!
//! Silence, keyboard/fan noise and non-speech audio must not produce text:
//! an utterance whose detected voiced duration is below `min_speech_ms` is
//! rejected. The full Silero VAD (ONNX model) replaces this backend in
//! P1 — same trait, same seam.

use voxli_core::{AudioBuffer, SpeechVerdict, VoiceActivity};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VadConfig {
    /// Minimum detected voiced duration before transcribing (ms).
    pub min_speech_ms: u32,
    /// Per-frame RMS threshold (0..1). Default ≈ −46 dBFS.
    pub threshold_rms: f32,
    /// Analysis frame duration (ms).
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

/// Energy-based VAD: RMS per `frame_ms` frame, accumulated across frames
/// above the threshold.
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
