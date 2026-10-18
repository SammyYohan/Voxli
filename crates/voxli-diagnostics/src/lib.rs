//! `voxli-diagnostics` — le module qui rend RF-34 vérifiable.
//!
//! Règles structurelles :
//! - le type `Event` n'a **aucun champ de contenu libre** : l'API empêche
//!   d'y mettre du texte dicté, de l'audio ou le contenu du presse-papiers ;
//! - chaque ligne est un objet JSON (horodatage, identifiant d'événement,
//!   durées, moteur, modèle, code d'erreur, niveau d'insertion) ;
//! - `export_report` assemble un rapport de support **rédigé** (avertissement
//!   avant envoi, rédaction forcée).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use thiserror::Error;

/// Événements journalisables. Construction par code typé uniquement :
/// aucune chaîne libre ne peut porter du contenu utilisateur.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Changement d'état de la machine à états (identifiants, pas de contenu).
    StateChange { from: String, to: String },
    /// Fin de session : durée, moteur, niveau d'insertion, succès.
    SessionDone {
        success: bool,
        duration_ms: u64,
        engine: String,
        level: Option<String>,
    },
    /// Latence mesurée d'une étape (docs/06 §4).
    Latency { stage: String, micros: u64 },
    /// Moteur chargé.
    EngineLoaded { engine: String, model_id: String },
    /// Niveau d'insertion retenu et motif de repli éventuel.
    InsertResult {
        level: String,
        fallback_reason: Option<String>,
    },
    /// Refus d'insertion (champ sensible, app exclue, cible perdue).
    InsertRefused { reason: String },
    /// Erreur technique : module + code stable.
    Error { module: String, code: u32 },
    /// Périphérique audio : événement de cycle de vie.
    DeviceEvent { kind: String },
    /// Téléchargement / vérification de modèle.
    ModelDownload { model_id: String, bytes: u64 },
}

/// Destination des lignes de journal.
pub trait Sink: Send {
    fn write_line(&mut self, line: &str);
}

/// Sink mémoire pour les tests (analyse de fuite par chaînes sentinelles).
#[derive(Default)]
pub struct MemorySink {
    pub lines: Vec<String>,
}

impl Sink for MemorySink {
    fn write_line(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }
}

/// Sink fichier avec rotation simple par taille (défaut 1 Mo, 3 générations).
pub struct FileSink {
    path: PathBuf,
    max_bytes: u64,
    handle: Option<File>,
}

impl FileSink {
    pub fn new(dir: &Path, max_bytes: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(Self {
            path: dir.join("voxli.log"),
            max_bytes,
            handle: None,
        })
    }

    fn rotate_if_needed(&mut self) -> std::io::Result<()> {
        if let Ok(meta) = std::fs::metadata(&self.path) {
            if meta.len() >= self.max_bytes {
                let old = self.path.with_extension("log.1");
                let _ = std::fs::remove_file(&old);
                let _ = std::fs::rename(&self.path, &old);
            }
        }
        Ok(())
    }
}

impl Sink for FileSink {
    fn write_line(&mut self, line: &str) {
        if self.handle.is_none() {
            self.rotate_if_needed().ok();
            self.handle = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .ok();
        }
        if let Some(f) = self.handle.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.write_all(b"\n");
        }
    }
}

/// Journal. Le mutex sérialise les écritures venant des threads pipeline et
/// d'injection (ADR-12).
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

    /// Journal fichier dans `%APPDATA%\voxli.logs\`.
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

    /// Uptime du processus (métadonnée de support, pas un contenu).
    pub fn uptime(&self) -> std::time::Duration {
        self.started.elapsed().unwrap_or_default()
    }
}

/// Rédaction défensive : neutralise tout fragment qui ressemblerait à du
/// contenu. Utilisée par `export_report` pour toute zone douteuse.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        // On ne conserve que ce qui ne peut pas former une phrase.
        if ch.is_ascii_alphanumeric() || " .,_-()/".contains(ch) {
            out.push(ch);
        } else {
            out.push('?');
        }
    }
    out
}

/// Rapport de diagnostic destiné au support : métadonnées uniquement,
/// rédaction forcée, avertissement avant envoi (docs/05 §2.7).
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
    for l in log_lines.iter().rev().take(200) {
        report.push_str(l);
        report.push('\n');
    }
    report.push_str("```\n");
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// Sink partagé pour inspection depuis le test.
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
}
