//! `voxli-diagnostics` — the module that makes RF-34 verifiable.
//!
//! Structural rules:
//! - the `Event` type has **no free-form content field**: the API makes it
//!   impossible to store dictated text, audio, or clipboard content in it;
//! - every line is a JSON object (timestamp, event identifier,
//!   durations, engine, model, error code, insertion level);
//! - `export_report` assembles a support report: a warning before
//!   sending, structural **forced redaction** — only lines conforming to
//!   the schema of the typed events are kept; any other line (altered file,
//!   or one not produced by the application) is ignored and reported.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;

/// Loggable events. Constructed through typed code only:
/// no free-form string can carry user content.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// State-machine transition (identifiers, no content).
    StateChange { from: String, to: String },
    /// End of session: duration, engine, insertion level, success.
    SessionDone {
        success: bool,
        duration_ms: u64,
        engine: String,
        level: Option<String>,
    },
    /// Measured latency of one stage (docs/06 §4).
    Latency { stage: String, micros: u64 },
    /// Engine loaded.
    EngineLoaded { engine: String, model_id: String },
    /// Retained insertion level and optional fallback reason.
    InsertResult {
        level: String,
        fallback_reason: Option<String>,
    },
    /// Insertion refusal (sensitive field, excluded app, lost target).
    InsertRefused { reason: String },
    /// Technical error: module + stable code.
    Error { module: String, code: u32 },
    /// Audio device: lifecycle event.
    DeviceEvent { kind: String },
    /// Model download / verification.
    ModelDownload { model_id: String, bytes: u64 },
}

/// Destination for journal lines.
pub trait Sink: Send {
    fn write_line(&mut self, line: &str);
}

/// In-memory sink for tests (leak analysis via sentinel strings).
#[derive(Default)]
pub struct MemorySink {
    pub lines: Vec<String>,
}

impl Sink for MemorySink {
    fn write_line(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }
}

/// File sink rotating by size **during writing**.
///
/// Layout — three files (default: 1 MB, 3 history generations):
/// - `voxli.log` — current file;
/// - `voxli.log.1` — previous generation;
/// - `voxli.log.2` — oldest generation.
///
/// As soon as the current file exceeds `max_bytes`, it is closed and the
/// generations are shifted (`.log.1` → `.log.2`, `.log` → `.log.1`):
/// no written line is lost, and no generation is deleted
/// before the third rotation.
pub struct FileSink {
    path: PathBuf,
    max_bytes: u64,
    /// Bytes written to the current generation since it was created.
    written: u64,
    handle: Option<File>,
}

impl FileSink {
    pub fn new(dir: &Path, max_bytes: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let mut this = Self {
            path: dir.join("voxli.log"),
            max_bytes,
            written: 0,
            handle: None,
        };
        this.open()?;
        Ok(this)
    }

    /// (Re)opens the current file. At startup or after an internal
    /// failure, rotation relies on the size **on disk** (the file may
    /// exceed `max_bytes` if a previous process left it large).
    fn open(&mut self) -> std::io::Result<()> {
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if meta.len() >= self.max_bytes {
                self.rotate();
            }
        }
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.written = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        self.handle = Some(f);
        Ok(())
    }

    /// Shifts the generations: `.log` → `.log.1`, `.log.1` → `.log.2`, the
    /// oldest one is replaced. The current file is closed first
    /// (renaming is impossible on Windows while a write handle is
    /// held). Shift failures are silently ignored: in the
    /// worst case the current file keeps growing rather than losing
    /// its lines.
    fn rotate(&mut self) {
        self.handle = None;
        let one = self.path.with_extension("log.1");
        let two = self.path.with_extension("log.2");
        let _ = std::fs::remove_file(&two);
        let _ = std::fs::rename(&one, &two);
        let _ = std::fs::rename(&self.path, &one);
    }
}

impl Sink for FileSink {
    fn write_line(&mut self, line: &str) {
        let extra = line.len() as u64 + 1;
        if let Some(f) = self.handle.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
            self.written += extra;
        }
        // Rotate as soon as the ceiling is crossed: the line that was just
        // written is already persisted (a complete line is never lost).
        if self.written >= self.max_bytes {
            self.rotate();
            let _ = self.open();
        }
    }
}

/// Logger. The mutex serializes writes coming from the pipeline and
/// injection threads (ADR-12).
pub struct Logger {
    sink: Mutex<Box<dyn Sink>>,
    started: SystemTime,
}

#[derive(Debug, Error)]
pub enum DiagError {
    #[error("écriture du journal impossible : {0}")]
    Write(String),
}

impl Logger {
    pub fn new(sink: Box<dyn Sink>) -> Self {
        Self {
            sink: Mutex::new(sink),
            started: SystemTime::now(),
        }
    }

    /// File logger in the directory provided by the caller
    /// (`logs` under the application data directory).
    pub fn file_logger(dir: &Path) -> Self {
        match FileSink::new(dir, 1_000_000) {
            Ok(sink) => Self::new(Box::new(sink)),
            Err(_) => Self::new(Box::new(MemorySink::default())),
        }
    }

    pub fn log(&self, event: Event) {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut line = serde_json::json!({ "ts": ts });
        if let Ok(map) = serde_json::to_value(&event) {
            if let (Some(obj), Some(target)) = (map.as_object(), line.as_object_mut()) {
                for (k, v) in obj {
                    target.insert(k.clone(), v.clone());
                }
            }
        }
        if let Ok(mut sink) = self.sink.lock() {
            sink.write_line(&line.to_string());
        }
    }

    /// Process uptime (support metadata, not content).
    pub fn uptime(&self) -> std::time::Duration {
        self.started.elapsed().unwrap_or_default()
    }
}

/// Defensive character filtering (docs/05 §2.7: `redact()` is part of
/// the module's public interface).
///
/// Keeps only a conservative set — letters, digits, space and the
/// punctuation ` .,_-()/` — and replaces every other character (quotes,
/// braces, colons, line breaks, control characters…) with `?`:
/// a fragment filtered this way can neither break a JSON structure, nor
/// escape a code block, nor carry hostile formatting.
///
/// **Limits**: this filter does not neutralize a natural-language sentence
/// (letters survive); it protects against structural injection, not
/// against text disclosure. The crate does not call it today:
/// `export_report` neutralizes differently (clean rejection of non-
/// conforming lines, without rewriting). It is offered for the free-form
/// areas a future support layer might expose; the "content-free"
/// guarantee of the current report rests on event typing (`Event` has no
/// free-form field) and on the structural validation described below.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        // Conservative set: identifiers, timestamps, business separators.
        if ch.is_ascii_alphanumeric() || " .,_-()/".contains(ch) {
            out.push(ch);
        } else {
            out.push('?');
        }
    }
    out
}

/// Allowed `event` tag for each `Event` variant, with the keys it may
/// carry (metadata only, cf. enum above). Deliberately redundant
/// list: it is a defense-in-depth guard over the lines read back
/// **from disk**, where the file may be altered or produced by another
/// application.
const KNOWN_EVENTS: &[(&str, &[&str])] = &[
    ("state_change", &["from", "to"]),
    (
        "session_done",
        &["success", "duration_ms", "engine", "level"],
    ),
    ("latency", &["stage", "micros"]),
    ("engine_loaded", &["engine", "model_id"]),
    ("insert_result", &["level", "fallback_reason"]),
    ("insert_refused", &["reason"]),
    ("error", &["module", "code"]),
    ("device_event", &["kind"]),
    ("model_download", &["model_id", "bytes"]),
];

/// Defensive ceiling on any text value of a conforming line: the real
/// fields (engine, model, stage, state…) are short, a line that
/// carried a huge string was not produced by this module.
const MAX_FIELD_LEN: usize = 256;

/// Line conforming to the typed-event schema: a JSON object with a numeric
/// `ts`, a known `event`, exactly its fields, and bounded scalar values. A
/// line that fails validation is not a proven leak but
/// can no longer be transmitted as is.
fn line_is_conforming(line: &str) -> bool {
    let value = match serde_json::from_str::<serde_json::Value>(line) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let Some(obj) = value.as_object() else {
        return false;
    };
    let Some(event) = obj.get("event").and_then(serde_json::Value::as_str) else {
        return false;
    };
    let Some((_, fields)) = KNOWN_EVENTS.iter().find(|(e, _)| *e == event) else {
        return false;
    };
    if !obj.get("ts").is_some_and(serde_json::Value::is_number) {
        return false;
    }
    // Exactly `ts` + `event` + the variant's fields: rejects any
    // additional field (e.g. a content field injected into an altered
    // file).
    if obj.len() != fields.len() + 2 {
        return false;
    }
    for key in *fields {
        match obj.get(*key) {
            Some(serde_json::Value::String(s)) => {
                if s.chars().count() > MAX_FIELD_LEN {
                    return false;
                }
            }
            Some(
                serde_json::Value::Number(_) | serde_json::Value::Bool(_) | serde_json::Value::Null,
            ) => {}
            _ => return false,
        }
    }
    true
}

/// Diagnostic report intended for support: metadata only,
/// forced redaction, warning before sending (docs/05 §2.7).
///
/// Lines are read back from disk by the caller; only the
/// [`line_is_conforming`] lines are reproduced as is. Any other
/// line is replaced by a fixed tombstone (no excerpt: the
/// content of an unverified line is never propagated) and the rejections are
/// tallied at the bottom of the report. The content-free guarantee of the
/// kept lines rests on [`Event`] typing (no free-form field).
pub fn export_report(log_lines: &[String], uptime: std::time::Duration) -> String {
    let mut report = String::new();
    report.push_str("# Rapport de diagnostic Voxli\n\n");
    report.push_str(
        "**À lire avant tout envoi** : ce rapport ne contient, par conception, \
         ni texte dicté, ni audio, ni contenu de presse-papiers. Vérifiez-le \
         avant transmission.\n\n",
    );
    report.push_str(&format!("- uptime : {:?}\n", uptime));
    report.push_str(&format!("- événements journalisés : {}\n", log_lines.len()));
    report.push_str("\n```json\n");
    let mut dropped = 0usize;
    for l in log_lines.iter().rev().take(200) {
        if line_is_conforming(l) {
            report.push_str(l);
            report.push('\n');
        } else {
            // Tombstone without an excerpt, deliberately: the content of an
            // unverified line (altered file, or one day a line
            // poisoned by a bug) is not reproduced, even filtered. Fixed
            // single-line text: nothing can close the code block.
            dropped += 1;
            report.push_str("[ligne non conforme ignorée]\n");
        }
    }
    report.push_str("```\n");
    if dropped > 0 {
        report.push_str(&format!(
            "\n- lignes ignorées (non conformes, rédaction forcée) : {dropped}\n"
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Shared sink for inspection from the test.
    #[derive(Clone, Default)]
    struct SharedSink(Arc<Mutex<Vec<String>>>);

    impl Sink for SharedSink {
        fn write_line(&mut self, line: &str) {
            self.0.lock().unwrap().push(line.to_string());
        }
    }

    #[test]
    fn t_events_serialise_without_content() {
        let ev = Event::SessionDone {
            success: true,
            duration_ms: 850,
            engine: "whisper.cpp".into(),
            level: Some("value-pattern".into()),
        };
        let line = serde_json::to_string(&ev).unwrap();
        assert!(line.contains("whisper.cpp"));
        assert!(
            !line.to_lowercase().contains("texte"),
            "aucun champ de contenu"
        );
    }

    #[test]
    fn t_logger_emits_json_lines() {
        let shared = SharedSink::default();
        let logger = Logger::new(Box::new(shared.clone()));
        logger.log(Event::Error {
            module: "asr".into(),
            code: 3,
        });
        logger.log(Event::Latency {
            stage: "inference".into(),
            micros: 12_500,
        });
        let lines = shared.0.lock().unwrap().clone();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"event\":\"error\""));
        assert!(lines[0].contains("\"code\":3"));
        assert!(lines[1].contains("\"micros\":12500"));
    }

    #[test]
    fn t_report_is_redacted_by_construction() {
        let shared = SharedSink::default();
        let logger = Logger::new(Box::new(shared.clone()));
        logger.log(Event::EngineLoaded {
            engine: "fake".into(),
            model_id: "tiny".into(),
        });
        let lines = shared.0.lock().unwrap().clone();
        let report = export_report(&lines, std::time::Duration::from_secs(30));
        assert!(report.contains("ne contient"));
        assert!(report.contains("tiny"));
    }

    /// Unique temporary directory per test (no external dependency).
    fn unique_dir(name: &str) -> std::path::PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "voxli-diag-{}-{}-{}",
            std::process::id(),
            name,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A 32-character line + '\n' = 33 bytes; with `max_bytes = 64`,
    /// every pair of lines crosses the ceiling and triggers a rotation.
    fn rot_line(i: usize) -> String {
        format!("{:032}", i)
    }

    #[test]
    fn t_filesink_rotates_and_shifts_generations() {
        let dir = unique_dir("rotation");
        const MAX: u64 = 64;
        let mut sink = FileSink::new(&dir, MAX).expect("création du sink");
        for i in 0..8 {
            sink.write_line(&rot_line(i));
        }
        // Trace: rotations after lines 1, 3, 5, 7. Each pair of
        // lines occupies one generation; only the 3 most recent remain.
        let two = std::fs::read_to_string(dir.join("voxli.log.2")).unwrap_or_default();
        let one = std::fs::read_to_string(dir.join("voxli.log.1")).unwrap_or_default();
        assert_eq!(two.lines().filter(|l| !l.is_empty()).count(), 2);
        assert_eq!(one.lines().filter(|l| !l.is_empty()).count(), 2);
        // The .log.2 generation is the START of the surviving history:
        // the content is not truncated but shifted.
        let first_of = |text: &str| text.lines().next().map(str::to_string);
        assert_eq!(first_of(&two), Some(rot_line(4)));
        assert_eq!(two.lines().nth(1).map(str::to_string), Some(rot_line(5)));
        assert_eq!(first_of(&one), Some(rot_line(6)));
        assert_eq!(one.lines().nth(1).map(str::to_string), Some(rot_line(7)));
        // Complete lines are never lost: the kept pair indeed starts at
        // line 4 — with the old code (which deleted .log.1 at
        // startup and did not rotate during writing) this scenario
        // used to fail.
        assert!(
            !dir.join("voxli.log.3").exists(),
            "pas de génération en plus"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn t_filesink_reopen_keeps_history() {
        let dir = unique_dir("reopen");
        {
            let mut sink = FileSink::new(&dir, 4096).expect("création du sink");
            sink.write_line("premier");
            sink.write_line("deuxieme");
        }
        let mut sink2 = FileSink::new(&dir, 4096).expect("réouverture du sink");
        sink2.write_line("troisieme");
        let content = std::fs::read_to_string(dir.join("voxli.log")).unwrap_or_default();
        let lines: Vec<&str> = content.lines().collect();
        assert!(content.starts_with("premier"), "ancien contenu intact");
        assert_eq!(lines.len(), 3, "aucune troncature à la réouverture");
        assert!(
            !dir.join("voxli.log.1").exists(),
            "aucune rotation indue sous le plafond"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Anti-drift guard: the `KNOWN_EVENTS` table (the report's contract)
    /// must exactly mirror the tag names and the fields that the `Event`
    /// enum actually produces.
    #[test]
    fn t_known_events_match_event_enum_tags() {
        let samples: Vec<(&str, serde_json::Value)> = vec![
            (
                "state_change",
                serde_json::to_value(Event::StateChange {
                    from: "a".into(),
                    to: "b".into(),
                })
                .unwrap(),
            ),
            (
                "session_done",
                serde_json::to_value(Event::SessionDone {
                    success: true,
                    duration_ms: 1,
                    engine: "e".into(),
                    level: None,
                })
                .unwrap(),
            ),
            (
                "latency",
                serde_json::to_value(Event::Latency {
                    stage: "s".into(),
                    micros: 1,
                })
                .unwrap(),
            ),
            (
                "engine_loaded",
                serde_json::to_value(Event::EngineLoaded {
                    engine: "e".into(),
                    model_id: "m".into(),
                })
                .unwrap(),
            ),
            (
                "insert_result",
                serde_json::to_value(Event::InsertResult {
                    level: "l".into(),
                    fallback_reason: None,
                })
                .unwrap(),
            ),
            (
                "insert_refused",
                serde_json::to_value(Event::InsertRefused {
                    reason: "sensitive".into(),
                })
                .unwrap(),
            ),
            (
                "error",
                serde_json::to_value(Event::Error {
                    module: "m".into(),
                    code: 1,
                })
                .unwrap(),
            ),
            (
                "device_event",
                serde_json::to_value(Event::DeviceEvent { kind: "k".into() }).unwrap(),
            ),
            (
                "model_download",
                serde_json::to_value(Event::ModelDownload {
                    model_id: "m".into(),
                    bytes: 1,
                })
                .unwrap(),
            ),
        ];
        assert_eq!(samples.len(), KNOWN_EVENTS.len());
        for (name, value) in samples {
            let fields = KNOWN_EVENTS
                .iter()
                .find(|(e, _)| *e == name)
                .map(|(_, f)| *f)
                .expect("nom d'événement connu");
            let obj = value.as_object().expect("sérialisé en objet JSON");
            let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
            keys.retain(|k| *k != "event");
            keys.sort_unstable();
            let mut want: Vec<&str> = fields.to_vec();
            want.sort_unstable();
            assert_eq!(keys, want, "clés de l'événement `{name}`");
        }
    }

    /// Forced redaction on an altered file: non-conforming lines
    /// (free text, injected content field, non-object JSON) are discarded
    /// without any fragment being propagated.
    #[test]
    fn t_report_drops_non_conforming_lines() {
        let good = {
            let shared = SharedSink::default();
            let logger = Logger::new(Box::new(shared.clone()));
            logger.log(Event::Latency {
                stage: "inference".into(),
                micros: 42,
            });
            // Cloned in a separate statement: the verbatim guard
            // (MutexGuard) must die here, not at the block's return.
            let lines = shared.0.lock().unwrap().clone();
            lines[0].clone()
        };
        // Lines simulating a journal read back from an untrusted source:
        // valid JSON but injected content field, free text, non-object JSON.
        let poisoned =
            r#"{"ts":1,"event":"error","module":"asr","code":1,"dictation":"phrase injectée"}"#
                .to_string();
        let free_text = "texte libre collé dans un fichier modifié".to_string();
        let not_object = r#"[{"ts":1}]"#.to_string();
        let report = export_report(
            &[good.clone(), poisoned, free_text, not_object],
            std::time::Duration::from_secs(5),
        );
        assert!(report.contains("\"micros\":42"), "ligne conforme conservée");
        assert!(
            !report.contains("dictation"),
            "champ de contenu injecté écarté"
        );
        assert!(!report.contains("injectée"), "contenu injecté non propagé");
        assert!(!report.contains("texte libre"), "texte libre non propagé");
        assert!(
            report.matches("[ligne non conforme ignorée]").count() == 3,
            "trois rejets signalés"
        );
        assert!(
            report.contains("lignes ignorées"),
            "compteur de rejets en pied de rapport"
        );
        assert!(report.contains(&good), "copie inline de la ligne conforme");
    }
}
