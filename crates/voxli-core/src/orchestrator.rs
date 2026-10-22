//! Dictation session orchestrator.
//!
//! The logic is **synchronous and pure**: it drives the ports (traits) and
//! emits events through an injected sink. Real threading (real-time
//! threads, pipeline, injection — ADR-12) belongs to the assembly layer
//! (`apps/desktop/src-tauri`).
//!
//! Invariants (docs/04 §3, verified by `tests/flow.rs`):
//! 1. the transcribed text is placed in the internal buffer **before** any
//!    insertion attempt (RF-22) and is never lost (RF-20);
//! 2. one active session at a time; a request while a session is busy is
//!    either explicitly refused with an indication (press during
//!    recording) or queued behind the current session (RF-05, queue
//!    depth 1);
//! 3. audio is released after insertion, refusal, or cancellation.
//!
//! In P0 the pipeline runs monolithically on the session thread; the
//! event-driven split into stages (V1, RF-05) will reuse these same states
//! without changing the ports.

use std::time::Duration;

use crate::errors::InsertError;
use crate::state::{DictationState, RecoveryReason};
use crate::traits::{Clock, SpeechVerdict, Stage};
use crate::types::{
    select_profile, AudioBuffer, FocusSnapshot, InsertLevel, Profile, RefuseReason,
    TranscribeOptions, Transcript, TriggerMode, DOUBLE_PUSH_WINDOW_MS,
};

/// Orchestrator input events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputEvent {
    HotkeyPressed,
    HotkeyReleased,
    Cancel,
    Retry,
    PasteAgain,
    Tick,
}

/// Output events. None carries text content, except `RescueRequested`: the
/// rescue window displays the text — that is its function (RF-31),
/// documented in the docs/05 §3 IPC table as the single, strictly local
/// exception.
#[derive(Clone, Debug, PartialEq)]
pub enum CoreEvent {
    StateChanged(DictationState),
    /// No usable speech: nothing gets inserted (RF-18).
    NothingToTranscribe,
    /// Request refused: busy session (RF-05) — visible indication.
    RejectedBusy,
    Inserted {
        level: InsertLevel,
        /// Attempts abandoned before the level finally used, formatted
        /// `level:code` (never content — RF-34): an honest measure of the
        /// fallbacks (e.g. ValuePattern → clipboard).
        attempts: Vec<String>,
    },
    /// Total insertion failure: rescue window (RF-28 level 4, RF-31).
    RescueRequested {
        transcript: Transcript,
    },
    /// Insertion refused: sensitive field, excluded app, or unavailable
    /// target (RF-29, RF-32) — neutral message, the text stays recoverable
    /// (RF-22).
    Refused {
        reason: RefuseReason,
    },
    /// Recoverable failure: audio kept, "retry" available (RF-20).
    Recovered {
        reason: RecoveryReason,
    },
    /// Maximum duration near: warning 30 s before the cutoff (RF-11).
    MaxDurationWarning {
        remaining_ms: u64,
    },
    /// RF-32 (option "current target"): the focused window differs from the
    /// remembered one at insert time — insertion resumed into the current
    /// foreground window.
    RetargetedToCurrent,
    /// Request queued (RF-05, queue of 1): it will start automatically when
    /// the current session ends.
    QueuedAfterCurrent,
    /// Recording locked by double push (RF-02c): it will survive the
    /// release until the next press.
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
    /// Technical error (stable codes, no content — RF-34).
    Error {
        module: &'static str,
        code: u32,
    },
}

/// Real-time (monotonic) clock for production.
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

/// Test clock: advanced manually.
pub struct TestClock(pub std::cell::Cell<u64>);

impl Clock for TestClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.get())
    }
}

/// Orchestrator configuration; default values come from `voxli-config`
/// (docs/05 §4).
#[derive(Clone, Debug)]
pub struct OrchestratorCfg {
    pub pre_roll_ms: u32,
    pub max_recording: Duration,
    pub min_speech_ms: u32,
    pub remember_target_at_start: bool,
    /// Profile used when no RF-25 binding matches the target.
    pub profile: Profile,
    /// Per-application profiles (RF-25) — priority follows array order.
    pub profile_bindings: Vec<crate::types::ProfileBinding>,
    pub excluded_apps: Vec<String>,
    /// Forced language code (`None`: automatic detection — RF-16).
    pub language: Option<String>,
    /// Fallback when detection is forbidden (utterance < 1.5 s, RF-16).
    pub auto_language_fallback: Option<String>,
    /// Explicit domain-vocabulary primer (RF-24); otherwise built from the
    /// profile dictionary.
    pub initial_prompt: Option<String>,
    /// Minimum duration under which language detection is forbidden (RF-16).
    /// Default 1.5 s.
    pub auto_language_min: Duration,
    /// Trigger mode (RF-02).
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

/// Injected dependencies. The kernel knows none beyond its trait.
pub struct Deps<'a> {
    pub capture: &'a mut dyn crate::traits::Capture,
    pub vad: &'a dyn crate::traits::VoiceActivity,
    pub transcriptor: &'a dyn crate::traits::Transcriptor,
    pub post: &'a dyn crate::traits::PostProcessor,
    pub inserter: &'a mut dyn crate::traits::Inserter,
    pub focus: &'a dyn crate::traits::FocusReader,
}

/// Job kept after a recoverable failure (RF-20): the capture audio is kept
/// until "retry" or cancellation (invariant 3).
struct RecoveryJob {
    target: FocusSnapshot,
    audio: AudioBuffer,
}

pub struct Orchestrator<'a> {
    cfg: OrchestratorCfg,
    deps: Deps<'a>,
    state: DictationState,
    /// Internal buffer of the last transcribed text (RF-22) — never lost.
    last: Option<Transcript>,
    recovering: Option<RecoveryJob>,
    /// Profile resolved for the current session (RF-25) — `None` outside a session.
    session_profile: Option<Profile>,
    /// Mode (c): recording locked (RF-02).
    recording_locked: bool,
    /// Instant of the last press (double-push detection, RF-02c).
    press_started: Option<Duration>,
    /// Queue of 1 (RF-05): a request arriving during the pipeline.
    queued: bool,
    /// `true` once the 30 s warning has been emitted for the current capture (RF-11).
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

    /// Text from the internal buffer — readable even after an insertion
    /// failure (RF-22). The rescue window and "paste again" use it.
    pub fn last_transcript(&self) -> Option<&Transcript> {
        self.last.as_ref()
    }

    /// Restores the buffer after a configuration reload (RF-01):
    /// "paste again" must keep working — RF-22 requires that no reload ever
    /// lose the last text.
    pub fn restore_buffer(&mut self, transcript: Transcript) {
        self.last = Some(transcript);
    }

    /// Is audio still held (not released)? Invariant 3.
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
                // RF-05: a press while a session is busy is queued, except
                // while still recording (explicit refusal). P0 effectively
                // exercises this path whenever the assembly channel delivers
                // a press; the core keeps the same semantics for the V1
                // event-driven split.
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
                        // Tight double push: lock (RF-02c).
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

    /// Did the current press (double-push candidate) land within
    /// `DOUBLE_PUSH_WINDOW_MS` after the previous one?
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
            // RF-02b: in toggle mode the release stops nothing (the next
            // press stops).
            TriggerMode::Toggle => {}
            TriggerMode::DoublePushLock => {
                // A press HELD past the window stops the capture; a brief
                // press or a locked recording continues.
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
        // Per-application profile (RF-25): resolved **once per session**.
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
                // Explicit cancellation: the captured audio is discarded.
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
                // Invariant 3: the recovery audio is released.
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
                // "Retry" after an insertion failure = re-insert the last
                // text (never lost, RF-22), toward the current target.
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
                // Safety maximum duration: stop, then transcribe (RF-11).
                self.finish_recording();
            } else if !self.duration_warned
                && self.cfg.max_recording > Duration::from_secs(30)
                && elapsed >= self.cfg.max_recording - Duration::from_secs(30)
            {
                // Warning 30 s before the safety cutoff (RF-11).
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
                // A request queued during the dictation must be served even
                // if stopping the capture fails (otherwise it would stay
                // stuck until the next session).
                self.serve_queue();
                return;
            }
        };
        self.set_state(DictationState::Finalizing {
            target: target.clone(),
        });
        self.run_pipeline(target, audio, profile);
    }

    /// Full pipeline: VAD → transcription → post-processing → insertion.
    /// Synchronous; the assembly layer runs it on the pipeline thread (ADR-12).
    fn run_pipeline(&mut self, target: FocusSnapshot, audio: AudioBuffer, profile: Profile) {
        let t0 = self.now();

        // --- VAD analysis (RF-18) -----------------------------------------
        let verdict = self.deps.vad.contains_speech(&audio);
        self.emit(CoreEvent::Latency {
            stage: Stage::Vad,
            micros: self.since(t0),
        });
        if verdict != SpeechVerdict::Speech {
            // Nothing gets inserted; the audio is released (invariant 3).
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

        // --- Transcription (RF-14: strictly local by default) --------
        // RF-16: automatic language detection is forbidden for an utterance
        // shorter than `auto_language_min` (default 1.5 s) — the fallback
        // applies.
        let mut language = self.cfg.language.clone();
        if language.is_none() && audio.duration() < self.cfg.auto_language_min {
            language = self.cfg.auto_language_fallback.clone();
        }
        // RF-24: primer = the explicit field, else the dictionary target
        // terms (up to 512 characters — Whisper's useful context window).
        let initial_prompt = self.cfg.initial_prompt.clone().or_else(|| {
            if profile.dictionary.is_empty() {
                None
            } else {
                let targets: Vec<&str> = profile
                    .dictionary
                    .iter()
                    .filter_map(|e| e.split('|').next().map(|s| s.trim()))
                    .filter(|s| !s.is_empty())
                    .collect();
                if targets.is_empty() {
                    None
                } else {
                    let terms = targets.join(", ");
                    Some(terms.chars().take(512).collect::<String>())
                }
            }
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
                // RF-20: the audio is kept, the text is not lost.
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
        drop(audio); // the audio is no longer needed after transcription
        self.set_state(DictationState::Transcribing {
            target: target.clone(),
            queued: None,
        });

        // --- Local post-processing level 0/1 (RF-26) --------------------
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

        // --- Insertion (RF-22: the text is in the buffer BEFORE the attempt) --
        self.last = Some(transcript.clone());
        self.insert_flow(transcript, target, profile);
    }

    /// Serves the queue (RF-05) when the session falls back to idle: the
    /// pending request starts immediately.
    fn serve_queue(&mut self) {
        if self.queued {
            self.queued = false;
            self.begin_recording();
        }
    }

    fn insert_flow(&mut self, transcript: Transcript, target: FocusSnapshot, profile: Profile) {
        // RF-32: with the "insert into the current window" option (default
        // `false`), the target is **re-read at insert time** — the memorized
        // window may have gone away; with the remember-at-start option, the
        // target captured at start prevails (all levels refused → rescue,
        // the RF-31 path tested elsewhere).
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
                // Text kept in the buffer (RF-22); neutral message (RF-29).
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
                // RF-28 level 4 / RF-31: rescue window, text intact.
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

/// Stable codes written to the logs (RF-34) — public so the assembly layer
/// logs the same codes as the core.
pub fn capture_code(e: &crate::errors::CaptureError) -> u32 {
    match e {
        crate::errors::CaptureError::NoDevice => 1,
        crate::errors::CaptureError::DeviceBusy => 2,
        crate::errors::CaptureError::DeviceLost => 3,
        crate::errors::CaptureError::StreamStart => 4,
        crate::errors::CaptureError::NotStarted => 5,
        crate::errors::CaptureError::UnsupportedFormat => 6,
        crate::errors::CaptureError::NoData => 7,
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
        // In-process SDK detail: dedicated code 8, message to the log when
        // the application path needs it (the bare number no longer suffices).
        crate::errors::AsrError::Runtime(_) => 8,
    }
}
