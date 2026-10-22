//! Nemotron 3.5 ASR streaming via the in-process C SDK (feature
//! `nemo-stream`). ADR-06 convention: the only `unsafe` allowed here is
//! the direct call to the C ABI wrapped by `voxli_win::nemo` (engine
//! FFI), every block justified. No panic on the application path: any
//! failure goes through `AsrError` (SDK detail carried by
//! `Runtime(String)`).
//!
//! The final text comes out of the batch trait (`Transcriptor`, offline);
//! the streaming trait (`StreamingEngine`) feeds the live display
//! (RF-34 + ADR-02 amendment). Automatic punctuation disabled as long
//! as the companion PNC model is not wired into the download
//! (documented choice, never silent).

use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};
use voxli_win::nemo as ffi;
use voxli_win::nemo::status;

use crate::streaming::{append_piece, StreamUpdate, StreamingEngine, StreamingSession};

// ------------------------------------------------------ handles RAII ---

/// Raw recognizer: Drop → `destroy`.
///
/// `Send`/`Sync` justification: the SDK contract ("One ASR recognizer can
/// serve independent calls from multiple threads", docs/sdk.md) guarantees
/// the independence of concurrent calls on a single recognizer; the config
/// is copied by `create`, no Rust-specific state is shared.
struct RawRecognizer {
    ptr: *mut ffi::nemo_speech_asr_recognizer,
}
unsafe impl Send for RawRecognizer {}
unsafe impl Sync for RawRecognizer {}

impl Drop for RawRecognizer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: positive handle, unique ownership at this instant.
            unsafe {
                ffi::nemo_speech_asr_destroy(self.ptr);
            }
        }
        self.ptr = std::ptr::null_mut();
    }
}

impl RawRecognizer {
    /// Loads the GGUF (`create` copies the config; the string can be
    /// dropped once the call returns).
    fn new(model_path: &Path) -> Result<Arc<Self>, AsrError> {
        if !model_path.is_file() {
            return Err(AsrError::ModelMissing(model_path.display().to_string()));
        }
        let cpath = CString::new(model_path.to_string_lossy().as_bytes().to_vec())
            .map_err(|_| AsrError::ModelMissing(model_path.display().to_string()))?;
        let backend = ffi::AsrBackendConfig {
            size: std::mem::size_of::<ffi::AsrBackendConfig>(),
            gpu: -1, // CPU
        };
        let model = ffi::AsrModelConfig {
            size: std::mem::size_of::<ffi::AsrModelConfig>(),
            path: cpath.as_ptr(),
            name: null(),
        };
        let cfg = ffi::AsrRecognizerConfig {
            size: std::mem::size_of::<ffi::AsrRecognizerConfig>(),
            backend: &backend,
            model: &model,
            streaming: null(),
            decoder: null(),
            vad: null(),
            endpointing: null(),
            postproc: null(),
            diar: null(),
            batching: null(),
        };
        let mut ptr: *mut ffi::nemo_speech_asr_recognizer = std::ptr::null_mut();
        // SAFETY: valid config (initialized with `size`), output allocated
        // on the caller side, path string alive for the duration of the call.
        let st = unsafe { ffi::nemo_speech_asr_create(&cfg, &mut ptr) };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        if ptr.is_null() {
            return Err(AsrError::Runtime(
                "nemo_speech_asr_create : handle NULL sans statut d'erreur".to_string(),
            ));
        }
        Ok(Arc::new(Self { ptr }))
    }
}

/// Raw stream: Drop → `close`.
///
/// `Send` justification: the SDK contract requires driving a stream from a
/// single thread — the application guarantees that (the worker's capture
/// faucet); ownership here is exclusive.
struct RawStream {
    ptr: *mut ffi::nemo_speech_asr_stream,
}
unsafe impl Send for RawStream {}

impl Drop for RawStream {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: stream owned exclusively (single-threaded, the
            // application guarantees it), close as a last resort.
            unsafe {
                ffi::nemo_speech_asr_stream_close(self.ptr);
            }
        }
        self.ptr = std::ptr::null_mut();
    }
}
/// RAII result: the API requires `result_destroy` on every obtained handle,
/// including after the text is extracted (the guard frees it systematically).
struct ResultGuard(*mut ffi::nemo_speech_asr_result);
impl Drop for ResultGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: result owned at this instant (never shared).
            unsafe {
                ffi::nemo_speech_asr_result_destroy(self.0);
            }
        }
    }
}

/// Raw text of a result (alternative 0), owned copy.
fn result_text_of(res: *const ffi::nemo_speech_asr_result) -> String {
    if res.is_null() {
        return String::new();
    }
    // SAFETY: read-only access on the owned result (the caller's guard
    // outlives the text extraction).
    let alt = unsafe { ffi::nemo_speech_asr_result_alternative_count(res) };
    if alt == 0 {
        return String::new();
    }
    // SAFETY: string valid until the result is destroyed.
    let ptr = unsafe { ffi::nemo_speech_asr_result_transcript(res, 0) };
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY: UTF-8 string guaranteed by the C SDK.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

// ---------------------------------------------------------- options ---

/// Builds the SDK options. The language `CString` is returned: the
/// pointer stored in the struct must stay alive until the ABI call.
fn make_options(
    language: Option<&str>,
    opts: &TranscribeOptions,
    interim: bool,
) -> Result<(ffi::AsrRecognitionOptions, Option<CString>), AsrError> {
    // SAFETY: library default value, `size` field preset.
    let mut o: ffi::AsrRecognitionOptions =
        unsafe { ffi::nemo_speech_asr_recognition_options_default() };
    let mut c_lang: Option<CString> = None;
    if let Some(l) = language {
        c_lang = Some(
            CString::new(l.as_bytes().to_vec())
                .map_err(|_| AsrError::Runtime(format!("langue non encodable : {l}")))?,
        );
        o.language_code = c_lang.as_ref().map(|s| s.as_ptr()).unwrap_or(null());
    } else {
        o.language_code = null();
    }
    o.request_id = null();
    o.interim_results = interim;
    o.enable_word_time_offsets = false;
    o.enable_automatic_punctuation = false; // PNC model not wired: documented choice
    o.verbatim_transcripts = opts.verbatim;
    o.profanity_filter = false;
    o.stop_history_eou_ms = 0;
    o.speech_contexts = null();
    o.speech_context_count = 0;
    o.max_alternatives = 1;
    o.enable_speaker_diarization = false;
    o.max_speaker_count = 0;
    Ok((o, c_lang))
}

// ------------------------------------------------------------ engine ---

/// Nemotron 3.5 ASR streaming engine (CPU): offline in the batch seam,
/// streaming for the live display. The recognizer is shared by ARC;
/// the "independent" calls (offline and sessions) are allowed by the
/// SDK contract.
pub struct NemoEngine {
    recognizer: Arc<RawRecognizer>,
    model: String,
}

impl NemoEngine {
    /// Loads the local GGUF. The SDK is statically linked into the build
    /// (DLLs next to the executable, copied by build.rs).
    pub fn load(model_path: &Path) -> Result<Self, AsrError> {
        let model = model_path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| model_path.display().to_string());
        let recognizer = RawRecognizer::new(model_path)?;
        Ok(Self { recognizer, model })
    }

    /// Model file name (for `Transcript::model`).
    fn label(&self) -> String {
        self.model.clone()
    }
}

impl Transcriptor for NemoEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        // Full warm-up (RF-19): the model is decoded by `new`, but the
        // first ABI call allocates its buffers too; we push 150 ms of
        // silence through the real `transcribe` path — the result is
        // discarded (silence ⇒ empty transcription, RF-18). A failure
        // is returned as is to the caller (`Result` trait contract):
        // the engine is never considered ready after an aborted
        // warm-up.
        let silence = AudioBuffer::new(
            vec![0.0; SAMPLE_RATE_HZ as usize * 15 / 100],
            SAMPLE_RATE_HZ,
        );
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
        let started = Instant::now();
        if audio.samples.is_empty() {
            return Ok(Transcript {
                text: String::new(),
                confidence: 0.0,
                duration: started.elapsed(),
                engine: EngineId::Nemo,
                model: Some(self.model.clone()),
            });
        }
        let (o, _keep) = make_options(opts.language.as_deref(), opts, false)?;
        let mut res: *mut ffi::nemo_speech_asr_result = null_mut();
        // SAFETY: valid recognizer, output memory allocated on the SDK side.
        let st = unsafe {
            ffi::nemo_speech_asr_recognize_f32(
                self.recognizer.ptr,
                &o,
                audio.samples.as_ptr(),
                audio.samples.len(),
                audio.sample_rate as i32,
                &mut res,
            )
        };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        if res.is_null() {
            return Err(AsrError::Inference);
        }
        let guard = ResultGuard(res);
        let text = result_text_of(guard.0);
        let confidence = unsafe { ffi::nemo_speech_asr_result_confidence(guard.0, 0) };
        // SAFETY: access reader of the owned result (guard alive).
        let confidence = confidence.clamp(0.0, 1.0);
        Ok(Transcript {
            text,
            confidence,
            duration: started.elapsed(),
            engine: EngineId::Nemo,
            model: Some(self.label()),
        })
    }

    fn id(&self) -> EngineId {
        EngineId::Nemo
    }
}

impl StreamingEngine for NemoEngine {
    fn open(
        &self,
        language: Option<&str>,
        verbatim: bool,
    ) -> Result<Box<dyn StreamingSession>, AsrError> {
        let opts = TranscribeOptions {
            language: language.map(|s| s.to_string()),
            verbatim,
            ..TranscribeOptions::default()
        };
        let (o, _keep) = make_options(language, &opts, true)?;
        let mut st_ptr: *mut ffi::nemo_speech_asr_stream = null_mut();
        // SAFETY: valid recognizer (ARC alive at least as long as this),
        // options and language CString alive for the call.
        let st = unsafe {
            ffi::nemo_speech_asr_streaming_recognize(self.recognizer.ptr, &o, &mut st_ptr)
        };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        if st_ptr.is_null() {
            return Err(AsrError::Runtime(
                "nemo_speech_asr_streaming_recognize : flux NULL sans statut".to_string(),
            ));
        }
        Ok(Box::new(NemoSession {
            stream: RawStream { ptr: st_ptr },
            committed: String::new(),
            tail: String::new(),
            conf_sum: 0.0,
            conf_n: 0,
        }))
    }
}

// ----------------------------------------------------------- session ---

/// Single-threaded streaming session: pushes slices, drains the updates,
/// final gather on release. Each final segment's confidence is
/// aggregated (average) for the fast transcription channel.
struct NemoSession {
    stream: RawStream,
    committed: String,
    tail: String,
    conf_sum: f32,
    conf_n: u32,
}

impl StreamingSession for NemoSession {
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError> {
        if samples.is_empty() {
            return Ok(());
        }
        // SAFETY: stream owned (single-threaded); rate 8–96 kHz fixed,
        // internal rescaling on the SDK side.
        let st = unsafe {
            ffi::nemo_speech_asr_stream_push_f32(
                self.stream.ptr,
                samples.as_ptr(),
                samples.len(),
                rate as i32,
            )
        };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        Ok(())
    }

    fn drain(&mut self) -> Result<Vec<StreamUpdate>, AsrError> {
        let mut updates = Vec::new();
        loop {
            let mut res: *mut ffi::nemo_speech_asr_result = null_mut();
            // SAFETY: exclusive pull on the session (single-threading
            // guaranteed by the SDK contract and honored on the
            // application side).
            let st = unsafe { ffi::nemo_speech_asr_stream_next(self.stream.ptr, &mut res) };
            if st != status::OK {
                return Err(AsrError::Runtime(ffi::describe(st)));
            }
            if res.is_null() {
                break;
            }
            let guard = ResultGuard(res);
            let text = result_text_of(guard.0);
            if text.trim().is_empty() {
                continue;
            }
            if unsafe { ffi::nemo_speech_asr_result_is_final(guard.0) } {
                // SAFETY: reader of the owned result (guard alive);
                // final segment confidence, aggregated for take_result.
                let conf = unsafe { ffi::nemo_speech_asr_result_confidence(guard.0, 0) };
                self.conf_sum += conf.clamp(0.0, 1.0);
                self.conf_n += 1;
                append_piece(&mut self.committed, &text);
                self.tail.clear();
                updates.push(StreamUpdate::Final { text });
            } else {
                self.tail = text;
                let shown = self.tail.clone();
                updates.push(StreamUpdate::Partial { text: shown });
            }
        }
        Ok(updates)
    }

    fn finish(&mut self) -> Result<String, AsrError> {
        // SAFETY: final flush on the owned session; the last ones arrive
        // at the `next` of the drain that follows.
        let st = unsafe { ffi::nemo_speech_asr_stream_finish(self.stream.ptr) };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        // Bounded pump-out: hard ceiling of 1,000 rounds (guard), early
        // exit as soon as 151 consecutive dry drains occur (~300 ms
        // without new slices) — every non-empty drain resets the
        // counter.
        let mut consecutive_empty: u32 = 0;
        for _ in 0..1_000 {
            let got = self.drain()?;
            if got.is_empty() {
                consecutive_empty += 1;
                if consecutive_empty > 150 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(2));
            } else {
                consecutive_empty = 0;
            }
        }
        Ok(self.committed.clone())
    }

    fn snapshot(&self) -> String {
        let mut s = self.committed.clone();
        if !self.tail.is_empty() {
            append_piece(&mut s, &self.tail);
        }
        s
    }

    fn take_result(&self) -> Option<(String, f32)> {
        if self.committed.is_empty() {
            // Nothing finalized: the caller falls back to its batch
            // channel (silence, very short interruption, in-flight
            // failure).
            return None;
        }
        let conf = if self.conf_n == 0 {
            0.5
        } else {
            (self.conf_sum / self.conf_n as f32).clamp(0.0, 1.0)
        };
        Some((self.committed.clone(), conf))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Local test GGUF (%APPDATA%\Voxli\models), or None when absent —
    /// the test then abstains (CI without a large binary, never
    /// silent: the message says so).
    fn gguf_local() -> Option<std::path::PathBuf> {
        let appdata = std::env::var("APPDATA").ok()?;
        let p_voxli = std::path::PathBuf::from(&appdata)
            .join("Voxli")
            .join("models")
            .join("nemotron-3.5-asr-streaming-0.6b.q8_0.gguf");
        if p_voxli.is_file() {
            Some(p_voxli)
        } else {
            None
        }
    }

    /// SAPI test voice (%APPDATA%\Voxli\test\voix.wav, generated by
    /// scripts/gen-voice.ps1) or None when absent — graceful abstention.
    fn voix_local() -> Option<(Vec<f32>, u32)> {
        let appdata = std::env::var("APPDATA").ok()?;
        let path = std::path::PathBuf::from(&appdata)
            .join("Voxli")
            .join("test")
            .join("voix.wav");
        if !path.is_file() {
            return None;
        }
        let (mut samples, rate, channels) = read_wav(&path)?;
        if channels > 1 {
            let mut mono = Vec::with_capacity(samples.len() / channels as usize);
            for frame in samples.chunks_exact(channels as usize) {
                mono.push(frame.iter().sum::<f32>() / channels as f32);
            }
            samples = mono;
        }
        Some((samples, rate))
    }

    /// Minimal PCM 16-bit WAV reader: scans the RIFF chunks (fmt, data).
    fn read_wav(path: &std::path::Path) -> Option<(Vec<f32>, u32, u16)> {
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return None;
        }
        let mut i = 12usize;
        let mut rate = 0u32;
        let mut channels = 0u16;
        let mut data: Option<&[u8]> = None;
        while i + 8 <= bytes.len() {
            let id = [bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]];
            let len = u32::from_le_bytes([bytes[i + 4], bytes[i + 5], bytes[i + 6], bytes[i + 7]])
                as usize;
            let start = (i + 8).min(bytes.len());
            let end = (start + len).min(bytes.len());
            match &id {
                b"fmt " => {
                    let body = &bytes[start..end];
                    if body.len() >= 14 {
                        channels = u16::from_le_bytes([body[2], body[3]]);
                        rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
                    }
                }
                b"data" => data = Some(&bytes[start..end]),
                _ => {}
            }
            i = start.checked_add(len)?.checked_add(len % 2)?;
        }
        let data = data?;
        let n = data.len() / 2;
        let samples: Vec<f32> = (0..n)
            .map(|k| i16::from_le_bytes([data[2 * k], data[2 * k + 1]]) as f32 / 32768.0)
            .collect();
        Some((samples, rate, channels))
    }

    /// Real offline inference: the downloaded GGUF is loaded and one
    /// second of silence is decoded. The expected text is empty
    /// (silence); no panic, no error status.
    #[test]
    fn inference_summary_silence() {
        let Some(path) = gguf_local() else {
            eprintln!("GGUF Nemotron absent : test abstenu (pas d'échec).");
            return;
        };
        let engine = NemoEngine::load(&path).expect("chargement du GGUF réel");
        let audio = AudioBuffer::new(vec![0.0_f32; 16_000], 16_000);
        let opts = TranscribeOptions::default();
        let t = engine
            .transcribe(&audio, &opts)
            .expect("transcription offline du silence");
        eprintln!(
            "silence → text=\"{}\" conf={:.2} en {:?}",
            t.text, t.confidence, t.duration
        );
        assert_eq!(t.engine, EngineId::Nemo);
    }

    /// Real streaming session: open, four 300 ms slices of silence,
    /// drain, finish. The SDK contract (the crucible of the `push`/
    /// `next`/`finish` calls) is exercised end to end without a panic.
    #[test]
    fn session_streaming_silence() {
        let Some(path) = gguf_local() else {
            eprintln!("GGUF Nemotron absent : test abstenu (pas d'échec).");
            return;
        };
        let engine = NemoEngine::load(&path).expect("chargement du GGUF réel");
        let mut s = engine.open(None, false).expect("ouverture du flux");
        let audio = vec![0.0_f32; 4_800];
        for _ in 0..4 {
            s.push(&audio, 16_000).expect("push streaming");
        }
        // Checking the cyclic drain: no error, a few updates.
        let updates = s.drain().expect("drain streaming");
        eprintln!("updates silence : {}", updates.len());
        let text = s.finish().expect("finish streaming");
        eprintln!("finish silence → \"{}\"", text);
        // Silence finalizes nothing: the fast channel must abstain
        // (batch fallback), never fabricate text.
        assert!(s.take_result().is_none(), "silence : aucun final attendu");
    }

    /// Offline decoding of a real sentence (SAPI voice): the complete
    /// chain without a microphone. The text is printed for the human
    /// jury (this is NOT the app's journal — RF-34 governs it; here
    /// the tool is the one looking).
    #[test]
    fn offline_phrase() {
        let Some((samples, rate)) = voix_local() else {
            eprintln!("voix.wav absente : exécuter scripts/gen-voice.ps1 — test abstenu.");
            return;
        };
        let Some(path) = gguf_local() else {
            eprintln!("GGUF absente : test abstenu.");
            return;
        };
        let engine = NemoEngine::load(&path).expect("chargement du GGUF réel");
        let audio = AudioBuffer::new(samples, rate);
        let opts = TranscribeOptions::default();
        let t = engine
            .transcribe(&audio, &opts)
            .expect("transcription offline de la voix de test");
        eprintln!(
            "OFFLINE ({:.1}s d'audio, {:.2}s de calcul) : \"{}\"",
            audio.samples.len() as f64 / rate as f64,
            t.duration.as_secs_f64(),
            t.text
        );
        assert_eq!(t.engine, EngineId::Nemo);
    }

    /// Real streaming throughput: the sentence is pushed slice by slice
    /// (50 ms, the worker's way) and drained in between — this test
    /// measures WHEN the session produces, during capture. The partial
    /// text is printed for the human jury.
    #[test]
    fn stream_debit_phrase() {
        let Some((samples, rate)) = voix_local() else {
            eprintln!("voix.wav absente : exécuter scripts/gen-voice.ps1 — test abstenu.");
            return;
        };
        let Some(path) = gguf_local() else {
            eprintln!("GGUF absente : test abstenu.");
            return;
        };
        let engine = NemoEngine::load(&path).expect("chargement du GGUF réel");
        // The application language (config `general.language = "fr"`) is
        // replicated here: the session opened with a constraint must
        // produce at the same pace.
        let mut s = engine.open(Some("fr"), false).expect("ouverture du flux");
        let chunk = (rate as usize) / 20; // 50 ms
        for (tick, piece) in samples.chunks(chunk).enumerate() {
            s.push(piece, rate).expect("push streaming");
            let updates = s.drain().expect("drain streaming");
            if !updates.is_empty() {
                // No content in the app's journal, but a rustc trial →
                // stderr is an instrument for the human: we show the
                // first-appearance instant and the partial text.
                let elapse = tick as f32 / 20.0;
                let shown = s.snapshot();
                eprintln!(
                    "tick {} ({:.2}s): {} updates → \"{}\"",
                    tick,
                    elapse,
                    updates.len(),
                    shown
                );
            }
        }
        let began = std::time::Instant::now();
        let fin = s.finish().expect("finish streaming");
        eprintln!(
            "FINISH {:.2}s → {} caractères (take_result: {})",
            began.elapsed().as_secs_f64(),
            fin.chars().count(),
            s.take_result()
                .as_ref()
                .map(|(text, _)| text.chars().count())
                .map(|c| c.to_string())
                .unwrap_or_else(|| "abstient".into())
        );
    }
}
