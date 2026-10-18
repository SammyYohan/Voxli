//! Conversion du signal (RF-12, RF-13).
//!
//! Tout format d'entrée (8 kHz à 192 kHz, 1 à 8 canaux, flottant) est converti
//! en **mono 16 kHz float32** avec un ré-échantillonnage sinc fenêtré — pas
//! d'interpolation linéaire naïve. Le test T-12/T-13 compare les propriétés
//! (longueur, fréquence, amplitude, réjection anti-repliement) sur des signaux
//! synthétiques.

/// Mixage N canaux → mono : moyenne des canaux (normalisée).
pub fn mixdown_to_mono(input: &[f32], channels: usize) -> Vec<f32> {
    let channels = channels.max(1);
    if channels == 1 {
        return input.to_vec();
    }
    let frames = input.len() / channels;
    let mut out = Vec::with_capacity(frames);
    let inv = 1.0 / channels as f32;
    for frame in 0..frames {
        let base = frame * channels;
        let sum: f32 = input[base..base + channels].iter().sum();
        out.push(sum * inv);
    }
    out
}

/// Ré-échantillonnage sinc fenêtré (Hann), qualité T-12 :
/// - coupure à 0,45 × min(Nyquist entrée, Nyquist sortie) ;
/// - 24 lobes par côté — réjection du repliement > 45 dB sur les tests.
pub fn resample(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || input.is_empty() {
        return input.to_vec();
    }
    if in_rate == 0 || out_rate == 0 {
        return Vec::new();
    }
    let ratio = out_rate as f64 / in_rate as f64;
    let fc = 0.45 * ratio.min(1.0); // fréquence de coupure normalisée
    let taps_per_side: i64 = 24;

    // Table de sinc fenêtré pré-calculée : les poids dépendent de la phase
    // fractionnaire ; on quantifie la phase sur 1024 pas.
    const PHASES: usize = 1024;
    let mut table = vec![0.0f64; PHASES * (taps_per_side as usize * 2)];
    for p in 0..PHASES {
        let frac = p as f64 / PHASES as f64;
        for (k, slot) in table
            [p * (taps_per_side as usize * 2)..(p + 1) * (taps_per_side as usize * 2)]
            .iter_mut()
            .enumerate()
        {
            let k = k as i64 - taps_per_side;
            let x = (k as f64) + frac; // distance au centre en échantillons entrée
            *slot = sinc_windowed(x, fc, taps_per_side as f64);
        }
    }

    let in_len = input.len();
    let out_len = ((in_len as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    let step = 1.0 / ratio; // pas de sortie en échantillons d'entrée

    for o in 0..out_len {
        let center = o as f64 * step;
        let center_idx = center.floor() as i64;
        let frac = center - center_idx as f64;
        let phase = ((frac * PHASES as f64).round() as usize).min(PHASES - 1);
        let row = &table
            [phase * (taps_per_side as usize * 2)..(phase + 1) * (taps_per_side as usize * 2)];
        let mut acc = 0.0f64;
        for k in -taps_per_side..taps_per_side {
            let idx = center_idx + k;
            let weight = row[(k + taps_per_side) as usize];
            let sample = if idx < 0 || idx >= in_len as i64 {
                0.0 // zéro-padding aux bords
            } else {
                input[idx as usize] as f64
            };
            acc += weight * sample;
        }
        // Normalisation : le gain DC de la fenêtre doit être 1 quand
        // ratio <= 1 (décimation sans coupure).
        out.push((acc * norm_factor(fc, taps_per_side as f64)) as f32);
    }
    out
}

fn sinc_windowed(x: f64, fc: f64, _taps: f64) -> f64 {
    if x.abs() < 1e-12 {
        return 2.0 * fc;
    }
    let window = hann_window(x);
    2.0 * fc * (std::f64::consts::PI * fc * x).sin() / (std::f64::consts::PI * fc * x) * window
}

fn hann_window(x: f64) -> f64 {
    // Fenêtre de Hann appliquée sur |x| <= taps_per_side.
    let t = x.abs() / 24.0;
    if t >= 1.0 {
        0.0
    } else {
        0.5 * (1.0 + (2.0 * std::f64::consts::PI * t).cos())
    }
}

fn norm_factor(fc: f64, _taps: f64) -> f64 {
    // Somme des poids pour x entier vaut 2*fc*sum(sinc fenêtré) ; on
    // normalise par la somme des poids entiers pour garantir un gain DC de 1.
    let mut sum = 0.0;
    for k in -24i64..24 {
        sum += sinc_windowed(k as f64, fc, 24.0);
    }
    1.0 / sum
}

/// Chaîne complète : mixdown puis ré-échantillonnage vers la fréquence
/// canonique 16 kHz (voxli-core::SAMPLE_RATE_HZ).
pub fn to_mono_16k(input: &[f32], channels: usize, in_rate: u32) -> Vec<f32> {
    let mono = mixdown_to_mono(input, channels);
    resample(&mono, in_rate, voxli_core::SAMPLE_RATE_HZ)
}
