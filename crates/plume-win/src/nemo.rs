//! Liaison au SDK C de NeMo-Speech.cpp (ADR-06 : l'unsafe vit ici et dans
//! `plume-asr` pour le façade). SDK NVIDIA Apache-2.0, prérequis machine :
//! installation locale (`scripts`/README §6) et `NEMO_SPEECH_PREFIX`
//! quand elle n'est pas à l'emplacement par défaut.
//!
//! Contrat : opaque handles, structs POD avec champ `size`. Le champ `size`
//! est FIXÉ à `std::mem::size_of::<T>()` : on compile contre l'archive
//! installée de la même version, l'append-only n'est donc pas exploitée ici.
//!
//! Aucune fonction ne panique ; tout échec passe par le code de statut.

use std::ffi::c_char;

/// Codes de statut (`nemo_speech_asr_status`).
pub mod status {
    pub const OK: i32 = 0;
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const OUT_OF_MEMORY: i32 = 2;
    pub const RUNTIME: i32 = 3;
    pub const CANCELLED: i32 = 4;
}

// (le module `status` ci-dessus est référencé directement)

// ----------------------------------------------------- opaque handles ---

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct nemo_speech_asr_recognizer {
    _private: [u8; 0],
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct nemo_speech_asr_stream {
    _private: [u8; 0],
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct nemo_speech_asr_result {
    _private: [u8; 0],
}

// ---------------------------------------------------------- structs ---

/// `gpu` : -1 = CPU, 0+ = index logique du périphérique CUDA.
#[repr(C)]
pub struct AsrBackendConfig {
    pub size: usize,
    pub gpu: i32,
}

/// `path` : GGUF local ; `name` : NULL = dérivé du modèle.
#[repr(C)]
pub struct AsrModelConfig {
    pub size: usize,
    pub path: *const c_char,
    pub name: *const c_char,
}

/// Configs optionnelles inutilisées pour l'instant (VAD, ponctuation,
/// endpointing, post-traitement) : l'ABI les reçoit par pointeur, NULL =
/// défauts du moteur. Le type réel serait celui du header C inchangé.
#[repr(C)]
pub struct AsrRecognizerConfig {
    pub size: usize,
    pub backend: *const AsrBackendConfig,
    pub model: *const AsrModelConfig,
    pub streaming: *const std::ffi::c_void,
    pub decoder: *const std::ffi::c_void,
    pub vad: *const std::ffi::c_void,
    pub endpointing: *const std::ffi::c_void,
    pub postproc: *const std::ffi::c_void,
    pub diar: *const std::ffi::c_void,
    pub batching: *const std::ffi::c_void,
}

#[repr(C)]
pub struct AsrRecognitionOptions {
    pub size: usize,
    pub request_id: *const c_char,
    pub language_code: *const c_char,
    /// Résultats intermédiaires (texte partiel en direct).
    pub interim_results: bool,
    pub enable_word_time_offsets: bool,
    pub enable_automatic_punctuation: bool,
    pub verbatim_transcripts: bool,
    pub profanity_filter: bool,
    pub stop_history_eou_ms: i32,
    pub speech_contexts: *const std::ffi::c_void,
    pub speech_context_count: usize,
    pub max_alternatives: i32,
    pub enable_speaker_diarization: bool,
    pub max_speaker_count: i32,
}

// ---------------------------------------------------------- externes ---

#[link(name = "nemo_speech_asr_c", kind = "dylib")]
extern "C" {
    /// Options par défaut de la bibliothèque (champ `size` déjà calé).
    pub fn nemo_speech_asr_recognition_options_default() -> AsrRecognitionOptions;

    pub fn nemo_speech_asr_create(
        cfg: *const AsrRecognizerConfig,
        out: *mut *mut nemo_speech_asr_recognizer,
    ) -> i32;
    pub fn nemo_speech_asr_destroy(recognizer: *mut nemo_speech_asr_recognizer);

    /// Décode un énoncé entier (mono f32, 8–96 kHz, resample interne).
    pub fn nemo_speech_asr_recognize_f32(
        recognizer: *mut nemo_speech_asr_recognizer,
        options: *const AsrRecognitionOptions,
        samples: *const f32,
        n_samples: usize,
        sample_rate: i32,
        out: *mut *mut nemo_speech_asr_result,
    ) -> i32;

    /// Ouvre un flux streaming piloté par `push` / `next` / `finish` /
    /// `close`. Un flux est piloté par UN SEUL thread (contrat SDK).
    pub fn nemo_speech_asr_streaming_recognize(
        recognizer: *mut nemo_speech_asr_recognizer,
        options: *const AsrRecognitionOptions,
        out: *mut *mut nemo_speech_asr_stream,
    ) -> i32;
    pub fn nemo_speech_asr_stream_push_f32(
        stream: *mut nemo_speech_asr_stream,
        samples: *const f32,
        n_samples: usize,
        sample_rate: i32,
    ) -> i32;
    /// Fin d'énoncé immédiate, honorée au prochain `next`.
    pub fn nemo_speech_asr_stream_force_endpoint(stream: *mut nemo_speech_asr_stream) -> i32;
    pub fn nemo_speech_asr_stream_finish(stream: *mut nemo_speech_asr_stream) -> i32;
    /// Pull : OK + NULL = rien de disponible (il faut plus d'audio).
    pub fn nemo_speech_asr_stream_next(
        stream: *mut nemo_speech_asr_stream,
        out: *mut *mut nemo_speech_asr_result,
    ) -> i32;
    pub fn nemo_speech_asr_stream_close(stream: *mut nemo_speech_asr_stream);

    // ----- accès résultats (mémoire propriété du résultat) -----
    pub fn nemo_speech_asr_result_is_final(result: *const nemo_speech_asr_result) -> bool;
    pub fn nemo_speech_asr_result_alternative_count(result: *const nemo_speech_asr_result)
        -> usize;
    /// Pointeur valable jusqu'à `nemo_speech_asr_result_destroy`.
    pub fn nemo_speech_asr_result_transcript(
        result: *const nemo_speech_asr_result,
        alt: usize,
    ) -> *const c_char;
    pub fn nemo_speech_asr_result_confidence(
        result: *const nemo_speech_asr_result,
        alt: usize,
    ) -> f32;
    pub fn nemo_speech_asr_result_destroy(result: *mut nemo_speech_asr_result);

    // ----- divers -----
    /// Message d'erreur du dernier appel raté sur CE thread (valable jusqu'à
    /// l'appel suivant de la famille).
    pub fn nemo_speech_asr_last_error() -> *const c_char;
    pub fn nemo_speech_asr_version() -> *const c_char;
}

// ------------------------------------------------------ façade sûre ---

use std::ffi::CStr;

/// Version du runtime (chaîne statique du SDK).
pub fn runtime_version() -> Result<String, String> {
    unsafe {
        let p = nemo_speech_asr_version();
        if p.is_null() {
            return Err("nemo_speech_asr_version a renvoyé NULL".to_string());
        }
        CStr::from_ptr(p)
            .to_str()
            .map(|s| s.to_string())
            .map_err(|e| e.to_string())
    }
}

/// Message d'erreur thread-local du SDK, copié (le pointeur meurt vite).
pub fn last_error() -> String {
    unsafe {
        let p = nemo_speech_asr_last_error();
        if p.is_null() {
            "(message d'erreur SDK indisponible)".to_string()
        } else {
            CStr::from_ptr(p)
                .to_str()
                .unwrap_or("(erreur illisible)")
                .to_string()
        }
    }
}

/// Traduit un statut en message cette façade : dernière erreur du thread.
pub fn describe(st: i32) -> String {
    match st {
        status::OK => "ok".to_string(),
        status::INVALID_ARGUMENT => {
            format!("argument invalide : {}", last_error())
        }
        status::OUT_OF_MEMORY => "mémoire insuffisante (SDK)".to_string(),
        status::RUNTIME => {
            format!("échec runtime : {}", last_error())
        }
        status::CANCELLED => "reconnaissance annulée".to_string(),
        other => {
            format!("statut inconnu {other} : {}", last_error())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// La DLL se charge et la version du runtime répond : preuve minimale
    /// de liaison sans modèle téléchargé.
    #[test]
    fn runtime_version_lies() {
        let v = runtime_version();
        assert!(
            v.is_ok() && v.as_deref().unwrap_or("").starts_with("nemo-speech-asr "),
            "version SDK absente ou inattendue : {v:?}"
        );
    }
}
