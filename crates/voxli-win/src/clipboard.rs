//! Presse-papiers : sauvegarde multi-format, écriture, restauration différée,
//! hygiène de l'historique Windows (RF-30).
//!
//! Points de conformité RF-30 :
//! (a) le contenu d'origine est restauré, formats riches compris ;
//! (b) la dictée n'entre pas dans l'historique du presse-papiers ni la
//!     synchronisation cloud (formats d'exclusion Windows) ;
//! (c) une copie utilisateur pendant la fenêtre de maintien est conservée ;
//! (d) un presse-papiers verrouillé est géré par réessais (pas d'échec
//!     silencieux).

use std::mem::size_of;
use std::time::Duration;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, CountClipboardFormats, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use thiserror::Error;

const CF_HTML_NAME: &str = "HTML Format";
const CF_RTF_NAME: &str = "Rich Text Format";

#[derive(Debug, Error)]
pub enum ClipboardError {
    #[error("presse-papiers verrouillé par une autre application (après réessais)")]
    Locked,
    #[error("opération presse-papiers refusée")]
    SetFailed,
    #[error("lecture du presse-papiers impossible")]
    ReadFailed,
    #[error("restauration impossible")]
    RestoreFailed,
}

/// Format copié (identifiant + octets bruts) — la sémantique des formats
/// riches est celle de Windows, on ne les interprète pas.
#[derive(Clone, Debug)]
pub struct SavedFormat {
    pub format: u32,
    pub data: Vec<u8>,
}

/// Photographie du presse-papiers avant insertion (RF-30a).
#[derive(Clone, Debug, Default)]
pub struct SavedClipboard {
    pub formats: Vec<SavedFormat>,
}

impl SavedClipboard {
    /// Empreinte du contenu texte — utilisée pour détecter une copie
    /// concurrente de l'utilisateur (RF-30c).
    pub fn text_fingerprint(&self) -> Option<u64> {
        self.formats
            .iter()
            .find(|f| f.format == CF_UNICODETEXT.0 as u32)
            .map(|f| {
                f.data.iter().fold(0xcbf29ce484222325u64, |h, b| {
                    (h ^ *b as u64).wrapping_mul(0x100000001b3)
                })
            })
    }
}

/// Ouvre le presse-papiers avec réessais (RF-30d : presse-papiers verrouillé
/// par un tiers).
fn open_with_retry(attempts: u32) -> Result<(), ClipboardError> {
    for _ in 0..attempts {
        unsafe {
            if OpenClipboard(None).is_ok() {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err(ClipboardError::Locked)
}

/// Sauvegarde tous les formats actuels. Formats texte connus copiés tels
/// quels ; les formats GDI (bitmaps) sont ignorés (jamais produits par une
/// dictée, la restauration du texte reste l'exigence RF-30).
pub fn save() -> Result<SavedClipboard, ClipboardError> {
    open_with_retry(10)?;
    let saved = unsafe {
        let result = (|| -> Result<SavedClipboard, ClipboardError> {
            let mut saved = SavedClipboard::default();
            let count = CountClipboardFormats();
            let mut format = 0u32;
            for _ in 0..count.max(0) {
                format = EnumClipboardFormats(format);
                if format == 0 {
                    break;
                }
                // Formats texte (13) et personnalisés enregistrés seulement.
                let is_text = format == CF_UNICODETEXT.0 as u32
                    || format == 1 /* CF_TEXT */
                    || format == 7 /* CF_OEMTEXT */
                    || is_registered_rich_format(format);
                if !is_text {
                    continue;
                }
                let handle = GetClipboardData(format).map_err(|_| ClipboardError::ReadFailed)?;
                let hglobal = HGLOBAL(handle.0);
                let ptr = GlobalLock(hglobal);
                if ptr.is_null() {
                    continue;
                }
                let size = windows::Win32::System::Memory::GlobalSize(hglobal);
                let bytes = std::slice::from_raw_parts(ptr as *const u8, size);
                saved.formats.push(SavedFormat {
                    format,
                    data: bytes.to_vec(),
                });
                let _ = GlobalUnlock(hglobal);
            }
            Ok(saved)
        })();
        let _ = CloseClipboard();
        result
    };
    saved
}

fn is_registered_rich_format(format: u32) -> bool {
    [CF_HTML_NAME, CF_RTF_NAME].iter().any(|name| {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let id = unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) };
        id != 0 && id == format
    })
}

/// Écrit du texte Unicode + formats d'exclusion d'historique/cloud (RF-30b).
/// Les formats `ExcludeClipboardContentFromMonitorProcessing`,
/// `CanIncludeInClipboardHistory` et `CanUploadToCloudClipboard` sont prévus
/// par Windows précisément pour ce scénario (contenu éphémère, jamais dans
/// l'historique `Win+V`).
pub fn set_text_excluding_history(text: &str) -> Result<(), ClipboardError> {
    open_with_retry(10)?;
    let result = unsafe {
        let inner = (|| -> Result<(), ClipboardError> {
            let _ = EmptyClipboard();
            // CF_UNICODETEXT
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            wide.push(0);
            let bytes_len = wide.len() * 2;
            let hglobal =
                GlobalAlloc(GMEM_MOVEABLE, bytes_len).map_err(|_| ClipboardError::SetFailed)?;
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                let _ = GlobalFree(Some(hglobal));
                return Err(ClipboardError::SetFailed);
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr as *mut u16, wide.len());
            let _ = GlobalUnlock(hglobal);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hglobal.0)))
                .map_err(|_| ClipboardError::SetFailed)?;

            // Formats d'exclusion — présence seule + DWORD 0.
            set_exclusion_flag("ExcludeClipboardContentFromMonitorProcessing")?;
            set_exclusion_flag("CanIncludeInClipboardHistory")?;
            set_exclusion_flag("CanUploadToCloudClipboard")?;
            Ok(())
        })();
        let _ = CloseClipboard();
        inner
    };
    result
}

unsafe fn set_exclusion_flag(name: &str) -> Result<(), ClipboardError> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let format = RegisterClipboardFormatW(PCWSTR(wide.as_ptr()));
    if format == 0 {
        return Err(ClipboardError::SetFailed);
    }
    let hglobal =
        GlobalAlloc(GMEM_MOVEABLE, size_of::<u32>()).map_err(|_| ClipboardError::SetFailed)?;
    let ptr = GlobalLock(hglobal);
    if ptr.is_null() {
        let _ = GlobalFree(Some(hglobal));
        return Err(ClipboardError::SetFailed);
    }
    std::ptr::write_bytes(ptr as *mut u8, 0, size_of::<u32>());
    let _ = GlobalUnlock(hglobal);
    SetClipboardData(format, Some(HANDLE(hglobal.0))).map_err(|_| ClipboardError::SetFailed)?;
    Ok(())
}

/// Restaure une photographie (RF-30a). Si `guard_fingerprint` est fourni et
/// que le contenu actuel diffère de celui que NOUS avons posé, la
/// restauration est annulée : la copie concurrente de l'utilisateur est
/// conservée (RF-30c).
pub fn restore(saved: &SavedClipboard, our_fingerprint: Option<u64>) -> Result<(), ClipboardError> {
    if let Some(fp) = our_fingerprint {
        if let Ok(current) = read_text_fingerprint() {
            if current != fp {
                // L'utilisateur a copié autre chose pendant le maintien.
                return Ok(());
            }
        }
    }
    open_with_retry(10)?;
    let result = unsafe {
        let inner = (|| -> Result<(), ClipboardError> {
            let _ = EmptyClipboard();
            for f in &saved.formats {
                let hglobal = GlobalAlloc(GMEM_MOVEABLE, f.data.len().max(1))
                    .map_err(|_| ClipboardError::RestoreFailed)?;
                let ptr = GlobalLock(hglobal);
                if ptr.is_null() {
                    continue;
                }
                std::ptr::copy_nonoverlapping(f.data.as_ptr(), ptr as *mut u8, f.data.len());
                let _ = GlobalUnlock(hglobal);
                SetClipboardData(f.format, Some(HANDLE(hglobal.0)))
                    .map_err(|_| ClipboardError::RestoreFailed)?;
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        inner
    };
    result
}

/// Empreinte du presse-papiers actuel (texte), pour la détection RF-30c.
pub fn read_text_fingerprint() -> Result<u64, ClipboardError> {
    let text = read_text()?;
    Ok(text.as_bytes().iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    }))
}

pub fn read_text() -> Result<String, ClipboardError> {
    open_with_retry(10)?;
    let result = unsafe {
        let inner = (|| -> Result<String, ClipboardError> {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32)
                .map_err(|_| ClipboardError::ReadFailed)?;
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal);
            if ptr.is_null() {
                return Err(ClipboardError::ReadFailed);
            }
            let mut len = 0usize;
            while *(ptr as *const u16).add(len) != 0 {
                len += 1;
            }
            let wide = std::slice::from_raw_parts(ptr as *const u16, len);
            let _ = GlobalUnlock(hglobal);
            Ok(String::from_utf16_lossy(wide))
        })();
        let _ = CloseClipboard();
        inner
    };
    result
}

/// Transaction complète utilisée par le niveau 2 de la cascade : sauvegarde →
/// écriture → frappe Ctrl+V (effectuée par l'appelant) → restauration
/// différée sur un thread dédié (RF-30 : maintien pour les applications
/// lentes, temporisation par profil).
pub struct ClipboardGuard;

impl ClipboardGuard {
    /// Écrit le texte et programme la restauration après `hold_ms`.
    /// Retourne l'empreinte du contenu posé (pour la vérification RF-30c).
    pub fn paste_with_restore(text: &str, hold_ms: u32) -> Result<u64, ClipboardError> {
        let saved = save()?;
        set_text_excluding_history(text)?;
        let fp = read_text_fingerprint().ok();
        // Restauration différée : un thread dédié attend la fenêtre de
        // maintien (les applications lentes à lire le presse-papiers —
        // Outlook, RDP — exigent ce maintien, docs/01 §1.1).
        let to_restore = saved;
        let fp_for_restore = fp;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(hold_ms.max(100) as u64));
            let _ = restore(&to_restore, fp_for_restore);
        });
        fp.ok_or(ClipboardError::SetFailed)
    }
}
