//! Unicode typing via `SendInput` — level 3 of the insertion cascade
//! (ADR-06): character by character, without touching the clipboard.
//!
//! Slower and incompatible with some IMEs/games — but destroys nothing.
//! Used when the clipboard is unavailable or forbidden by the profile.

use std::mem::size_of;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_SPACE, VK_V,
};

const KEYEVENTF_DOWN: u32 = 0;

fn key_event(scan: u16, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn vk_event(vk: VIRTUAL_KEY, flags: u32) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: KEYBD_EVENT_FLAGS(flags),
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Types out all UTF-16 units of `text` (surrogate pairs included).
pub fn type_unicode(text: &str) -> Result<(), String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.is_empty() {
        return Ok(());
    }
    // Sent in batches of 64 (reasonable limit of the system input queue).
    for chunk in units.chunks(64) {
        let mut inputs: Vec<INPUT> = Vec::with_capacity(chunk.len() * 2);
        for &unit in chunk {
            inputs.push(key_event(unit, KEYEVENTF_UNICODE.0));
            inputs.push(key_event(unit, KEYEVENTF_UNICODE.0 | KEYEVENTF_KEYUP.0));
        }
        let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            return Err(format!(
                "SendInput : {sent}/{} événements injectés",
                inputs.len()
            ));
        }
    }
    Ok(())
}

/// Sends `Ctrl+V` (level 2: paste after the clipboard write).
///
/// Bug observed (18/10/2026): the `V` was built via `key_event`
/// (the `KEYEVENTF_UNICODE` path, `wVk = 0`) — without that flag, an INPUT
/// with `wVk = 0` is **dead** (Windows requires a virtual code 1-254);
/// the paste never happened, while `SendInput` reported
/// "success" (4 events injected). The four events now go through the
/// `vk_event` path (virtual codes `VK_CONTROL` /
/// `VK_V` — the virtual code of `V` is NOT a scan code).
pub fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        vk_event(VK_CONTROL, KEYEVENTF_DOWN),
        vk_event(VK_V, KEYEVENTF_DOWN),
        vk_event(VK_V, KEYEVENTF_KEYUP.0),
        vk_event(VK_CONTROL, KEYEVENTF_KEYUP.0),
    ];
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(format!(
            "SendInput : {sent}/{} événements injectés",
            inputs.len()
        ));
    }
    Ok(())
}

// Reserved space for future modifiers; the VK_SPACE reference avoids an
// unused-import warning until the per-application profiles (RF-25).
#[allow(dead_code)]
const _KEEP: VIRTUAL_KEY = VK_SPACE;

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for the dead paste: every event built by
    /// `send_ctrl_v` carries a valid virtual code (never 0) and the
    /// expected flags. A `wVk = 0` without `KEYEVENTF_UNICODE` is an event
    /// that Windows accepts… with no effect — hence the symptom
    /// "insertion succeeded, nothing appears".
    #[test]
    fn ctrl_v_porte_des_codes_virtuels_valides() {
        // Direct access to the 4 INPUTs without triggering a real
        // keystroke: the construction is duplicated here rather than
        // calling send_ctrl_v (which would type into the active
        // window of the machine).
        let inputs = [
            vk_event(VK_CONTROL, KEYEVENTF_DOWN),
            vk_event(VK_V, KEYEVENTF_DOWN),
            vk_event(VK_V, KEYEVENTF_KEYUP.0),
            vk_event(VK_CONTROL, KEYEVENTF_KEYUP.0),
        ];
        // SAFETY: pure read of the union fields (no FFI call).
        let vks: Vec<VIRTUAL_KEY> = inputs
            .iter()
            .map(|i| unsafe { i.Anonymous.ki.wVk })
            .collect();
        let flags: Vec<u32> = inputs
            .iter()
            .map(|i| unsafe { i.Anonymous.ki.dwFlags.0 })
            .collect();
        assert_eq!(vks[0], VK_CONTROL);
        assert_eq!(vks[1], VK_V);
        assert_eq!(vks[2], VK_V);
        assert_eq!(vks[3], VK_CONTROL);
        for vk in &vks {
            assert_ne!(vk.0, 0, "événement mort (wVk = 0)");
        }
        for f in &flags {
            assert_eq!(f & KEYEVENTF_UNICODE.0, 0);
        }
        assert_eq!(flags[0] & KEYEVENTF_KEYUP.0, 0);
        assert_eq!(flags[1] & KEYEVENTF_KEYUP.0, 0);
        assert_eq!(flags[2] & KEYEVENTF_KEYUP.0, KEYEVENTF_KEYUP.0);
        assert_eq!(flags[3] & KEYEVENTF_KEYUP.0, KEYEVENTF_KEYUP.0);
    }
}
