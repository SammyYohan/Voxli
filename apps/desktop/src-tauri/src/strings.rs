//! Chaînes visibles par l'utilisateur, centralisées (convention docs/05 §6) :
//! tout message affiché par l'application est écrit **en français**, ici.
//!
//! Aucune de ces chaînes ne porte de contenu dicté : la fenêtre de secours
//! affiche le texte depuis le tampon (exception RF-31), pas depuis ce module.

/// Titre de la fenêtre de secours (RF-31).
pub const RESCUE_WINDOW_TITLE: &str = "Voxli — Texte de secours";

// -------------------------------------------------------- notifications ---

/// Aucune voix exploitable : aucune insertion (RF-18).
pub const MSG_NOTHING: &str = "Aucune voix exploitable — rien n'a été inséré.";

/// Demande refusée : une session est déjà active (RF-05).
pub const MSG_BUSY: &str = "Une dictée est déjà en cours — terminez-la ou annulez-la d'abord.";

/// Panne moteur récupérable (RF-20/RF-21) : audio conservé.
pub const MSG_RECOVERED: &str =
    "La transcription a échoué. L'audio est conservé : utilisez « Réessayer » dans la fenêtre de secours.";

// ------------------------------------------------------------- refus RF-29 ---

/// Champ de mot de passe ou fenêtre élevée (RF-29) — ton neutre, sans
/// deviner le type de champ.
pub const MSG_REFUSED_SENSITIVE: &str =
    "Insertion refusée : champ protégé ou fenêtre à élévation. Le texte est conservé dans la fenêtre de secours.";

/// Application exclue parce que configurée comme sensible (RF-29).
pub const MSG_REFUSED_EXCLUDED: &str =
    "Insertion refusée : application exclue par vos réglages. Le texte est conservé dans la fenêtre de secours.";

/// Fenêtre cible fermée ou perdue entre la prise et le collage (RF-32).
pub const MSG_REFUSED_TARGET: &str =
    "Insertion refusée : la fenêtre cible n'est plus disponible. Le texte est conservé dans la fenêtre de secours.";

/// Erreur technique générique (codes stables de RF-34, jamais de contenu).
pub const MSG_ERROR_TMPL: &str =
    "Erreur technique ({module}, code {code}) — voir le rapport de diagnostic.";

// ---------------------------------------------------------------- secours ---

// (Le texte « fenêtre vide » du secours vit dans le frontend :
// apps/desktop/src/rescue.ts — seule source pour éviter une divergence.)

// -------------------------------------------------------------- erreurs ---

pub const ERR_CONFIG_SAVE: &str = "Enregistrement de la configuration impossible";
pub const ERR_HOTKEY_P0: &str =
    "Les raccourcis ne sont pas modifiables depuis l'interface en P0. Modifiez %APPDATA%\\Voxli\\config.json puis relancez Voxli.";
pub const ERR_WRITE: &str = "Écriture du fichier impossible";
pub const ERR_WORKER: &str = "Démarrage du thread pipeline impossible";

/// Message d'insertion réussie avec le niveau retenu (ADR-06).
pub fn msg_inserted(level: &str) -> String {
    format!("Texte inséré (niveau « {level} »).")
}

/// Variantes de refus → message.
pub fn msg_refused(reason: &str) -> &'static str {
    match reason {
        "sensitive-field" => MSG_REFUSED_SENSITIVE,
        "excluded-app" => MSG_REFUSED_EXCLUDED,
        _ => MSG_REFUSED_TARGET,
    }
}

/// Message d'erreur technique substitué (RF-34 : module + code, aucun contenu).
pub fn msg_error(module: &str, code: u32) -> String {
    MSG_ERROR_TMPL
        .replace("{module}", module)
        .replace("{code}", &code.to_string())
}

/// Titre de la pastille (RF-09) — invisible (sans décor), réservé
/// à l'accessibilité.
pub const PILL_WINDOW_TITLE: &str = "Voxli — pastille d'état";

// ------------------------------------------------------------ nouveautés ---

/// RF-11 : avertissement unique, 30 s avant le plafond de durée.
pub fn msg_max_duration(remaining_ms: u64) -> String {
    let seconds = (remaining_ms / 1000).max(1);
    format!(
        "Dictée longue : la prise s'arrêtera dans environ {seconds} s. Relâchez et reprenez ensuite."
    )
}

/// RF-02 (c) : la dictée est verrouillée par double appui.
pub const MSG_LOCKED: &str =
    "Dictée verrouillée : elle continue même quand vous relâchez la touche. Double appui pour arrêter.";

/// RF-05 : une prise est déjà en attente, elle démarrera à la fin.
pub const MSG_QUEUED: &str =
    "Une prise est déjà terminée — la suivante démarrera automatiquement à la fin.";

/// RF-32 : la cible mémorisée a disparu ; insertion dans la fenêtre courante.
pub const MSG_RETARGET: &str =
    "La fenêtre de départ a changé — le texte sera inséré dans la fenêtre active.";
