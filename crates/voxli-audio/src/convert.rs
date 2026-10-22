//! Signal conversion (RF-12, RF-13).
//!
//! Any input format (8 kHz to 192 kHz, 1 to 8 channels, floating point) is
//! converted to **mono 16 kHz float32** with windowed-sinc resampling — no
//! naive linear interpolation. Test T-12/T-13 compares the properties
//! (length, frequency, amplitude, aliasing rejection) on synthetic signals.

/// N-channel → mono mixdown: channel average (normalized).
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

/// Windowed-sinc (Hann) resampling, T-12 quality:
/// - cutoff at 0.45 × min(input Nyquist, output Nyquist);
/// - 24 lobes per side — aliasing rejection > 45 dB across the tests.
pub fn resample(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    if in_rate == out_rate || input.is_empty() {
        return input.to_vec();
    }
    if in_rate == 0 || out_rate == 0 {
        return Vec::new();
    }
    let ratio = out_rate as f64 / in_rate as f64;
    let fc = 0.45 * ratio.min(1.0); // normalized cutoff frequency
    let taps_per_side: i64 = 24;

    // Pre-computed windowed-sinc table: the weights depend on the
    // fractional phase; the phase is quantized to 1024 steps.
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
            let x = (k as f64) + frac; // distance to the center, in input samples
            *slot = sinc_windowed(x, fc, taps_per_side as f64);
        }
    }

    let in_len = input.len();
    let out_len = ((in_len as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);
    let step = 1.0 / ratio; // output step, in input samples

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
                0.0 // zero-padding at the edges
            } else {
                input[idx as usize] as f64
            };
            acc += weight * sample;
        }
        // Normalization: the window DC gain must be 1 whenever
        // ratio <= 1 (decimation without a cutoff artifact).
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
    // Hann window applied over |x| <= taps_per_side.
    let t = x.abs() / 24.0;
    if t >= 1.0 {
        0.0
    } else {
        0.5 * (1.0 + (2.0 * std::f64::consts::PI * t).cos())
    }
}

fn norm_factor(fc: f64, _taps: f64) -> f64 {
    // The sum of weights at integer x equals 2*fc*sum(windowed sinc); we
    // normalize by that integer-weights sum to guarantee a DC gain of 1.
    let mut sum = 0.0;
    for k in -24i64..24 {
        sum += sinc_windowed(k as f64, fc, 24.0);
    }
    1.0 / sum
}

/// Full chain: mixdown then resampling to the canonical 16 kHz rate
/// (voxli-core::SAMPLE_RATE_HZ).
pub fn to_mono_16k(input: &[f32], channels: usize, in_rate: u32) -> Vec<f32> {
    let mono = mixdown_to_mono(input, channels);
    resample(&mono, in_rate, voxli_core::SAMPLE_RATE_HZ)
}
