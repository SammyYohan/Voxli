//! Frappe Unicode via `SendInput` — niveau 3 de la cascade d'insertion
//! (ADR-06) : caractère par caractère, sans toucher au presse-papiers.
//!
//! Plus lent et incompatible avec certains IME/jeux — mais ne détruit rien.
//! Utilisé quand le presse-papiers est indisponible ou interdit par le profil.

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
///
/// Bug constaté (18/10/2026) : le `V` était construit via `key_event`
/// (façade `KEYEVENTF_UNICODE`, `wVk = 0`) — sans ce drapeau, un INPUT
/// avec `wVk = 0` est **mort** (Windows exige un code virtuel 1-254) ;
/// le collage ne se produisait jamais, alors que `SendInput` rendait
/// « succès » (4 événements injectés). Les quatre événements passent
/// désormais par la façade `vk_event` (codes virtuels `VK_CONTROL` /
/// `VK_V` — le code virtuel de `V` n'est PAS un scan code).
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

// Espace réservé aux futurs modificateurs ; référence VK_SPACE pour éviter un
// avertissement d'import inutilisé jusqu'aux profils par application (RF-25).
#[allow(dead_code)]
const _KEEP: VIRTUAL_KEY = VK_SPACE;

#[cfg(test)]
mod tests {
    use super::*;

    /// Régression du collage mort : chaque événement de `send_ctrl_v`
    /// porte un code virtuel valide (jamais 0) et les drapeaux attendus.
    /// Un `wVk = 0` sans `KEYEVENTF_UNICODE` est un événement que Windows
    /// accepte… sans effet — d'où le symptôme « insertion réussie, rien
    /// n'apparaît ».
    #[test]
    fn ctrl_v_porte_des_codes_virtuels_valides() {
        // Accès direct aux 4 INPUT sans déclencher une frappe réelle :
        // la construction est dupliquée ici plutôt que d'appeler
        // send_ctrl_v (qui taperait dans la fenêtre active du poste).
        let inputs = [
            vk_event(VK_CONTROL, KEYEVENTF_DOWN),
            vk_event(VK_V, KEYEVENTF_DOWN),
            vk_event(VK_V, KEYEVENTF_KEYUP.0),
            vk_event(VK_CONTROL, KEYEVENTF_KEYUP.0),
        ];
        // SAFETY : lecture pure des champs de l'union (aucun appel FFI).
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
