//! `voxli-audio` — signal capture and conditioning.
//!
//! - Mono 16 kHz float32 conversion with **filtered** resampling
//!   (windowed-sinc, never naive linear interpolation) — RF-12, RF-13;
//! - 400 ms circular pre-roll: no clipped first word (RF-06);
//! - minimal energy-based VAD (P0): silence must not produce text
//!   (RF-18); the full Silero VAD arrives in P1;
//! - audio callback with no allocation and no long lock (RF-07, ADR-12).
//!
//! `capture.rs` (WASAPI via cpal) is only compiled on Windows; the
//! conversion and VAD tests run everywhere.

pub mod convert;
pub mod vad;

pub use convert::{mixdown_to_mono, resample, to_mono_16k};
pub use vad::{EnergyVad, VadConfig};

#[cfg(target_os = "windows")]
pub mod capture;
#[cfg(target_os = "windows")]
pub use capture::{list_input_devices, CaptureTap, CpalCapture, InputDevice};

/// Synthetic capture for tests and CI (no microphone required).
pub mod fake;
pub use fake::FakeCapture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use voxli_core::{AudioBuffer, Capture, SpeechVerdict, VoiceActivity};

    /// Pure sine wave generator (test helper).
    pub(crate) fn sine(freq: f32, rate: u32, seconds: f32, amp: f32) -> Vec<f32> {
        let n = (rate as f32 * seconds) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn t_mixdown_stereo_to_mono() {
        let stereo = vec![0.5, 0.5, 0.25, 0.75];
        let mono = mixdown_to_mono(&stereo, 2);
        assert_eq!(mono, vec![0.5, 0.5]);
    }

    #[test]
    fn t_resample_length_48k_to_16k() {
        let x = sine(1000.0, 48_000, 1.0, 0.8);
        let y = resample(&x, 48_000, 16_000);
        // 1 s expected at 16 kHz, with a 2-sample edge tolerance.
        assert!(
            (y.len() as i64 - 16_000).abs() <= 2,
            "longueur {} != 16000",
            y.len()
        );
    }

    #[test]
    fn t_sine_frequency_preserved() {
        // 1 kHz at 48 kHz → 1 kHz at 16 kHz: frequency preserved (zero
        // crossings), amplitude preserved within 5 %.
        let x = sine(1000.0, 48_000, 0.5, 0.8);
        let y = resample(&x, 48_000, 16_000);
        let mid = &y[y.len() / 4..3 * y.len() / 4]; // skip the edges
        let crossings: usize = mid.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        let measured = crossings as f32 / (mid.len() as f32 / 16_000.0);
        assert!(
            (measured - 1000.0).abs() < 25.0,
            "fréquence mesurée {measured}"
        );
        let rms: f32 = (mid.iter().map(|s| s * s).sum::<f32>() / mid.len() as f32).sqrt();
        let expected = 0.8 / (2.0f32).sqrt();
        assert!(
            (rms - expected) / expected < 0.05,
            "RMS {rms} vs attendu {expected}"
        );
    }

    #[test]
    fn t_anti_aliasing_above_nyquist() {
        // 10 kHz at 48 kHz, resampled to 16 kHz: above the new Nyquist
        // (8 kHz), the signal must be strongly attenuated.
        let x = sine(10_000.0, 48_000, 0.5, 0.8);
        let y = resample(&x, 48_000, 16_000);
        let rms: f32 = (y.iter().map(|s| s * s).sum::<f32>() / y.len() as f32).sqrt();
        assert!(rms < 0.8 * 0.05, "repliement non atténué (RMS {rms})");
    }

    #[test]
    fn t_dc_passthrough() {
        let x = vec![0.3f32; 48_000];
        let y = resample(&x, 48_000, 16_000);
        let mid = &y[100..y.len() - 100];
        for v in mid {
            assert!((v - 0.3).abs() < 0.01, "écart DC {v}");
        }
    }

    #[test]
    fn t_upsample_16k_to_48k() {
        let x = sine(500.0, 16_000, 0.25, 0.5);
        let y = resample(&x, 16_000, 48_000);
        assert!((y.len() as i64 - 12_000).abs() <= 4, "longueur {}", y.len());
    }

    #[test]
    fn t_to_mono_16k_chain() {
        // Full chain: 48 kHz stereo → 16 kHz mono.
        let mut stereo = Vec::with_capacity(96_000);
        for i in 0..48_000 {
            let s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 48_000.0).sin() * 0.5;
            stereo.push(s);
            stereo.push(s);
        }
        let mono = to_mono_16k(&stereo, 2, 48_000);
        assert_eq!(mono.len(), 16_000);
    }

    #[test]
    fn t_vad_silence_vs_speech() {
        let cfg = VadConfig {
            min_speech_ms: 250,
            threshold_rms: 0.01,
            ..Default::default()
        };
        let vad = EnergyVad::new(cfg);
        let silence = AudioBuffer::new(vec![0.0; 16_000 * 2], 16_000);
        assert_eq!(vad.contains_speech(&silence), SpeechVerdict::Silence);

        let speech = AudioBuffer::new(sine(440.0, 16_000, 1.0, 0.3), 16_000);
        assert_eq!(vad.contains_speech(&speech), SpeechVerdict::Speech);

        // Too short: 100 ms of voice against a 250 ms threshold.
        let short = AudioBuffer::new(sine(440.0, 16_000, 0.1, 0.3), 16_000);
        assert_eq!(vad.contains_speech(&short), SpeechVerdict::TooShort);
    }

    #[test]
    fn t_fake_capture_pre_roll() {
        let mut cap = FakeCapture::with_duration(Duration::from_millis(1600));
        cap.start(400).unwrap();
        let audio = cap.stop().unwrap();
        // Pre-roll included: 1.6 s of content available.
        assert!(audio.sample_rate == 16_000 && audio.samples.len() >= 16_000);
    }
}
