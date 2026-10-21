//! Moteur whisper.cpp via `whisper-rs` (ADR-03) — feature `whisper`.
//!
//! Paramètres anti-hallucination (RF-18) et amorce de vocabulaire (RF-24).
//! Le modèle est chargé depuis le disque ; son empreinte SHA-256 est vérifiée
//! à l'installation (voxli-diagnostics consigne le chargement, jamais le
//! contenu des dictées).
//!
//! Choix de calcul CPU/GPU (`load_with`) : un build sans backend accéléré
//! reste CPU (valeur par défaut du compilé) ; un build avec backend
//! (Vulkan/CUDA, feature opt-in `whisper-gpu`) utilise le GPU selon le
//! choix — repli interne de ggml sur CPU si l'initialisation échoue
//! (pilote absent/occupé), jamais d'échec muet.

use std::path::Path;
use std::time::Duration;

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};

/// Motifs d'hallucination connus de Whisper sur silence/bruit (RF-18). La
/// liste est filtrée après transcription ; les motifs s'étendent avec les
/// campagnes de mesure (docs/06 §5.1).
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
    /// Choix de calcul demandé au chargement (`auto`|`cpu`|`gpu`), gardé
    /// pour l'observabilité (le journal du noyau consigne le DEMANDÉ ;
    /// le backend réellement utilisé dépend du build et des pilotes).
    compute: String,
}

// whisper_rs::WhisperContext est Send+Sync.
unsafe impl Send for WhisperCppEngine {}
unsafe impl Sync for WhisperCppEngine {}

impl WhisperCppEngine {
    /// Charge un modèle ggml (ex. `ggml-base-q5_1.bin`). Vérifie l'existence
    /// AVANT de contacter le moteur — aucun échec silencieux (RF-20).
    /// Choix de calcul par défaut : `auto` (GPU quand le build en a un).
    pub fn load(model_path: &Path) -> Result<Self, AsrError> {
        Self::load_with(model_path, "auto")
    }

    /// Charge avec un choix de calcul explicite :
    /// - `cpu` : whisper.cpp ne MONTERA jamais de GPU, même si le build en
    ///   embarque un (sémantique : le souhait explicite n'est pas contourné) ;
    /// - `gpu` : accélérateur attendu — l'utilisateur a vu l'état du build ;
    /// - `auto`/inconnu/vide : `use_gpu` reste la valeur par défaut du
    ///   compilé (GPU quand un backend est embarqué, CPU sinon) — si
    ///   l'initialisation GPU échoue (pilotes, machine occupée), ggml replie
    ///   LUI-MÊME sur le CPU : jamais d'échec muet de sortie.
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

    /// Choix de calcul DEMANDÉ au chargement (observabilité ; le réel
    /// dépend du build et des pilotes — jamais affirmer sans mesure).
    pub fn compute_choice(&self) -> &str {
        &self.compute
    }

    /// Filtre les hallucinations de silence (RF-18) — retourne `None` si le
    /// texte ne doit pas être inséré. Ancrage en DÉBUT de texte uniquement :
    /// un motif isolé au milieu d'une dictée légitime (« bye-bye » en fin de
    /// message) ne doit jamais effacer le tout. Un motif de tête est une
    /// hallucination seulement si le résidu ne contient plus rien de verbal.
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
            // Résidu après le motif : la ponctuation/espaces qui suit n'est
            // pas de la parole (« sous-titres, » → vide).
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
        // whisper.cpp compile son graphe au premier appel : un warm-up complet
        // (RF-19) passe par une inférence réelle sur 100 ms de silence ; le
        // résultat est jeté (l'audio est un silence, rien n'est conservé ni
        // inséré, RF-18). Un échec ici révèle le problème AVANT une dictée.
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
        // RF-17 : le réglage utilisateur prime ; sinon défaut moteur = 4
        // threads bornés au parallélisme disponible. Sans `set_threads`
        // explicite, whisper.cpp reste à un seul thread (constat : 32 s
        // d'inférence pour 13 s d'audio sur un i5-10210U — journal latency
        // du 19/10/2026).
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
        let duration_ms = ((t1 - t0).max(0) / 10) as u64; // unités de 10 ms
        Ok(Transcript {
            text: filtered.unwrap_or_default(),
            confidence: 0.0, // moyenne logprob exposée en P1 (RF-27)
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

    // RF-18 : le filtre ancre les motifs en TÊTE (hallucinations de
    // silence/fond) — un même motif au milieu ou en fin de dictée légitime
    // ne doit jamais effacer le tout (« À demain, bye-bye »).
    #[test]
    fn motifes_tete_sans_residu_verbal_alpha() {
        // Résidu uniquement ponctuation/espaces → hallucination (None).
        assert!(WhisperCppEngine::filter_hallucination("Merci d'avoir regardé !").is_none());
        assert!(WhisperCppEngine::filter_hallucination("bye-bye.").is_none());
        assert!(WhisperCppEngine::filter_hallucination("  Sous-titres .  ").is_none());
        assert!(WhisperCppEngine::filter_hallucination("BYE-BYE").is_none());
        // Plusieurs motifs successifs en tête.
        assert!(WhisperCppEngine::filter_hallucination(
            "Merci d'avoir regardé. Merci d'avoir écouté."
        )
        .is_none());
        // Erreur typique de Whisper sur silence : motif + « à nous… » (RF-18).
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
        // Résidu verbal après le motif de tête → dictée légitime, conservée.
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
