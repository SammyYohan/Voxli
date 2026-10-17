//! Frappe Unicode via `SendInput` — niveau 3 de la cascade d'insertion
//! (ADR-06) : caractère par caractère, sans toucher au presse-papiers.
//!
//! Plus lent et incompatible avec certains IME/jeux — mais ne détruit rien.
//! Utilisé quand le presse-papiers est indisponible ou interdit par le profil.

use std::mem::size_of;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_SPACE,
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

/// Fait défiler tous les éléments UTF-16 de `text` (pairs surrogates compris).
pub fn type_unicode(text: &str) -> Result<(), String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    if units.is_empty() {
        return Ok(());
    }
    // Envoi par lots de 64 (limite raisonnable de la file d'entrée système).
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

/// Envoie `Ctrl+V` (niveau 2 : collage après écriture du presse-papiers).
pub fn send_ctrl_v() -> Result<(), String> {
    let inputs = [
        vk_event(VK_CONTROL, KEYEVENTF_DOWN),
        key_event(0x56 /* 'V' */, KEYEVENTF_DOWN),
        key_event(0x56, KEYEVENTF_KEYUP.0),
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

// Espace réservé aux futurs modificateurs ; référence VK_SPACE pour éviter un
// avertissement d'import inutilisé jusqu'aux profils par application (RF-25).
#[allow(dead_code)]
const _KEEP: VIRTUAL_KEY = VK_SPACE;
