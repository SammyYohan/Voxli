//! Capture synthétique — double de test de `Capture` (aucun micro requis en
//! CI ; docs/05 §2.2 « injection d'un AudioSource synthétique »).

use std::time::Duration;

use voxli_core::{AudioBuffer, Capture, CaptureError, Level, SAMPLE_RATE_HZ};

/// Génère de l'audio 16 kHz mono : tonalité continue quand `voiced`, silence
/// sinon. Pratique pour tester l'orchestrateur et la latence de bout en bout.
pub struct FakeCapture {
    started: bool,
    voiced: bool,
    seconds: f32,
}

impl FakeCapture {
    pub fn with_duration(seconds: Duration) -> Self {
        Self {
            started: false,
            voiced: true,
            seconds: seconds.as_secs_f32(),
        }
    }

    pub fn silence(seconds: Duration) -> Self {
        Self {
            started: false,
            voiced: false,
            seconds: seconds.as_secs_f32(),
        }
    }
}

impl Default for FakeCapture {
    fn default() -> Self {
        Self {
            started: false,
            voiced: true,
            seconds: 1.0,
        }
    }
}

impl Capture for FakeCapture {
    fn start(&mut self, _pre_roll_ms: u32) -> Result<(), CaptureError> {
        self.started = true;
        Ok(())
    }

    fn stop(&mut self) -> Result<AudioBuffer, CaptureError> {
        if !self.started {
            return Err(CaptureError::NotStarted);
        }
        self.started = false;
        let n = (self.seconds * SAMPLE_RATE_HZ as f32) as usize;
        let samples = if self.voiced {
            (0..n)
                .map(|i| {
                    0.3 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SAMPLE_RATE_HZ as f32)
                        .sin()
                })
                .collect()
        } else {
            vec![0.0; n]
        };
        Ok(AudioBuffer::new(samples, SAMPLE_RATE_HZ))
    }

    fn level(&self) -> Level {
        if self.started && self.voiced {
            0.6
        } else {
            0.0
        }
    }
}
