//! Orchestrateur de la session de dictée.
//!
//! La logique est **synchrone et pure** : elle pilote les ports (traits) et
//! émet des événements via un sink injecté. Le threading réel (threads
//! temps réel, pipeline, injection — ADR-12) est de la responsabilité de la
//! couche d'assemblage (`apps/desktop/src-tauri`).
//!
//! Invariants (docs/04 §3, vérifiés par `tests/flow.rs`) :
//! 1. le texte transcrit est placé dans le tampon interne **avant** toute
//!    tentative d'insertion (RF-22) et n'est jamais perdu (RF-20) ;
//! 2. une seule session active ; une demande pendant une session occupée est
//!    refusée explicitement avec indication (RF-05, option « refus » — la
//!    file d'attente de 1 arrive en V1) ;
//! 3. l'audio est libéré après insertion, refus ou annulation.
//!
//! En P0 le pipeline s'exécute à bloc sur le thread de session ; la découpe
//! en étapes pilotées par événements (V1, RF-05) réutilisera ces mêmes
//! états sans changer les ports.

use std::time::Duration;

use crate::errors::InsertError;
use crate::state::{DictationState, RecoveryReason};
use crate::traits::{Clock, SpeechVerdict, Stage};
use crate::types::{
    AudioBuffer, FocusSnapshot, InsertLevel, Profile, RefuseReason, TranscribeOptions, Transcript,
};

/// Événements d'entrée de l'orchestrateur.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    HotkeyPressed,
    HotkeyReleased,
    Cancel,
    Retry,
    PasteAgain,
    Tick,
}

/// Événements de sortie. Aucun ne porte de contenu de texte, sauf
/// `RescueRequested` : la fenêtre de secours affiche le texte — c'est sa
/// fonction (RF-31), documentée dans la table IPC de docs/05 §3 comme
/// exception unique, strictement locale.
#[derive(Clone, Debug, PartialEq)]
pub enum CoreEvent {
    StateChanged(DictationState),
    /// Aucune voix exploitable : aucune insertion (RF-18).
    NothingToTranscribe,
    /// Demande refusée : session occupée (RF-05) — indication visible.
    RejectedBusy,
    Inserted {
        level: InsertLevel,
    },
    /// Échec total d'insertion : fenêtre de secours (RF-28 niveau 4, RF-31).
    RescueRequested {
        transcript: Transcript,
    },
    /// Insertion refusée : champ sensible, app exclue ou cible indisponible
    /// (RF-29, RF-32) — message neutre, texte récupérable (RF-22).
    Refused {
        reason: RefuseReason,
    },
    /// Panne récupérable : audio conservé, « réessayer » disponible (RF-20).
    Recovered {
        reason: RecoveryReason,
    },
    Latency {
        stage: Stage,
        micros: u64,
    },
    SessionFinished {
        success: bool,
        duration: Duration,
        level: Option<InsertLevel>,
    },
    /// Erreur technique (codes stables, sans contenu — RF-34).
    Error {
        module: &'static str,
        code: u32,
    },
}

/// Horloge temps réel (monotone) pour la production.
pub struct SteadyClock {
    base: std::time::Instant,
}

impl Default for SteadyClock {
    fn default() -> Self {
        Self {
            base: std::time::Instant::now(),
        }
    }
}

impl Clock for SteadyClock {
    fn now(&self) -> Duration {
        self.base.elapsed()
    }
}

/// Horloge de test : avancée manuellement.
pub struct TestClock(pub std::cell::Cell<u64>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.get())
    }
}

/// Configuration de l'orchestrateur ; valeurs par défaut issues de
/// `plume-config` (docs/05 §4).
#[derive(Clone, Debug)]
pub struct OrchestratorCfg {
    pub pre_roll_ms: u32,
    pub max_recording: Duration,
    pub min_speech_ms: u32,
    pub remember_target_at_start: bool,
    pub profile: Profile,
    pub excluded_apps: Vec<String>,
    pub language: Option<String>,
    pub initial_prompt: Option<String>,
}

impl Default for OrchestratorCfg {
    fn default() -> Self {
        Self {
            pre_roll_ms: 400,
            max_recording: Duration::from_secs(300),
            min_speech_ms: 250,
            remember_target_at_start: true,
            profile: Profile::default(),
            excluded_apps: Vec::new(),
            language: Some("fr".to_string()),
            initial_prompt: None,
        }
    }
}

/// Dépendances injectées. Aucune n'est connue du noyau au-delà de son trait.
pub struct Deps<'a> {
    pub capture: &'a mut dyn crate::traits::Capture,
    pub vad: &'a dyn crate::traits::VoiceActivity,
    pub transcriptor: &'a dyn crate::traits::Transcriptor,
    pub post: &'a dyn crate::traits::PostProcessor,
    pub inserter: &'a mut dyn crate::traits::Inserter,
    pub focus: &'a dyn crate::traits::FocusReader,
}

/// Travail retenu après une panne récupérable (RF-20) : l'audio de la prise
/// est conservé jusqu'au « réessayer » ou à l'annulation (invariant 3).
struct RecoveryJob {
    target: FocusSnapshot,
    audio: AudioBuffer,
}

pub struct Orchestrator<'a> {
    cfg: OrchestratorCfg,
    deps: Deps<'a>,
    state: DictationState,
    /// Tampon interne du dernier texte transcrit (RF-22) — jamais perdu.
    last: Option<Transcript>,
    recovering: Option<RecoveryJob>,
    sink: Box<dyn FnMut(CoreEvent) + 'a>,
    clock: &'a dyn Clock,
}

impl<'a> Orchestrator<'a> {
    pub fn new(
        cfg: OrchestratorCfg,
        deps: Deps<'a>,
        sink: Box<dyn FnMut(CoreEvent) + 'a>,
        clock: &'a dyn Clock,
    ) -> Self {
        Self {
            cfg,
            deps,
            state: DictationState::Idle,
            last: None,
            recovering: None,
            sink,
            clock,
        }
    }

    pub fn state(&self) -> &DictationState {
        &self.state
    }

    /// Texte du tampon interne — accessible même après échec d'insertion
    /// (RF-22). La fenêtre de secours et « coller à nouveau » s'en servent.
    pub fn last_transcript(&self) -> Option<&Transcript> {
        self.last.as_ref()
    }

    /// L'audio est-il encore détenu (non libéré) ? Invariant 3.
    pub fn holds_audio(&self) -> bool {
        self.recovering.is_some()
    }

    pub fn handle(&mut self, ev: InputEvent) {
        match ev {
            InputEvent::HotkeyPressed => self.on_pressed(),
            InputEvent::HotkeyReleased => self.on_released(),
            InputEvent::Cancel => self.on_cancel(),
            InputEvent::Retry => self.on_retry(),
            InputEvent::PasteAgain => self.on_paste_again(),
            InputEvent::Tick => self.on_tick(),
        }
    }

    fn now(&self) -> Duration {
        self.clock.now()
    }

    fn emit(&mut self, ev: CoreEvent) {
        (self.sink)(ev);
    }

    fn set_state(&mut self, s: DictationState) {
        self.state = s.clone();
        self.emit(CoreEvent::StateChanged(s));
    }

    fn on_pressed(&mut self) {
        match self.state.clone() {
            DictationState::Idle | DictationState::Recovery { .. } => self.begin_recording(),
            // RF-05 (option refus, P0) : session occupée → indication visible,
            // aucun mélange de deux dictées.
            _ => self.emit(CoreEvent::RejectedBusy),
        }
    }

    fn begin_recording(&mut self) {
        let target = match self.deps.focus.snapshot() {
            Some(t) => t,
            None => {
                self.emit(CoreEvent::Refused {
                    reason: RefuseReason::TargetUnavailable,
                });
                return;
            }
        };
        if let Err(e) = self.deps.capture.start(self.cfg.pre_roll_ms) {
            self.emit(CoreEvent::Error {
                module: "capture",
                code: capture_code(&e),
            });
            return;
        }
        self.set_state(DictationState::Recording {
            target,
            started_at: self.now(),
        });
    }

    fn on_released(&mut self) {
        if !matches!(self.state, DictationState::Recording { .. }) {
            return;
        }
        self.finish_recording();
    }

    fn on_cancel(&mut self) {
        match self.state.clone() {
            DictationState::Recording { started_at, .. } => {
                // Annulation explicite : l'audio capté est jeté.
                let _ = self.deps.capture.stop();
                self.set_state(DictationState::Idle);
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration: self.now().saturating_sub(started_at),
                    level: None,
                });
            }
            DictationState::Recovery { .. } => {
                // Invariant 3 : l'audio de récupération est libéré.
                self.recovering = None;
                self.set_state(DictationState::Idle);
            }
            _ => self.emit(CoreEvent::RejectedBusy),
        }
    }

    fn on_retry(&mut self) {
        match self.state.clone() {
            DictationState::Recovery {
                reason: RecoveryReason::EngineError,
            } => {
                let Some(job) = self.recovering.take() else {
                    self.set_state(DictationState::Idle);
                    return;
                };
                self.run_pipeline(job.target, job.audio);
            }
            DictationState::Recovery {
                reason: RecoveryReason::InsertFailed,
            } => {
                // « Réessayer » après échec d'insertion = réinsérer le dernier
                // texte (jamais perdu, RF-22), vers la cible actuelle.
                let Some(t) = self.last.clone() else {
                    self.set_state(DictationState::Idle);
                    return;
                };
                let target = self.deps.focus.snapshot().unwrap_or_else(|| FocusSnapshot {
                    valid: false,
                    ..Default::default()
                });
                self.insert_flow(t, target);
            }
            _ => {}
        }
    }

    fn on_paste_again(&mut self) {
        match self.state.clone() {
            DictationState::Idle | DictationState::Recovery { .. } => {
                let Some(t) = self.last.clone() else { return };
                let Some(target) = self.deps.focus.snapshot() else {
                    self.emit(CoreEvent::Refused {
                        reason: RefuseReason::TargetUnavailable,
                    });
                    return;
                };
                self.insert_flow(t, target);
            }
            _ => self.emit(CoreEvent::RejectedBusy),
        }
    }

    fn on_tick(&mut self) {
        if let DictationState::Recording { started_at, .. } = self.state.clone() {
            if self.now().saturating_sub(started_at) >= self.cfg.max_recording {
                // Durée maximale de sécurité : arrêt puis transcription (RF-11).
                self.finish_recording();
            }
        }
    }

    fn finish_recording(&mut self) {
        let Some(target) = self.state.target().cloned() else {
            return;
        };
        let start = match self.state.clone() {
            DictationState::Recording { started_at, .. } => started_at,
            _ => self.now(),
        };
        let audio = match self.deps.capture.stop() {
            Ok(a) => a,
            Err(e) => {
                self.emit(CoreEvent::Error {
                    module: "capture",
                    code: capture_code(&e),
                });
                self.set_state(DictationState::Idle);
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration: self.now().saturating_sub(start),
                    level: None,
                });
                return;
            }
        };
        self.set_state(DictationState::Finalizing {
            target: target.clone(),
        });
        self.run_pipeline(target, audio);
    }

    /// Pipeline complet : VAD → transcription → post-traitement → insertion.
    /// Synchrone ; l'assemblage l'exécute sur le thread pipeline (ADR-12).
    fn run_pipeline(&mut self, target: FocusSnapshot, audio: AudioBuffer) {
        let t0 = self.now();

        // --- Analyse VAD (RF-18) -----------------------------------------
        let verdict = self.deps.vad.contains_speech(&audio);
        self.emit(CoreEvent::Latency {
            stage: Stage::Vad,
            micros: self.since(t0),
        });
        if verdict != SpeechVerdict::Speech {
            // Aucune insertion ; l'audio est libéré (invariant 3).
            let captured = audio.duration();
            drop(audio);
            self.set_state(DictationState::Idle);
            self.emit(CoreEvent::NothingToTranscribe);
            self.emit(CoreEvent::SessionFinished {
                success: false,
                duration: captured,
                level: None,
            });
            return;
        }

        // --- Transcription (RF-14 : strictement locale par défaut) --------
        let opts = TranscribeOptions {
            language: self.cfg.language.clone(),
            initial_prompt: self.cfg.initial_prompt.clone(),
            verbatim: self.cfg.profile.style == crate::types::Style::Verbatim,
            threads: None,
        };
        let t1 = self.now();
        let result = self.deps.transcriptor.transcribe(&audio, &opts);
        self.emit(CoreEvent::Latency {
            stage: Stage::Inference,
            micros: self.since(t1),
        });
        let base = match result {
            Ok(t) if t.text.trim().is_empty() => {
                let captured = audio.duration();
                drop(audio);
                self.set_state(DictationState::Idle);
                self.emit(CoreEvent::NothingToTranscribe);
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration: captured,
                    level: None,
                });
                return;
            }
            Ok(t) => t,
            Err(e) => {
                // RF-20 : l'audio est conservé, le texte n'est pas perdu.
                self.recovering = Some(RecoveryJob {
                    target: target.clone(),
                    audio,
                });
                self.set_state(DictationState::Recovery {
                    reason: RecoveryReason::EngineError,
                });
                self.emit(CoreEvent::Recovered {
                    reason: RecoveryReason::EngineError,
                });
                self.emit(CoreEvent::Error {
                    module: "asr",
                    code: asr_code(&e),
                });
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration: Duration::ZERO,
                    level: None,
                });
                return;
            }
        };
        drop(audio); // l'audio n'est plus nécessaire après la transcription
        self.set_state(DictationState::Transcribing {
            target: target.clone(),
            queued: None,
        });

        // --- Post-traitement local niveau 0/1 (RF-26) --------------------
        let t2 = self.now();
        let text = match self.deps.post.process(&base.text, &self.cfg.profile) {
            Ok(t) => t,
            Err(_) => base.text.clone(),
        };
        self.emit(CoreEvent::Latency {
            stage: Stage::PostProcess,
            micros: self.since(t2),
        });
        let transcript = Transcript { text, ..base };
        self.set_state(DictationState::PostProcessing {
            target: target.clone(),
        });

        // --- Insertion (RF-22 : le texte est au tampon AVANT la tentative) --
        self.last = Some(transcript.clone());
        self.insert_flow(transcript, target);
    }

    fn insert_flow(&mut self, transcript: Transcript, target: FocusSnapshot) {
        let t3 = self.now();
        self.set_state(DictationState::Inserting {
            target: target.clone(),
        });
        let plan = self.deps.inserter.plan(&target, &self.cfg.profile);
        let result = self.deps.inserter.insert(&transcript.text, &plan);
        let duration = self.now().saturating_sub(t3);
        let micros = duration.as_micros() as u64;
        self.emit(CoreEvent::Latency {
            stage: Stage::Insert,
            micros,
        });

        match result {
            Ok(report) => {
                let level = report.level_used;
                self.set_state(DictationState::Idle);
                if let Some(level) = level {
                    self.emit(CoreEvent::Inserted { level });
                }
                self.emit(CoreEvent::SessionFinished {
                    success: true,
                    duration,
                    level,
                });
            }
            Err(
                err @ (InsertError::RefusedSensitive
                | InsertError::RefusedExcluded
                | InsertError::TargetUnavailable),
            ) => {
                let reason = match err {
                    InsertError::RefusedSensitive => RefuseReason::SensitiveField,
                    InsertError::RefusedExcluded => RefuseReason::ExcludedApp,
                    _ => RefuseReason::TargetUnavailable,
                };
                // Texte conservé au tampon (RF-22) ; message neutre (RF-29).
                self.set_state(DictationState::Idle);
                self.emit(CoreEvent::Refused { reason });
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration,
                    level: None,
                });
            }
            Err(InsertError::AllLevelsFailed(_)) => {
                // RF-28 niveau 4 / RF-31 : fenêtre de secours, texte intact.
                self.set_state(DictationState::Recovery {
                    reason: RecoveryReason::InsertFailed,
                });
                self.emit(CoreEvent::RescueRequested { transcript });
                self.emit(CoreEvent::SessionFinished {
                    success: false,
                    duration,
                    level: None,
                });
            }
        }
    }

    fn since(&self, start: Duration) -> u64 {
        self.now().saturating_sub(start).as_micros() as u64
    }
}

/// Codes stables consignés dans les journaux (RF-34) — publics pour que
/// l'assemblage journalise les mêmes codes que le noyau.
pub fn capture_code(e: &crate::errors::CaptureError) -> u32 {
    match e {
        crate::errors::CaptureError::NoDevice => 1,
        crate::errors::CaptureError::DeviceBusy => 2,
        crate::errors::CaptureError::DeviceLost => 3,
        crate::errors::CaptureError::StreamStart => 4,
        crate::errors::CaptureError::NotStarted => 5,
        crate::errors::CaptureError::UnsupportedFormat => 6,
    }
}

pub fn asr_code(e: &crate::errors::AsrError) -> u32 {
    match e {
        crate::errors::AsrError::NotLoaded => 1,
        crate::errors::AsrError::ModelMissing(_) => 2,
        crate::errors::AsrError::Inference => 3,
        crate::errors::AsrError::Cancelled => 4,
        crate::errors::AsrError::OutOfMemory => 5,
        crate::errors::AsrError::ConsentMissing => 6,
    }
}
