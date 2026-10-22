//! Application pipeline thread (ADR-12): it owns the orchestrator and
//! its dependencies, feeds them with events (`InputEvent`) through a channel,
//! and relays every `CoreEvent` to the logger and to the interface.
//!
//! Why a dedicated thread: the orchestrator is synchronous and pure (docs/05
//! §2.1); the UI event loop must never be blocked by inference or insertion.
//! The finer event-driven pipeline split (V1, RF-05) will reuse these
//! same components.
//!
//! Events emitted to the UI:
//! - `state://changed` `{ state, atMs }` — no content (table docs/05 §3);
//! - `meter://level` `{ level, atMs }` — audio meter, 1 sample out of 2 ticks
//!   (~100 ms): documented addition to the table (RF-09);
//! - `session://finished` `{ success, durationMs, level, atMs }`;
//! - `notice://notice` `{ kind, message }` — **documented addition** to the
//!   table: neutral FR messages defined in `strings.rs`, never any content;
//! - `dictation://partial` `{ text, atMs }` — **documented addition**
//!   (ADR-02 amendment): transient streaming hypotheses during a capture
//!   (Nemotron P2, or windowed whisper re-decoding — `whisper_partial`):
//!   pure display, never logged nor persisted (RF-34).

use serde::Serialize;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use voxli_asr::FakeEngine;
#[cfg(feature = "nemo-stream")]
use voxli_asr::NemoEngine;
#[cfg(feature = "parakeet-asr")]
use voxli_asr::ParakeetOnnxEngine;
#[cfg(feature = "real-asr")]
use voxli_asr::WhisperCppEngine;
#[cfg(feature = "real-asr")]
use voxli_asr::WhisperPartialStreamer;
use voxli_asr::{StreamingEngine, StreamingSession};
use voxli_audio::{CaptureTap, CpalCapture, EnergyVad, FakeCapture, VadConfig};
use voxli_config::{Config, EngineChoice, TriggerMode as TriggerModeCfg};
use voxli_core::{
    orchestrator::{asr_code, capture_code},
    AudioBuffer, Capture, CaptureError, CoreEvent, Deps, DictationState, InputEvent, Inserter,
    Level, Orchestrator, OrchestratorCfg, Profile, ProfileBinding, SteadyClock, Transcriptor,
    TriggerMode,
};
use voxli_diagnostics::{Event, Logger};
use voxli_storage::{HistoryStore, MemoryStore};
use voxli_text::PassThroughProcessor;
#[cfg(target_os = "windows")]
use voxli_text::{WindowsFocusReader, WindowsInserter};

use crate::commands::{lock_or_recover, UiShared};
use crate::strings;

// Doc-banner update: the pill and the audio meter are emitted from this
// thread after all (RF-09).

/// Loop polling period: doubles as the "tick" for the maximum safety
/// duration (RF-11) and, in V1, for the ~50 ms release polling
/// (docs/03 ADR-05).
const LOOP_TICK: Duration = Duration::from_millis(50);

/// Messages consumed by the pipeline thread.
pub enum WorkerMsg {
    /// State-machine events (hotkeys, commands, `Retry`).
    Input(InputEvent),
    /// "Last text" (Shift+Alt+X shortcut, RF-22): opens the rescue window
    /// IF a text is pending.
    ShowRescue,
    /// RF-01: the configuration has been validated and saved; the pipeline
    /// rebuilds its orchestrator (hotkeys re-registered on the main
    /// side) — without restarting the application.
    ConfigChanged(Box<Config>),
    /// RF-15: a model has just been installed on disk. The worker
    /// re-evaluates the fit of the loaded engine (`engine_stale`) without
    /// cancelling an ongoing dictation: reloading waits for the end of the
    /// current capture in that case (never any loss, RF-20).
    ModelInstalled,
    #[allow(dead_code)]
    Shutdown,
}

// -------------------------------------------------- non-Windows stubs ---

/// Outside Windows (CI Linux): no usable focused-field source — refusal.
#[cfg(not(target_os = "windows"))]
struct FocusStub;
#[cfg(not(target_os = "windows"))]
impl FocusReader for FocusStub {
    fn snapshot(&self) -> Option<FocusSnapshot> {
        None
    }
}

#[cfg(not(target_os = "windows"))]
struct InserterStub;
#[cfg(not(target_os = "windows"))]
impl Inserter for InserterStub {
    fn plan(&self, _target: &FocusSnapshot, _profile: &Profile) -> InsertPlan {
        InsertPlan::refused(RefuseReason::TargetUnavailable)
    }
    fn insert(&self, _text: &str, _plan: &InsertPlan) -> Result<InsertReport, InsertError> {
        Err(InsertError::TargetUnavailable)
    }
}

// ----------------------------------------------------------- payloads ---

#[derive(Clone, Serialize)]
struct StatePayload {
    state: String,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

#[derive(Clone, Serialize)]
struct SessionPayload {
    success: bool,
    #[serde(rename = "durationMs")]
    duration_ms: u64,
    level: Option<String>,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

#[derive(Clone, Serialize)]
struct NoticePayload {
    kind: &'static str,
    message: String,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ----------------------------------------------------------- assembly ---

/// Capture proxy (RF-08/RF-09/RF-10): the orchestrator holds
/// `&mut dyn Capture` exclusively, but the audio meter and the
/// device-lost detection are read **outside the orchestrator** at regular
/// intervals. The proxy goes through a brief lock (a level read ≈ a few
/// hundred bytes, no allocation).
struct SharedCapture(
    Arc<std::sync::Mutex<Box<dyn Capture + Send>>>,
    Arc<StreamSlot>,
);

impl SharedCapture {
    fn new(inner: Box<dyn Capture + Send>, stream: Arc<StreamSlot>) -> Self {
        Self(Arc::new(std::sync::Mutex::new(inner)), stream)
    }

    /// Second key into the same compartment: audio meter + device-lost sensor.
    fn inner_arc(&self) -> Arc<std::sync::Mutex<Box<dyn Capture + Send>>> {
        Arc::clone(&self.0)
    }
}

impl Capture for SharedCapture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError> {
        // Possibly stale session (silent capture: the VAD refused the
        // transcription): **bounded** closure — the FFI work lives in
        // the driver; past the bound, we move on (the batch path remains
        // the authority). Any pending text is dropped (silent capture =
        // nothing to insert).
        let _ = close_stream_bornee(&self.1, std::time::Duration::from_millis(600));
        let started = lock_or_recover(&self.0).start(pre_roll_ms);
        if started.is_ok() {
            // Display session opened through the driver, without waiting
            // (opening can be slow, even suspended — partial emission
            // will follow; the batch path never depends on it).
            // Slices already fallen into `feed` are pushed at the
            // driver's first Pump.
            let factory = lock_or_recover(&self.1.engine).clone();
            if let Some(eng) = factory {
                let language = lock_or_recover(&self.1.language).clone();
                let verbatim = self.1.verbatim.load(Ordering::Relaxed);
                let _ = self.1.tx.send(StreamCmd::Open {
                    engine: eng,
                    language,
                    verbatim,
                });
            }
        }
        started
    }

    fn stop(&mut self) -> Result<AudioBuffer, CaptureError> {
        let buffer = lock_or_recover(&self.0).stop();
        // The audio capture is closed: the transcriptor will finalize the
        // session (the batch text remains the authority); in case of a
        // silent capture (VAD refusal), the next start also closes via
        // close_stream_bornee.
        self.1.end.store(true, Ordering::Relaxed);
        buffer
    }

    fn level(&self) -> Level {
        lock_or_recover(&self.0).level()
    }

    fn device_lost(&self) -> bool {
        lock_or_recover(&self.0).device_lost()
    }
}

// ------------------------------------------------------ streaming (P2) ---

/// Partial display (documented addition, docs/05 §3): transient
/// streaming hypotheses during a capture — pure display, never
/// logged nor persisted (RF-34, ADR-02 amendment).
#[derive(Clone, Serialize)]
struct PartialPayload {
    text: String,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

/// Commands of the streaming session driver. The SDK contract ("a stream
/// is driven by ONE SINGLE thread") is honored **by construction**:
/// this thread is the only one touching the engine FFI. Incident of
/// 18/10/2026: a suspended SDK call froze the whole worker thread
/// — no more tick, no more release polling, no more duration guard (RF-11);
/// the capture stayed in "recording" and the application looked dead.
/// Since then: **the worker thread never waits on a native call** — the
/// driver absorbs the hits, the batch path (ADR-02) remains the authority.
enum StreamCmd {
    /// Opens a session (factory + language + verbatim **cloned** into the
    /// message: nothing references the calling thread).
    Open {
        engine: Arc<dyn StreamingEngine>,
        language: Option<String>,
        verbatim: bool,
    },
    /// Pushes the current tap remainder into the session then drains
    /// the updates (partial emission done here — this thread may
    /// suspend without damage).
    Pump,
    /// End of capture: residual, finish, bounded drain, closure;
    /// the result is written into the slot (consumed by the transcriptor
    /// under bound).
    Close,
}

/// Streaming wiring of a capture. The capture tap (real-time
/// thread) only touches the `feed` slice buffer (try_lock: a slice
/// may be sacrificed — the definitive text, batch path, never
/// depends on it). The rest (session, push, drain, closure) lives **entirely
/// in the driver thread** (`stream-driver`); the worker only talks to it
/// through non-blocking commands.
struct StreamSlot {
    /// Session factory (streaming-capable engine); copied at every
    /// engine rebuild; empty when the engine has none.
    engine: std::sync::Mutex<Option<Arc<dyn StreamingEngine>>>,
    /// Language + verbatim of the config in service (copied at rebuild).
    language: std::sync::Mutex<Option<String>>,
    verbatim: AtomicBool,
    /// Driver queue: all session access goes through it (send never
    /// blocking — unbounded channel, the driver consumes at its own pace).
    tx: std::sync::mpsc::Sender<StreamCmd>,
    /// Whether a session exists at the driver (diagnostic data without FFI).
    has_session: AtomicBool,
    /// Ack of the last closure (bound on the worker side).
    close_done: AtomicBool,
    /// Finalized result of the last capture, written by the driver.
    outcome: std::sync::Mutex<Option<StreamOutcome>>,
    /// Set by `SharedCapture::stop`: the capture is over on the capture side.
    end: AtomicBool,
    /// Mono slices of the tap, preallocated (clear without deallocation).
    feed: std::sync::Mutex<Vec<f32>>,
    /// Tap rate (fixed at the device's first slice).
    rate: AtomicU32,
    /// Numeric diagnostics (numbers only, never content — RF-34):
    /// samples swallowed from the tap and updates produced by the
    /// session, consumed by the diagnostic tick (~1 Hz during a capture).
    fed: std::sync::atomic::AtomicU64,
    updates_seen: std::sync::atomic::AtomicU64,
    /// Logger (documented failures, never silent).
    logger: Arc<Logger>,
}

impl StreamSlot {
    fn new(logger: Arc<Logger>, tx: std::sync::mpsc::Sender<StreamCmd>) -> Self {
        Self {
            engine: std::sync::Mutex::new(None),
            language: std::sync::Mutex::new(None),
            verbatim: AtomicBool::new(false),
            tx,
            has_session: AtomicBool::new(false),
            close_done: AtomicBool::new(true),
            outcome: std::sync::Mutex::new(None),
            end: AtomicBool::new(false),
            feed: std::sync::Mutex::new(Vec::with_capacity(24_000)),
            rate: AtomicU32::new(0),
            fed: std::sync::atomic::AtomicU64::new(0),
            updates_seen: std::sync::atomic::AtomicU64::new(0),
            logger,
        }
    }

    /// Slot remount: engine factory + language + verbatim copied at every
    /// engine rebuild.
    fn update_mount(
        &self,
        engine: Option<Arc<dyn StreamingEngine>>,
        language: Option<String>,
        verbatim: bool,
    ) {
        *lock_or_recover(&self.engine) = engine;
        *lock_or_recover(&self.language) = language;
        self.verbatim.store(verbatim, Ordering::Relaxed);
    }
}

/// Result of the display path: the text the streaming session already
/// finalized during the capture, with its average confidence. Usable
/// as a transcription — the audio was already decoded, no re-decoding.
struct StreamOutcome {
    text: String,
    confidence: f32,
}

/// Session closure **via the driver**, with a time bound on the worker
/// side: the command enters the queue, then the worker waits for the
/// ack for at most `budget`. Beyond it (jammed SDK), `None`
/// is returned: the batch path resumes its rank of authority (ADR-02) and
/// the capture continues — this is the anti-freeze guarantee (incident
/// 18/10/2026).
fn close_stream_bornee(slot: &StreamSlot, budget: std::time::Duration) -> Option<StreamOutcome> {
    slot.close_done.store(false, Ordering::Relaxed);
    if slot.tx.send(StreamCmd::Close).is_err() {
        return None; // driver dead (queue closed): nothing to wait for
    }
    let deadline = std::time::Instant::now() + budget;
    loop {
        if slot.close_done.load(Ordering::Relaxed) {
            return lock_or_recover(&slot.outcome).take();
        }
        if std::time::Instant::now() >= deadline {
            slot.logger.log(Event::DeviceEvent {
                kind: "stream-driver-close-temps-depasse".into(),
            });
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(6));
    }
}

/// Driver thread body: the **only** FFI belt of the streaming path.
/// Suspended by a jammed SDK, this thread wastes its time; the worker,
/// freed from any native call, keeps going (tick, RF-11, release polling)
/// and the capture ends through the batch path. The session dies here
/// (Drop → FFI close) — if the Drop hangs, the following commands
/// wait: streaming display stops, insertion does not.
fn stream_driver_body(
    slot: Arc<StreamSlot>,
    app: AppHandle<Wry>,
    rx: std::sync::mpsc::Receiver<StreamCmd>,
) {
    let mut session: Option<Box<dyn StreamingSession>> = None;
    // Partial cutoff logged once per session (otherwise the auto-off
    // would stay mute: UX finding of 19/10, never partial text on the
    // pill without an explanation).
    let mut coupe_journalisee = false;
    for cmd in rx.iter() {
        match cmd {
            StreamCmd::Open {
                engine,
                language,
                verbatim,
            } => {
                if session.is_none() {
                    match engine.open(language.as_deref(), verbatim) {
                        Ok(s) => {
                            session = Some(s);
                            slot.has_session.store(true, Ordering::Relaxed);
                            coupe_journalisee = false;
                        }
                        Err(err) => {
                            slot.logger.log(Event::Error {
                                module: "asr".into(),
                                code: asr_code(&err),
                            });
                        }
                    }
                } else {
                    // Session already present (a previous closure still in
                    // flight): the open is ignored and logged (RF-34).
                    slot.logger.log(Event::DeviceEvent {
                        kind: "stream-driver-open-ignore".into(),
                    });
                }
            }
            StreamCmd::Pump => {
                if slot.end.load(Ordering::Relaxed) || session.is_none() {
                    continue; // residual kept for the upcoming close/open
                }
                let rate = slot.rate.load(Ordering::Relaxed);
                let chunk = {
                    let mut feed = lock_or_recover(&slot.feed);
                    std::mem::take(&mut *feed)
                };
                let mut mort = false;
                if rate > 0 && !chunk.is_empty() {
                    let s = session.as_mut().expect("session vérifiée ci-dessus");
                    match s.push(&chunk, rate) {
                        Ok(()) => {
                            slot.fed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        }
                        Err(err) => {
                            slot.logger.log(Event::Error {
                                module: "asr".into(),
                                code: asr_code(&err),
                            });
                            mort = true;
                        }
                    }
                }
                if mort {
                    session = None; // Drop → FFI close (this thread only)
                    slot.has_session.store(false, Ordering::Relaxed);
                } else {
                    let s = session.as_mut().expect("session vérifiée ci-dessus");
                    match s.drain() {
                        Ok(updates) => {
                            if !updates.is_empty() {
                                slot.updates_seen
                                    .fetch_add(updates.len() as u64, Ordering::Relaxed);
                                // Partials: pure display, never
                                // logged nor persisted (RF-34).
                                let _ = app.emit(
                                    "dictation://partial",
                                    PartialPayload {
                                        text: s.snapshot(),
                                        at_ms: now_ms(),
                                    },
                                );
                            }
                        }
                        Err(err) => {
                            slot.logger.log(Event::Error {
                                module: "asr".into(),
                                code: asr_code(&err),
                            });
                            mort = true;
                        }
                    }
                    // Auto-off of partials (slow machine): logged once
                    // "cleanly" — the cap stays adjustable via
                    // VOXLI_PARTIAL_MAX_DECODE_MS (cf. whisper_partial.rs).
                    if !mort
                        && !coupe_journalisee
                        && session.as_ref().map(|s| s.is_disabled()) == Some(true)
                    {
                        coupe_journalisee = true;
                        slot.logger.log(Event::DeviceEvent {
                            kind: "partiels-coupés:re-décodage-trop-lent-sur-cette-machine".into(),
                        });
                    }
                    if mort {
                        session = None;
                        slot.has_session.store(false, Ordering::Relaxed);
                    }
                }
            }
            StreamCmd::Close => {
                // 1) residual of the tap — the session receives everything left.
                let rate = slot.rate.load(Ordering::Relaxed);
                let chunk = {
                    let mut feed = lock_or_recover(&slot.feed);
                    std::mem::take(&mut *feed)
                };
                let mut mort = false;
                if rate > 0 && !chunk.is_empty() {
                    if let Some(s) = session.as_mut() {
                        match s.push(&chunk, rate) {
                            Ok(()) => {
                                slot.fed.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                            }
                            Err(err) => {
                                slot.logger.log(Event::Error {
                                    module: "asr".into(),
                                    code: asr_code(&err),
                                });
                                mort = true;
                            }
                        }
                    }
                }
                if mort {
                    session = None;
                    slot.has_session.store(false, Ordering::Relaxed);
                }
                // 2) finish + bounded drain (inside the facade).
                let mut outcome = None;
                if let Some(s) = session.as_mut() {
                    outcome = match s.finish() {
                        Ok(_) => s
                            .take_result()
                            .map(|(text, confidence)| StreamOutcome { text, confidence }),
                        Err(err) => {
                            slot.logger.log(Event::Error {
                                module: "asr".into(),
                                code: asr_code(&err),
                            });
                            None
                        }
                    };
                }
                *lock_or_recover(&slot.outcome) = outcome;
                if !mort {
                    // Drop → FFI close (this thread only). If suspended
                    // here, the closure is never acknowledged:
                    // the worker has already crossed its bound and the
                    // batch path has resumed its rank of authority.
                    session = None;
                }
                slot.has_session.store(false, Ordering::Relaxed);
                slot.end.store(false, Ordering::Relaxed);
                // Ack last: the worker's bound opens.
                slot.close_done.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Batch route: at transcription time, the streaming session is finalized
/// BEFORE the offline decoding. If it produced a finalized text during
/// the capture, that result (already decoded incrementally) becomes the
/// transcription — otherwise the authority remains the batch path over
/// the whole buffer (pre-roll included). Both routes are logged (without
/// content, RF-34) for an honest latency measurement.
struct StreamTranscriptor {
    inner: Arc<dyn Transcriptor>,
    slot: Arc<StreamSlot>,
}

impl Transcriptor for StreamTranscriptor {
    fn warmup(&self) -> Result<(), voxli_core::AsrError> {
        self.inner.warmup()
    }

    fn transcribe(
        &self,
        audio: &AudioBuffer,
        opts: &voxli_core::TranscribeOptions,
    ) -> Result<voxli_core::Transcript, voxli_core::AsrError> {
        let began = std::time::Instant::now();
        // Closure via the driver, **bounded** (2.5 s): if the SDK is
        // jammed, the bound opens and the batch path resumes its rank
        // of authority (ADR-02). Worker-side: no indefinite wait.
        let outcome = close_stream_bornee(&self.slot, std::time::Duration::from_millis(2500));
        if let Some(outcome) = outcome {
            // Fast route: the audio was already decoded during the capture;
            // the gain is major (otherwise ~0.95× the spoken duration spent
            // waiting at the end of the capture). Accepted limit: the
            // pre-roll (~0.4 s of audio preceding the button) never went
            // through the stream — the VAD and the capture punctuation
            // cover the rest; critical work can force a single-path
            // engine (whisper).
            self.slot.logger.log(Event::DeviceEvent {
                kind: "transcription-via-stream".into(),
            });
            return Ok(voxli_core::Transcript {
                text: outcome.text,
                confidence: outcome.confidence,
                duration: began.elapsed(),
                engine: self.inner.id(),
                // The session's model identity is not carried by the
                // slot (missing information ≠ false information).
                model: None,
            });
        }
        self.slot.logger.log(Event::DeviceEvent {
            kind: "transcription-via-batch".into(),
        });
        self.inner.transcribe(audio, opts)
    }

    fn id(&self) -> voxli_core::EngineId {
        self.inner.id()
    }
}

/// Thread entry point: dependency assembly then loop.
pub fn run(app: AppHandle<Wry>, logger: Arc<Logger>, rx: Receiver<WorkerMsg>) {
    let shared = app.state::<UiShared>();
    let mut cfg = lock_or_recover(&shared.config).clone();
    // Rights (RF-39): multiple profiles ⇢ "extra-profiles". License
    // verified at startup (main.rs); re-verified at every ConfigChanged.
    let pro_profiles = {
        let lic = lock_or_recover(&shared.license);
        lic.features.iter().any(|f| f == "extra-profiles")
    };
    let orch_cfg = to_orchestrator_cfg(&cfg, pro_profiles);
    let excluded = cfg.insertion.excluded_apps.clone();

    // --- Capture (ADR-04, RF-08): configured device → default →
    // explicit fake fallback in the log (never silent). The capture is
    // wrapped in a proxy: the orchestrator keeps `&mut dyn Capture`,
    // while the sound level (RF-09) and the device-lost detection
    // (RF-10) are read outside the orchestrator through a shared `Arc`.
    // --- Streaming (docs/05, documented addition): the slot is created
    // BEFORE the capture — the tap hooks its slices into it from
    // construction; the definitive text remains the batch path (no
    // semantics changed), partials are transient display.
    // The **driver** (`stream-driver`) is the only thread touching the
    // session FFI: a jammed SDK stalls THAT thread, never the worker
    // (incident of 18/10/2026: a suspended call froze tick, RF-11 and
    // release polling — the capture stayed "recording" endlessly).
    let (stream_tx, stream_rx) = std::sync::mpsc::channel::<StreamCmd>();
    let stream = Arc::new(StreamSlot::new(Arc::clone(&logger), stream_tx));
    if let Err(spawn_err) = std::thread::Builder::new()
        .name("stream-driver".into())
        .spawn({
            let slot = Arc::clone(&stream);
            let app = app.clone();
            move || stream_driver_body(slot, app, stream_rx)
        })
    {
        // No silent fallback (docs/03): the failure is logged;
        // the batch path remains the authority, display streaming is
        // simply absent (sess=0 in the diagnostics).
        logger.log(Event::DeviceEvent {
            kind: format!("stream-driver-spawn-echec:{}", spawn_err),
        });
    }
    let capture_tap: Option<CaptureTap> = Some({
        let slot = Arc::clone(&stream);
        Arc::new(move |mono: &[f32], rate: u32| {
            if rate == 0 || mono.is_empty() {
                return;
            }
            slot.rate.store(rate, Ordering::Relaxed);
            if let Ok(mut feed) = slot.feed.try_lock() {
                feed.extend_from_slice(mono);
            }
            // try_lock busy (drain in progress): the display slice is
            // sacrificed — the definitive text (batch) never depends on it.
        })
    });
    let init: Box<dyn Capture + Send> = {
        #[cfg(target_os = "windows")]
        {
            build_capture(&cfg.audio.device, &logger, capture_tap.clone())
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = &capture_tap;
            Box::new(FakeCapture::with_duration(Duration::from_millis(1600)))
        }
    };
    let mut capture = SharedCapture::new(init, Arc::clone(&stream));
    let capture_meter = capture.inner_arc();
    let mut last_device: String = cfg.audio.device.clone();
    #[cfg(not(target_os = "windows"))]
    let _ = &mut last_device;

    // --- VAD (RF-18): bounds calibrated on the chosen aggressiveness.
    let vad = EnergyVad::new(vad_config(&cfg));

    // --- ASR engine (ADR-03): real if compiled + model present; otherwise
    // an explicit fake engine in the log (never a silent fallback).
    let (mut engine, mounted_model) = build_engine(&cfg, &logger);
    if let Err(err) = engine.batch.warmup() {
        logger.log(Event::Error {
            module: "asr".into(),
            code: asr_code(&err),
        });
    }
    let engine_label = Arc::new(std::sync::Mutex::new(
        engine.batch.id().as_str().to_string(),
    ));
    // Identity of the mounted model file: complement of the label — two
    // successive "whisper.cpp" mounts can carry different weights
    // (base → small), the comparison only makes sense with
    // BOTH (shell + content), otherwise activating another installed
    // voice stayed mute (inconsistency found on 21/10).
    let engine_model = Arc::new(std::sync::Mutex::new(mounted_model));
    // Engine metadata for the UI (state_get).
    *lock_or_recover(&app.state::<UiShared>().engine) = engine.batch.id().as_str().to_string();
    // Streaming slot: factory (if the engine is capable) + language/verbatim.
    stream.update_mount(
        engine.streaming.clone(),
        Some(cfg.general.language.clone()).filter(|s| !s.is_empty()),
        cfg.profiles.default.style == voxli_config::Style::Verbatim,
    );

    // --- Post-processing (RF-26): level 1 when `rules` is chosen;
    // identity otherwise (the verbatim profile guarantees it anyway).
    let mut post: Box<dyn voxli_core::PostProcessor> = match cfg.general.post_processing {
        voxli_config::PostProcessing::Rules => Box::new(voxli_text::FrenchProcessor),
        _ => Box::new(PassThroughProcessor),
    };

    // --- History (RF-33): the store lives in `UiShared` (shared with
    // the RF-33 IPC read commands); the worker only touches it at the
    // right moment via `maybe_record_history`.
    // --- Focus + insertion (voxli-text / voxli-win).
    #[cfg(target_os = "windows")]
    let focus_reader = WindowsFocusReader::new(excluded);
    #[cfg(not(target_os = "windows"))]
    let focus_reader = FocusStub;

    #[allow(unused_mut)]
    let mut inserter: Box<dyn Inserter> = build_inserter(&cfg);

    let clock = SteadyClock::default();
    let mut streamed = StreamTranscriptor {
        inner: Arc::clone(&engine.batch),
        slot: Arc::clone(&stream),
    };
    let deps = Deps {
        capture: &mut capture,
        vad: &vad,
        transcriptor: &streamed,
        post: &*post,
        inserter: &mut *inserter,
        focus: &focus_reader,
    };
    let mut orch = Orchestrator::new(
        orch_cfg,
        deps,
        make_sink(&app, &logger, &engine_label),
        &clock,
    );

    // --- Loop: event relay + RF-11 safety tick.
    // The borrow checker forces a full orchestrator rebuild whenever a
    // dependency changes: the old one is dropped BEFORE the new assembly
    // (explicit `drop`) and the RF-22 buffer is preserved via `restore_buffer`.
    let mut tick: u32 = 0;
    let mut last_recorded_text: Option<String> = None;
    loop {
        match rx.recv_timeout(LOOP_TICK) {
            Ok(WorkerMsg::Input(ev)) => {
                orch.handle(ev);
                maybe_record_history(&app, &orch, &engine_label, &mut last_recorded_text);
                sync_rescue_buffer(&app, &orch);
            }
            Ok(WorkerMsg::ShowRescue) => {
                if lock_or_recover(&app.state::<UiShared>().last).is_some() {
                    let _ = open_rescue_window(&app);
                }
            }
            Ok(WorkerMsg::ConfigChanged(new_cfg)) => {
                cfg = *new_cfg;
                // Mid-capture: clean cancellation (never a rebuild under
                // the pipeline's feet); the user sees a clean resumption.
                if !matches!(
                    orch.state(),
                    DictationState::Idle | DictationState::Recovery { .. }
                ) {
                    orch.handle(InputEvent::Cancel);
                    logger.log(Event::DeviceEvent {
                        kind: "config-reload-rebuild".into(),
                    });
                }
                // Rights refreshed (an imported license takes effect here).
                let (pro_profiles, pro_history_disk) = {
                    let lic = lock_or_recover(&shared.license);
                    (
                        lic.features.iter().any(|f| f == "extra-profiles"),
                        lic.features.iter().any(|f| f == "history-on-disk"),
                    )
                };
                let orch_cfg = to_orchestrator_cfg(&cfg, pro_profiles);
                // RF-08: the device swap happens AFTER the orchestrator
                // has been dropped (it holds an exclusive borrow).
                #[allow(unused_mut, unused_assignments)]
                let mut reopen_device = false;
                #[cfg(target_os = "windows")]
                if cfg.audio.device != last_device {
                    last_device = cfg.audio.device.clone();
                    reopen_device = true;
                }
                #[cfg(not(target_os = "windows"))]
                let _ = reopen_device;
                // RF-33: the store lives in UiShared, shared with the
                // read commands — rebuilt according to the new config.
                rebuild_history(&shared, &cfg, pro_history_disk, &logger);

                // All dependencies are MUTATED after the drop: the old
                // orchestrator exclusively borrows them until here.
                let last = orch.last_transcript().cloned();
                drop(orch);
                // Engine: reloaded only if the choice or the model changed
                // (a real load is expensive, RF-20).
                // Streaming slot: language/verbatim copied even without an
                // engine change (the factory remains valid).
                stream.update_mount(
                    engine.streaming.clone(),
                    Some(cfg.general.language.clone()).filter(|s| !s.is_empty()),
                    cfg.profiles.default.style == voxli_config::Style::Verbatim,
                );
                if engine_stale(&engine_label, &engine_model, &cfg) {
                    let (rebuilt, mounted) = build_engine(&cfg, &logger);
                    engine = rebuilt;
                    *lock_or_recover(&engine_model) = mounted;
                    if let Err(err) = engine.batch.warmup() {
                        logger.log(Event::Error {
                            module: "asr".into(),
                            code: asr_code(&err),
                        });
                    }
                    *lock_or_recover(&engine_label) = engine.batch.id().as_str().to_string();
                    *lock_or_recover(&app.state::<UiShared>().engine) =
                        engine.batch.id().as_str().to_string();
                    stream.update_mount(
                        engine.streaming.clone(),
                        Some(cfg.general.language.clone()).filter(|s| !s.is_empty()),
                        cfg.profiles.default.style == voxli_config::Style::Verbatim,
                    );
                    streamed.inner = Arc::clone(&engine.batch);
                }
                post = match cfg.general.post_processing {
                    voxli_config::PostProcessing::Rules => Box::new(voxli_text::FrenchProcessor),
                    _ => Box::new(PassThroughProcessor),
                };
                #[cfg(target_os = "windows")]
                if reopen_device {
                    let mut slot = lock_or_recover(&capture_meter);
                    *slot = build_capture(&cfg.audio.device, &logger, capture_tap.clone());
                }
                inserter = build_inserter(&cfg);

                streamed.inner = Arc::clone(&engine.batch);
                let deps = Deps {
                    capture: &mut capture,
                    vad: &vad,
                    transcriptor: &streamed,
                    post: &*post,
                    inserter: &mut *inserter,
                    focus: &focus_reader,
                };
                orch = Orchestrator::new(
                    orch_cfg,
                    deps,
                    make_sink(&app, &logger, &engine_label),
                    &clock,
                );
                if let Some(t) = last {
                    orch.restore_buffer(t);
                }
            }
            Ok(WorkerMsg::Shutdown) => break,
            Ok(WorkerMsg::ModelInstalled) => {
                // RF-15: the model has just been installed; the engine can
                // only change outside a capture (exclusive capture). If a
                // dictation is running, the revalidation will wait for the
                // next useful cycle (ConfigChanged or device swap).
                if matches!(
                    orch.state(),
                    DictationState::Idle | DictationState::Recovery { .. }
                ) && engine_stale(&engine_label, &engine_model, &cfg)
                {
                    logger.log(Event::DeviceEvent {
                        kind: "model-installed-rebuild".into(),
                    });
                    let last = orch.last_transcript().cloned();
                    let orch_cfg = to_orchestrator_cfg(&cfg, pro_profiles);
                    drop(orch);
                    let (rebuilt, mounted) = build_engine(&cfg, &logger);
                    engine = rebuilt;
                    *lock_or_recover(&engine_model) = mounted;
                    if let Err(err) = engine.batch.warmup() {
                        logger.log(Event::Error {
                            module: "asr".into(),
                            code: asr_code(&err),
                        });
                    }
                    *lock_or_recover(&engine_label) = engine.batch.id().as_str().to_string();
                    *lock_or_recover(&app.state::<UiShared>().engine) =
                        engine.batch.id().as_str().to_string();
                    stream.update_mount(
                        engine.streaming.clone(),
                        Some(cfg.general.language.clone()).filter(|s| !s.is_empty()),
                        cfg.profiles.default.style == voxli_config::Style::Verbatim,
                    );
                    streamed.inner = Arc::clone(&engine.batch);
                    post = match cfg.general.post_processing {
                        voxli_config::PostProcessing::Rules => {
                            Box::new(voxli_text::FrenchProcessor)
                        }
                        _ => Box::new(PassThroughProcessor),
                    };
                    let deps = Deps {
                        capture: &mut capture,
                        vad: &vad,
                        transcriptor: &streamed,
                        post: &*post,
                        inserter: &mut *inserter,
                        focus: &focus_reader,
                    };
                    orch = Orchestrator::new(
                        orch_cfg,
                        deps,
                        make_sink(&app, &logger, &engine_label),
                        &clock,
                    );
                    if let Some(t) = last {
                        orch.restore_buffer(t);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                tick = tick.wrapping_add(1);
                orch.handle(InputEvent::Tick);
                // Streaming (docs/05, addition): non-blocking ping of the
                // driver — IT is the one pushing the tap into the session
                // and emitting the display partials (transient, never
                // persisted). No FFI on this thread: the worker no
                // longer depends on SDK health (incident 18/10/2026).
                let _ = stream.tx.send(StreamCmd::Pump);
                // Numeric diagnostics (~1 Hz during a capture — numbers
                // only, never content, RF-34): health of the tap
                // and of the display session. A `sess=0` with an ongoing
                // capture = failed session open; `fed=0` = dry tap;
                // `updates=0` ⇔ the SDK decoded nothing at this stage.
                if tick.is_multiple_of(20)
                    && matches!(orch.state(), DictationState::Recording { .. })
                {
                    let fed = stream.fed.swap(0, Ordering::Relaxed);
                    let updates = stream.updates_seen.swap(0, Ordering::Relaxed);
                    let session_open = stream.has_session.load(Ordering::Relaxed);
                    logger.log(Event::DeviceEvent {
                        kind: format!(
                            "stream-diag:fed={fed},updates={updates},sess={}",
                            u8::from(session_open)
                        ),
                    });
                }
                // RF-08: mic disconnected → the orchestrator got the
                // end-of-capture error; outside any capture, the capture is
                // rebuilt (orchestrator re-assembled, RF-22 buffer
                // preserved). Read through the shared Arc: the orchestrator
                // holds an exclusive borrow of its own path.
                let device_lost_now = {
                    #[cfg(target_os = "windows")]
                    {
                        lock_or_recover(&capture_meter).device_lost()
                    }
                    #[cfg(not(target_os = "windows"))]
                    {
                        false
                    }
                };
                #[cfg(target_os = "windows")]
                if device_lost_now
                    && matches!(
                        orch.state(),
                        DictationState::Idle | DictationState::Recovery { .. }
                    )
                {
                    logger.log(Event::DeviceEvent {
                        kind: "device-lost-rebuild".into(),
                    });
                    let now_cfg = lock_or_recover(&shared.config).clone();
                    let last = orch.last_transcript().cloned();
                    let orch_cfg = to_orchestrator_cfg(&now_cfg, pro_profiles);
                    drop(orch);
                    {
                        let mut slot = lock_or_recover(&capture_meter);
                        *slot = build_capture(&now_cfg.audio.device, &logger, capture_tap.clone());
                    }
                    let deps = Deps {
                        capture: &mut capture,
                        vad: &vad,
                        transcriptor: &streamed,
                        post: &*post,
                        inserter: &mut *inserter,
                        focus: &focus_reader,
                    };
                    orch = Orchestrator::new(
                        orch_cfg,
                        deps,
                        make_sink(&app, &logger, &engine_label),
                        &clock,
                    );
                    if let Some(t) = last {
                        orch.restore_buffer(t);
                    }
                }
                #[cfg(not(target_os = "windows"))]
                let _ = device_lost_now;
                // RF-09: audio meter — sampled 1 out of 2 (~100 ms,
                // read inside the proxy lock, very brief): the pill's
                // gauge fills and empties with the voice.
                if tick.is_multiple_of(2) {
                    let level = lock_or_recover(&capture_meter).level();
                    let _ = app.emit(
                        "meter://level",
                        LevelPayload {
                            level: (level * 1000.0).round() / 1000.0,
                            at_ms: now_ms(),
                        },
                    );
                }
                sync_rescue_buffer(&app, &orch);
                maybe_record_history(&app, &orch, &engine_label, &mut last_recorded_text);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Mirror (RF-22): the orchestrator buffer is mirrored to the rescue
/// window. The orchestrator writes its buffer **before** any insertion
/// attempt, so the mirror never shows a text that was not transcribed.
fn sync_rescue_buffer(app: &AppHandle<Wry>, orch: &Orchestrator<'_>) {
    let Some(t) = orch.last_transcript() else {
        return;
    };
    let shared = app.state::<UiShared>();
    let mut last = lock_or_recover(&shared.last);
    if last.as_deref() != Some(t.text.as_str()) {
        *last = Some(t.text.clone());
    }
}

// ------------------------------------------------- configuration -> core ---

fn to_orchestrator_cfg(c: &Config, pro_bindings: bool) -> OrchestratorCfg {
    let style_of = |s: voxli_config::Style| match s {
        voxli_config::Style::Verbatim => voxli_core::Style::Verbatim,
        voxli_config::Style::Punctuated => voxli_core::Style::Punctuated,
    };
    // RF-25: profile-app bindings; RF-39: "multiple profiles" is a
    // Pro feature — without a license, only the default profile is
    // active (bindings stay configured but inactive, shown on the
    // profiles screen).
    let bindings = if pro_bindings {
        let profile_of = |p: &voxli_config::ProfileCfg| Profile {
            style: style_of(p.style),
            insertion: voxli_core::InsertionPolicy::ClipboardFirst,
            dictionary: p.dictionary.clone(),
        };
        let mut b = Vec::new();
        if !c.profiles.code.matches.is_empty() {
            b.push(ProfileBinding {
                matches: c.profiles.code.matches.clone(),
                profile: profile_of(&c.profiles.code),
            });
        }
        if !c.profiles.email.matches.is_empty() {
            b.push(ProfileBinding {
                matches: c.profiles.email.matches.clone(),
                profile: profile_of(&c.profiles.email),
            });
        }
        b
    } else {
        Vec::new()
    };
    let profile_of = |p: &voxli_config::ProfileCfg| Profile {
        style: style_of(p.style),
        insertion: voxli_core::InsertionPolicy::ClipboardFirst,
        dictionary: p.dictionary.clone(),
    };
    // Listening language: "auto" (empty string) is a legitimate UI
    // choice (recommended). RF-16: the fallback under 1.5 s of speech
    // MUST NEVER be `None` (random detection on two words) — fall back
    // to the display language, always populated (default fr).
    let listen_language = Some(c.general.language.clone()).filter(|s| !s.is_empty());
    let auto_fallback = listen_language
        .clone()
        .or_else(|| Some(c.general.ui_language.clone()).filter(|s| !s.is_empty()));
    OrchestratorCfg {
        pre_roll_ms: c.audio.pre_roll_ms,
        max_recording: Duration::from_secs(c.audio.max_recording_seconds as u64),
        min_speech_ms: c.audio.vad.min_speech_ms,
        remember_target_at_start: c.insertion.remember_target_at_start,
        profile: profile_of(&c.profiles.default),
        profile_bindings: bindings,
        excluded_apps: c.insertion.excluded_apps.clone(),
        language: listen_language,
        initial_prompt: None, // dictionary-built initial prompt (RF-24)
        auto_language_min: Duration::from_millis(1_500),
        auto_language_fallback: auto_fallback,
        trigger: match c.hotkeys.mode {
            TriggerModeCfg::Hold => TriggerMode::Hold,
            TriggerModeCfg::Toggle => TriggerMode::Toggle,
            TriggerModeCfg::DoublePushLock => TriggerMode::DoublePushLock,
        },
    }
}

#[cfg(feature = "real-asr")]
fn model_path(cfg: &Config) -> std::path::PathBuf {
    let dir = Config::default_dir().join("models");
    // "auto" = the best whisper present (SAME resolution as
    // `engines_list` — a single truth, cf. `resolve_whisper_model`).
    dir.join(crate::commands::resolve_whisper_model(cfg, &dir))
}

/// Normalized compute choice (`auto`|`duo`|`cpu`|`gpu`): empty or unknown
/// = `auto` — any wrong choice falls back to the safe value (RF-29).
#[cfg(feature = "real-asr")]
fn normalize_compute(raw: String) -> String {
    match raw.trim() {
        "cpu" => "cpu".into(),
        "gpu" => "gpu".into(),
        "duo" => "duo".into(),
        _ => "auto".into(),
    }
}

/// Mounted engine: the batch path — authority of the inserted text — and,
/// when the engine allows it, the streaming session factory (live
/// partial display, docs/05).
struct EngineBundle {
    batch: Arc<dyn Transcriptor>,
    streaming: Option<Arc<dyn StreamingEngine>>,
}

/// GGUF ID of the streaming Nemotron 3.5 (model catalogue, docs/04).
/// Unconditional: `engine_stale` must be able to name the expected model
/// even in a build without SDK (wanted nemo never occurs there, but the
/// constant documents the canonical identity).
const NEMO_MODEL_ID: &str = "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf";

/// Directory of the installed Nemotron GGUF (model manager).
#[cfg(feature = "nemo-stream")]
fn nemo_model_path(_cfg: &Config) -> std::path::PathBuf {
    Config::default_dir().join("models").join(NEMO_MODEL_ID)
}

#[cfg(feature = "nemo-stream")]
fn nemo_available(cfg: &Config) -> bool {
    nemo_model_path(cfg).is_file()
}

#[cfg(not(feature = "nemo-stream"))]
fn nemo_available(_cfg: &Config) -> bool {
    false
}

/// Logging of an engine mount failure: the core code AND, for
/// `Runtime` (in-process SDK detail), the process diagnostic text —
/// never dictated content (RF-34). (Single caller, under
/// `nemo-stream`; the no-SDK core carries no detail.)
#[cfg(feature = "nemo-stream")]
fn log_engine_failure(logger: &Logger, err: &voxli_core::AsrError) {
    if let voxli_core::AsrError::Runtime(detail) = err {
        let compact: String = detail.chars().take(160).collect();
        logger.log(Event::DeviceEvent {
            kind: format!("moteur-échec:{compact}"),
        });
    }
    logger.log(Event::Error {
        module: "asr".into(),
        code: asr_code(err),
    });
}

fn build_engine(cfg: &Config, logger: &Logger) -> (EngineBundle, String) {
    // Nemotron 3.5 (feature `nemo-stream`): `auto` priority when the GGUF
    // is installed (it brings the display streaming); explicit choice
    // requires the model (absent → code 7 in the log, audible fake
    // fallback, never silent).
    #[cfg(feature = "nemo-stream")]
    if matches!(cfg.general.engine, EngineChoice::Nemo | EngineChoice::Auto) {
        if nemo_model_path(cfg).is_file() {
            match NemoEngine::load(&nemo_model_path(cfg)) {
                Ok(e) => {
                    logger.log(Event::EngineLoaded {
                        engine: "nemo".into(),
                        model_id: NEMO_MODEL_ID.into(),
                    });
                    let e = Arc::new(e);
                    return (
                        EngineBundle {
                            batch: e.clone(),
                            streaming: Some(e),
                        },
                        NEMO_MODEL_ID.into(),
                    );
                }
                Err(err) => log_engine_failure(logger, &err),
            }
        } else if matches!(cfg.general.engine, EngineChoice::Nemo) {
            logger.log(Event::Error {
                module: "asr".into(),
                code: 7,
            });
        }
    }
    #[cfg(not(feature = "nemo-stream"))]
    if matches!(cfg.general.engine, EngineChoice::Nemo) {
        // Explicit choice but a binary compiled without the SDK: documented refusal.
        logger.log(Event::Error {
            module: "asr".into(),
            code: 9,
        });
    }
    // Parakeet (ADR-03 point 2, feature `parakeet-asr`): explicit choice,
    // or `auto` fallback when whisper has no model installed.
    #[cfg(feature = "parakeet-asr")]
    {
        let parakeet_selected = matches!(cfg.general.engine, EngineChoice::Parakeet);
        let parakeet_auto =
            matches!(cfg.general.engine, EngineChoice::Auto) && !whisper_available(cfg);
        if parakeet_selected || parakeet_auto {
            if parakeet_model_dir(cfg).is_dir() {
                match ParakeetOnnxEngine::load(&parakeet_model_dir(cfg), 2) {
                    Ok(engine) => {
                        logger.log(Event::EngineLoaded {
                            engine: "parakeet".into(),
                            model_id: "parakeet-tdt-0.6b-v3".into(),
                        });
                        return (
                            EngineBundle {
                                batch: Arc::new(engine),
                                streaming: None,
                            },
                            "parakeet-tdt-0.6b-v3".into(),
                        );
                    }
                    Err(err) => {
                        logger.log(Event::Error {
                            module: "asr".into(),
                            code: asr_code(&err),
                        });
                    }
                }
            } else if parakeet_selected {
                logger.log(Event::Error {
                    module: "asr".into(),
                    code: 7,
                });
            }
        }
    }
    #[cfg(not(feature = "parakeet-asr"))]
    if matches!(cfg.general.engine, EngineChoice::Parakeet) {
        logger.log(Event::Error {
            module: "asr".into(),
            code: 7,
        });
    }
    // whisper.cpp (feature `real-asr`) for `whisper` and `auto` if the model
    // is installed (model manager or scripts/fetch-model.ps1).
    // An explicit `Nemo` choice without a model skips this branch: no silent
    // switch to an engine other than the chosen one.
    #[cfg(feature = "real-asr")]
    if !matches!(
        cfg.general.engine,
        EngineChoice::Parakeet | EngineChoice::Nemo
    ) {
        let path = model_path(cfg);
        if path.is_file() {
            // Compute choice (RF-optimal): environment variable first
            // (instant test-time toggle), else the config; any unknown
            // value = `auto` (never an obscure screen).
            // `duo`: two separate contexts, distinct roles — display
            // partials go to the GPU (requested; a build without backend
            // simply falls back to the same context as the batch), the
            // final text stays on CPU (stable arbiter, RF-22). Both
            // devices then genuinely work side by side, each on its own
            // stream (RF-34).
            let compute = normalize_compute(
                std::env::var("VOXLI_COMPUTE")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| cfg.general.compute.clone()),
            );
            let batch_choice = if compute == "duo" {
                "cpu"
            } else {
                compute.as_str()
            };
            match WhisperCppEngine::load_with(&path, batch_choice) {
                Ok(engine) => {
                    // Two roles: batch path (authority of the inserted
                    // text, full buffer RF-22) and display partials
                    // (windowed re-decoding, never inserted — RF-34 +
                    // ADR-02 amendment).
                    let engine = Arc::new(engine);
                    let batch: Arc<dyn Transcriptor> = engine.clone();
                    let model_id = path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if compute != "duo" {
                        // The REQUESTED compute is logged with the choice;
                        // the backend actually used is read off the latencies.
                        logger.log(Event::DeviceEvent {
                            kind: format!("calcul:{compute}"),
                        });
                    }
                    // Partials context: in duo, a second load of the same
                    // weights, intended for the requested GPU; failure →
                    // transparent fallback onto the batch context (logged).
                    let partial_ctx: Arc<WhisperCppEngine> = if compute == "duo" {
                        match WhisperCppEngine::load_with(&path, "gpu") {
                            Ok(e) => {
                                logger.log(Event::DeviceEvent {
                                    kind: "calcul:duo (batch=cpu, partiels=gpu demandé)".into(),
                                });
                                Arc::new(e)
                            }
                            Err(err) => {
                                logger.log(Event::DeviceEvent {
                                    kind: format!("calcul:duo→cpu (contexte-gpu refusé: {err})"),
                                });
                                engine.clone()
                            }
                        }
                    } else {
                        engine.clone()
                    };
                    logger.log(Event::EngineLoaded {
                        engine: "whisper.cpp".into(),
                        model_id: model_id.clone(),
                    });
                    return (
                        EngineBundle {
                            batch,
                            streaming: Some(Arc::new(WhisperPartialStreamer::new(partial_ctx))),
                        },
                        model_id,
                    );
                }
                // Model present yet unloadable: visible fallback, never silent.
                Err(err) => {
                    logger.log(Event::Error {
                        module: "asr".into(),
                        code: asr_code(&err),
                    });
                }
            }
        } else if matches!(cfg.general.engine, EngineChoice::Whisper) {
            // Explicit choice without an installed model: same signal as the
            // other engines (code 7 = missing model) — never a silent fake
            // fallback (UX finding of 19/10).
            logger.log(Event::Error {
                module: "asr".into(),
                code: 7,
            });
        }
    }
    // Fake fallback: always logged DISTINCTLY — a dictation that
    // "succeeds" on prefabricated text must never be confusable with a
    // real engine (UX finding of 19/10).
    logger.log(Event::DeviceEvent {
        kind: "moteur-factice:aucun-moteur-réel-disponible".into(),
    });
    logger.log(Event::EngineLoaded {
        engine: "fake".into(),
        model_id: "faux".into(),
    });
    (
        EngineBundle {
            batch: Arc::new(FakeEngine::default()),
            streaming: None,
        },
        "faux".into(),
    )
}

/// Directory of the installed Parakeet model (model manager).
#[cfg(feature = "parakeet-asr")]
fn parakeet_model_dir(_cfg: &Config) -> std::path::PathBuf {
    Config::default_dir()
        .join("models")
        .join("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8")
}

/// Real whisper engine availability (feature + model present).
#[cfg(feature = "real-asr")]
fn whisper_available(cfg: &Config) -> bool {
    model_path(cfg).is_file()
}

#[cfg(not(feature = "real-asr"))]
fn whisper_available(_cfg: &Config) -> bool {
    false
}

/// Real Parakeet engine availability (feature + directory present).
#[cfg(feature = "parakeet-asr")]
fn parakeet_available(cfg: &Config) -> bool {
    parakeet_model_dir(cfg).is_dir()
}

#[cfg(not(feature = "parakeet-asr"))]
fn parakeet_available(_cfg: &Config) -> bool {
    false
}

/// RF-01: detects whether the engine to rebuild differs from the one in
/// service — avoids reloading an expensive model for an unrelated change.
/// The comparison covers the SHELL (label: whisper.cpp, nemo…) AND the
/// CONTENT (mounted model file): two whisper.cpp mounts do not
/// necessarily carry the same weights — without this, activating another
/// installed voice triggered no rebuild (finding of 21/10).
fn engine_stale(
    label: &Arc<std::sync::Mutex<String>>,
    mounted_model: &Arc<std::sync::Mutex<String>>,
    cfg: &Config,
) -> bool {
    let wanted = match cfg.general.engine {
        EngineChoice::Whisper => {
            if whisper_available(cfg) {
                "whisper.cpp"
            } else {
                "fake"
            }
        }
        EngineChoice::Parakeet => {
            if parakeet_available(cfg) {
                "parakeet"
            } else {
                "fake"
            }
        }
        EngineChoice::Nemo => {
            if nemo_available(cfg) {
                "nemo"
            } else {
                "fake"
            }
        }
        EngineChoice::Auto => {
            // Auto priority: Nemotron (display streaming), then whisper,
            // then Parakeet (same rules as build_engine, docs/05 §4).
            if nemo_available(cfg) {
                "nemo"
            } else if whisper_available(cfg) {
                "whisper.cpp"
            } else if parakeet_available(cfg) {
                "parakeet"
            } else {
                "fake"
            }
        }
    };
    // Expected model: resolution SHARED with build_engine and
    // engines_list (`resolve_whisper_model`) — a single truth. The log
    // mirror (`model_id` of engine_loaded) serves as the mounted-side
    // reference; "faux" for the fake fallback.
    let wanted_model = if wanted == "whisper.cpp" {
        crate::commands::resolve_whisper_model(cfg, &Config::default_dir().join("models"))
    } else if wanted == "nemo" {
        NEMO_MODEL_ID.into()
    } else if wanted == "parakeet" {
        "parakeet-tdt-0.6b-v3".into()
    } else {
        "faux".into()
    };
    let current = lock_or_recover(label).clone();
    let current_model = lock_or_recover(mounted_model).clone();
    current != wanted || current_model != wanted_model
}

// ------------------------------------------------- event forwarding ---

fn forward_event(app: &AppHandle<Wry>, logger: &Logger, engine_label: &str, ev: CoreEvent) {
    match ev {
        CoreEvent::StateChanged(s) => {
            let shared = app.state::<UiShared>();
            let from = lock_or_recover(&shared.state).clone();
            logger.log(Event::StateChange {
                from,
                to: s.name().into(),
            });
            *lock_or_recover(&shared.state) = s.name().to_string();
            // RF-09 v2: the pill is a permanent indicator (armed listening
            // dot at the top center) — the webview subscribes itself to
            // `state://changed` and carries all the visuals; the core no
            // longer touches its visibility or content (ADR-02, RF-34).
            let _ = app.emit(
                "state://changed",
                StatePayload {
                    state: s.name().to_string(),
                    at_ms: now_ms(),
                },
            );
        }
        CoreEvent::NothingToTranscribe => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "nothing",
                    message: strings::MSG_NOTHING.into(),
                },
            );
        }
        CoreEvent::RejectedBusy => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "busy",
                    message: strings::MSG_BUSY.into(),
                },
            );
        }
        // RF-11: duration warning, once per capture.
        CoreEvent::MaxDurationWarning { remaining_ms } => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "max-warning",
                    message: strings::msg_max_duration(remaining_ms),
                },
            );
        }
        // RF-02 (c): double-push locking.
        CoreEvent::RecordingLocked => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "locked",
                    message: strings::MSG_LOCKED.into(),
                },
            );
        }
        // RF-05: queueing on the application side.
        CoreEvent::QueuedAfterCurrent => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "queued",
                    message: strings::MSG_QUEUED.into(),
                },
            );
        }
        // RF-32: retargeting to the foreground window.
        CoreEvent::RetargetedToCurrent => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "retarget",
                    message: strings::MSG_RETARGET.into(),
                },
            );
        }
        CoreEvent::Inserted { level, attempts } => {
            logger.log(Event::InsertResult {
                level: level.as_str().into(),
                fallback_reason: if attempts.is_empty() {
                    None
                } else {
                    Some(attempts.join(","))
                },
            });
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "inserted",
                    message: strings::msg_inserted(level.as_str()),
                },
            );
        }
        // Single RF-31 exception: the transcribed text leaves the core for
        // the rescue window, strictly local.
        CoreEvent::RescueRequested { transcript } => {
            logger.log(Event::InsertResult {
                level: "rescue".into(),
                fallback_reason: Some("all-levels-failed".into()),
            });
            let shared = app.state::<UiShared>();
            let mut last = lock_or_recover(&shared.last);
            *last = Some(transcript.text.clone());
            drop(last);
            if open_rescue_window(app).is_err() {
                logger.log(Event::Error {
                    module: "ui".into(),
                    code: 1,
                });
            }
        }
        CoreEvent::Refused { reason } => {
            logger.log(Event::InsertRefused {
                reason: reason.as_str().into(),
            });
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "refused",
                    message: strings::msg_refused(reason.as_str()).into(),
                },
            );
            // The text is kept (RF-22): the rescue window is its single
            // visible surface for P0.
            if open_rescue_window(app).is_err() {
                logger.log(Event::Error {
                    module: "ui".into(),
                    code: 1,
                });
            }
        }
        CoreEvent::Recovered { .. } => {
            logger.log(Event::Error {
                module: "pipeline".into(),
                code: 10,
            });
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "recovered",
                    message: strings::MSG_RECOVERED.into(),
                },
            );
            // No text yet: the rescue window carries "Retry"
            // (the audio is kept, RF-20).
            if open_rescue_window(app).is_err() {
                logger.log(Event::Error {
                    module: "ui".into(),
                    code: 1,
                });
            }
        }
        CoreEvent::Latency { stage, micros } => {
            logger.log(Event::Latency {
                stage: stage.as_str().into(),
                micros,
            });
        }
        CoreEvent::SessionFinished {
            success,
            duration,
            level,
        } => {
            logger.log(Event::SessionDone {
                success,
                duration_ms: duration.as_millis() as u64,
                engine: engine_label.into(),
                level: level.map(|l| l.as_str().into()),
            });
            let _ = app.emit(
                "session://finished",
                SessionPayload {
                    success,
                    duration_ms: duration.as_millis() as u64,
                    level: level.map(|l| l.as_str().to_string()),
                    at_ms: now_ms(),
                },
            );
        }
        CoreEvent::Error { module, code } => {
            logger.log(Event::Error {
                module: module.into(),
                code,
            });
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "error",
                    message: strings::msg_error(module, code),
                },
            );
        }
    }
}

// ------------------------------------------------- rescue window ---

/// Opens (or reveals) the RF-31 rescue window. Creation on the main
/// thread (UIA event affinity) then buffer re-read by the page
/// (`rescueLast` command) — no content in the creation event.
pub fn open_rescue_window(app: &AppHandle<Wry>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("rescue") {
        w.show()?;
        w.set_focus()?;
        return Ok(());
    }
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = WebviewWindowBuilder::new(&handle, "rescue", WebviewUrl::App("rescue.html".into()))
            .title(strings::RESCUE_WINDOW_TITLE)
            .inner_size(540.0, 440.0)
            .min_inner_size(380.0, 320.0)
            .resizable(true)
            .visible(true)
            .build();
    })
}

// ------------------------------------------------------- assembly helpers ---

/// RF-08: capture built with the configured device.
/// Explicit chain: configured → default → fake (the log documents it).
/// The streaming tap (native slices during a capture, docs/05)
/// accompanies the real capture; the fake fallback lacks it
/// (log `capture-fallback-fake` — live display switches off).
#[cfg(target_os = "windows")]
fn build_capture(
    device: &str,
    logger: &Logger,
    tap: Option<CaptureTap>,
) -> Box<dyn Capture + Send> {
    match CpalCapture::new_with_tap(device, tap.clone()) {
        Ok(c) => {
            logger.log(Event::DeviceEvent {
                kind: format!("capture-opened:{}", c.device_name()),
            });
            Box::new(c)
        }
        Err(first) => {
            logger.log(Event::Error {
                module: "capture".into(),
                code: capture_code(&first),
            });
            // Fallback to the current default microphone before giving up.
            match CpalCapture::new_with_tap("default", tap) {
                Ok(c) => {
                    logger.log(Event::DeviceEvent {
                        kind: "capture-fallback-default".into(),
                    });
                    Box::new(c)
                }
                Err(second) => {
                    logger.log(Event::Error {
                        module: "capture".into(),
                        code: capture_code(&second),
                    });
                    logger.log(Event::DeviceEvent {
                        kind: "capture-fallback-fake".into(),
                    });
                    Box::new(FakeCapture::with_duration(Duration::from_millis(1600)))
                }
            }
        }
    }
}

/// VAD bounds calibrated on the chosen aggressiveness (RF-18).
fn vad_config(cfg: &Config) -> VadConfig {
    VadConfig {
        min_speech_ms: cfg.audio.vad.min_speech_ms,
        threshold_rms: match cfg.audio.vad.aggressiveness {
            voxli_config::Aggressiveness::Low => 0.010,
            voxli_config::Aggressiveness::Balanced => 0.005,
            voxli_config::Aggressiveness::High => 0.002,
        },
        ..Default::default()
    }
}

/// Configured inserter (clipboard hold time per class).
#[cfg(target_os = "windows")]
fn build_inserter(cfg: &Config) -> Box<dyn Inserter> {
    let mut ins = WindowsInserter::new();
    ins.hold_ms_default = cfg.insertion.clipboard_hold_ms.default;
    ins.hold_ms_slow_apps = cfg.insertion.clipboard_hold_ms.slow_apps;
    ins.hold_ms_remote_desktop = cfg.insertion.clipboard_hold_ms.remote_desktop;
    ins.exclude_from_clipboard_history = cfg.insertion.exclude_from_clipboard_history;
    Box::new(ins)
}

#[cfg(not(target_os = "windows"))]
fn build_inserter(_cfg: &Config) -> Box<dyn Inserter> {
    Box::new(InserterStub)
}

/// History enabled (RF-33): memory by default, encrypted disk when
/// (a) the `history-disk` feature is compiled, (b) the license allows it,
/// (c) the config option requests it. Otherwise `None` (never silent:
/// the choice is logged at startup).
#[allow(unused_mut, unused_variables)]
pub fn build_history(
    cfg: &Config,
    licensed_disk: bool,
    logger: &Logger,
) -> Option<Box<dyn HistoryStore>> {
    if !cfg.general.history.enabled {
        logger.log(Event::DeviceEvent {
            kind: "history-off".into(),
        });
        return None;
    }
    #[cfg(feature = "history-disk")]
    {
        if cfg.general.history.storage == voxli_config::StorageKind::Disk && licensed_disk {
            let path = Config::default_dir().join("histoire.db");
            match voxli_storage::SqliteStore::open(&path) {
                Ok(sql) => {
                    logger.log(Event::DeviceEvent {
                        kind: "history-disk-encrypted".into(),
                    });
                    return Some(Box::new(voxli_storage::EncryptedStore::new(
                        sql,
                        voxli_storage::DpapiCodec,
                    )));
                }
                Err(err) => {
                    logger.log(Event::Error {
                        module: "history".into(),
                        code: 1,
                    });
                    let _ = err;
                }
            }
        } else if cfg.general.history.storage == voxli_config::StorageKind::Disk && !licensed_disk {
            // Explicit refusal: NO memory fallback masking the intent
            // (RF-33: disk history requires the license).
            logger.log(Event::DeviceEvent {
                kind: "history-disk-requires-license".into(),
            });
            return None;
        }
    }
    #[cfg(not(feature = "history-disk"))]
    {
        if cfg.general.history.storage == voxli_config::StorageKind::Disk {
            logger.log(Event::DeviceEvent {
                kind: "history-disk-feature-off".into(),
            });
        }
    }
    logger.log(Event::DeviceEvent {
        kind: "history-memory".into(),
    });
    // Bounded in memory (RF-33): 200 entries, most recent first.
    Some(Box::new(MemoryStore::new(200)))
}

/// (Re)builds the history store of `UiShared` (RF-33) — called at
/// startup (main.rs) and at every configuration reload (here).
pub fn rebuild_history(shared: &UiShared, cfg: &Config, licensed_disk: bool, logger: &Logger) {
    *lock_or_recover(&shared.history) = build_history(cfg, licensed_disk, logger);
}

/// RF-33: write the transcription into the history on returning to Idle.
/// The text read is the one of the core buffer; the entry is stored in
/// `UiShared.history` (encrypted when disk is chosen, RF-33) and a config
/// reload does not duplicate an entry
/// (`last_recorded_text`).
fn maybe_record_history(
    app: &AppHandle<Wry>,
    orch: &Orchestrator<'_>,
    engine_label: &Arc<std::sync::Mutex<String>>,
    last_recorded_text: &mut Option<String>,
) {
    let shared = app.state::<UiShared>();
    if lock_or_recover(&shared.history).is_none() {
        return;
    }
    if !lock_or_recover(&shared.config).general.history.enabled {
        return;
    }
    if !matches!(orch.state(), DictationState::Idle) {
        return;
    }
    let Some(t) = orch.last_transcript() else {
        return;
    };
    if last_recorded_text.as_deref() == Some(t.text.as_str()) {
        return;
    }
    *last_recorded_text = Some(t.text.clone());
    let engine = lock_or_recover(engine_label).clone();
    let now = now_ms();
    let mut guard = lock_or_recover(&shared.history);
    let Some(store) = guard.as_mut() else { return };
    if store.append(&t.text, &engine, now).is_ok() {
        // Bounded retention: sliding window applied after each
        // write (negligible cost, no dedicated thread).
        let days = lock_or_recover(&shared.config)
            .general
            .history
            .retention_days as u64;
        let cutoff = now.saturating_sub(days * 86_400_000);
        let _ = store.retain_since(cutoff);
    }
}

/// Audio meter (RF-09): instantaneous level [0..1].
#[derive(Clone, Serialize)]
struct LevelPayload {
    level: f32,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

/// Core event sink factory — rebuilt at every orchestrator
/// reload (the `Arc`s are shared, zero cost).
fn make_sink(
    app: &AppHandle<Wry>,
    logger: &Arc<Logger>,
    engine_label: &Arc<std::sync::Mutex<String>>,
) -> Box<dyn FnMut(CoreEvent)> {
    let app = app.clone();
    let logger = Arc::clone(logger);
    let label = Arc::clone(engine_label);
    Box::new(move |ev: CoreEvent| {
        let eng = lock_or_recover(&label).clone();
        forward_event(&app, &logger, &eng, ev);
    })
}
