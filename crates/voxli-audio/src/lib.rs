//! `voxli-audio` — capture et préparation du signal.
//!
//! - Conversion mono 16 kHz float32 avec ré-échantillonnage **filtré**
//!   (sinc fenêtré, jamais une interpolation linéaire naïve) — RF-12, RF-13 ;
//! - pré-roll circulaire de 400 ms : aucun premier mot tronqué (RF-06) ;
//! - VAD minimal par énergie (P0) : le silence ne produit pas de texte
//!   (RF-18) ; le VAD Silero complet arrive en P1 ;
//! - callback audio sans allocation, sans verrou long (RF-07, ADR-12).
//!
//! Le fichier `capture.rs` (WASAPI via cpal) est compilé uniquement sous
//! Windows ; les tests de conversion et de VAD tournent partout.

pub mod convert;
pub mod vad;

pub use convert::{mixdown_to_mono, resample, to_mono_16k};
pub use vad::{EnergyVad, VadConfig};

#[cfg(target_os = "windows")]
pub mod capture;
#[cfg(target_os = "windows")]
pub use capture::{list_input_devices, CaptureTap, CpalCapture, InputDevice};

/// Capture synthétique pour les tests et la CI (aucun micro requis).
pub mod fake;
pub use fake::FakeCapture;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use voxli_core::{AudioBuffer, Capture, SpeechVerdict, VoiceActivity};

    /// Générateur de sinusoïde pure (test).
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
        // 1 s attendue à 16 kHz, tolérance de bord de 2 échantillons.
        assert!(
            (y.len() as i64 - 16_000).abs() <= 2,
            "longueur {} != 16000",
            y.len()
        );
    }

    #[test]
    fn t_sine_frequency_preserved() {
        // 1 kHz à 48 kHz → 1 kHz à 16 kHz : fréquence préservée (zéro
        // crossings), amplitude préservée à 5 %.
        let x = sine(1000.0, 48_000, 0.5, 0.8);
        let y = resample(&x, 48_000, 16_000);
        let mid = &y[y.len() / 4..3 * y.len() / 4]; // éviter les bords
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
        // 10 kHz à 48 kHz, ré-échantillonné à 16 kHz : au-dessus du nouveau
        // Nyquist (8 kHz), le signal doit être atténué fortement.
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
        // Chaîne complète : stéréo 48 kHz → mono 16 kHz.
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

        // Trop court : 100 ms de voix avec un seuil à 250 ms.
        let short = AudioBuffer::new(sine(440.0, 16_000, 0.1, 0.3), 16_000);
        assert_eq!(vad.contains_speech(&short), SpeechVerdict::TooShort);
    }

    #[test]
    fn t_fake_capture_pre_roll() {
        let mut cap = FakeCapture::with_duration(Duration::from_millis(1600));
        cap.start(400).unwrap();
        let audio = cap.stop().unwrap();
        // Pré-roll inclus : 1,6 s de contenu disponible.
        assert!(audio.sample_rate == 16_000 && audio.samples.len() >= 16_000);
    }
}
