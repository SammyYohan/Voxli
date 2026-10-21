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
//! - `meter://level` `{ level, atMs }` — vumètre, 1 échantillon sur 2 ticks
//!   (~100 ms) : addition documentée à la table (RF-09) ;
//! - `session://finished` `{ success, durationMs, level, atMs }` ;
//! - `notice://notice` `{ kind, message }` — **addition documentée** à la
//!   table : messages FR neutres définis dans `strings.rs`, jamais de contenu ;
//! - `dictation://partial` `{ text, atMs }` — **addition documentée** (amendement
//!   ADR-02) : hypothèses transitoires du streaming pendant une prise
//!   (Nemotron P2, ou re-décodage fenêtré whisper — `whisper_partial`) :
//!   pur affichage, jamais journalisées ni conservées (RF-34).

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

// Mise à jour du bandeau doc : la pastille (« pill ») et le vumètre sont
// émis depuis ce thread après tout (RF-09).

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
    /// RF-01 : la configuration a été validée et sauvée ; le pipeline
    /// reconstruit son orchestrateur (raccourcis re-registrés côté
    /// principal) — sans redémarrer l'application.
    ConfigChanged(Box<Config>),
    /// RF-15 : un modèle vient d'être installé sur le disque. Le worker
    /// réévalue la pertinence du moteur chargé (`engine_stale`) sans annuler
    /// une dictée en cours : le rechargement attend la fin de la prise en
    /// cours dans ce cas (jamais de perte, RF-20).
    ModelInstalled,
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

/// Proxy de capture (RF-08/RF-09/RF-10) : l'orchestrateur tient
/// `&mut dyn Capture` en exclusivité, mais le vumètre et la détection de
/// déconnexion sont lus **hors orchestrateur** à intervalles réguliers. Le
/// proxy passe par un verrou bref (une lecture de niveau ≈ quelques
/// centaines d'octets, aucune allocation).
struct SharedCapture(
    Arc<std::sync::Mutex<Box<dyn Capture + Send>>>,
    Arc<StreamSlot>,
);

impl SharedCapture {
    fn new(inner: Box<dyn Capture + Send>, stream: Arc<StreamSlot>) -> Self {
        Self(Arc::new(std::sync::Mutex::new(inner)), stream)
    }

    /// Deuxième clé du même compartiment : vumètre + capteur de déconnexion.
    fn inner_arc(&self) -> Arc<std::sync::Mutex<Box<dyn Capture + Send>>> {
        Arc::clone(&self.0)
    }
}

impl Capture for SharedCapture {
    fn start(&mut self, pre_roll_ms: u32) -> Result<(), CaptureError> {
        // Session périmée éventuelle (prise muette : le VAD a refusé la
        // transcription) : fermeture **bornée** — le travail FFI vit dans
        // le pilote ; au-delà de la borne, on passe (voie batch restera
        // l'autorité). Son texte éventuel est abandonné (prise muette =
        // rien à insérer).
        let _ = close_stream_bornee(&self.1, std::time::Duration::from_millis(600));
        let started = lock_or_recover(&self.0).start(pre_roll_ms);
        if started.is_ok() {
            // Session d'affichage ouverte via le pilote, sans attente
            // (l'ouverture peut être lente, voire suspendue — l'émission
            // des partiels suivra ; la voie batch n'en dépend jamais).
            // Les tranches déjà tombées dans `feed` sont poussées au
            // premier Pump du pilote.
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
        // La prise audio est close : le transcriptor finalisera la session
        // (le texte batch reste l'autorité) ; en cas de prise muette (VAD
        // refuse), le prochain start ferme aussi via close_stream_bornee.
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

/// Affichage partiel (addition documentée, docs/05 §3) : hypothèses
/// transitoires du streaming pendant une prise — pur affichage, jamais
/// journalisées ni conservées (RF-34, amendement ADR-02).
#[derive(Clone, Serialize)]
struct PartialPayload {
    text: String,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

/// Commandes du pilote de session streaming. Le contrat SDK (« un flux
/// est piloté par UN SEUL thread ») est respecté **par construction** :
/// ce thread est le seul à toucher aux FFI du moteur. Incident du
/// 18/10/2026 : un appel du SDK suspendu figeait le thread worker entier
/// — plus de tick, plus de relâchement, plus de garde de durée (RF-11) ;
/// la prise restait en « recording » et l'application semblait morte.
/// Depuis : **le thread worker n'attend jamais un appel natif** — le
/// pilote absorbe les coups, la voie batch (ADR-02) reste l'autorité.
enum StreamCmd {
    /// Ouvre une session (fabrique + langue + verbatim **clonés** dans le
    /// message : rien ne référence le thread appelant).
    Open {
        engine: Arc<dyn StreamingEngine>,
        language: Option<String>,
        verbatim: bool,
    },
    /// Pousse le résidu courant du robinet vers la session puis draine
    /// les mises à jour (émission des partiels faite ici — ce thread peut
    /// se suspendre sans dommage).
    Pump,
    /// Fin de prise : résiduel, finish, aspiration bornée, fermeture ;
    /// le résultat est écrit dans le slot (consommé par le transcriptor
    /// sous borne).
    Close,
}

/// Câblage streaming d'une prise. Le robinet de capture (thread temps
/// réel) ne touche qu'au tampon de tranches `feed` (try_lock : une tranche
/// peut être sacrifiée — le texte définitif, voie batch, n'en dépend
/// jamais). Le reste (session, push, drain, fermeture) vit **entièrement
/// dans le thread pilote** (`stream-driver`) ; le worker ne lui parle
/// que par des commandes non bloquantes.
struct StreamSlot {
    /// Fabrique de session (moteur streaming capable) ; recopiée à chaque
    /// rebuild moteur ; vide quand le moteur n'en a pas.
    engine: std::sync::Mutex<Option<Arc<dyn StreamingEngine>>>,
    /// Langue + verbatim de la config en service (recopiés au rebuild).
    language: std::sync::Mutex<Option<String>>,
    verbatim: AtomicBool,
    /// File du pilote : toute l'accès session passe par là (send jamais
    /// bloquant — canalisé non borné, le pilote consomme à son rythme).
    tx: std::sync::mpsc::Sender<StreamCmd>,
    /// Présence d'une session au pilote (donnée de diagnostic sans FFI).
    has_session: AtomicBool,
    /// Acquittal de la dernière fermeture (borne côté worker).
    close_done: AtomicBool,
    /// Résultat finalisé de la dernière prise, écrit par le pilote.
    outcome: std::sync::Mutex<Option<StreamOutcome>>,
    /// Posé par `SharedCapture::stop` : la prise est finie côté capture.
    end: AtomicBool,
    /// Tranches mono du robinet, préallouées (clear sans désallocation).
    feed: std::sync::Mutex<Vec<f32>>,
    /// Débit du robinet (fixé à la première tranche du périphérique).
    rate: AtomicU32,
    /// Diagnostics chiffrés (nombres seulement, jamais de contenu — RF-34) :
    /// échantillons avalés du robinet et mises à jour produites par la
    /// session, consommés par le tick de diagnostic (~1 Hz pendant la prise).
    fed: std::sync::atomic::AtomicU64,
    updates_seen: std::sync::atomic::AtomicU64,
    /// Journal (échecs documentés, jamais silencieux).
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

    /// Fermeture capotée (config/verbatim recopiés au rebuild moteur).
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

/// Résultat de la voie d'affichage : le texte que la session streaming a
/// déjà finalisé pendant la prise, avec sa confiance moyenne. Réutilisable
/// comme transcription — l'audio a déjà été décodé, aucun re-décodage.
struct StreamOutcome {
    text: String,
    confidence: f32,
}

/// Fermeture de la session **via le pilote**, avec une borne de temps
/// côté worker : la commande part dans la file, puis le worker attend
/// l'acquittement pendant au plus `budget`. Au-delà (SDK enrayé), `None`
/// : la voie batch reprend son rang d'autorité (ADR-02) et la prise
/// continue — c'est la garantie anti-figeance (incident 18/10/2026).
fn close_stream_bornee(slot: &StreamSlot, budget: std::time::Duration) -> Option<StreamOutcome> {
    slot.close_done.store(false, Ordering::Relaxed);
    if slot.tx.send(StreamCmd::Close).is_err() {
        return None; // pilote mort (file fermée) : rien à attendre
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

/// Corps du thread pilote : la **seule** courroie FFI du streaming.
/// Suspendu par un SDK enrayé, ce thread perd son temps ; le worker,
/// libéré de tout appel natif, continue (tick, RF-11, relâchement) et
/// la prise se termine par la voie batch. La session meurt ici (Drop →
/// close FFI) — en cas de suspension du Drop, les commandes suivantes
/// patientent : l'affichage streaming s'arrête, l'insertion non.
fn stream_driver_body(
    slot: Arc<StreamSlot>,
    app: AppHandle<Wry>,
    rx: std::sync::mpsc::Receiver<StreamCmd>,
) {
    let mut session: Option<Box<dyn StreamingSession>> = None;
    // Coupure partiels consignée une seule fois par session (l'auto-off
    // resterait sinon muet : constat UX du 19/10, jamais de texte en
    // pastille sans explication).
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
                    // Session déjà là (fermeture antérieure encore en
                    // route) : l'ouverture est ignorée, consignée en
                    // compteurs (RF-34).
                    slot.logger.log(Event::DeviceEvent {
                        kind: "stream-driver-open-ignore".into(),
                    });
                }
            }
            StreamCmd::Pump => {
                if slot.end.load(Ordering::Relaxed) || session.is_none() {
                    continue; // résiduel conservé pour la fermeture/ouverture à venir
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
                    session = None; // Drop → close FFI (ce thread seulement)
                    slot.has_session.store(false, Ordering::Relaxed);
                } else {
                    let s = session.as_mut().expect("session vérifiée ci-dessus");
                    match s.drain() {
                        Ok(updates) => {
                            if !updates.is_empty() {
                                slot.updates_seen
                                    .fetch_add(updates.len() as u64, Ordering::Relaxed);
                                // Partiels : pur affichage, jamais
                                // journalisés ni conservés (RF-34).
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
                    // Auto-off des partiels (machine lente) : consigné une
                    // fois « proprement » — le plafonneur reste réglable via
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
                // 1) résiduel du robinet — la session reçoit tout le reste.
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
                // 2) finish + aspiration bornée (dans la façade).
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
                    // Drop → close FFI (ce thread seulement). En cas de
                    // suspension ici, la fermeture n'est jamais acquittée :
                    // le worker a déjà franchi sa borne et la voie batch
                    // a repris son rang d'autorité.
                    session = None;
                }
                slot.has_session.store(false, Ordering::Relaxed);
                slot.end.store(false, Ordering::Relaxed);
                // Acquittement en dernier : la borne worker s'ouvre.
                slot.close_done.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Voirie batch : à la transcription, la session streaming est finalisée
/// AVANT le décodage offline. Si elle a produit un texte finalisé pendant
/// la prise, ce résultat (déjà décodé incrémentalement) devient la
/// transcription — sinon l'autorité reste la voie batch sur le tampon
/// entier (pré-roll inclus). Les deux routes sont journalisées (sans
/// contenu, RF-34) pour mesure honnête de la latence.
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
        // Fermeture via le pilote, **bornée** (2,5 s) : si le SDK est
        // enrayé, la borne s'ouvre et la voie batch reprend son rang
        // d'autorité (ADR-02). Côté worker, aucune attente indéfinie.
        let outcome = close_stream_bornee(&self.slot, std::time::Duration::from_millis(2500));
        if let Some(outcome) = outcome {
            // Voie rapide : l'audio a déjà décodé pendant la prise ; le
            // gain est majeur (sinon ~0,95× la durée parlée d'attente en
            // fin de prise). Limite assumée : le pré-roll (~0,4 s d'audio
            // antérieur au bouton) n'a pas transité par le flux — le VAD
            // et la ponctuation de prise couvrent le reste ; un travail
            // critique peut forcer un moteur uni-voie (whisper).
            self.slot.logger.log(Event::DeviceEvent {
                kind: "transcription-via-stream".into(),
            });
            return Ok(voxli_core::Transcript {
                text: outcome.text,
                confidence: outcome.confidence,
                duration: began.elapsed(),
                engine: self.inner.id(),
                // L'identité de modèle de la session n'est pas portée par le
                // slot (absence d'information ≠ information fausse).
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

/// Point d'entrée du thread : assemblage des dépendances puis boucle.
pub fn run(app: AppHandle<Wry>, logger: Arc<Logger>, rx: Receiver<WorkerMsg>) {
    let shared = app.state::<UiShared>();
    let mut cfg = lock_or_recover(&shared.config).clone();
    // Droits (RF-39) : profils multiples ⇢ « extra-profiles ». Licence
    // vérifiée au démarrage (main.rs) ; re-vérifiée à chaque ConfigChanged.
    let pro_profiles = {
        let lic = lock_or_recover(&shared.license);
        lic.features.iter().any(|f| f == "extra-profiles")
    };
    let orch_cfg = to_orchestrator_cfg(&cfg, pro_profiles);
    let excluded = cfg.insertion.excluded_apps.clone();

    // --- Capture (ADR-04, RF-08) : périphérique configuré → défaut →
    // repli factice explicite au journal (jamais silencieux). La capture est
    // enveloppée dans un proxy : l'orchestrateur garde `&mut dyn Capture`,
    // pendant que le niveau sonore (RF-09) et la détection de déconnexion
    // (RF-10) sont lus hors orchestrateur par un partagé `Arc`.
    // --- Streaming (docs/05, addition documentée) : le slot est créé
    // AVANT la capture — le robinet y accroche ses tranches dès la
    // construction ; le texte définitif reste la voie batch (aucune
    // sémantique changée), les partiels sont de l'affichage transitoire.
    // Le **pilote** (`stream-driver`) est le seul thread à toucher aux
    // FFI de session : un SDK enrayé figure CE thread, jamais le worker
    // (incident du 18/10/2026 : un appel suspendu gelait tick, RF-11 et
    // relâchement — la prise restait « recording » sans fin).
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
        // Aucun repli silencieux (docs/03) : le défaut est consigné ;
        // la voie batch reste l'autorité, le streaming d'affichage est
        // simplement absent (sess=0 au diagnostic).
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
            // try_lock occupé (drain en cours) : la tranche d'affichage est
            // sacrifiée — le texte définitif (batch) n'en dépend jamais.
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

    // --- VAD (RF-18) : bornes calées sur l'agressivité choisie.
    let vad = EnergyVad::new(vad_config(&cfg));

    // --- Moteur ASR (ADR-03) : réel si compilé + modèle présent ; sinon
    // moteur factice explicite au journal (jamais de fallback silencieux).
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
    // Identité du fichier modèle monté : complément du label — deux
    // montages « whisper.cpp » successifs peuvent porter des poids
    // différents (base → small), la comparaison n'a de sens qu'avec les
    // DEUX (coquille + contenu), sinon l'activation d'une autre voix
    // restait muette (incohérence constatée le 21/10).
    let engine_model = Arc::new(std::sync::Mutex::new(mounted_model));
    // Métadonnée moteur pour l'UI (état_get).
    *lock_or_recover(&app.state::<UiShared>().engine) = engine.batch.id().as_str().to_string();
    // Slot streaming : fabrique (si moteur capable) + langue/verbatim.
    stream.update_mount(
        engine.streaming.clone(),
        Some(cfg.general.language.clone()).filter(|s| !s.is_empty()),
        cfg.profiles.default.style == voxli_config::Style::Verbatim,
    );

    // --- Post-traitement (RF-26) : niveau 1 quand `rules` est choisi ;
    // identité sinon (le profil verbatim la garantit de toute façon).
    let mut post: Box<dyn voxli_core::PostProcessor> = match cfg.general.post_processing {
        voxli_config::PostProcessing::Rules => Box::new(voxli_text::FrenchProcessor),
        _ => Box::new(PassThroughProcessor),
    };

    // --- Historique (RF-33) : le store vit dans `UiShared` (partagé avec
    // les commandes IPC de lecture RF-33) ; le worker n'y touche qu'au
    // moment opportun via `maybe_record_history`.
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

    // --- Boucle : relais des événements + tick de sécurité RF-11.
    // La reconnaissance d'emprunt impose une reconstruction complète de
    // l'orchestrateur quand une dépendance change : l'ancien est abandonné
    // AVANT le nouvel assemblage (`drop` explicite) et le tampon RF-22 est
    // préservé via `restore_buffer`.
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
                // Mi-prise : annulation propre (jamais de rebuild sous les
                // pieds du pipeline) ; l'utilisateur voit la reprise nette.
                if !matches!(
                    orch.state(),
                    DictationState::Idle | DictationState::Recovery { .. }
                ) {
                    orch.handle(InputEvent::Cancel);
                    logger.log(Event::DeviceEvent {
                        kind: "config-reload-rebuild".into(),
                    });
                }
                // Droits rafraîchis (une licence importée prend effet ici).
                let (pro_profiles, pro_history_disk) = {
                    let lic = lock_or_recover(&shared.license);
                    (
                        lic.features.iter().any(|f| f == "extra-profiles"),
                        lic.features.iter().any(|f| f == "history-on-disk"),
                    )
                };
                let orch_cfg = to_orchestrator_cfg(&cfg, pro_profiles);
                // RF-08 : l'échange de périphérique se fait APRÈS avoir
                // abandonné l'orchestrateur (il tient un emprunt exclusif).
                #[allow(unused_mut, unused_assignments)]
                let mut reopen_device = false;
                #[cfg(target_os = "windows")]
                if cfg.audio.device != last_device {
                    last_device = cfg.audio.device.clone();
                    reopen_device = true;
                }
                #[cfg(not(target_os = "windows"))]
                let _ = reopen_device;
                // RF-33 : le store vit dans UiShared, partagé avec les
                // commandes de lecture — reconstruit selon nouvelle config.
                rebuild_history(&shared, &cfg, pro_history_disk, &logger);

                // Toutes les dépendances sont MUTÉES après le drop : l'ancien
                // orchestrateur les emprunte exclusivement jusqu'ici.
                let last = orch.last_transcript().cloned();
                drop(orch);
                // Moteur : rechargé seulement si le choix ou le modèle a
                // changé (un chargement réel coûte cher, RF-20).
                // Slot streaming : langue/verbatim recopiés même sans
                // changement de moteur (la fabrique reste valable).
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
                // RF-15 : le modèle vient d'être installé ; le moteur ne peut
                // changer que hors d'une prise (capture exclusive). Si une
                // dictée est en cours, le recalage attendra le prochain
                // cycle utile (ConfigChanged ou échange de périphérique).
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
                // Streaming (docs/05, addition) : ping non bloquant du
                // pilote — c'est LUI qui pousse le robinet vers la session
                // et émet les partiels d'affichage (transitoires, jamais
                // conservés). Aucun FFI sur ce thread : le worker ne
                // dépend plus de la santé du SDK (incident 18/10/2026).
                let _ = stream.tx.send(StreamCmd::Pump);
                // Diagnostic chiffré (~1 Hz pendant la prise — nombres
                // seulement, jamais de contenu, RF-34) : santé du robinet
                // et de la session d'affichage. Un `sess=0` avec une prise
                // en cours = ouverture de session ratée ; `fed=0` = robinet
                // sec ; `updates=0` ↔ le SDK n'a rien décodé à ce stade.
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
                // RF-08 : micro déconnecté → l'orchestrateur a eu l'erreur de
                // fin de prise ; hors de toute prise, la capture est
                // reconstruite (orchestrateur ré-assemblé, tampon RF-22
                // préservé). Lecture via l'Arc partagé : l'orchestrateur
                // tient un emprunt exclusif de sa voie à lui.
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
                // RF-09 : vumètre — échantillonné 1 fois sur 2 (~100 ms,
                // lecture intérieure au verrou du proxy, très bref) : la
                // jauge de la pastille se remplit et se vide avec la voix.
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

fn to_orchestrator_cfg(c: &Config, pro_bindings: bool) -> OrchestratorCfg {
    let style_of = |s: voxli_config::Style| match s {
        voxli_config::Style::Verbatim => voxli_core::Style::Verbatim,
        voxli_config::Style::Punctuated => voxli_core::Style::Punctuated,
    };
    // RF-25 : liaisons profil-app ; RF-39 : « profils multiples » est une
    // fonctionnalité Pro — hors licence, seul le profil par défaut est
    // actif (les liaisons restent configurées mais inactives, indiqué à
    // l'écran des profils).
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
    // Langue d'écoute : « auto » (chaîne vide) est un choix légitime de
    // l'UI (recommandée). RF-16 : le repli sous 1,5 s d'énoncé NE DOIT
    // JAMAIS être `None` (détection aléatoire sur deux mots) — retomber
    // sur la langue d'affichage, toujours renseignée (défaut fr).
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
        initial_prompt: None, // amorce construite du dictionnaire (RF-24)
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
    // « auto » = le meilleur whisper présent (MÊME résolution que
    // `engines_list` — une seule vérité, cf. `resolve_whisper_model`).
    dir.join(crate::commands::resolve_whisper_model(cfg, &dir))
}

/// Choix de calcul normalisé (`auto`|`duo`|`cpu`|`gpu`) : vide ou inconnu
/// = `auto` — tout choix érroné retombe sur la valeur sûre (RF-29).
#[cfg(feature = "real-asr")]
fn normalize_compute(raw: String) -> String {
    match raw.trim() {
        "cpu" => "cpu".into(),
        "gpu" => "gpu".into(),
        "duo" => "duo".into(),
        _ => "auto".into(),
    }
}

/// Moteur monté : la voie batch — autorité du texte inséré — et, quand le
/// moteur le permet, la fabrique de sessions streaming (affichage des
/// partiels en direct, docs/05).
struct EngineBundle {
    batch: Arc<dyn Transcriptor>,
    streaming: Option<Arc<dyn StreamingEngine>>,
}

/// ID du GGUF Nemotron 3.5 streaming (catalogue modèles, docs/04).
/// Inconditionnel : `engine_stale` doit pouvoir nommer le modèle attendu
/// même dans un build sans SDK (le wanted nemo n'y survient pas, mais la
/// constante documente l'identité canonique).
const NEMO_MODEL_ID: &str = "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf";

/// Dossier du GGUF Nemotron installé (gestionnaire de modèles).
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

/// Journalisation d'un échec de montage moteur : le code du noyau ET, pour
/// `Runtime` (détail SDK in-process), le texte de diagnostic process —
/// jamais de contenu dicté (RF-34). (Souvent utilisé : un seul appelant,
/// sous `nemo-stream` ; le noyau sans SDK n'a pas de détail à porter.)
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
    // Nemotron 3.5 (feature `nemo-stream`) : priorité `auto` quand le GGUF
    // est installé (il apporte le streaming d'affichage) ; choix explicite
    // exige le modèle (absent → code 7 au journal, repli factice audible,
    // jamais silencieux).
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
        // Choix explicite mais binaire compilé sans le SDK : refus documenté.
        logger.log(Event::Error {
            module: "asr".into(),
            code: 9,
        });
    }
    // Parakeet (ADR-03 point 2, feature `parakeet-asr`) : choix explicite,
    // ou repli `auto` quand whisper n'a pas de modèle installé.
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
    // whisper.cpp (feature `real-asr`) pour `whisper` et `auto` si le modèle
    // est installé (gestionnaire de modèles ou scripts/fetch-model.ps1).
    // `Nemo` explicite sans modèle saute cette branche : pas de bascule
    // silencieuse vers un autre moteur que celui choisi.
    #[cfg(feature = "real-asr")]
    if !matches!(
        cfg.general.engine,
        EngineChoice::Parakeet | EngineChoice::Nemo
    ) {
        let path = model_path(cfg);
        if path.is_file() {
            // Choix de calcul (RF-optimal) : variable d'environnement en
            // priorité (marchandage de test instantané), sinon la config ;
            // toute valeur inconnue = `auto` (jamais d'écran abscons).
            // `duo` : deux contextes séparés, rôles distincts — les
            // partiels d'affichage passent sur GPU (demandé ; un build
            // sans backend retombe simplement sur le même contexte que le
            // batch), le texte final reste sur CPU (arbitre stable,
            // RF-22). Les deux appareils travaillent alors réellement de
            // front, chacun sur son flux (RF-34).
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
                    // Deux rôles : voie batch (autorité du texte inséré,
                    // tampon complet RF-22) et partiels d'affichage
                    // (re-décodage fenêtré, jamais inséré — RF-34 +
                    // amendement ADR-02).
                    let engine = Arc::new(engine);
                    let batch: Arc<dyn Transcriptor> = engine.clone();
                    let model_id = path
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if compute != "duo" {
                        // Le DEMANDÉ est consigné avec le choix de calcul ;
                        // le backend réellement utilisé se lit aux latences.
                        logger.log(Event::DeviceEvent {
                            kind: format!("calcul:{compute}"),
                        });
                    }
                    // Contexte des partiels : en duo, un second chargement
                    // du même poids destiné au GPU demandé ; échec → repli
                    // transparent sur le contexte du batch (consigné).
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
                // Modèle absent ou illisible : repli visible, jamais silencieux.
                Err(err) => {
                    logger.log(Event::Error {
                        module: "asr".into(),
                        code: asr_code(&err),
                    });
                }
            }
        } else if matches!(cfg.general.engine, EngineChoice::Whisper) {
            // Choix explicite sans modèle installé : même signal que les
            // autres moteurs (code 7 = modèle manquant) — jamais de repli
            // factice muet (constat UX du 19/10).
            logger.log(Event::Error {
                module: "asr".into(),
                code: 7,
            });
        }
    }
    // Repli factice : toujours CONSIGNÉ distinctement — une dictée « qui
    // réussit » sur texte préfabriqué ne doit jamais pouvoir se confondre
    // avec un vrai moteur (constat UX du 19/10).
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

/// Dossier du modèle Parakeet installé (gestionnaire de modèles).
#[cfg(feature = "parakeet-asr")]
fn parakeet_model_dir(_cfg: &Config) -> std::path::PathBuf {
    Config::default_dir()
        .join("models")
        .join("sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8")
}

/// Disponibilité du moteur whisper réelle (feature + modèle présent).
#[cfg(feature = "real-asr")]
fn whisper_available(cfg: &Config) -> bool {
    model_path(cfg).is_file()
}

#[cfg(not(feature = "real-asr"))]
fn whisper_available(_cfg: &Config) -> bool {
    false
}

/// Disponibilité du moteur Parakeet réelle (feature + dossier présent).
#[cfg(feature = "parakeet-asr")]
fn parakeet_available(cfg: &Config) -> bool {
    parakeet_model_dir(cfg).is_dir()
}

#[cfg(not(feature = "parakeet-asr"))]
fn parakeet_available(_cfg: &Config) -> bool {
    false
}

/// RF-01 : détecte si le moteur à reconstruire diffère de celui en service —
/// évite de recharger un modèle coûteux pour un changement sans rapport.
/// La comparaison porte sur la COQUILLE (label : whisper.cpp, nemo…) ET le
/// CONTENU (fichier modèle monté) : deux montages whisper.cpp n'ont pas
/// nécessairement le même poids — sans cela, activer une autre voix
/// installée ne déclenchait aucune reconstruction (constat du 21/10).
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
            // Priorité auto : Nemotron (streaming d'affichage), puis whisper,
            // puis Parakeet (mêmes règles que build_engine, docs/05 §4).
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
    // Modèle attendu : résolution PARTAGÉE avec build_engine et
    // engines_list (`resolve_whisper_model`) — une seule vérité. Le
    // miroir du journal (`model_id` d'engine_loaded) sert de référence
    // côté monté ; « faux » pour le repli factice.
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
            // RF-09 v2 : la pastille est un indicateur permanent (point
            // d'écoute armée en haut au centre) — le webview s'abonne
            // lui-même à `state://changed` et porte tout le visuel ; le noyau
            // n'y manipule plus ni visibilité ni contenu (ADR-02, RF-34).
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
        // RF-11 : avertissement de durée, une seule fois par prise.
        CoreEvent::MaxDurationWarning { remaining_ms } => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "max-warning",
                    message: strings::msg_max_duration(remaining_ms),
                },
            );
        }
        // RF-02 (c) : verrouillage par double appui.
        CoreEvent::RecordingLocked => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "locked",
                    message: strings::MSG_LOCKED.into(),
                },
            );
        }
        // RF-05 : file d'attente côté application.
        CoreEvent::QueuedAfterCurrent => {
            let _ = app.emit(
                "notice://notice",
                NoticePayload {
                    kind: "queued",
                    message: strings::MSG_QUEUED.into(),
                },
            );
        }
        // RF-32 : re-ciblage vers la fenêtre au premier plan.
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

// ------------------------------------------------------- helpers d'assemblage ---

/// RF-08 : construction de la capture avec le périphérique configuré.
/// Chaîne explicite : configuré → défaut → factice (journal documente).
/// Le robinet streaming (tranches natives pendant une prise, docs/05)
/// accompagne la capture réelle ; le repli factice en est dépourvu
/// (journal `capture-fallback-fake` — l'affichage en direct s'éteint).
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
            // Repli sur le micro par défaut courant avant d'abandonner.
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

/// Bornes VAD calées sur l'agressivité choisie (RF-18).
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

/// Inserteur configuré (temps de maintien du presse-papiers par classe).
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

/// Historique activé (RF-33) : mémoire par défaut, disque chiffré quand
/// (a) la feature `history-disk` est compilée, (b) la licence le permet,
/// (c) l'option config le demande. Sinon `None` (jamais silencieux :
/// le choix est journalisé au démarrage).
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
            // Refus explicite : PAS de repli mémoire masquant l'intention
            // (RF-33 : l'historique disque exige la licence).
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
    // Borné en mémoire (RF-33) : 200 entrées, la plus récente d'abord.
    Some(Box::new(MemoryStore::new(200)))
}

/// (Re)construit le store d'historique de `UiShared` (RF-33) — appelé au
/// démarrage (main.rs) et à chaque rechargement de configuration (ici).
pub fn rebuild_history(shared: &UiShared, cfg: &Config, licensed_disk: bool, logger: &Logger) {
    *lock_or_recover(&shared.history) = build_history(cfg, licensed_disk, logger);
}

/// RF-33 : écrire la transcription dans l'historique à la retombée à Idle.
/// Le texte lu est celui du tampon noyau ; l'écriture est stockée dans
/// `UiShared.history` (chiffré quand le disque est choisi, RF-33) et le
/// rechargement de configuration ne duplique pas d'entrée
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
        // Rétention bornée : fenêtre glissante appliquée après chaque
        // écriture (coût négligeable, pas de thread dédié).
        let days = lock_or_recover(&shared.config)
            .general
            .history
            .retention_days as u64;
        let cutoff = now.saturating_sub(days * 86_400_000);
        let _ = store.retain_since(cutoff);
    }
}

/// Vumètre (RF-09) : niveau instantané [0..1].
#[derive(Clone, Serialize)]
struct LevelPayload {
    level: f32,
    #[serde(rename = "atMs")]
    at_ms: u64,
}

/// Fabrique de fermeture d'événements noyau — reconstruite à chaque
/// rechargement de l'orchestrateur (les `Arc` sont partagés, coût nul).
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
