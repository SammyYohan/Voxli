//! Moteur **Parakeet TDT v3** (ONNX, nemo_transducer) via la crate
//! officielle `sherpa-onnx` (ADR-03 point 2, décision de recherche §7).
//!
//! - modèles : `sherpa-onnx-nemo-parakeet-tdt-0.6b-v3[-int8]` — 25 langues,
//!   ponctuation et casse natives, poids CC-BY-4.0 (attribution requise :
//!   écran « Licences » de l'application et THIRD-PARTY-LICENSES.md) ;
//! - `warmup` recharge le contexte (aucun précharge utile : création à
//!   l'installation) ; la transcription reste strictement locale ;
//! - l'amorce de vocabulaire (RF-24) n'existe pas pour ce moteur : les
//!   néologismes métier passent par le dictionnaire post-traitement.
//!   Documenté à l'écran de choix du moteur.

use std::path::Path;

use voxli_core::{AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor};

use sherpa_onnx::{OfflineRecognizer, OfflineRecognizerConfig, OfflineTransducerModelConfig};

pub struct ParakeetOnnxEngine {
    recognizer: OfflineRecognizer,
    model_dir: String,
}

// Les structures sherpa-onnx sont conçues pour être partagées entre threads
// (panier global onnxruntime). L'invariant doc est celui de la crate.
unsafe impl Send for ParakeetOnnxEngine {}
unsafe impl Sync for ParakeetOnnxEngine {}

/// Cherche un fichier du modèle (int8 d'abord, sinon fp32) dans le dossier.
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
    /// Charge le dossier du modèle (contenant encoder/decoder/joiner/tokens).
    /// Vérifie la présence des fichiers AVANT tout chargement lourd (RF-20).
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
        // Le modèle est chargé à `load` ; réchauffer coûterait un modèle.
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
            // Pas de score native exploitable côté parakeet : confiance
            // neutre (les règles RF-27 veillent au reste).
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
