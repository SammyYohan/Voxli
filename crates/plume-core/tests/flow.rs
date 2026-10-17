//! Tests de la machine à états avec doubles de test (docs/06 §1 : intégration
//! noyau — succès, silence, erreur moteur, échec d'insertion, focus perdu,
//! champ sensible, durée maximale). Aucun matériel ni Windows requis.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use plume_core::*;

// ---------------------------------------------------------------- fakes ----

#[derive(Default)]
struct FakeCapture {
    started: bool,
    /// 0 = silence, 1 = parole synthétique
    content: u8,
}

impl Capture for FakeCapture {
    fn start(&mut self, _pre_roll_ms: u32) -> Result<(), CaptureError> {
        self.started = true;
        Ok(())
    }
    fn stop(&mut self) -> Result<AudioBuffer, CaptureError> {
        if !self.started {
            return Err(CaptureError::NotStarted);
        }
        self.started = false;
        Ok(match self.content {
            1 => AudioBuffer::new(vec![0.5; 16_000], 16_000),
            _ => AudioBuffer::new(vec![0.0; 16_000], 16_000),
        })
    }
    fn level(&self) -> Level {
        0.5
    }
}

struct FixedVad(SpeechVerdict);

impl VoiceActivity for FixedVad {
    fn contains_speech(&self, _a: &AudioBuffer) -> SpeechVerdict {
        self.0
    }
}

struct FakeEngine {
    /// Partagé pour basculer en cours de test (panne puis rétablissement).
    /// `Arc` (et non `Rc`) : le trait `Transcriptor` exige `Send + Sync`.
    fail: Arc<AtomicBool>,
    calls: Arc<AtomicU32>,
    text: &'static str,
}

impl Transcriptor for FakeEngine {
    fn warmup(&self) -> Result<(), AsrError> {
        Ok(())
    }
    fn transcribe(&self, _a: &AudioBuffer, _o: &TranscribeOptions) -> Result<Transcript, AsrError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.fail.load(Ordering::SeqCst) {
            return Err(AsrError::Inference);
        }
        Ok(Transcript {
            text: self.text.to_string(),
            confidence: 0.9,
            duration: Duration::from_millis(500),
            engine: EngineId::Fake,
            model: None,
        })
    }
    fn id(&self) -> EngineId {
        EngineId::Fake
    }
}

/// Post-traitement identité (niveau 0, défaut RF-26).
struct IdentityPost;

impl PostProcessor for IdentityPost {
    fn process(&self, text: &str, _p: &Profile) -> Result<String, PostError> {
        Ok(text.to_string())
    }
}

struct FakeFocus(Option<FocusSnapshot>);

impl FocusReader for FakeFocus {
    fn snapshot(&self) -> Option<FocusSnapshot> {
        self.0.clone()
    }
}

struct FakeInserter {
    /// Exclusion côté test (simule la liste de plume-config).
    excluded: Vec<String>,
    fail_all: Rc<Cell<bool>>,
    inserts: Rc<RefCell<Vec<String>>>,
}

impl Inserter for FakeInserter {
    fn plan(&self, target: &FocusSnapshot, _p: &Profile) -> InsertPlan {
        if target.is_sensitive(&self.excluded) {
            let reason = if target.caps.is_password {
                RefuseReason::SensitiveField
            } else {
                RefuseReason::ExcludedApp
            };
            return InsertPlan::refused(reason);
        }
        if !target.valid {
            return InsertPlan::refused(RefuseReason::TargetUnavailable);
        }
        InsertPlan::ordered(vec![
            InsertLevel::ValuePattern,
            InsertLevel::Clipboard,
            InsertLevel::SendInput,
            InsertLevel::Rescue,
        ])
    }
    fn insert(&self, text: &str, plan: &InsertPlan) -> Result<InsertReport, InsertError> {
        if let Some(reason) = plan.refuse {
            return Err(match reason {
                RefuseReason::SensitiveField => InsertError::RefusedSensitive,
                RefuseReason::ExcludedApp => InsertError::RefusedExcluded,
                RefuseReason::TargetUnavailable => InsertError::TargetUnavailable,
            });
        }
        if self.fail_all.get() {
            return Err(InsertError::AllLevelsFailed("test".into()));
        }
        self.inserts.borrow_mut().push(text.to_string());
        Ok(InsertReport {
            level_used: Some(InsertLevel::ValuePattern),
            attempts: vec![],
        })
    }
}

// ---------------------------------------------------------------- glue -----

fn snapshot() -> FocusSnapshot {
    FocusSnapshot {
        app_id: "notepad.exe".into(),
        title: "Sans titre — Bloc-notes".into(),
        caps: Default::default(),
        elevated: false,
        valid: true,
    }
}

struct SharedClock(Rc<Cell<u64>>);

impl Clock for SharedClock {
    fn now(&self) -> Duration {
        Duration::from_millis(self.0.get())
    }
}

struct Bench {
    capture: FakeCapture,
    vad: SpeechVerdict,
    engine: FakeEngine,
    inserter: FakeInserter,
    focus: FakeFocus,
}

fn bench(vad: SpeechVerdict) -> Bench {
    Bench {
        capture: FakeCapture {
            content: 1,
            ..Default::default()
        },
        vad,
        engine: FakeEngine {
            fail: Arc::new(AtomicBool::new(false)),
            calls: Arc::new(AtomicU32::new(0)),
            text: "Compte rendu de réunion.",
        },
        inserter: FakeInserter {
            excluded: Vec::new(),
            fail_all: Rc::new(Cell::new(false)),
            inserts: Rc::new(RefCell::new(Vec::new())),
        },
        focus: FakeFocus(Some(snapshot())),
    }
}

/// Exécute un scénario et restitue les événements émis, le tampon interne,
/// l'état final. Les compteurs restent accessibles via les `Rc` du banc.
fn run(
    b: Bench,
    script: impl FnOnce(&mut Orchestrator),
) -> (Vec<CoreEvent>, Option<Transcript>, DictationState) {
    run_with_cfg(OrchestratorCfg::default(), b, script)
}

fn run_with_cfg(
    cfg: OrchestratorCfg,
    b: Bench,
    script: impl FnOnce(&mut Orchestrator),
) -> (Vec<CoreEvent>, Option<Transcript>, DictationState) {
    let events: Rc<RefCell<Vec<CoreEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let mut capture = b.capture;
    let vad = FixedVad(b.vad);
    let engine = b.engine;
    let post = IdentityPost;
    let mut inserter = b.inserter;
    let focus = b.focus;
    let clock = SharedClock(Rc::new(Cell::new(0)));

    let sink_events = events.clone();
    let mut orch = Orchestrator::new(
        cfg,
        Deps {
            capture: &mut capture,
            vad: &vad,
            transcriptor: &engine,
            post: &post,
            inserter: &mut inserter,
            focus: &focus,
        },
        Box::new(move |e| sink_events.borrow_mut().push(e)),
        &clock,
    );
    script(&mut orch);
    let last = orch.last_transcript().cloned();
    let state = orch.state().clone();
    let evs = events.borrow().clone();
    (evs, last, state)
}

fn states_of(events: &[CoreEvent]) -> Vec<&'static str> {
    events
        .iter()
        .filter_map(|e| match e {
            CoreEvent::StateChanged(s) => Some(s.name()),
            _ => None,
        })
        .collect()
}

fn find<'e>(events: &'e [CoreEvent], f: impl Fn(&CoreEvent) -> bool) -> Option<&'e CoreEvent> {
    events.iter().find(|e| f(e))
}

// --------------------------------------------------------------- tests ----

/// Flux nominal : appui → enregistrement → relâchement → transcription →
/// insertion (niveau 1 : ValuePattern).
#[test]
fn t_nominal_insertion() {
    let b = bench(SpeechVerdict::Speech);
    let (events, last, state) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
    });
    assert!(states_of(&events).contains(&"recording"));
    assert!(states_of(&events).contains(&"transcribing"));
    assert!(states_of(&events).contains(&"inserting"));
    assert_eq!(state, DictationState::Idle);
    assert!(matches!(
        find(&events, |e| matches!(e, CoreEvent::Inserted { .. })),
        Some(CoreEvent::Inserted {
            level: InsertLevel::ValuePattern
        })
    ));
    assert_eq!(
        last.map(|t| t.text),
        Some("Compte rendu de réunion.".to_string())
    );
}

/// Silence total : aucune insertion (RF-18), audio libéré (invariant 3).
#[test]
fn t_silence_produces_no_insertion() {
    let b = bench(SpeechVerdict::Silence);
    let (events, last, state) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
    });
    assert!(find(&events, |e| matches!(e, CoreEvent::NothingToTranscribe)).is_some());
    assert!(!states_of(&events).contains(&"transcribing"));
    assert_eq!(state, DictationState::Idle);
    assert!(last.is_none());
}

/// Énoncé trop court (RF-18 : seuil de durée minimale).
#[test]
fn t_too_short_rejected() {
    let b = bench(SpeechVerdict::TooShort);
    let (events, last, _) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
    });
    assert!(find(&events, |e| matches!(e, CoreEvent::NothingToTranscribe)).is_some());
    assert!(last.is_none());
}

/// Erreur moteur : état récupérable, audio conservé, « réessayer » aboutit
/// (RF-20, RF-21). Le texte n'est jamais perdu.
#[test]
fn t_engine_error_then_retry() {
    let b = bench(SpeechVerdict::Speech);
    let fail = b.engine.fail.clone();
    let calls = b.engine.calls.clone();
    let (events, last, state) = run(b, |o| {
        fail.store(true, Ordering::SeqCst);
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
        assert!(
            o.holds_audio(),
            "l'audio doit être conservé après erreur moteur"
        );
        assert!(o.last_transcript().is_none());
        fail.store(false, Ordering::SeqCst);
        o.handle(InputEvent::Retry);
    });
    assert!(find(&events, |e| {
        matches!(
            e,
            CoreEvent::Recovered {
                reason: RecoveryReason::EngineError
            }
        )
    })
    .is_some());
    assert!(matches!(
        events
            .iter()
            .rev()
            .find(|e| matches!(e, CoreEvent::Inserted { .. })),
        Some(CoreEvent::Inserted { .. })
    ));
    assert_eq!(state, DictationState::Idle);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "deux appels moteur : échec puis réessai"
    );
    assert!(last.is_some());
}

/// Échec total d'insertion : fenêtre de secours avec texte intact (RF-28 n.4,
/// RF-31) ; le texte reste au tampon (RF-22) et « réessayer » réinsère.
#[test]
fn t_insert_failure_rescue_then_retry() {
    let b = bench(SpeechVerdict::Speech);
    let fail_all = b.inserter.fail_all.clone();
    let inserts = b.inserter.inserts.clone();
    let (events, last, state) = run(b, |o| {
        fail_all.set(true);
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
        assert_eq!(o.state().name(), "recovery");
        // Le texte est dans le tampon AVANT la tentative d'insertion (RF-22).
        assert!(o.last_transcript().is_some());
        fail_all.set(false);
        o.handle(InputEvent::Retry);
    });
    match find(&events, |e| matches!(e, CoreEvent::RescueRequested { .. })) {
        Some(CoreEvent::RescueRequested { transcript }) => {
            assert_eq!(transcript.text, "Compte rendu de réunion.");
        }
        other => panic!("un RescueRequested doit être émis, obtenu : {other:?}"),
    }
    assert_eq!(state, DictationState::Idle);
    assert_eq!(inserts.borrow().len(), 1, "la réinsertion a abouti");
    assert!(last.is_some());
}

/// Champ de mot de passe : insertion refusée, message neutre, texte conservé
/// au tampon (RF-29, RF-22).
#[test]
fn t_sensitive_field_refused() {
    let b = bench(SpeechVerdict::Speech);
    let inserts = b.inserter.inserts.clone();
    let mut snap = snapshot();
    snap.caps.is_password = true;
    let (events, last, state) = run(
        Bench {
            focus: FakeFocus(Some(snap)),
            ..b
        },
        |o| {
            o.handle(InputEvent::HotkeyPressed);
            o.handle(InputEvent::HotkeyReleased);
        },
    );
    assert!(find(&events, |e| {
        matches!(
            e,
            CoreEvent::Refused {
                reason: RefuseReason::SensitiveField
            }
        )
    })
    .is_some());
    assert!(
        inserts.borrow().is_empty(),
        "aucune insertion dans un champ sensible"
    );
    assert_eq!(state, DictationState::Idle);
    assert!(last.is_some(), "le texte reste récupérable (RF-22)");
}

/// Application exclue : insertion refusée, même cheminement (RF-29).
#[test]
fn t_excluded_app_refused() {
    let b = bench(SpeechVerdict::Speech);
    let inserts = b.inserter.inserts.clone();
    let mut snap = snapshot();
    snap.app_id = "keepassxc.exe".into();
    let (events, _, state) = run(
        Bench {
            focus: FakeFocus(Some(snap)),
            inserter: FakeInserter {
                excluded: vec!["keepassxc.exe".to_string()],
                ..b.inserter
            },
            ..b
        },
        |o| {
            o.handle(InputEvent::HotkeyPressed);
            o.handle(InputEvent::HotkeyReleased);
        },
    );
    assert!(find(&events, |e| {
        matches!(
            e,
            CoreEvent::Refused {
                reason: RefuseReason::ExcludedApp
            }
        )
    })
    .is_some());
    assert!(inserts.borrow().is_empty());
    assert_eq!(state, DictationState::Idle);
}

/// Fenêtre cible perdue : cible invalide au moment du démarrage → refus
/// immédiat, aucune capture (RF-32).
#[test]
fn t_no_focus_at_press() {
    let b = bench(SpeechVerdict::Speech);
    let (events, _, state) = run(
        Bench {
            focus: FakeFocus(None),
            ..b
        },
        |o| {
            o.handle(InputEvent::HotkeyPressed);
        },
    );
    assert!(find(&events, |e| {
        matches!(
            e,
            CoreEvent::Refused {
                reason: RefuseReason::TargetUnavailable
            }
        )
    })
    .is_some());
    assert_eq!(state, DictationState::Idle);
}

/// Fenêtre mémorisée invalide au moment d'insérer : refus explicite (RF-32).
#[test]
fn t_target_gone_at_insert() {
    let b = bench(SpeechVerdict::Speech);
    let mut snap = snapshot();
    snap.valid = false;
    let (events, last, _) = run(
        Bench {
            focus: FakeFocus(Some(snap)),
            ..b
        },
        |o| {
            o.handle(InputEvent::HotkeyPressed);
            o.handle(InputEvent::HotkeyReleased);
        },
    );
    assert!(find(&events, |e| {
        matches!(
            e,
            CoreEvent::Refused {
                reason: RefuseReason::TargetUnavailable
            }
        )
    })
    .is_some());
    assert!(last.is_some(), "texte récupérable malgré le refus (RF-22)");
}

/// Durée maximale de sécurité : un Tick au-delà du plafond finalise et
/// transcrit normalement (RF-11).
#[test]
fn t_max_duration_auto_finalize() {
    let b = bench(SpeechVerdict::Speech);
    let clock_base = Rc::new(Cell::new(0u64));
    let events: Rc<RefCell<Vec<CoreEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let mut capture = b.capture;
    let vad = FixedVad(b.vad);
    let engine = b.engine;
    let post = IdentityPost;
    let mut inserter = b.inserter;
    let focus = b.focus;
    let sink_events = events.clone();
    let clock = SharedClock(clock_base.clone());
    let mut orch = Orchestrator::new(
        OrchestratorCfg::default(),
        Deps {
            capture: &mut capture,
            vad: &vad,
            transcriptor: &engine,
            post: &post,
            inserter: &mut inserter,
            focus: &focus,
        },
        Box::new(move |e| sink_events.borrow_mut().push(e)),
        &clock,
    );
    orch.handle(InputEvent::HotkeyPressed);
    clock_base.set(301_000); // 301 s
    orch.handle(InputEvent::Tick);
    let evs = events.borrow().clone();
    assert!(states_of(&evs).contains(&"finalizing"));
    assert!(matches!(
        evs.iter()
            .rev()
            .find(|e| matches!(e, CoreEvent::Inserted { .. })),
        Some(CoreEvent::Inserted { .. })
    ));
    assert_eq!(orch.state().name(), "idle");
}

/// Appui pendant une session en récupération : une nouvelle dictée démarre
/// (l'audio de récupération est conservé) ; re-appui en enregistrement :
/// refus explicite (RF-05, P0).
#[test]
fn t_press_while_busy_rejected() {
    let b = bench(SpeechVerdict::Speech);
    let fail = b.engine.fail.clone();
    let (events, _, _state) = run(b, |o| {
        fail.store(true, Ordering::SeqCst);
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
        assert_eq!(o.state().name(), "recovery");
        fail.store(false, Ordering::SeqCst);
        o.handle(InputEvent::HotkeyPressed);
        assert_eq!(o.state().name(), "recording");
        o.handle(InputEvent::HotkeyPressed); // déjà en enregistrement
        assert_eq!(o.state().name(), "recording");
    });
    assert!(find(&events, |e| matches!(e, CoreEvent::RejectedBusy)).is_some());
}

/// Annulation pendant l'enregistrement : audio jeté, aucune insertion.
#[test]
fn t_cancel_discards_audio() {
    let b = bench(SpeechVerdict::Speech);
    let inserts = b.inserter.inserts.clone();
    let (events, last, state) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::Cancel);
    });
    assert_eq!(state, DictationState::Idle);
    assert!(inserts.borrow().is_empty());
    assert!(last.is_none());
    assert!(!states_of(&events).contains(&"transcribing"));
}

/// « Coller à nouveau » depuis l'état inactif : réinsère le dernier texte
/// (RF-04, RF-22).
#[test]
fn t_paste_again_reinserts_last() {
    let b = bench(SpeechVerdict::Speech);
    let inserts = b.inserter.inserts.clone();
    let (_, last, _) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
        assert_eq!(inserts.borrow().len(), 1);
        o.handle(InputEvent::PasteAgain);
    });
    assert_eq!(
        inserts.borrow().len(),
        2,
        "le même texte est inséré à nouveau"
    );
    assert_eq!(inserts.borrow()[0], inserts.borrow()[1]);
    assert!(last.is_some());
}

/// Annulation d'une récupération : l'audio est libéré (invariant 3).
#[test]
fn t_cancel_recovery_releases_audio() {
    let b = bench(SpeechVerdict::Speech);
    let fail = b.engine.fail.clone();
    let (_, _, state) = run(b, |o| {
        fail.store(true, Ordering::SeqCst);
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
        assert!(o.holds_audio());
        o.handle(InputEvent::Cancel);
        assert!(!o.holds_audio());
    });
    assert_eq!(state, DictationState::Idle);
}

/// Chaînes sentinelles (motif T-34) : les événements du noyau ne contiennent
/// jamais le texte dicté, sauf RescueRequested (fonction de la fenêtre de
/// secours, exception documentée).
#[test]
fn t_events_never_carry_content() {
    const SENTINELLE: &str = "SENTINELLE-7f3b-secret";
    let b = Bench {
        engine: FakeEngine {
            fail: Arc::new(AtomicBool::new(false)),
            calls: Arc::new(AtomicU32::new(0)),
            text: SENTINELLE,
        },
        ..bench(SpeechVerdict::Speech)
    };
    let (events, _, _) = run(b, |o| {
        o.handle(InputEvent::HotkeyPressed);
        o.handle(InputEvent::HotkeyReleased);
    });
    for ev in &events {
        if matches!(ev, CoreEvent::RescueRequested { .. }) {
            continue; // exception documentée (fenêtre de secours)
        }
        let rendered = format!("{ev:?}");
        assert!(
            !rendered.contains(SENTINELLE),
            "fuite de contenu détectée dans un événement du noyau"
        );
    }
}
