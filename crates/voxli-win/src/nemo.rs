//! Binding to the NeMo-Speech.cpp C SDK (ADR-06: the unsafe lives here and
//! in `voxli-asr` for the façade). NVIDIA SDK, Apache-2.0, machine
//! prerequisite: local installation (`scripts`/README §6) and
//! `NEMO_SPEECH_PREFIX` when it is not at the default location.
//!
//! Contract: opaque handles, POD structs with a `size` field. The `size`
//! field is FIXED to `std::mem::size_of::<T>()`: we compile against the
//! installed archive of the same version, so append-only evolution is
//! not leveraged here.
//!
//! No function panics; every failure goes through the status code.

use std::ffi::c_char;

/// Status codes (`nemo_speech_asr_status`).
pub mod status {
    pub const OK: i32 = 0;
    pub const INVALID_ARGUMENT: i32 = 1;
    pub const OUT_OF_MEMORY: i32 = 2;
    pub const RUNTIME: i32 = 3;
    pub const CANCELLED: i32 = 4;
}

// (the `status` module above is referenced directly)

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

/// `gpu`: -1 = CPU, 0+ = logical CUDA device index.
#[repr(C)]
pub struct AsrBackendConfig {
    pub size: usize,
    pub gpu: i32,
}

/// `path`: local GGUF; `name`: NULL = derived from the model.
#[repr(C)]
pub struct AsrModelConfig {
    pub size: usize,
    pub path: *const c_char,
    pub name: *const c_char,
}

/// Optional configs unused for now (VAD, punctuation,
/// endpointing, post-processing): the ABI receives them by pointer, NULL =
/// engine defaults. The real type would be that of the unchanged C header.
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
    /// Interim results (live partial text).
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

// --------------------------------------------------------- externs ---

#[link(name = "nemo_speech_asr_c", kind = "dylib")]
extern "C" {
    /// Library default options (`size` field already set).
    pub fn nemo_speech_asr_recognition_options_default() -> AsrRecognitionOptions;

    pub fn nemo_speech_asr_create(
        cfg: *const AsrRecognizerConfig,
        out: *mut *mut nemo_speech_asr_recognizer,
    ) -> i32;
    pub fn nemo_speech_asr_destroy(recognizer: *mut nemo_speech_asr_recognizer);

    /// Decodes a whole utterance (mono f32, 8–96 kHz, internal resampling).
    pub fn nemo_speech_asr_recognize_f32(
        recognizer: *mut nemo_speech_asr_recognizer,
        options: *const AsrRecognitionOptions,
        samples: *const f32,
        n_samples: usize,
        sample_rate: i32,
        out: *mut *mut nemo_speech_asr_result,
    ) -> i32;

    /// Opens a streaming stream driven by `push` / `next` / `finish` /
    /// `close`. A stream is driven by A SINGLE thread (SDK contract).
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
    /// Immediate end of utterance, honored at the next `next`.
    pub fn nemo_speech_asr_stream_force_endpoint(stream: *mut nemo_speech_asr_stream) -> i32;
    pub fn nemo_speech_asr_stream_finish(stream: *mut nemo_speech_asr_stream) -> i32;
    /// Pull: OK + NULL = nothing available (more audio is needed).
    pub fn nemo_speech_asr_stream_next(
        stream: *mut nemo_speech_asr_stream,
        out: *mut *mut nemo_speech_asr_result,
    ) -> i32;
    pub fn nemo_speech_asr_stream_close(stream: *mut nemo_speech_asr_stream);

    // ----- result accessors (memory owned by the result) -----
    pub fn nemo_speech_asr_result_is_final(result: *const nemo_speech_asr_result) -> bool;
    pub fn nemo_speech_asr_result_alternative_count(result: *const nemo_speech_asr_result)
        -> usize;
    /// Pointer valid until `nemo_speech_asr_result_destroy`.
    pub fn nemo_speech_asr_result_transcript(
        result: *const nemo_speech_asr_result,
        alt: usize,
    ) -> *const c_char;
    pub fn nemo_speech_asr_result_confidence(
        result: *const nemo_speech_asr_result,
        alt: usize,
    ) -> f32;
    pub fn nemo_speech_asr_result_destroy(result: *mut nemo_speech_asr_result);

    // ----- misc -----
    /// Error message of the last failed call on THIS thread (valid until
    /// the next call of the family).
    pub fn nemo_speech_asr_last_error() -> *const c_char;
    pub fn nemo_speech_asr_version() -> *const c_char;
}

// --------------------------------------------------- safe façade ---

use std::ffi::CStr;

/// Runtime version (static string in the SDK).
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

/// Thread-local SDK error message, copied (the pointer dies quickly).
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

/// Maps a status to a message in this façade: the thread's last error.
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

    /// The DLL loads and the runtime version answers: minimal proof
    /// of linking without a downloaded model.
    #[test]
    fn runtime_version_lies() {
        let v = runtime_version();
        assert!(
            v.is_ok() && v.as_deref().unwrap_or("").starts_with("nemo-speech-asr "),
            "version SDK absente ou inattendue : {v:?}"
        );
    }
}
