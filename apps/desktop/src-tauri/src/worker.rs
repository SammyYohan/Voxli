//! Thread pipeline de l'application (ADR-12) : il possède l'orchestrateur et
//! ses dépendances, les alimente en événements (`InputEvent`) via un canal,
//! et relaie chaque `CoreEvent` vers la journalisation et l'interface.
//!
//! Pourquoi un thread dédié : l'orchestrateur est synchrone et pur (docs/05
//! §2.1) ; la boucle d'événements UI ne doit jamais être bloquée par
//! l'inférence ou l'insertion. La découpe en pipeline événementiel fin
//! (V1, RF-05) réutilisera ces mêmes composants.
//!
//! Événements émis vers l'UI :
//! - `state://changed` `{ state, atMs }` — aucun contenu (table docs/05 §3) ;
//! - `session://finished` `{ success, durationMs, level, atMs }` ;
//! - `notice://notice` `{ kind, message }` — **addition documentée** à la
//!   table : messages FR neutres définis dans `strings.rs`, jamais de contenu.
//!   `meter://level` (réglages) n'est pas émis en P0 : le niveau demanderait
//!   une deuxième voie depuis le thread temps réel — V1.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use plume_asr::FakeEngine;
#[cfg(feature = "real-asr")]
use plume_asr::WhisperCppEngine;
use plume_audio::{CpalCapture, EnergyVad, FakeCapture, VadConfig};
use plume_config::{Config, EngineChoice};
use plume_core::{
    orchestrator::{asr_code, capture_code},
    Capture, CoreEvent, Deps, InputEvent, Inserter, Orchestrator, OrchestratorCfg, Profile,
    SteadyClock, Transcriptor,
};
use plume_diagnostics::{Event, Logger};
use plume_text::PassThroughProcessor;
#[cfg(target_os = "windows")]
use plume_text::{WindowsFocusReader, WindowsInserter};

use crate::commands::{lock_or_recover, UiShared};
use crate::strings;

/// Période de scrutation de la boucle : sert de « tick » pour la durée
/// maximale de sécurité (RF-11) et, en V1, au scrutin de relâchement ~50 ms
/// (docs/03 ADR-05).
const LOOP_TICK: Duration = Duration::from_millis(50);

/// Messages consommés par le thread pipeline.
pub enum WorkerMsg {
    /// Événements de la machine à états (raccourcis, commandes, `Retry`).
    Input(InputEvent),
    /// « Dernier texte » (raccourci Shift+Alt+X, RF-22) : ouvre la fenêtre de
    /// secours SI un texte est en attente.
    ShowRescue,
    #[allow(dead_code)]
    Shutdown,
}

// ------------------------------------------------- doubleurs hors Windows ---

/// Hors Windows (CI Linux) : aucun champ focalisé exploitable — refus.
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

// ------------------------------------------------------------ charges ---

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

// ----------------------------------------------------------- assemblage ---

/// Point d'entrée du thread : assemblage des dépendances puis boucle.
pub fn run(app: AppHandle<Wry>, logger: Arc<Logger>, rx: Receiver<WorkerMsg>) {
    let cfg = lock_or_recover(&app.state::<UiShared>().config).clone();
    let orch_cfg = to_orchestrator_cfg(&cfg);
    let excluded = cfg.insertion.excluded_apps.clone();

    // --- Capture (ADR-04) : WASAPI via cpal ; repli factice si le micro est
    // absent pour laisser la démo tourner (étiquette « fake » au journal).
    #[cfg(target_os = "windows")]
    let mut capture: Box<dyn Capture> = match CpalCapture::new() {
        Ok(c) => Box::new(c),
        Err(err) => {
            logger.log(Event::Error {
                module: "capture".into(),
                code: capture_code(&err),
            });
            logger.log(Event::DeviceEvent {
                kind: "capture-fallback-fake".into(),
            });
            Box::new(FakeCapture::with_duration(Duration::from_millis(1600)))
        }
    };
    #[cfg(not(target_os = "windows"))]
    let mut capture: Box<dyn Capture> =
        Box::new(FakeCapture::with_duration(Duration::from_millis(1600)));

    // --- VAD énergie (P0, RF-18) : bornes calées sur l'agressivité choisie.
    let vad_cfg = VadConfig {
        min_speech_ms: cfg.audio.vad.min_speech_ms,
        threshold_rms: match cfg.audio.vad.aggressiveness {
            plume_config::Aggressiveness::Low => 0.010,
            plume_config::Aggressiveness::Balanced => 0.005,
            plume_config::Aggressiveness::High => 0.002,
        },
        ..Default::default()
    };
    let vad = EnergyVad::new(vad_cfg);

    // --- Moteur ASR (ADR-03) : réel si compilé + modèle présent ; sinon
    // moteur factice explicite au journal (jamais de fallback silencieux).
    let engine = build_engine(&cfg, &logger);
    let engine_label = engine.id().as_str().to_string();
    if let Err(err) = engine.warmup() {
        logger.log(Event::Error {
            module: "asr".into(),
            code: asr_code(&err),
        });
    }
    // Métadonnée moteur pour l'UI (état_get).
    *lock_or_recover(&app.state::<UiShared>().engine) = engine_label.clone();

    // --- Post-traitement niveau 0 (P0 ; règles V1 — mêmes traits).
    let post = PassThroughProcessor;

    // --- Focus + insertion (plume-text / plume-win).
    #[cfg(target_os = "windows")]
    let focus_reader = WindowsFocusReader::new(excluded);
    #[cfg(not(target_os = "windows"))]
    let focus_reader = FocusStub;

    #[allow(unused_mut)]
    let mut inserter: Box<dyn Inserter> = {
        #[cfg(target_os = "windows")]
        {
            let mut ins = WindowsInserter::new();
            ins.hold_ms_default = cfg.insertion.clipboard_hold_ms.default;
            ins.hold_ms_slow_apps = cfg.insertion.clipboard_hold_ms.slow_apps;
            ins.hold_ms_remote_desktop = cfg.insertion.clipboard_hold_ms.remote_desktop;
            ins.exclude_from_clipboard_history = cfg.insertion.exclude_from_clipboard_history;
            Box::new(ins)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Box::new(InserterStub)
        }
    };

    // --- Orchestrateur : propriétaire de la machine à états.
    let app_for_sink = app.clone();
    let logger_for_sink = Arc::clone(&logger);
    let engine_for_sink = engine_label.clone();
    let sink = Box::new(move |ev: CoreEvent| {
        forward_event(&app_for_sink, &logger_for_sink, &engine_for_sink, ev);
    });

    let deps = Deps {
        capture: &mut *capture,
        vad: &vad,
        transcriptor: &*engine,
        post: &post,
        inserter: &mut *inserter,
        focus: &focus_reader,
    };
    let clock = SteadyClock::default();
    let mut orch = Orchestrator::new(orch_cfg, deps, sink, &clock);

    // --- Boucle : relais des événements + tick de sécurité RF-11.
    loop {
        match rx.recv_timeout(LOOP_TICK) {
            Ok(WorkerMsg::Input(ev)) => {
                orch.handle(ev);
                sync_rescue_buffer(&app, &orch);
            }
            Ok(WorkerMsg::ShowRescue) => {
                if lock_or_recover(&app.state::<UiShared>().last).is_some() {
                    let _ = open_rescue_window(&app);
                }
            }
            Ok(WorkerMsg::Shutdown) => break,
            Err(RecvTimeoutError::Timeout) => {
                orch.handle(InputEvent::Tick);
                sync_rescue_buffer(&app, &orch);
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Miroir (RF-22) : le tampon d'orchestrateur est reflété vers la fenêtre de
/// secours. L'orchestrateur écrit son tampon **avant** toute tentative
/// d'insertion, donc le miroir ne montre jamais un texte qui n'a pas été
/// transcrit.
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

// ------------------------------------------------- configuration -> noyau ---

fn to_orchestrator_cfg(c: &Config) -> OrchestratorCfg {
    OrchestratorCfg {
        pre_roll_ms: c.audio.pre_roll_ms,
        max_recording: Duration::from_secs(c.audio.max_recording_seconds as u64),
        min_speech_ms: c.audio.vad.min_speech_ms,
        remember_target_at_start: c.insertion.remember_target_at_start,
        profile: Profile {
            style: match c.profiles.default.style {
                plume_config::Style::Verbatim => plume_core::Style::Verbatim,
                plume_config::Style::Punctuated => plume_core::Style::Punctuated,
            },
            insertion: plume_core::InsertionPolicy::ClipboardFirst,
        },
        excluded_apps: c.insertion.excluded_apps.clone(),
        language: Some(c.general.language.clone()).filter(|s| !s.is_empty()),
        initial_prompt: None, // amorce métier (RF-24) : V1
    }
}

#[cfg(feature = "real-asr")]
fn model_path(cfg: &Config) -> std::path::PathBuf {
    let dir = Config::default_dir().join("models");
    let name = if cfg.general.model.is_empty() || cfg.general.model == "auto" {
        "ggml-base-q5_0.bin"
    } else {
        cfg.general.model.as_str()
    };
    dir.join(name)
}

fn build_engine(cfg: &Config, logger: &Logger) -> Box<dyn Transcriptor> {
    // Parakeet n'est pas livré en P0 (ADR-03) : consigne explicite.
    if matches!(cfg.general.engine, EngineChoice::Parakeet) {
        logger.log(Event::Error {
            module: "asr".into(),
            code: 7,
        });
    }
    // whisper.cpp (feature `real-asr`) pour `whisper` et `auto` si le modèle
    // est installé par scripts/fetch-model.ps1.
    #[cfg(feature = "real-asr")]
    if !matches!(cfg.general.engine, EngineChoice::Parakeet) {
        let path = model_path(cfg);
        match WhisperCppEngine::load(&path) {
            Ok(engine) => {
                let model_id = path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                logger.log(Event::EngineLoaded {
                    engine: "whisper.cpp".into(),
                    model_id,
                });
                return Box::new(engine);
            }
            // Modèle absent : repli visible au journal, jamais silencieux.
            Err(err) => {
                logger.log(Event::Error {
                    module: "asr".into(),
                    code: asr_code(&err),
                });
            }
        }
    }
    logger.log(Event::EngineLoaded {
        engine: "fake".into(),
        model_id: "faux".into(),
    });
    Box::new(FakeEngine::default())
}

// ------------------------------------------------- diffusion des événements ---

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
        CoreEvent::Inserted { level } => {
            logger.log(Event::InsertResult {
                level: level.as_str().into(),
                fallback_reason: None,
            });
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "inserted",
                    message: strings::msg_inserted(level.as_str()),
                },
            );
        }
        // Exception RF-31 unique : le texte transcrit sort du noyau pour la
        // fenêtre de secours, strictement locale.
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
            // Le texte est conservé (RF-22) : la fenêtre de secours en est la
            // surface visible unique en P0.
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
            // Pas de texte encore : la fenêtre de secours porte « Réessayer »
            // (l'audio est conservé, RF-20).
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

// ------------------------------------------------- fenêtre de secours ---

/// Ouvre (ou révèle) la fenêtre de secours RF-31. Création sur le thread
/// principal (goût d'événements UIA) puis relecture du tampon par la page
/// (commande `rescueLast`) — aucun contenu dans l'événement de création.
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
