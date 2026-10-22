//! **Parakeet TDT v3** engine (ONNX, nemo_transducer) through the
//! official `sherpa-onnx` crate (ADR-03 point 2, research decision §7).
//!
//! - models: `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3[-int8]` — 25
//!   languages, native punctuation and casing, CC-BY-4.0 weights
//!   (attribution required: the app's "Licenses" screen and
//!   THIRD-PARTY-LICENSES.md);
//! - `warmup` pushes 150 ms of silence through the real decoding path
//!   (RF-19): onnxruntime allocates its buffers on the first decode,
//!   ours are ready before the first dictation; the transcription
//!   stays strictly local;
//! - the vocabulary bootstrap (RF-24) does not exist for this engine:
//!   domain neologisms go through the post-processing dictionary.
//!   Documented on the engine-choice screen.

use std::path::Path;

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

pub struct ParakeetOnnxEngine {
    recognizer: OfflineRecognizer,
    model_dir: String,
}

// The sherpa-onnx structures are designed to be shared across threads
// (global onnxruntime pool). The documented invariant is the crate's.
unsafe impl Send for ParakeetOnnxEngine {}
unsafe impl Sync for ParakeetOnnxEngine {}

/// Looks up a model file (int8 first, else fp32) in the directory.
fn find_file(dir: &Path, base: &str, exts: &[&str]) -> Option<std::path::PathBuf> {
    for name in [format!("{base}.int8.onnx"), format!("{base}.onnx")] {
        let p = dir.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    for ext in exts {
        let p = dir.join(format!("{base}{ext}"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

impl ParakeetOnnxEngine {
    /// Loads the model directory (containing encoder/decoder/joiner/tokens).
    /// Checks the files' presence BEFORE any heavy loading (RF-20).
    pub fn load(model_dir: &Path, num_threads: i32) -> Result<Self, AsrError> {
        let missing = |n: &str| {
            AsrError::ModelMissing(format!(
                "{n} absent de {} — installez le modèle "
                    + "(sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8)",
                model_dir.display()
            ))
        };
        let encoder =
            find_file(model_dir, "encoder", &[]).ok_or_else(|| missing("encoder.onnx"))?;
        let decoder =
            find_file(model_dir, "decoder", &[]).ok_or_else(|| missing("decoder.onnx"))?;
        let joiner = find_file(model_dir, "joiner", &[]).ok_or_else(|| missing("joiner.onnx"))?;
        let tokens =
            find_file(model_dir, "tokens", &[".txt"]).ok_or_else(|| missing("tokens.txt"))?;

        let mut config = OfflineRecognizerConfig::default();
        config.model_config.transducer = OfflineTransducerModelConfig {
            encoder: Some(encoder.to_string_lossy().into_owned()),
            decoder: Some(decoder.to_string_lossy().into_owned()),
            joiner: Some(joiner.to_string_lossy().into_owned()),
        };
        config.model_config.tokens = Some(tokens.to_string_lossy().into_owned());
        config.model_config.model_type = Some("nemo_transducer".into());
        config.model_config.num_threads = num_threads.max(1);
        config.model_config.debug = 0;

        let recognizer = OfflineRecognizer::create(&config)
            .map_err(|e| AsrError::ModelMissing(e.to_string()))?;
        Ok(Self {
            recognizer,
            model_dir: model_dir.display().to_string(),
        })
    }

    pub fn model_dir(&self) -> &str {
        &self.model_dir
    }
}

impl Transcriptor for ParakeetOnnxEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        // Full warm-up (RF-19): onnxruntime allocates its internal buffers
        // on the first decode; we push 150 ms of silence through the real
        // path (stream creation + decoding). An empty result is normal
        // on silence — neither an error nor an insertion.
        let silence = AudioBuffer::new(
            vec![0.0; SAMPLE_RATE_HZ as usize * 15 / 100],
            SAMPLE_RATE_HZ,
        );
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(silence.sample_rate, &silence.samples);
        self.recognizer.decode(&stream);
        let _ = stream.get_result();
        Ok(())
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        _opts: &TranscribeOptions,
    ) -> Result<Transcript, AsrError> {
        if audio.samples.is_empty() {
            return Err(AsrError::Inference);
        }
        let stream = self.recognizer.create_stream();
        stream.accept_waveform(audio.sample_rate, &audio.samples);
        self.recognizer.decode(&stream);
        let result = stream.get_result().ok_or(AsrError::Inference)?;
        Ok(Transcript {
            text: result.text,
            // No natively exploitable score on the parakeet side: neutral
            // confidence (the RF-27 rules watch over the rest).
            confidence: 0.9,
            duration: audio.duration(),
            engine: EngineId::ParakeetOnnx,
            model: Some("parakeet-tdt-0.6b-v3".into()),
        })
    }

    fn id(&self) -> EngineId {
        EngineId::ParakeetOnnx
    }
}
