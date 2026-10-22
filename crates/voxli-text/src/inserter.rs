//! Insertion executor — implements the kernel's `Inserter` trait.
//!
//! Runs the plan level by level; every failure tells the next level *why*
//! it failed (design rule docs/04 §4) and the reason is logged locally
//! **without content** (RF-34) to measure the per-app fallback rate
//! (hypothesis H3).

use voxli_core::{
    FocusSnapshot, InsertError, InsertLevel, InsertPlan, InsertReport, Inserter, Profile,
};
use voxli_win::clipboard::ClipboardGuard;
use voxli_win::sendinput::{send_ctrl_v, type_unicode};
use voxli_win::uia::Uia;

pub struct WindowsInserter {
    uia: Option<Uia>,
    /// Hold timeout actually applied at level 2: the restore thread
    /// (`paste_with_restore`) waits this long before restoring the old
    /// clipboard.
    pub hold_ms_default: u32,
    /// Set from the config (worker.rs) but **read nowhere today**: the
    /// "slow apps" timeout (Outlook, RDP) would suppose choosing the hold
    /// based on the application identified at insertion time, which
    /// `try_level` does not do yet. Impact: a uniform `hold_ms_default`
    /// delay everywhere for now. Wiring tracked via docs/09 (M2).
    pub hold_ms_slow_apps: u32,
    /// Same, for remote desktop.
    pub hold_ms_remote_desktop: u32,
    /// Set but **irreversibly without effect**: the RF-30b hygiene flags
    /// (`CanIncludeInClipboardHistory` etc.) are set unconditionally by
    /// `set_text_excluding_history` in voxli-win. Turning this setting off
    /// would require a low-level parameter.
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

    /// Runs one level; returns the failure reason (short code), if any.
    fn try_level(&self, level: InsertLevel, text: &str) -> Result<(), &'static str> {
        match level {
            InsertLevel::ValuePattern => {
                let Some(uia) = &self.uia else {
                    return Err("uia-indisponible");
                };
                let element = uia.focused_element().map_err(|_| "focus-illisible")?;
                // `SetValue` **replaces the control's whole value** (Windows
                // API): write ONLY into an empty field, otherwise the
                // dictated text would destroy the text already there
                // (observed bug). Non-empty or unreadable field → immediate
                // fallback to the next level (clipboard: Ctrl+V inserts at
                // the cursor, without erasing). Codes logged without content
                // (RF-34).
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
                // Full hygiene: backup, exclusion formats, Ctrl+V, deferred
                // restore (RF-30).
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
        // Pure function — tested outside Windows (plan.rs).
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
        // Every level failed: the text stays in the kernel's buffer
        // (RF-22) and the rescue window takes over (RF-31).
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
