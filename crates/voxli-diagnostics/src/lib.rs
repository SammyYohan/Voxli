//! `voxli-diagnostics` — le module qui rend RF-34 vérifiable.
//!
//! Règles structurelles :
//! - le type `Event` n'a **aucun champ de contenu libre** : l'API empêche
//!   d'y mettre du texte dicté, de l'audio ou le contenu du presse-papiers ;
//! - chaque ligne est un objet JSON (horodatage, identifiant d'événement,
//!   durées, moteur, modèle, code d'erreur, niveau d'insertion) ;
//! - `export_report` assemble un rapport de support : avertissement avant
//!   envoi, **rédaction forcée** structurelle — seules les lignes conformes au
//!   schéma des événements typés sont conservées, toute autre ligne (fichier
//!   altéré ou non produit par l'application) est ignorée et signalée.

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

/// Sink fichier avec rotation par taille **pendant l'écriture**.
///
/// Schéma — trois fichiers (défaut : 1 Mo, 3 générations d'historique) :
/// - `voxli.log` — fichier courant ;
/// - `voxli.log.1` — génération précédente ;
/// - `voxli.log.2` — génération la plus ancienne.
///
/// Dès que le fichier courant dépasse `max_bytes`, il est fermé puis les
/// générations sont décalées (`.log.1` → `.log.2`, `.log` → `.log.1`) :
/// aucune ligne écrite n'est perdue, et aucune génération n'est supprimée
/// avant la troisième rotation.
pub struct FileSink {
    path: PathBuf,
    max_bytes: u64,
    /// Octets écrits dans la génération courante depuis sa création.
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

    /// (Re)ouvre le fichier courant. Au démarrage ou après une panne interne,
    /// la rotation s'appuie sur la taille **sur disque** (le fichier peut
    /// dépasser `max_bytes` si un process précédent l'a laissé gros).
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

    /// Décale les générations : `.log` → `.log.1`, `.log.1` → `.log.2`, la
    /// plus ancienne est remplacée. Le fichier courant est d'abord fermé
    /// (renommage impossible sous Windows tant qu'un handle en écriture est
    /// détenu). Les échecs de décalage sont ignorés silencieusement : dans le
    /// pire cas le fichier courant continue de croître plutôt que de perdre
    /// ses lignes.
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
        // Rotation dès que le plafond est franchi : la ligne qui vient d'être
        // écrite est déjà enregistrée (on ne perd jamais une ligne complète).
        if self.written >= self.max_bytes {
            self.rotate();
            let _ = self.open();
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

    /// Journal fichier dans le dossier fourni par l'appelant
    /// (`logs` sous le dossier de données de l'application).
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

/// Filtrage défensif de caractères (docs/05 §2.7 : `redact()` fait partie de
/// l'interface publique du module).
///
/// Ne conserve qu'un jeu conservatoire — lettres, chiffres, espace et la
/// ponctuation ` .,_-()/` — et remplace tout autre caractère (guillemets,
/// accolades, deux-points, retours ligne, caractères de contrôle…) par `?` :
/// un fragment ainsi filtré ne peut ni rompre une structure JSON, ni sortir
/// d'un bloc de code, ni porter une mise en forme hostile.
///
/// **Limites** : ce filtre ne neutralise pas une phrase en langue naturelle
/// (les lettres survivent) ; il protège contre l'injection de structure, pas
/// contre la divulgation de texte. La crate ne l'appelle pas aujourd'hui :
/// `export_report` neutralise autrement (rejet propre des lignes non
/// conformes, sans réécriture). Il est proposé pour des zones libres qu'une
/// future couche support exposerait ; la garantie « sans contenu » du
/// rapport actuel repose sur le typage des événements (`Event` n'a aucun
/// champ libre) et sur la validation structurelle décrite ci-dessous.
pub fn redact(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        // Jeu conservatoire : identifiants, horodatages, séparateurs métier.
        if ch.is_ascii_alphanumeric() || " .,_-()/".contains(ch) {
            out.push(ch);
        } else {
            out.push('?');
        }
    }
    out
}

/// Champ `event` autorisé pour chaque variante d'`Event` et clés qu'il peut
/// porter (uniquement des métadonnées, cf. enum ci-dessus). Liste redondante
/// à dessein : c'est une garde de défense en profondeur sur les lignes
/// relues **depuis le disque**, où le fichier peut être altéré ou issu d'une
/// autre application.
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

/// Plafond défensif sur toute valeur texte d'une ligne conforme : les vrais
/// champs (moteur, modèle, étape, état…) sont courts, une ligne qui
/// embarquerait une chaine énorme n'a pas été produite par ce module.
const MAX_FIELD_LEN: usize = 256;

/// Ligne conforme au schéma des événements typés : objet JSON avec `ts`
/// numérique, un `event` connu, exactement ses champs, et des valeurs
/// scalaires bornées. Une ligne qui échoue n'est pas une fuite prouvée mais
/// ne peut plus être diffusée telle quelle.
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
    // Exactement `ts` + `event` + les champs de la variante : rejette tout
    // champ additionnel (ex. un champ de contenu injecté dans un fichier
    // modifié).
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

/// Rapport de diagnostic destiné au support : métadonnées uniquement,
/// rédaction forcée, avertissement avant envoi (docs/05 §2.7).
///
/// Les lignes sont relues depuis le disque par l'appelant ; seules les
/// lignes [`line_is_conforming`] sont reportées telles quelles. Toute autre
/// ligne est remplacée par une pierre tombale fixe (aucun extrait : le
/// contenu d'une ligne non vérifiée n'est jamais propagé) et les rejets sont
/// comptabilisés en bas de rapport. La garantie de non-contenu des lignes
/// conservées repose sur le typage de [`Event`] (aucun champ libre).
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
            // Pierre tombale sans extrait, volontairement : le contenu d'une
            // ligne non vérifiée (fichier altéré, ou un jour une ligne
            // empoisonnée par un bug) n'est pas repris, même filtré. Texte
            // fixe d'une seule ligne : rien ne peut fermer le bloc de code.
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

    /// Répertoire temporaire unique par test (aucune dépendance externe).
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

    /// Ligne de 32 caractères + '\n' = 33 octets ; avec `max_bytes = 64`,
    /// chaque paire de lignes franchit le plafond et déclenche une rotation.
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
        // Traçage : rotations après les lignes 1, 3, 5, 7. Chaque paire de
        // lignes occupe une génération ; seules les 3 plus récentes restent.
        let two = std::fs::read_to_string(dir.join("voxli.log.2")).unwrap_or_default();
        let one = std::fs::read_to_string(dir.join("voxli.log.1")).unwrap_or_default();
        assert_eq!(two.lines().filter(|l| !l.is_empty()).count(), 2);
        assert_eq!(one.lines().filter(|l| !l.is_empty()).count(), 2);
        // La génération .log.2 est le DÉBUT de l'historique qui subsiste :
        // le contenu n'est pas tronqué mais décalé.
        let first_of = |text: &str| text.lines().next().map(str::to_string);
        assert_eq!(first_of(&two), Some(rot_line(4)));
        assert_eq!(two.lines().nth(1).map(str::to_string), Some(rot_line(5)));
        assert_eq!(first_of(&one), Some(rot_line(6)));
        assert_eq!(one.lines().nth(1).map(str::to_string), Some(rot_line(7)));
        // Lignes complètes jamais perdues : la paire gardée commence bien à
        // la ligne 4 — avec l'ancien code (qui supprimait .log.1 au
        // démarrage et ne tournait pas pendant l'écriture) ce scénario
        // échouait.
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

    /// Garde anti-dérive : la table `KNOWN_EVENTS` (contrat du rapport) doit
    /// refléter exactement les noms de tag et les champs que l'enum `Event`
    /// produit réellement.
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

    /// Rédaction forcée sur un fichier altéré : les lignes non conformes
    /// (texte libre, champ de contenu injecté, JSON non objet) sont écartées
    /// sans qu'aucun fragment ne soit propagé.
    #[test]
    fn t_report_drops_non_conforming_lines() {
        let good = {
            let shared = SharedSink::default();
            let logger = Logger::new(Box::new(shared.clone()));
            logger.log(Event::Latency {
                stage: "inference".into(),
                micros: 42,
            });
            // Cloné dans une instruction distincte : le garde-verbatim
            // (MutexGuard) doit mourir ici, pas au retour du bloc.
            let lines = shared.0.lock().unwrap().clone();
            lines[0].clone()
        };
        // Lignes simulant un journal relu d'une source non fiable : JSON
        // valide mais champ de contenu injecté, texte libre, JSON non objet.
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
