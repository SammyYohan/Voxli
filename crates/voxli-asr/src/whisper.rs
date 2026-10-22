//! whisper.cpp engine via `whisper-rs` (ADR-03) — feature `whisper`.
//!
//! Anti-hallucination settings (RF-18) and vocabulary bootstrap (RF-24).
//! The model is loaded from disk; its SHA-256 fingerprint is verified at
//! install time (voxli-diagnostics logs the load, never the content of
//! dictations).
//!
//! CPU/GPU compute choice (`load_with`): a build without an accelerated
//! backend stays on CPU (the compiled default); a build with a backend
//! (Vulkan/CUDA, opt-in feature `whisper-gpu`) uses the GPU according to
//! the choice — internal ggml fallback to CPU if initialization fails
//! (missing/busy driver), never a silent failure.

use std::path::Path;
use std::time::Duration;

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};

/// Hallucination patterns Whisper is known to produce on silence/noise
/// (RF-18). The list is filtered after transcription; patterns grow with
/// the measurement campaigns (docs/06 §5.1).
const HALLUCINATION_PATTERNS: &[&str] = &[
    "merci d'avoir regardé",
    "merci d'avoir écouté",
    "sous-titrage",
    "sous-titres",
    "à nous dans la prochaine vidéo",
    "thanks for watching",
    "bye-bye",
    "amish",
];

pub struct WhisperCppEngine {
    ctx: whisper_rs::WhisperContext,
    model_id: String,
    /// Compute choice requested at load time (`auto`|`cpu`|`gpu`), kept
    /// for observability (the core log records the REQUESTED choice;
    /// the backend actually used depends on the build and the drivers).
    compute: String,
}

// whisper_rs::WhisperContext is Send+Sync.
unsafe impl Send for WhisperCppEngine {}
unsafe impl Sync for WhisperCppEngine {}

impl WhisperCppEngine {
    /// Loads a ggml model (e.g. `ggml-base-q5_1.bin`). Verifies existence
    /// BEFORE touching the engine — no silent failure (RF-20).
    /// Default compute choice: `auto` (GPU when the build bundles one).
    pub fn load(model_path: &Path) -> Result<Self, AsrError> {
        Self::load_with(model_path, "auto")
    }

    /// Loads with an explicit compute choice:
    /// - `cpu`: whisper.cpp will never bring up a GPU, even if the build
    ///   bundles one (semantics: an explicit wish is not bypassed);
    /// - `gpu`: accelerator expected — the user has seen the build state;
    /// - `auto`/unknown/empty: `use_gpu` keeps the compiled default
    ///   (GPU when a backend is bundled, CPU otherwise) — if GPU
    ///   initialization fails (drivers, busy machine), ggml falls back
    ///   on its own to the CPU: never a silent failure handed out.
    pub fn load_with(model_path: &Path, compute: &str) -> Result<Self, AsrError> {
        if !model_path.is_file() {
            return Err(AsrError::ModelMissing(model_path.display().to_string()));
        }
        let mut params = whisper_rs::WhisperContextParameters::default();
        if compute == "cpu" {
            params.use_gpu = false;
        }
        let ctx = whisper_rs::WhisperContext::new_with_params(model_path, params)
            .map_err(|e| AsrError::ModelMissing(e.to_string()))?;
        let model_id = model_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let normalized = match compute {
            "cpu" => "cpu".to_string(),
            "gpu" => "gpu".to_string(),
            _ => "auto".to_string(),
        };
        Ok(Self {
            ctx,
            model_id,
            compute: normalized,
        })
    }

    /// Compute choice REQUESTED at load time (observability; the actual
    /// one depends on the build and the drivers — never claim without
    /// measuring).
    pub fn compute_choice(&self) -> &str {
        &self.compute
    }

    /// Filters silence hallucinations (RF-18) — returns `None` when the
    /// text must not be inserted. Anchoring at the START of the text only:
    /// an isolated pattern in the middle of a legitimate dictation
    /// ("bye-bye" at the end of a message) must never wipe it all. A
    /// leading pattern is a hallucination only when the residue no longer
    /// holds anything verbal.
    pub fn filter_hallucination(text: &str) -> Option<String> {
        let trimmed = text.trim();
        let mut lower = trimmed.to_lowercase();
        if lower.is_empty() {
            return None;
        }
        loop {
            let matched = HALLUCINATION_PATTERNS
                .iter()
                .find(|pattern| lower.starts_with(**pattern));
            let Some(pattern) = matched else {
                break;
            };
            // Residue after the pattern: the punctuation/spaces that follows
            // is not speech ("sous-titres," → empty).
            lower = lower[pattern.len()..]
                .trim()
                .trim_start_matches(|c: char| !c.is_alphanumeric())
                .trim()
                .to_string();
        }
        if lower.chars().all(|c| !c.is_alphanumeric()) {
            return None;
        }
        Some(trimmed.to_string())
    }
}

impl Transcriptor for WhisperCppEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        // whisper.cpp compiles its graph on the first call: full warm-up
        // (RF-19) goes through a real inference on 100 ms of silence; the
        // result is discarded (the audio is silence, nothing is kept nor
        // inserted, RF-18). A failure here reveals the problem BEFORE a
        // dictation.
        let silence = AudioBuffer::new(vec![0.0; (SAMPLE_RATE_HZ / 10) as usize], SAMPLE_RATE_HZ);
        let opts = TranscribeOptions {
            language: None,
            initial_prompt: None,
            verbatim: false,
            threads: None,
        };
        self.transcribe(&silence, &opts).map(|_| ())
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if audio.sample_rate != SAMPLE_RATE_HZ {
            return Err(AsrError::Inference);
        }
        let mut state = self.ctx.create_state().map_err(|_| AsrError::Inference)?;

        let mut params =
            whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        // RF-17: the user setting wins; otherwise the engine default is 4
        // threads capped at available parallelism. Without an explicit
        // `set_threads`, whisper.cpp stays on a single thread (observed: 32 s
        // of inference for 13 s of audio on an i5-10210U — latency journal
        // entry of 19/10/2026).
        let n_threads = opts.threads.map(|t| t.max(1) as i32).unwrap_or_else(|| {
            let cores = std::thread::available_parallelism()
                .map(|n| n.get() as i32)
                .unwrap_or(4);
            4.min(cores)
        });
        params.set_n_threads(n_threads);
        params.set_language(opts.language.as_deref());
        params.set_temperature(0.0);
        params.set_no_speech_thold(0.6);
        params.set_suppress_blank(true);
        params.set_no_context(true);
        if let Some(prompt) = &opts.initial_prompt {
            if !prompt.is_empty() {
                params.set_initial_prompt(prompt);
            }
        }

        state
            .full(params, &audio.samples)
            .map_err(|_| AsrError::Inference)?;

        let n_segments = state.full_n_segments();
        let mut text = String::new();
        let mut t0: i64 = 0;
        let mut t1: i64 = 0;
        for i in 0..n_segments {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };
            let seg_text = seg.to_str().map_err(|_| AsrError::Inference)?;
            text.push_str(seg_text);
            t1 = seg.end_timestamp();
            if i == 0 {
                t0 = seg.start_timestamp();
            }
        }

        let filtered = Self::filter_hallucination(&text);
        let duration_ms = ((t1 - t0).max(0) / 10) as u64; // units of 10 ms
        Ok(Transcript {
            text: filtered.unwrap_or_default(),
            confidence: 0.0, // logprob average exposed in P1 (RF-27)
            duration: Duration::from_millis(duration_ms),
            engine: EngineId::WhisperCpp,
            model: Some(self.model_id.clone()),
        })
    }

    fn id(&self) -> EngineId {
        EngineId::WhisperCpp
    }
}

#[cfg(test)]
mod filter_tests {
    use super::WhisperCppEngine;

    // RF-18: the filter anchors patterns at the HEAD (hallucinations of
    // silence/background) — the same pattern in the middle or at the end
    // of a legitimate dictation must never wipe it all
    // ("À demain, bye-bye").
    #[test]
    fn motifes_tete_sans_residu_verbal_alpha() {
        // Residue is punctuation/whitespace only → hallucination (None).
        assert!(WhisperCppEngine::filter_hallucination("Merci d'avoir regardé !").is_none());
        assert!(WhisperCppEngine::filter_hallucination("bye-bye.").is_none());
        assert!(WhisperCppEngine::filter_hallucination("  Sous-titres .  ").is_none());
        assert!(WhisperCppEngine::filter_hallucination("BYE-BYE").is_none());
        // Several successive leading patterns.
        assert!(WhisperCppEngine::filter_hallucination(
            "Merci d'avoir regardé. Merci d'avoir écouté."
        )
        .is_none());
        // Typical Whisper mistake on silence: pattern + "à nous…" (RF-18).
        assert!(WhisperCppEngine::filter_hallucination(
            "Merci d'avoir regardé — à nous dans la prochaine vidéo !"
        )
        .is_none());
    }

    #[test]
    fn motif_isole_en_fin_ou_milieu_est_conserve() {
        assert_eq!(
            WhisperCppEngine::filter_hallucination("À demain, bye-bye."),
            Some("À demain, bye-bye.".to_string())
        );
        assert_eq!(
            WhisperCppEngine::filter_hallucination("La sous-titrage n'existe pas ici."),
            Some("La sous-titrage n'existe pas ici.".to_string())
        );
    }

    #[test]
    fn motif_tete_avec_residu_verbal_est_conserve() {
        // Verbal residue after the leading pattern → legitimate dictation,
        // kept.
        assert_eq!(
            WhisperCppEngine::filter_hallucination("Sous-titrage du dialogue, s'il te plaît"),
            Some("Sous-titrage du dialogue, s'il te plaît".to_string())
        );
    }

    #[test]
    fn vide_ou_blancs() {
        assert!(WhisperCppEngine::filter_hallucination("").is_none());
        assert!(WhisperCppEngine::filter_hallucination("   ").is_none());
        assert_eq!(
            WhisperCppEngine::filter_hallucination("  Bonjour. "),
            Some("Bonjour.".to_string())
        );
    }
}
