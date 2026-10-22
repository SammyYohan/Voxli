//! Display partials for the **batch offline** engines (whisper.cpp,
//! feature `whisper`).
//!
//! Principle: during capture, a **windowed** re-decode (the last seconds
//! of the cumulative buffer) is triggered at a bounded interval and
//! emitted over the usual streaming channel (`StreamUpdate::Partial`,
//! `dictation://partial`) — display only, **never inserted nor logged**
//! (RF-34 + ADR-02 amendment). The final text keeps coming from the
//! batch channel over the full buffer, which remains the authority
//! (docs/05 §3.1, exception 3).
//!
//! Self-disabling: if a re-decode exceeds the ceiling (see
//! [`max_decode_ms_from_env`]), partials stop for the rest of the
//! session (slow machine) — graceful degradation: the user gets back
//! the batch behavior without a break or an error message; the shutdown
//! is still logged in the journal by the driver.

use std::sync::Arc;
use std::time::Instant;

use voxli_core::{AsrError, AudioBuffer, TranscribeOptions, Transcriptor, SAMPLE_RATE_HZ};

use crate::streaming::{StreamUpdate, StreamingEngine, StreamingSession};
use crate::whisper::WhisperCppEngine;

/// Sliding window decoded for the partials: the re-decode only covers the
/// last seconds of the buffer. **Adaptive (production)**: the starting
/// window is short (2 s) and follows the machine — too slow, it shrinks
/// down to [`MIN_WINDOW_SAMPLES`] (0.6 s); fast enough, it grows back,
/// capped at 8 s = [`PARTIAL_WINDOW_SAMPLES`]. Advanced setting possible
/// via the environment variable `VOXLI_PARTIAL_WINDOW_SEC` (starting
/// value).
pub const PARTIAL_WINDOW_SAMPLES: usize = (SAMPLE_RATE_HZ as usize / 10) * 80; // 8 s
/// Window floor (0.6 s): below this length the re-decode no longer has
/// useful context (truncated words), the auto-shutdown takes over.
pub const MIN_WINDOW_SAMPLES: usize = SAMPLE_RATE_HZ as usize * 3 / 5; // 0.6 s
/// Target duration of a partial re-decode: the equilibrium point between
/// live display and reasonable CPU; the window converges toward it.
const TARGET_DECODE_MS: u128 = 1_500;
/// Minimal new audio (samples) between two re-decodes (1.2 s).
pub const MIN_NEW_SAMPLES: usize = (SAMPLE_RATE_HZ as usize / 10) * 12;
/// Minimal interval between two re-decodes (CPU throughput clamping).
pub const MIN_INTERVAL_MS: u128 = 1_500;
/// A re-decode longer than this ceiling disables the session's partials
/// (slow machine): batch finalization takes over.
pub const MAX_DECODE_MS: u128 = 2_500;

/// Ceiling duration of a partial re-decode: 2,500 ms by default, settable
/// through the environment variable `VOXLI_PARTIAL_MAX_DECODE_MS` (advanced
/// setting, like `VOXLI_PARTIAL_WINDOW_SEC`; the production configuration
/// never touches it) — bounded 250..120,000. Read at the opening of every
/// session.
fn max_decode_ms_from_env() -> u128 {
    let ms: u128 = std::env::var("VOXLI_PARTIAL_MAX_DECODE_MS")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(MAX_DECODE_MS);
    ms.clamp(250, 120_000)
}
/// Memory guard for the partial buffer: 60 s of canonical 16 kHz audio.
const MAX_BUFFER_SAMPLES: usize = (SAMPLE_RATE_HZ as usize) * 60;

/// Duration (s) of the partial starting window: 2 s by default, settable
/// through the environment variable `VOXLI_PARTIAL_WINDOW_SEC` (advanced
/// setting, the v5 production configuration never touches it) —
/// bounded 0.6..8; afterwards the window adjusts itself to the machine
/// (`adapt_window`), tests do not go through here (they tighten via
/// `with_thresholds` and keep a constant window).
fn window_samples_from_env() -> usize {
    let sec: f64 = std::env::var("VOXLI_PARTIAL_WINDOW_SEC")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(2.0);
    let clamped = sec.clamp(0.6, 8.0);
    (SAMPLE_RATE_HZ as f64 * clamped) as usize
}

/// Factory of partial sessions on top of `WhisperCppEngine`.
///
/// The full buffer stays with the orchestrator (RF-22); the session only
/// replicates the window needed for the display re-decode.
pub struct WhisperPartialStreamer {
    engine: Arc<WhisperCppEngine>,
}

impl WhisperPartialStreamer {
    pub fn new(engine: Arc<WhisperCppEngine>) -> Self {
        Self { engine }
    }
}

impl StreamingEngine for WhisperPartialStreamer {
    fn open(
        &self,
        language: Option<&str>,
        verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError> {
        let engine = Arc::clone(&self.engine);
        let language = language.map(str::to_owned);
        let decode = move |audio: &AudioBuffer| -> Result<String, AsrError> {
            let opts = TranscribeOptions {
                language: language.clone(),
                initial_prompt: None,
                verbatim,
                threads: None, // engine default (cf. corrected RF-17)
            };
            decode_window(&engine, audio, &opts)
        };
        Ok(Box::new(PartialSession::new(decode)))
    }
}

/// Window decoding through the shared engine (real engine path; the
/// tests inject their own decoder into `PartialSession`).
fn decode_window(
    engine: &Arc<WhisperCppEngine>,
    audio: &AudioBuffer,
    opts: &TranscribeOptions,
) -> Result<String, AsrError> {
    engine.transcribe(audio, opts).map(|t| {
        // Silence hallucination (RF-18): no partial at all.
        WhisperCppEngine::filter_hallucination(&t.text).unwrap_or_default()
    })
}

/// Partial-session state, decoupled from the engine: `decode` is injected
/// (real engine or test fake double).
pub struct PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError>,
{
    decode: D,
    buffer: Vec<f32>,
    /// Buffer length already covered by a decode (new audio = delta).
    decoded_until: usize,
    last_decode_at: Option<Instant>,
    /// Last decoded partial text (the snapshot's display source) —
    /// see `snapshot()` below.
    last_text: String,
    /// Decode duration ceiling (production: [`MAX_DECODE_MS`]).
    max_decode_ms: u128,
    /// Minimal interval between decodes (production: [`MIN_INTERVAL_MS`]).
    min_interval_ms: u128,
    /// Minimal new audio before a re-decode (production: 1.2 s).
    min_new_samples: usize,
    /// Length of the re-decoded window (production: starts at 2 s via env,
    /// then adjusted by `adapt_window`; tests: exact constant).
    window_samples: usize,
    /// Automatic adjustment of the window to the measured machine
    /// (production only — tests stay hermetic).
    adaptive: bool,
    /// Consecutive ticks above the ceiling: the cut (slow machine)
    /// happens only on the second — the window shrinks first; we never
    /// cut on a first measurement that adaptation alone may fix.
    slow_streak: u8,
    disabled: bool,
}

impl<D> PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError>,
{
    pub fn new(decode: D) -> Self {
        Self {
            decode,
            buffer: Vec::new(),
            decoded_until: 0,
            last_decode_at: None,
            last_text: String::new(),
            max_decode_ms: max_decode_ms_from_env(),
            min_interval_ms: MIN_INTERVAL_MS,
            min_new_samples: MIN_NEW_SAMPLES,
            window_samples: window_samples_from_env(),
            adaptive: true,
            slow_streak: 0,
            disabled: false,
        }
    }

    /// Full constructor for the tests (tightened thresholds).
    #[cfg(test)]
    fn with_thresholds(
        decode: D,
        max_decode_ms: u128,
        min_interval_ms: u128,
        min_new_samples: usize,
    ) -> Self {
        Self {
            decode,
            buffer: Vec::new(),
            decoded_until: 0,
            last_decode_at: None,
            last_text: String::new(),
            max_decode_ms,
            min_interval_ms,
            min_new_samples,
            window_samples: PARTIAL_WINDOW_SAMPLES,
            adaptive: false,
            slow_streak: 0,
            disabled: false,
        }
    }

    /// Window adjustment to the measured machine — called after every
    /// re-decode: above the target ([`TARGET_DECODE_MS`]), the window
    /// shrinks proportionally (floor [`MIN_WINDOW_SAMPLES`]); clearly
    /// below, it grows back (ceiling [`PARTIAL_WINDOW_SAMPLES`]).
    /// The goal: a display that writes live on the machines that can
    /// afford it, without ever choking the slow machines — the
    /// measurement decides, not an assumption.
    fn adapt_window(&mut self, took_ms: u128) {
        if !self.adaptive {
            return;
        }
        let target = TARGET_DECODE_MS as f64;
        let took = took_ms.max(1) as f64;
        if took > target {
            // Too slow: direct convergence toward the window that should
            // meet the target, damped at 85% to avoid oscillation.
            let scale = (target / took).min(0.9) * 0.85;
            let next = self.window_samples as f64 * scale;
            self.window_samples = (next as usize).clamp(MIN_WINDOW_SAMPLES, PARTIAL_WINDOW_SAMPLES);
        } else if target / took > 2.0 {
            // Well below the target: the machine has headroom, the
            // window (and thus the displayed context) grows back.
            let next = self.window_samples as f64 * 1.3;
            self.window_samples = (next as usize).clamp(MIN_WINDOW_SAMPLES, PARTIAL_WINDOW_SAMPLES);
        }
    }

    pub fn disabled(&self) -> bool {
        self.disabled
    }
}

impl<D> StreamingSession for PartialSession<D>
where
    D: FnMut(&AudioBuffer) -> Result<String, AsrError> + Send,
{
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError> {
        if rate == SAMPLE_RATE_HZ {
            self.buffer.extend_from_slice(samples);
        } else {
            // Linear resampling to the canonical 16 kHz (the batch
            // engine accepts ONLY 16 kHz — RF-12). Secondary path: the
            // capture faucet already delivers 16 kHz.
            let ratio = SAMPLE_RATE_HZ as f64 / rate as f64;
            if ratio.is_finite() && ratio > 0.0 {
                let out_len = (samples.len() as f64 * ratio).ceil() as usize;
                let start = self.buffer.len();
                self.buffer.resize(start + out_len.max(1), 0.0);
                for (i, out) in self.buffer[start..].iter_mut().enumerate() {
                    let src = i as f64 / ratio;
                    let left = src.floor() as usize;
                    let right = (left + 1).min(samples.len().saturating_sub(1));
                    let frac = (src - left as f64) as f32;
                    *out = samples[left] * (1.0 - frac) + samples[right] * frac;
                }
            }
        }
        // Memory guard: beyond 60 s cumulative, truncate the beginning.
        if self.buffer.len() > MAX_BUFFER_SAMPLES {
            let drop = self.buffer.len() - MAX_BUFFER_SAMPLES;
            self.buffer.drain(..drop);
            self.decoded_until = self.decoded_until.saturating_sub(drop);
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError> {
        if self.disabled {
            return Ok(Vec::new());
        }
        let new_audio = self.buffer.len().saturating_sub(self.decoded_until);
        if new_audio < self.min_new_samples {
            return Ok(Vec::new());
        }
        if let Some(last) = self.last_decode_at {
            if last.elapsed().as_millis() < self.min_interval_ms {
                return Ok(Vec::new());
            }
        }
        // Window: only the tail of the buffer (CPU cost bounded per tick,
        // settable duration — see `window_samples_from_env`).
        let window_len = self.window_samples.min(self.buffer.len());
        let start = self.buffer.len() - window_len;
        let window = AudioBuffer::new(self.buffer[start..].to_vec(), SAMPLE_RATE_HZ);
        let t0 = Instant::now();
        let text = (self.decode)(&window)?;
        let took_ms = t0.elapsed().as_millis();
        self.decoded_until = self.buffer.len();
        self.last_decode_at = Some(Instant::now());
        // The window first adapts to the measurement (production); any
        // cut then applies and stops all future commitments.
        self.adapt_window(took_ms);
        if took_ms > self.max_decode_ms {
            // Machine too slow: NO cut on the first measurement — the
            // window just shrank (`adapt_window`), the cost may
            // suffice on the next tick. TWO consecutive ticks above
            // the ceiling prove the capping: partials then stop for
            // the rest of the session — the ALREADY-PAID decode
            // stays displayed (the read is not discarded), the
            // following ticks abstain (the driver logs the cut once).
            self.slow_streak = self.slow_streak.saturating_add(1);
            if self.slow_streak >= 2 {
                self.disabled = true;
            }
        } else {
            self.slow_streak = 0;
        }
        if text.is_empty() {
            return Ok(Vec::new());
        }
        self.last_text = text.clone();
        Ok(vec![StreamUpdate::Partial { text }])
    }

    fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn finish(&mut self) -> Result<String, AsrError> {
        // The final text comes exclusively from the batch channel (full
        // buffer): `take_result` keeps its default `None`.
        Ok(String::new())
    }

    fn snapshot(&self) -> String {
        // The consumer (`worker.rs`, emits `dictation://partial`) reads the
        // snapshot as the display source: return the last re-decode. The
        // final text remains the batch channel, no second truth
        // (RF-34: display only, never inserted nor logged).
        self.last_text.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    const US: usize = SAMPLE_RATE_HZ as usize / 10; // 100 ms of audio

    /// Slow decoder: the first decode past the ceiling -> the window
    /// shrinks first (NO immediate cut); the second consecutive tick
    /// above the ceiling -> auto-off, the already-paid reads stay
    /// emitted.
    #[test]
    fn t_partial_disable_on_slow_decode() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = std::sync::Arc::new(AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&calls);
        let mut sess = PartialSession::with_thresholds(
            move |_: &AudioBuffer| {
                counter.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(5));
                Ok("texte".to_string())
            },
            1, // very low max_decode_ms (test)
            0, // no throttle
            0, // no new-audio minimum
        );
        sess.push(&[0.0; US], SAMPLE_RATE_HZ).unwrap();
        let out = sess.drain().unwrap();
        // First slow tick: not cut yet (the window tries to adapt), but
        // the paid decode is indeed emitted.
        assert_eq!(
            out.len(),
            1,
            "premier tick lent -> émis, PAS de coupe immédiate"
        );
        assert!(matches!(out[0], StreamUpdate::Partial { ref text } if text == "texte"));
        assert!(
            !sess.disabled(),
            "une seule mesure lente ne coupe pas (la fenêtre s'adapte)"
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        sess.push(&[0.0; US], SAMPLE_RATE_HZ).unwrap();
        let out = sess.drain().unwrap();
        // Second consecutive slow tick: the capping is proven, the
        // session cuts itself — the already-paid read stays emitted once.
        assert_eq!(out.len(), 1, "lecture du deuxième tick toujours émise");
        assert!(sess.is_disabled(), "session auto-désactivée (trait)");
        assert!(sess.disabled(), "session auto-désactivée (inhérente)");
        assert_eq!(calls.load(Ordering::Relaxed), 2, "deux décodages tentés");
        sess.push(&[0.0; US], SAMPLE_RATE_HZ).unwrap();
        assert!(sess.drain().unwrap().is_empty(), "plus jamais après off");
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }

    /// Fast decoder: new audio required, emission bounded to the window.
    #[test]
    fn t_partial_requires_new_audio_window_bounded() {
        let seen: Mutex<Vec<usize>> = Mutex::new(Vec::new());
        let lense = &seen;
        let mut sess = PartialSession::with_thresholds(
            move |audio: &AudioBuffer| {
                lense.lock().unwrap().push(audio.samples.len());
                Ok("partiel-eco".to_string())
            },
            u128::MAX, // never too slow
            0,         // no throttle
            10 * US,   // 1 s of new audio minimum
        );
        // 0.5 s of new audio: below the threshold -> nothing.
        sess.push(&[0.0; 5 * US], SAMPLE_RATE_HZ).unwrap();
        assert!(sess.drain().unwrap().is_empty());
        // Total 1.1 s: a partial is emitted.
        sess.push(&[0.0; 6 * US], SAMPLE_RATE_HZ).unwrap();
        let out = sess.drain().unwrap();
        assert_eq!(out.len(), 1);
        assert!(matches!(out[0], StreamUpdate::Partial { ref text } if text.contains("partiel")));
        // Without new audio: no re-decode.
        assert!(sess.drain().unwrap().is_empty());
        // The final window (buffer > 8 s) decodes only the last 8 s.
        sess.push(&[0.0; 70 * US], SAMPLE_RATE_HZ).unwrap();
        assert_eq!(sess.drain().unwrap().len(), 1);
        for len in seen.lock().unwrap().iter() {
            assert!(len <= &PARTIAL_WINDOW_SAMPLES, "fenêtre > 8 s");
        }
        assert_eq!(
            *seen.lock().unwrap().last().unwrap(),
            PARTIAL_WINDOW_SAMPLES,
            "appel final = fenêtre exacte de 8 s"
        );
        // Snapshot: the last decoded partial (the real display text).
        assert_eq!(sess.snapshot(), "partiel-eco");
        assert_eq!(sess.finish().unwrap(), "");
    }

    /// Window adjustment (production): slow → shrinks down to the
    /// floor; fast → grows back up to the ceiling; hermetic (no real
    /// clock in play, the measurement is provided).
    #[test]
    fn t_adapt_window_shrinks_and_grows() {
        let mut sess =
            PartialSession::with_thresholds(|_: &AudioBuffer| Ok(String::new()), u128::MAX, 0, 0);
        sess.adaptive = true;
        // Decode at 6× the target (9 s measured) from an 8 s window:
        // converging below the target…
        sess.window_samples = PARTIAL_WINDOW_SAMPLES;
        sess.adapt_window(6 * TARGET_DECODE_MS);
        // …the window clearly shrank but never goes below the floor.
        assert!(sess.window_samples < PARTIAL_WINDOW_SAMPLES);
        assert!(sess.window_samples >= MIN_WINDOW_SAMPLES);
        // If even the floor exceeds the target (very slow machine), the
        // window stays at the floor, then the cut takes over.
        sess.window_samples = MIN_WINDOW_SAMPLES;
        sess.adapt_window(20 * TARGET_DECODE_MS);
        assert_eq!(sess.window_samples, MIN_WINDOW_SAMPLES);
        // Very fast decode: the window grows back, never beyond 8 s.
        sess.window_samples = MIN_WINDOW_SAMPLES;
        sess.adapt_window(TARGET_DECODE_MS / 10);
        assert!(sess.window_samples > MIN_WINDOW_SAMPLES);
        sess.window_samples = PARTIAL_WINDOW_SAMPLES;
        sess.adapt_window(TARGET_DECODE_MS / 10);
        assert_eq!(sess.window_samples, PARTIAL_WINDOW_SAMPLES);
        // Test hermeticity: `with_thresholds` never adjusts.
        let mut fixe =
            PartialSession::with_thresholds(|_: &AudioBuffer| Ok(String::new()), u128::MAX, 0, 0);
        fixe.window_samples = PARTIAL_WINDOW_SAMPLES;
        fixe.adapt_window(50 * TARGET_DECODE_MS);
        assert_eq!(fixe.window_samples, PARTIAL_WINDOW_SAMPLES);
    }
}
