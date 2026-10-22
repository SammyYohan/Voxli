//! User-visible strings, centralized (convention docs/05 §6):
//! every message displayed by the application is written **in French**, here.
//!
//! None of these strings carries dictated content: the rescue window
//! displays the text from the buffer (RF-31 exception), not from this module.

/// Rescue window title (RF-31).
pub const RESCUE_WINDOW_TITLE: &str = "Voxli — Texte de secours";

// -------------------------------------------------------- notifications ---

/// No usable voice: no insertion (RF-18).
pub const MSG_NOTHING: &str = "Aucune voix exploitable — rien n'a été inséré.";

/// Request refused: a session is already active (RF-05).
pub const MSG_BUSY: &str = "Une dictée est déjà en cours — terminez-la ou annulez-la d'abord.";

/// Recoverable engine failure (RF-20/RF-21): audio kept.
pub const MSG_RECOVERED: &str =
    "La transcription a échoué. L'audio est conservé : utilisez « Réessayer » dans la fenêtre de secours.";

// ------------------------------------------------------------- RF-29 refusals ---

/// Password field or elevated window (RF-29) — neutral tone, without
/// guessing the field type.
pub const MSG_REFUSED_SENSITIVE: &str =
    "Insertion refusée : champ protégé ou fenêtre à élévation. Le texte est conservé dans la fenêtre de secours.";

/// Application excluded because configured as sensitive (RF-29).
pub const MSG_REFUSED_EXCLUDED: &str =
    "Insertion refusée : application exclue par vos réglages. Le texte est conservé dans la fenêtre de secours.";

/// Target window closed or lost between the capture and the paste (RF-32).
pub const MSG_REFUSED_TARGET: &str =
    "Insertion refusée : la fenêtre cible n'est plus disponible. Le texte est conservé dans la fenêtre de secours.";

/// Generic technical error (stable RF-34 codes, never content).
pub const MSG_ERROR_TMPL: &str =
    "Erreur technique ({module}, code {code}) — voir le rapport de diagnostic.";

// ---------------------------------------------------------------- rescue ---

// (The "empty window" text of the rescue lives in the frontend:
// apps/desktop/src/rescue.ts — single source to avoid divergence.)

// -------------------------------------------------------------- errors ---

pub const ERR_CONFIG_SAVE: &str = "Enregistrement de la configuration impossible";
/// A hotkey could not be registered with the OS: almost always
/// already taken by another application (the log carries the RF-34
/// detail). Since the interface edits hotkeys (RF-01), the user can fix
/// it here.
pub const ERR_HOTKEY_TAKEN: &str =
    "Ce raccourci est indisponible — il est probablement déjà pris par une autre application. Modifiez-le dans les réglages de Voxli.";
pub const ERR_WRITE: &str = "Écriture du fichier impossible";
pub const ERR_WORKER: &str = "Démarrage du thread pipeline impossible";

/// Successful insertion message with the retained level (ADR-06).
pub fn msg_inserted(level: &str) -> String {
    format!("Texte inséré (niveau « {level} »).")
}

/// Refusal variants → message.
pub fn msg_refused(reason: &str) -> &'static str {
    match reason {
        "sensitive-field" => MSG_REFUSED_SENSITIVE,
        "excluded-app" => MSG_REFUSED_EXCLUDED,
        _ => MSG_REFUSED_TARGET,
    }
}

/// Substituted technical error message (RF-34: module + code, no content).
pub fn msg_error(module: &str, code: u32) -> String {
    MSG_ERROR_TMPL
        .replace("{module}", module)
        .replace("{code}", &code.to_string())
}

/// Pill title (RF-09) — invisible (no decorations), reserved
/// for accessibility.
pub const PILL_WINDOW_TITLE: &str = "Voxli — pastille d'état";

// ------------------------------------------------------------ additions ---

/// RF-11: single warning, 30 s before the duration ceiling.
pub fn msg_max_duration(remaining_ms: u64) -> String {
    let seconds = (remaining_ms / 1000).max(1);
    format!(
        "Dictée longue : la prise s'arrêtera dans environ {seconds} s. Relâchez et reprenez ensuite."
    )
}

/// RF-02 (c): the dictation is locked by a double push.
pub const MSG_LOCKED: &str =
    "Dictée verrouillée : elle continue même quand vous relâchez la touche. Double appui pour arrêter.";

/// RF-05: a capture is already waiting, it will start at the end.
pub const MSG_QUEUED: &str =
    "Une prise est déjà terminée — la suivante démarrera automatiquement à la fin.";

/// RF-32: the remembered target has disappeared; insertion into the current window.
pub const MSG_RETARGET: &str =
    "La fenêtre de départ a changé — le texte sera inséré dans la fenêtre active.";
