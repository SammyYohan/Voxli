//! UI Automation — sensitive-field detection and level 1 insertion
//! (ADR-06, RF-28, RF-29).
//!
//! Windows facts (docs/03 §5):
//! - `ValuePattern.SetValue` only applies to simple controls; multiline
//!   controls do not expose the pattern, and `TextPattern` cannot
//!   write — hence the cascade;
//! - `CurrentIsPassword` is the only reliable detection of a password
//!   field: no insertion and no read in those fields (RF-29).
//!
//! All COM lives here, on the injection thread (COM initialization is
//! per-thread). The other modules only go through `probe`/`set_value`.

use windows::core::BSTR;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationValuePattern,
    UIA_ValuePatternId,
};

/// Capabilities read through UIA for the `FocusSnapshot` (voxli-core).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiCapabilities {
    pub is_password: bool,
    pub enabled: bool,
    pub value_pattern_available: bool,
}

pub struct Uia {
    automation: IUIAutomation,
}

// NOTE: `Uia` is deliberately NOT Send/Sync. COM is initialized STA
// (COINIT_APARTMENTTHREADED) below; crossing apartments would violate COM
// threading rules. Every use site constructs its own `Uia` on its own
// thread (foreground probe, insertion cascade) and drops it there.

impl Uia {
    /// Initializes COM for the current thread and creates the automation
    /// object. Call once per injection thread.
    pub fn new() -> Result<Self, String> {
        unsafe {
            // S_FALSE (already initialized) is a success; not a fatal error.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| e.to_string())?;
            Ok(Self { automation })
        }
    }

    /// Focused element (preferred: it is exactly the typing target).
    pub fn focused_element(&self) -> Result<IUIAutomationElement, String> {
        unsafe {
            self.automation
                .GetFocusedElement()
                .map_err(|e| e.to_string())
        }
    }

    /// Element by window handle (remembered target, RF-32).
    pub fn element_from_hwnd(&self, hwnd: isize) -> Result<IUIAutomationElement, String> {
        unsafe {
            self.automation
                .ElementFromHandle(windows::Win32::Foundation::HWND(hwnd as *mut _))
                .map_err(|e| e.to_string())
        }
    }

    /// Reads an element's capabilities: password? editable? ValuePattern
    /// available? (RF-28, RF-29.)
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
            // GetCurrentPatternAs performs the pattern's QueryInterface directly.
            element
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .map_err(|e| e.to_string())
        }
    }

    /// Reads the control's current value via `ValuePattern` — mandatory
    /// precondition to `set_value`: "SetValue" **replaces the entire
    /// value**; writing into a non-empty field would destroy its content
    /// (hardened RF-28: the dictation never destroys existing text).
    pub fn current_value(&self, element: &IUIAutomationElement) -> Result<String, String> {
        let pattern = self.value_pattern(element)?;
        let v = unsafe { pattern.CurrentValue().map_err(|e| e.to_string())? };
        Ok(v.to_string())
    }

    /// Writes a value via `ValuePattern` then **reads it back** to verify
    /// (RF-28: "SetValue then read-back verification").
    pub fn set_value(&self, element: &IUIAutomationElement, text: &str) -> Result<(), String> {
        let pattern = self.value_pattern(element)?;
        let bstr = BSTR::from(text);
        unsafe {
            pattern.SetValue(&bstr).map_err(|e| e.to_string())?;
            // Read-back verification.
            let read_back = pattern.CurrentValue().map_err(|e| e.to_string())?;
            if read_back != text {
                return Err("relecture différente du texte posé".into());
            }
        }
        Ok(())
    }
}
