//! Nemotron 3.5 ASR streaming via le SDK C in-process (feature
//! `nemo-stream`). Convention ADR-06 : le seul `unsafe` autorisé ici est
//! l'appel direct à l'ABI C enveloppée par `voxli_win::nemo` (FFI moteur),
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

use voxli_core::{
    AsrError, AudioBuffer, EngineId, TranscribeOptions, Transcript, Transcriptor, SAMPLE_RATE_HZ,
};
use voxli_win::nemo as ffi;
use voxli_win::nemo::status;

use crate::streaming::{append_piece, StreamUpdate, StreamingEngine, StreamingSession};

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
        // Warm-up complet (RF-19) : le modèle est décodé par `new`, mais le
        // premier appel ABI alloue lui aussi ses tampons ; on passe 150 ms de
        // silence par le chemin réel de `transcribe` — résultat jeté
        // (silence ⇒ transcription vide, RF-18). Un échec est remonté tel
        // quel à l'appelant (contrat `Result` du trait) : le moteur n'est
        // jamais considéré prêt après une warm-up avortée.
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
            conf_sum: 0.0,
            conf_n: 0,
        }))
    }
}

// ----------------------------------------------------------- session ---

/// Session streaming mono-thread : pousse des tranches, draine les mises
/// à jour, gather final au relâchement. La confiance de chaque segment
/// final est agrégée (moyenne) pour la voie rapide de transcription.
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
                // SAFETY : lecteur du résultat possédé (garde vivant) ;
                // confiance du segment final, agrégée pour take_result.
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
        // SAFETY : flush final sur la session possédée ; les derniers
        // arrivent au `next` du drain qui suit.
        let st = unsafe { ffi::nemo_speech_asr_stream_finish(self.stream.ptr) };
        if st != status::OK {
            return Err(AsrError::Runtime(ffi::describe(st)));
        }
        // Aspiration bornée : plafond dur de 1 000 tours (garde-fou), sortie
        // anticipée dès 151 drains secs consécutifs (~300 ms sans nouvelles
        // tranches) — tout drain non vide remet le compteur à zéro.
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
            // Rien de finalisé : l'appelant retombe sur sa voie batch
            // (silence, interruption très courte, échec en cours).
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

    /// GGUF local de test (%APPDATA%\Voxli\models), ou rien si absent —
    /// le test s'abstient alors (CI sans binaire volumineux, jamais
    /// silencieux : le message le dit).
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

    /// Voix de test SAPI (%APPDATA%\Voxli\test\voix.wav, générée par
    /// scripts/gen-voice.ps1) ou rien si absente — abstention gracieuse.
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

    /// Lecteur WAV PCM 16 bits minimal : balaye les chunks RIFF (fmt, data).
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
        // Le silence ne finalise rien : la voie rapide doit s'abstenir
        // (repli batch), jamais fabriquer de texte.
        assert!(s.take_result().is_none(), "silence : aucun final attendu");
    }

    /// Décodage offline d'une vraie phrase (voix SAPI) : la chaîne complète
    /// sans microphone. Impression du texte pour le jury humain (ce n'est
    /// PAS le journal de l'app — RF-34 y règle, ici l'outil regarde).
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

    /// Débit streaming réel : la phrase est poussée tranche par tranche
    /// (50 ms, à la manière du worker) et drain entre chaque — ce test
    /// mesure QUAND la session produit, pendant la prise. Impression du
    /// texte partiel pour le jury humain.
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
        // La langue de l'application (config `general.language = "fr"`) est
        // répliquée ici : la session ouverte avec contrainte doit produire
        // au même rythme.
        let mut s = engine.open(Some("fr"), false).expect("ouverture du flux");
        let chunk = (rate as usize) / 20; // 50 ms
        for (tick, piece) in samples.chunks(chunk).enumerate() {
            s.push(piece, rate).expect("push streaming");
            let updates = s.drain().expect("drain streaming");
            if !updates.is_empty() {
                // Aucun contenu dans le journal de l'app, mais un essai
                // rustc → stderr est un instrument pour l'humain : on montre
                // l'instant d'apparition et le texte partiel.
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
