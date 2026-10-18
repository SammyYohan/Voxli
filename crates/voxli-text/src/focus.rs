//! Lecteur de focus Windows : alimente le `FocusSnapshot` mémorisé au
//! démarrage de la dictée (RF-32) avec les capacités UIA (RF-28, RF-29).

use voxli_core::{FocusReader, FocusSnapshot};
use voxli_win::{foreground_info, uia::Uia};

/// Exclusion d'applications portée par la configuration
/// (`insertion.excludedApps`, RF-29).
pub struct WindowsFocusReader {
    pub excluded_apps: Vec<String>,
    uia: Option<Uia>,
}

impl WindowsFocusReader {
    pub fn new(excluded_apps: Vec<String>) -> Self {
        // L'initialisation UIA peut échouer (COM indisponible) : dégradation
        // propre — le niveau 1 n'est simplement pas proposé (RF-28).
        let uia = Uia::new().ok();
        Self { excluded_apps, uia }
    }
}

impl FocusReader for WindowsFocusReader {
    fn snapshot(&self) -> Option<FocusSnapshot> {
        let info = foreground_info()?;
        let mut caps = voxli_core::FieldCapabilities::default();
        if let Some(uia) = &self.uia {
            if let Ok(element) = uia.focused_element() {
                let probed = uia.probe(&element);
                caps = voxli_core::FieldCapabilities {
                    value_pattern_writable: probed.value_pattern_available && probed.enabled,
                    editable: probed.enabled,
                    is_password: probed.is_password,
                    multiline: false,
                };
            }
        }
        let sensitive = self
            .excluded_apps
            .iter()
            .any(|a| a.eq_ignore_ascii_case(&info.app_id));
        if sensitive {
            caps.is_password = true; // même traitement : refus neutre (RF-29)
        }
        Some(FocusSnapshot {
            app_id: info.app_id,
            title: info.title,
            caps,
            elevated: info.elevated,
            valid: true,
        })
    }
}
