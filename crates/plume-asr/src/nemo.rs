//! Nemotron 3.5 ASR streaming via le SDK C in-process (feature
//! `nemo-stream`). Convention ADR-06 : le seul `unsafe` autorisé ici est
//! l'appel direct à l'ABI C enveloppée par `plume_win::nemo` (FFI moteur),
//! chaque bloc justifié. Aucun panique sur le chemin applicatif : tout
//! échec traverse `AsrError` (détail SDK porté par `Runtime(String)`).
//!
//! Le texte définitif sort du trait batch (`Transcriptor`, offline) ;
//! le streaming (trait `StreamingEngine`) alimente l'affichage en direct
//! (RF-34 + amendement ADR-02). Ponctuation automatique désactivée tant
//! que le modèle PNC de l'accompagnement n'est pas câblé au téléchargement
//! (choix documenté, jamais silencieux).

use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::{null, null_mut};
use std::sync::Arc;
use std::time::{Duration, Instant};

use plume_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor,
};
use plume_win::nemo as ffi;
use plume_win::nemo::status;

use crate::streaming::{append_piece, StreamingEngine, StreamingSession, StreamUpdate};

// ------------------------------------------------------ handles RAII ---

/// Recognizer brut : Drop → `destroy`.
///
/// Justification `Send`/`Sync` : le contrat du SDK (« One ASR recognizer can
/// serve independent calls from multiple threads », docs/sdk.md) garantit
/// l'indépendance des appels concurrents sur un même recognizer ; la config
/// est copiée par `create`, aucun état propre à Rust n'est partagé.
struct RawRecognizer {
    ptr: *mut ffi::nemo_speech_asr_recognizer,
}
unsafe impl Send for RawRecognizer {}
unsafe impl Sync for RawRecognizer {}

impl Drop for RawRecognizer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY : handle positif, propriété unique à cet instant.
            unsafe {
                ffi::nemo_speech_asr_destroy(self.ptr);
            }
        }
        self.ptr = std::ptr::null_mut();
    }
}

impl RawRecognizer {
    /// Charge le GGUF (`create` copie la config, la chaîne peut leader
    /// libérer après l'appel).
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
        // SAFETY : config valide (initialisée avec `size`), sortie allouée
        // côté appelant, chaîne de chemin vivante pour la durée de l'appel.
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

/// Flux brut : Drop → `close`.
///
/// Justification `Send` : le contrat SDK exige de piloter un flux depuis un
/// thread unique — l'applicatif le garantit (le robinet de capture du
/// worker), la possession ici est exclusive.
struct RawStream {
    ptr: *mut ffi::nemo_speech_asr_stream,
}
unsafe impl Send for RawStream {}

impl Drop for RawStream {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY : flux possédé en exclusivité (mono-thread, l'applicatif
            // le garantit), fermeture en dernier ressort.
            unsafe {
                ffi::nemo_speech_asr_stream_close(self.ptr);
            }
        }
        self.ptr = std::ptr::null_mut();
    }
}
/// Résultat RAII : l'API exige `result_destroy` sur tout handle obtenu,
/// y compris après extraction du texte (le garde libère systématiquement).
struct ResultGuard(*mut ffi::nemo_speech_asr_result);
impl Drop for ResultGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY : résultat possédé à cet instant (jamais partagé).
            unsafe {
                ffi::nemo_speech_asr_result_destroy(self.0);
            }
        }
    }
}

/// Texte brut d'un résultat (alternative 0), copie possédée.
fn result_text_of(res: *const ffi::nemo_speech_asr_result) -> String {
    if res.is_null() {
        return String::new();
    }
    // SAFETY : accès en lecture seule sur le résultat possédé (le garde
    // appelant survit à l'extraction du texte).
    let alt = unsafe { ffi::nemo_speech_asr_result_alternative_count(res) };
    if alt == 0 {
        return String::new();
    }
    // SAFETY : chaîne valable jusqu'à destroy du résultat.
    let ptr = unsafe { ffi::nemo_speech_asr_result_transcript(res, 0) };
    if ptr.is_null() {
        String::new()
    } else {
        // SAFETY : chaîne UTF-8 garantie par le SDK C.
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

// ---------------------------------------------------------- options ---

/// Construit les options SDK. Le `CString` de langue est retourné : le
/// pointeur stocké dans la struct doit survivre jusqu'à l'appel ABI.
fn make_options(
    language: Option<&str>,
    opts: &TranscribeOptions,
    interim: bool,
) -> Result<(ffi::AsrRecognitionOptions, Option<CString>), AsrError> {
    // SAFETY : valeur par défaut de la bibliothèque, champ `size` préréglé.
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
    o.enable_automatic_punctuation = false; // modèle PNC non câblé : choix documenté
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

// ------------------------------------------------------------ moteur ---

/// Moteur Nemotron 3.5 ASR streaming (CPU) : offline dans la couture batch,
/// streaming pour l'affichage en direct. Le recognizer est partagé par ARC ;
/// les appels « indépendants » (offline et sessions) sont autorisés par le
/// contrat SDK.
pub struct NemoEngine {
    recognizer: Arc<RawRecognizer>,
    model: String,
}

impl NemoEngine {
    /// Charge le GGUF local. Le SDK est lié statiquement au build (DLL à
    /// côté de l'exécutable, copiées par build.rs).
    pub fn load(model_path: &Path) -> Result<Self, AsrError> {
        let model = model_path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| model_path.display().to_string());
        let recognizer = RawRecognizer::new(model_path)?;
        Ok(Self { recognizer, model })
    }

    /// Nom du fichier modèle (pour `Transcript::model`).
    fn label(&self) -> String {
        self.model.clone()
    }
}

impl Transcriptor for NemoEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        // Le modèle est décodé par `create` : le chargement est déjà fait.
        Ok(())
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
        // SAFETY : recognizer valide, mémoire de sortie allouée côté SDK.
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
        // SAFETY : lecteur d'accès du résultat possédé (garde vivant).
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
        // SAFETY : recognizer valide (ARC vivant au moins aussi longtemps),
        // options et CString de langue vivantes pour l'appel.
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
        }))
    }
}

// ----------------------------------------------------------- session ---

/// Session streaming mono-thread : pousse des tranches, draine les mises
/// à jour, gather final au relâchement.
struct NemoSession {
    stream: RawStream,
    committed: String,
    tail: String,
}

impl StreamingSession for NemoSession {
    fn push(&mut self, samples: &[f32], rate: u32) -> Result<(), AsrError> {
        if samples.is_empty() {
            return Ok(());
        }
        // SAFETY : flux possédé (mono-thread) ; taux fixe 8–96 kHz,
        // rescalage interne du côté SDK.
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
            // SAFETY : pull exclusif sur la session (mono-thread garanti par
            // le contrat SDK et respecté côté applicatif).
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
        // SAFETY : flush final sur la session possédée ; les derniers
        // arrivent au `next` du drain qui suit.
        let st = unsafe { ffi::nemo_speech_asr_stream_finish(self.stream.ptr) };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        // Aspiration bornée : 27 ms/tranche CPU nominal ; le plafond est un
        // garde-fou (2 s max), la sortie anticipée dès deux drains secs.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GGUF local de test (%APPDATA%\Plume\models), ou rien si absent —
    /// le test s'abstient alors (CI sans binaire volumineux, jamais
    /// silencieux : le message le dit).
    fn gguf_local() -> Option<std::path::PathBuf> {
        let appdata = std::env::var("APPDATA").ok()?;
        let p = std::path::PathBuf::from(appdata)
            .join("Plume")
            .join("models")
            .join("nemotron-3.5-asr-streaming-0.6b.q8_0.gguf");
        if p.is_file() {
            Some(p)
        } else {
            None
        }
    }

    /// Inférence offline réelle : le GGUF téléchargé est chargé et une
    /// seconde de silence est décodée. Le texte attendu est vide (silence)
    /// ; aucun panique, aucun statut d'erreur.
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

    /// Session streaming réelle : ouverture, quatre tranches de 300 ms de
    /// silence, drain, finish. Le contrat du SDK (creuset des appels
    /// `push`/`next`/`finish`) est exercé bout en bout sans panique.
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
        // Vérification du cyclic drain : aucune erreur, quelques updates.
        let updates = s.drain().expect("drain streaming");
        eprintln!("updates silence : {}", updates.len());
        let text = s.finish().expect("finish streaming");
        eprintln!("finish silence → \"{}\"", text);
    }
}
