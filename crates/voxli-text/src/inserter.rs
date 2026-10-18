//! Exécuteur d'insertion — implémente le trait `Inserter` du noyau.
//!
//! Exécute le plan niveau par niveau ; chaque échec indique au niveau
//! suivant *pourquoi* il a échoué (règle de conception docs/04 §4) et le
//! motif est consigné localement **sans contenu** (RF-34) pour mesurer le
//! taux de repli par application (hypothèse H3).

use voxli_core::{
    FocusSnapshot, InsertError, InsertLevel, InsertPlan, InsertReport, Inserter, Profile,
};
use voxli_win::clipboard::ClipboardGuard;
use voxli_win::sendinput::{send_ctrl_v, type_unicode};
use voxli_win::uia::Uia;

pub struct WindowsInserter {
    uia: Option<Uia>,
    /// Temporisations de maintien du presse-papiers (par profil d'app, RF-30).
    pub hold_ms_default: u32,
    pub hold_ms_slow_apps: u32,
    pub hold_ms_remote_desktop: u32,
    pub exclude_from_clipboard_history: bool,
}

impl WindowsInserter {
    pub fn new() -> Self {
        Self {
            uia: Uia::new().ok(),
            hold_ms_default: 500,
            hold_ms_slow_apps: 3000,
            hold_ms_remote_desktop: 5000,
            exclude_from_clipboard_history: true,
        }
    }

    /// Exécute un niveau ; retourne le motif d'échec (code court) le cas
    /// échéant.
    fn try_level(&self, level: InsertLevel, text: &str) -> Result<(), &'static str> {
        match level {
            InsertLevel::ValuePattern => {
                let Some(uia) = &self.uia else {
                    return Err("uia-indisponible");
                };
                let element = uia.focused_element().map_err(|_| "focus-illisible")?;
                // `SetValue` **remplace la valeur entière** du contrôle
                // (API Windows) : n'écrire QUE dans un champ vide, sinon le
                // texte dicté détruirait le texte déjà posé (bug constaté).
                // Champ non vide ou illisible → repli immédiat au niveau
                // suivant (presse-papiers : Ctrl+V insère au curseur, sans
                // effacement). Codes consignés sans contenu (RF-34).
                let current = uia
                    .current_value(&element)
                    .map_err(|_| "valeur-illisible")?;
                if !current.is_empty() {
                    return Err("valeur-non-vide");
                }
                uia.set_value(&element, text)
                    .map_err(|_| "setvalue-ou-relecture-echec")
            }
            InsertLevel::Clipboard => {
                // Hygiène complète : sauvegarde, formats d'exclusion,
                // Ctrl+V, restauration différée (RF-30).
                ClipboardGuard::paste_with_restore(text, self.hold_ms_default)
                    .map(|_| ())
                    .map_err(|_| "presse-papiers-echec")?;
                send_ctrl_v().map_err(|_| "ctrl-v-refuse")
            }
            InsertLevel::SendInput => type_unicode(text).map_err(|_| "sendinput-partiel"),
            InsertLevel::Rescue => Err("secours-demande"),
        }
    }
}

impl Default for WindowsInserter {
    fn default() -> Self {
        Self::new()
    }
}

impl Inserter for WindowsInserter {
    fn plan(&self, target: &FocusSnapshot, profile: &Profile) -> InsertPlan {
        // Fonction pure — testée hors Windows (plan.rs).
        crate::plan::plan_insertion(target, profile)
    }

    fn insert(&self, text: &str, plan: &InsertPlan) -> Result<InsertReport, InsertError> {
        if let Some(reason) = plan.refuse {
            return Err(match reason {
                voxli_core::RefuseReason::SensitiveField => InsertError::RefusedSensitive,
                voxli_core::RefuseReason::ExcludedApp => InsertError::RefusedExcluded,
                voxli_core::RefuseReason::TargetUnavailable => InsertError::TargetUnavailable,
            });
        }
        let mut report = InsertReport::default();
        for &level in &plan.levels {
            match self.try_level(level, text) {
                Ok(()) => {
                    report.level_used = Some(level);
                    return Ok(report);
                }
                Err(failure_code) => {
                    report.attempts.push((level, failure_code));
                }
            }
        }
        // Tous les niveaux ont échoué : le texte reste au tampon du noyau
        // (RF-22) et la fenêtre de secours prend le relais (RF-31).
        Err(InsertError::AllLevelsFailed(
            report
                .attempts
                .iter()
                .map(|(l, c)| format!("{}:{}", l.as_str(), c))
                .collect::<Vec<_>>()
                .join(","),
        ))
    }
}
