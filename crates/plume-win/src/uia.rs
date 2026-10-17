//! UI Automation — détection de champ sensible et insertion niveau 1
//! (ADR-06, RF-28, RF-29).
//!
//! Faits Windows (docs/03 §5) :
//! - `ValuePattern.SetValue` ne vaut que pour les contrôles simples ; les
//!   contrôles multilignes n'exposent pas le motif, et `TextPattern` ne
//!   permet pas d'écrire — d'où la cascade ;
//! - `CurrentIsPassword` est la seule détection fiable d'un champ de mot de
//!   passe : aucune insertion ni lecture dans ces champs (RF-29).
//!
//! Tout le COM vit ici, sur le thread d'injection (initialisation COM par
//! thread). Les autres modules ne voient que `probe`/`set_value_focused`.

use windows::core::BSTR;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
    UIA_ValuePatternId,
};

/// Capacités lues par UIA pour le `FocusSnapshot` (plume-core).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiCapabilities {
    pub is_password: bool,
    pub enabled: bool,
    pub value_pattern_available: bool,
}

pub struct Uia {
    automation: IUIAutomation,
}

// Les interfaces UIA sont à thread cloisonné (STA) mais la coclasse accepte
// un client MTA ; l'orchestrateur garde tout usage sur le thread d'injection.
unsafe impl Send for Uia {}
unsafe impl Sync for Uia {}

impl Uia {
    /// Initialise COM pour le thread courant et crée l'automate. À appeler une
    /// fois par thread d'injection.
    pub fn new() -> Result<Self, String> {
        unsafe {
            // S_FALSE (déjà initialisé) est un succès ; pas d'erreur fatale.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
            Ok(Self { automation })
        }
    }

    /// Élément focalisé (préféré : c'est exactement la cible de frappe).
    pub fn focused_element(&self) -> Result<IUIAutomationElement, String> {
        unsafe {
            self.automation
                .GetFocusedElement()
                .map_err(|e| e.to_string())
        }
    }

    /// Élément par handle de fenêtre (cible mémorisée, RF-32).
    pub fn element_from_hwnd(&self, hwnd: isize) -> Result<IUIAutomationElement, String> {
        unsafe {
            self.automation
                .ElementFromHandle(windows::Win32::Foundation::HWND(hwnd as *mut _))
                .map_err(|e| e.to_string())
        }
    }

    /// Lit les capacités d'un élément : mot de passe ? éditable ? ValuePattern
    /// disponible ? (RF-28, RF-29.)
    pub fn probe(&self, element: &IUIAutomationElement) -> UiCapabilities {
        unsafe {
            let is_password = element
                .CurrentIsPassword()
                .map(|v| v.as_bool())
                .unwrap_or(false);
            let enabled = element
                .CurrentIsEnabled()
                .map(|v| v.as_bool())
                .unwrap_or(true);
            let value_pattern_available = self.value_pattern(element).is_ok();
            UiCapabilities {
                is_password,
                enabled,
                value_pattern_available,
            }
        }
    }

    fn value_pattern(
        &self,
        element: &IUIAutomationElement,
    ) -> Result<IUIAutomationValuePattern, String> {
        unsafe {
            // GetCurrentPatternAs fait le QueryInterface du motif directement.
            element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .map_err(|e| e.to_string())
        }
    }

    /// Écrit une valeur via `ValuePattern` puis **relit** pour vérifier
    /// (RF-28 : « SetValue puis vérification de lecture »).
    pub fn set_value(&self, element: &IUIAutomationElement, text: &str) -> Result<(), String> {
        let pattern = self.value_pattern(element)?;
        let bstr = BSTR::from(text);
        unsafe {
            pattern.SetValue(&bstr).map_err(|e| e.to_string())?;
            // Vérification de relecture.
            let read_back = pattern.CurrentValue().map_err(|e| e.to_string())?;
            if read_back.to_string() != text {
                return Err("relecture différente du texte posé".into());
            }
        }
        Ok(())
    }
}
