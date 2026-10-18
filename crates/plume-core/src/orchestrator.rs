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
    select_profile, AudioBuffer, FocusSnapshot, InsertLevel, Profile, RefuseReason,
    TranscribeOptions, Transcript, TriggerMode, DOUBLE_PUSH_WINDOW_MS,
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
        /// Essais abandonnés avant le niveau final, formatés `niveau:code`
        /// (jamais de contenu — RF-34) : la mesure honnête des replis
        /// (ex. ValuePattern → presse-papiers).
        attempts: Vec<String>,
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
    /// Durée maximale proche : avertissement 30 s avant la coupure (RF-11).
    MaxDurationWarning {
        remaining_ms: u64,
    },
    /// RF-32 (option « cible actuelle ») : la fenêtre mémorisée a disparu
    /// et l'insertion a repris dans la fenêtre au premier plan.
    RetargetedToCurrent,
    /// Demande mise en file d'attente (RF-05, file de 1) : elle démarrera
    /// automatiquement à la fin de la session en cours.
    QueuedAfterCurrent,
    /// Prise verrouillée par double appui (RF-02c) : elle survivra au
    /// relâchement jusqu'au prochain appui.
    RecordingLocked,
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
    /// Profil utilisé quand aucune liaison RF-25 ne correspond à la cible.
    pub profile: Profile,
    /// Profils par application (RF-25) — priorité dans l'ordre du tableau.
    pub profile_bindings: Vec<crate::types::ProfileBinding>,
    pub excluded_apps: Vec<String>,
    /// Code de langue forcé (`None` : détection automatique — RF-16).
    pub language: Option<String>,
    /// Repli quand la détection est interdite (énoncé < 1,5 s, RF-16).
    pub auto_language_fallback: Option<String>,
    /// Amorce de vocabulaire métier explicite (RF-24) ; sinon construite
    /// depuis le dictionnaire du profil.
    pub initial_prompt: Option<String>,
    /// Durée minimale en dessous de laquelle la détection de langue est
    /// interdite (RF-16). Défaut 1,5 s.
    pub auto_language_min: Duration,
    /// Mode de déclenchement (RF-02).
    pub trigger: TriggerMode,
}

impl Default for OrchestratorCfg {
    fn default() -> Self {
        Self {
            pre_roll_ms: 400,
            max_recording: Duration::from_secs(300),
            min_speech_ms: 250,
            remember_target_at_start: true,
            profile: Profile::default(),
            profile_bindings: Vec::new(),
            excluded_apps: Vec::new(),
            language: Some("fr".to_string()),
            auto_language_fallback: Some("fr".to_string()),
            initial_prompt: None,
            auto_language_min: Duration::from_millis(1_500),
            trigger: TriggerMode::default(),
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
    /// Profil résolu pour la session en cours (RF-25) — `None` hors session.
    session_profile: Option<Profile>,
    /// Mode (c) : prise verrouillée (RF-02).
    recording_locked: bool,
    /// Instant du dernier appui (détection du double appui, RF-02c).
    press_started: Option<Duration>,
    /// File d'attente de 1 (RF-05) : une demande pendant le pipeline.
    queued: bool,
    /// `true` une fois l'avertissement 30 s émis pour la prise en cours (RF-11).
    duration_warned: bool,
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
            session_profile: None,
            recording_locked: false,
            press_started: None,
            queued: false,
            duration_warned: false,
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

    /// Restaure le tampon après un rechargement de configuration (RF-01) :
    /// « coller à nouveau » doit rester fonctionnel — RF-22 exige qu'aucun
    /// rechargement ne perde le dernier texte.
    pub fn restore_buffer(&mut self, transcript: Transcript) {
        self.last = Some(transcript);
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
        match self.cfg.trigger {
            TriggerMode::Hold => match self.state.clone() {
                // RF-05 : appui pendant une session occupée → mise en file
                // (exercée de fait par le canal d'assemblage) ; le noyau
                // garde la même sémantique pour la découpe événementielle V1.
                DictationState::Idle | DictationState::Recovery { .. } => self.begin_recording(),
                DictationState::Recording { .. } => self.emit(CoreEvent::RejectedBusy),
                _ => {
                    self.queued = true;
                    self.emit(CoreEvent::QueuedAfterCurrent);
                }
            },
            TriggerMode::Toggle => match self.state.clone() {
                DictationState::Idle | DictationState::Recovery { .. } => self.begin_recording(),
                DictationState::Recording { .. } => self.finish_recording(),
                _ => {
                    self.queued = true;
                    self.emit(CoreEvent::QueuedAfterCurrent);
                }
            },
            TriggerMode::DoublePushLock => match self.state.clone() {
                DictationState::Idle | DictationState::Recovery { .. } => self.begin_recording(),
                DictationState::Recording { started_at, .. } => {
                    if self.push_within_window(started_at) {
                        // Double appui rapproché : verrouillage (RF-02c).
                        self.recording_locked = true;
                        self.emit(CoreEvent::RecordingLocked);
                    } else {
                        self.finish_recording();
                    }
                }
                _ => {
                    self.queued = true;
                    self.emit(CoreEvent::QueuedAfterCurrent);
                }
            },
        }
    }

    /// L'appui courant (candidat au double appui) est-il survenu dans la
    /// fenêtre `DOUBLE_PUSH_WINDOW_MS` suivant le précédent ?
    fn push_within_window(&self, started_at: Duration) -> bool {
        self.press_started
            .map(|p| self.now().saturating_sub(p) <= Duration::from_millis(DOUBLE_PUSH_WINDOW_MS))
            .unwrap_or_else(|| {
                self.now().saturating_sub(started_at)
                    <= Duration::from_millis(DOUBLE_PUSH_WINDOW_MS)
            })
    }

    fn on_released(&mut self) {
        if !matches!(self.state, DictationState::Recording { .. }) {
            return;
        }
        match self.cfg.trigger {
            TriggerMode::Hold => self.finish_recording(),
            // RF-02b : en bascule, le relâchement n'arrête rien (l'appui
            // suivant arrête).
            TriggerMode::Toggle => {}
            TriggerMode::DoublePushLock => {
                // Un appui MAINTENU au-delà de la fenêtre arrête la prise ;
                // un appui bref ou une prise verrouillée continue.
                let held_long = self
                    .press_started
                    .map(|p| {
                        self.now().saturating_sub(p) > Duration::from_millis(DOUBLE_PUSH_WINDOW_MS)
                    })
                    .unwrap_or(false);
                if !self.recording_locked && held_long {
                    self.finish_recording();
                }
            }
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
        // Profil par application (RF-25) : résolu **une fois par session**.
        self.session_profile = select_profile(&self.cfg.profile_bindings, &target.app_id).cloned();
        self.recording_locked = false;
        self.duration_warned = false;
        self.press_started = Some(self.now());
        self.set_state(DictationState::Recording {
            target,
            started_at: self.now(),
        });
    }

    fn on_cancel(&mut self) {
        match self.state.clone() {
            DictationState::Recording { started_at, .. } => {
                // Annulation explicite : l'audio capté est jeté.
                let _ = self.deps.capture.stop();
                self.recording_locked = false;
                self.queued = false;
                self.press_started = None;
                self.session_profile = None;
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
                self.queued = false;
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
                let profile = self
                    .session_profile
                    .take()
                    .unwrap_or_else(|| self.cfg.profile.clone());
                self.run_pipeline(job.target, job.audio, profile);
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
                let profile = self
                    .session_profile
                    .take()
                    .unwrap_or_else(|| self.cfg.profile.clone());
                self.insert_flow(t, target, profile);
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
                self.insert_flow(t, target, self.cfg.profile.clone());
            }
            _ => self.emit(CoreEvent::RejectedBusy),
        }
    }

    fn on_tick(&mut self) {
        if let DictationState::Recording { started_at, .. } = self.state.clone() {
            let elapsed = self.now().saturating_sub(started_at);
            if elapsed >= self.cfg.max_recording {
                // Durée maximale de sécurité : arrêt puis transcription (RF-11).
                self.finish_recording();
            } else if !self.duration_warned
                && self.cfg.max_recording > Duration::from_secs(30)
                && elapsed >= self.cfg.max_recording - Duration::from_secs(30)
            {
                // Avertissement 30 s avant la coupure de sécurité (RF-11).
                self.duration_warned = true;
                self.emit(CoreEvent::MaxDurationWarning {
                    remaining_ms: (self.cfg.max_recording.as_millis() - elapsed.as_millis()) as u64,
                });
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
        self.recording_locked = false;
        self.press_started = None;
        self.duration_warned = false;
        let profile = self
            .session_profile
            .take()
            .unwrap_or_else(|| self.cfg.profile.clone());
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
        self.run_pipeline(target, audio, profile);
    }

    /// Pipeline complet : VAD → transcription → post-traitement → insertion.
    /// Synchrone ; l'assemblage l'exécute sur le thread pipeline (ADR-12).
    fn run_pipeline(&mut self, target: FocusSnapshot, audio: AudioBuffer, profile: Profile) {
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
            self.serve_queue();
            return;
        }

        // --- Transcription (RF-14 : strictement locale par défaut) --------
        // RF-16 : la détection automatique de langue est interdite sur un
        // énoncé plus court que `auto_language_min` (défaut 1,5 s) — repli.
        let mut language = self.cfg.language.clone();
        if language.is_none() && audio.duration() < self.cfg.auto_language_min {
            language = self.cfg.auto_language_fallback.clone();
        }
        // RF-24 : amorce = champ explicite sinon les termes du dictionnaire
        // (les 64 premiers caractères — la fenêtre utile de whisper).
        let initial_prompt = self.cfg.initial_prompt.clone().or_else(|| {
            let terms = profile.dictionary.join(", ");
            (!terms.is_empty()).then(|| terms.chars().take(64).collect::<String>())
        });
        let opts = TranscribeOptions {
            language,
            initial_prompt,
            verbatim: profile.style == crate::types::Style::Verbatim,
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
                self.serve_queue();
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
        let text = match self.deps.post.process(&base.text, &profile) {
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
        self.insert_flow(transcript, target, profile);
    }

    /// Dispose de la file d'attente (RF-05) quand la session retombe à
    /// l'inactif : la demande en attente démarre immédiatement.
    fn serve_queue(&mut self) {
        if self.queued {
            self.queued = false;
            self.begin_recording();
        }
    }

    fn insert_flow(&mut self, transcript: Transcript, target: FocusSnapshot, profile: Profile) {
        // RF-32 : avec l'option « insérer dans la fenêtre actuelle » (défaut
        // `false`), la cible est **relue au moment d'insérer** — la fenêtre
        // mémorisée peut avoir disparu ; avec l'option mémorisation, la cible
        // de départ fait foi (refus → secours, chemin RF-31 testé ailleurs).
        let target = if !self.cfg.remember_target_at_start {
            match self.deps.focus.snapshot() {
                Some(fresh) if fresh.app_id != target.app_id || fresh.title != target.title => {
                    self.emit(CoreEvent::RetargetedToCurrent);
                    fresh
                }
                Some(fresh) => fresh,
                None => target,
            }
        } else {
            target
        };

        let t3 = self.now();
        self.set_state(DictationState::Inserting {
            target: target.clone(),
        });
        let plan = self.deps.inserter.plan(&target, &profile);
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
                    let attempts = report
                        .attempts
                        .iter()
                        .map(|(l, c)| format!("{}:{}", l.as_str(), c))
                        .collect();
                    self.emit(CoreEvent::Inserted { level, attempts });
                }
                self.emit(CoreEvent::SessionFinished {
                    success: true,
                    duration,
                    level,
                });
                self.serve_queue();
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
                self.serve_queue();
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
        // Détail SDK in-process : code dédié 8, message au journal si le
        // chemin applicatif en a besoin (le numéro seul ne suffit plus).
        crate::errors::AsrError::Runtime(_) => 8,
    }
}
