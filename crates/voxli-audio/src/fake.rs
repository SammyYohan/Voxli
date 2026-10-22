//! Synthetic capture — test double of `Capture` (no microphone required in
//! CI; docs/05 §2.2 "injecting a synthetic AudioSource").

use std::time::Duration;

use voxli_core::{AudioBuffer, Capture, CaptureError, Level, SAMPLE_RATE_HZ};

/// Generates 16 kHz mono audio: a continuous tone when `voiced`, silence
/// otherwise. Handy for testing the orchestrator and end-to-end latency.
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
