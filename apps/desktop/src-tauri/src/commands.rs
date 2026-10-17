//! Commandes IPC — surface webview ⇄ noyau (docs/05 §3).
//!
//! Principe : le webview **observe** et **configure**. Il ne voit jamais
//! l'audio ; il ne déclenche aucune insertion de texte arbitraire.
//!
//! Commandes P0 (chaque entrée fait partie de la table IPC) :
//! - `configGet` / `configSet` — configuration (validation côté noyau) ;
//! - `stateGet` — état courant de la machine à états ;
//! - `rescueLast` / `rescuePasteAgain` / `rescueRetry` / `rescueClose` —
//!   fenêtre de secours : **exception documentée** (RF-31, RF-22). « Coller à
//!   nouveau » et « Réessayer » ne peuvent insérer QUE le texte déjà présent
//!   dans le tampon interne de l'orchestrateur — aucune insertion arbitraire
//!   n'est possible par IPC ;
//! - `cancelDictation` — annulation (RF-09) ;
//! - `diagnosticsExport` — rapport de support rédigé (docs/05 §2.7).
//!
//! Ajouts autorisés à la table : l'événement `notice://notice` (messages
//! neutres, jamais de contenu) — documenté dans worker.rs.

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager, State, Wry};

use plume_config::Config;
use plume_diagnostics::Logger;

use crate::strings;
use crate::worker::WorkerMsg;

/// État partagé : thread principal ⇄ webview ⇄ thread pipeline. Tout champ est
/// `Send + Sync` ; les accès passent par `Mutex` avec récupération du poison
/// (jamais de panique sur un verrou empoisonné).
pub struct UiShared {
    /// File d'entrée du thread pipeline (événements de dictée).
    pub tx: Sender<WorkerMsg>,
    /// Configuration durablement en mémoire ; le disque en est le miroir.
    pub config: Mutex<Config>,
    pub config_path: PathBuf,
    /// Journal partagé : thread pipeline + commandes.
    pub logger: Arc<Logger>,
    /// Tampon « texte de secours » affiché dans la fenêtre RF-31 ; exception
    /// documentée : miroir du tampon interne de l'orchestrateur (RF-22).
    pub last: Mutex<Option<String>>,
    /// Nom d'état courant (machine à états docs/04 §3), jamais de contenu.
    pub state: Mutex<String>,
    /// Identifiant du moteur chargé (métadonnée, RF-34).
    pub engine: Mutex<String>,
}

/// Helper d'accès antipanique au contenu d'un mutex.
pub(crate) fn lock_or_recover<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ------------------------------------------------------------ config ----

#[tauri::command]
pub fn config_get(shared: State<'_, UiShared>) -> Config {
    lock_or_recover(&shared.config).clone()
}

/// Écriture de configuration : validation complète côté noyau, confiance
/// nulle envers le webview. P0 : les raccourcis ne sont pas modifiables (le
/// ré-enregistrement à chaud des raccourcis globaux arrive en P1) — toute
/// modification est rejetée explicitement avec le message dédié.
#[tauri::command]
pub fn config_set(shared: State<'_, UiShared>, new_config: Config) -> Result<(), String> {
    let normalized = new_config.normalized();
    {
        let current = lock_or_recover(&shared.config);
        if normalized.hotkeys != current.hotkeys {
            return Err(strings::ERR_HOTKEY_P0.to_string());
        }
    }
    normalized
        .save(&shared.config_path)
        .map_err(|e| format!("{} : {e}", strings::ERR_CONFIG_SAVE))?;
    *lock_or_recover(&shared.config) = normalized;
    Ok(())
}

// ------------------------------------------------------------- état ----

#[derive(Serialize)]
pub struct StateInfo {
    /// Nom d'état de la machine à états (docs/04 §3).
    pub state: String,
    /// Identifiant du moteur chargé (métadonnée RF-34).
    pub engine: String,
}

#[tauri::command]
pub fn state_get(shared: State<'_, UiShared>) -> StateInfo {
    StateInfo {
        state: lock_or_recover(&shared.state).clone(),
        engine: lock_or_recover(&shared.engine).clone(),
    }
}

// ------------------------------------------------------- secours RF-31 ---

/// Texte courant du tampon de secours ; `null` si aucun.
#[tauri::command]
pub fn rescue_last(shared: State<'_, UiShared>) -> Option<String> {
    lock_or_recover(&shared.last).clone()
}

/// « Coller à nouveau » (RF-31) — ré-insère le texte du tampon interne.
/// Exception documentée : le texte arbitraire ne peut pas entrer par IPC.
#[tauri::command]
pub fn rescue_paste_again(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(plume_core::InputEvent::PasteAgain));
    Ok(())
}

/// « Réessayer » après panne moteur (RF-21) : l'orchestrateur relance le
/// pipeline avec l'audio qu'il a conservé.
#[tauri::command]
pub fn rescue_retry(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(plume_core::InputEvent::Retry));
    Ok(())
}

#[tauri::command]
pub fn rescue_close(app: AppHandle<Wry>) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("rescue") {
        let _ = w.close();
    }
    Ok(())
}

// -------------------------------------------------------- dictée ----

/// Annule une dictée en cours ou remet à zéro l'état de récupération
/// (l'audio retenu est libéré — invariant 3).
#[tauri::command]
pub fn cancel_dictation(shared: State<'_, UiShared>) -> Result<(), String> {
    let _ = shared
        .tx
        .send(WorkerMsg::Input(plume_core::InputEvent::Cancel));
    Ok(())
}

// ------------------------------------------------- diagnostic RF-34 ----

/// Exporte le rapport de support (rédigé par construction). Destination :
/// chemin fourni, sinon `Bureau\rapport-diagnostic-plume.txt`.
#[tauri::command]
pub fn diagnostics_export(
    shared: State<'_, UiShared>,
    destination: Option<String>,
) -> Result<String, String> {
    let log_file = plume_config::Config::default_dir()
        .join("logs")
        .join("plume.log");
    let raw = std::fs::read_to_string(&log_file).unwrap_or_default();
    let lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();
    let report = plume_diagnostics::export_report(&lines, shared.logger.uptime());
    let path = destination.map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("USERPROFILE")
            .map(|u| PathBuf::from(u).join("Desktop"))
            .unwrap_or_else(std::env::temp_dir)
            .join("rapport-diagnostic-plume.txt")
    });
    std::fs::write(&path, report).map_err(|e| format!("{} : {e}", strings::ERR_WRITE))?;
    Ok(path.display().to_string())
}
