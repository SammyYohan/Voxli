//! Clipboard: multi-format save, write, deferred restore, Windows
//! clipboard history hygiene (RF-30).
//!
//! RF-30 compliance points:
//! (a) the original content is restored, rich formats included;
//! (b) the dictation never enters the clipboard history nor the cloud
//!     sync (Windows exclusion formats);
//! (c) a user copy during the hold window is preserved;
//! (d) a locked clipboard is handled with retries (no silent failure).

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

/// Saved format (identifier + raw bytes) — the semantics of the rich
/// formats is Windows' own; we do not interpret them.
#[derive(Clone, Debug)]
pub struct SavedFormat {
    pub format: u32,
    pub data: Vec<u8>,
}

/// Snapshot of the clipboard before insertion (RF-30a).
#[derive(Clone, Debug, Default)]
pub struct SavedClipboard {
    pub formats: Vec<SavedFormat>,
}

impl SavedClipboard {
    // (No "fingerprint" computed here: the only fingerprint used is
    // `read_text_fingerprint`, which hashes the pasted TEXT, not the raw
    // allocation bytes — two diverging helpers would be a source of
    // RF-30c disagreement.)
}

/// Opens the clipboard with retries (RF-30d: clipboard locked by a
/// third party).
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

/// Saves the formats that can be copied through global-memory semantics:
/// text (CF_TEXT/OEM/UNICODE), DIB/DIBV5 (bitmap content — Windows
/// usually synthesizes CF_DIB for bitmap clips) and the registered rich
/// formats (HTML/RTF). Handle-class formats (CF_BITMAP and friends)
/// cannot be captured this way — documented limitation; when nothing was
/// captured, `restore` leaves the clipboard AS IS instead of emptying it
/// (RF-30a: nothing is lost, not even replaced by emptiness).
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
                // Global-memory formats ONLY: text (1/7/13), DIB (8),
                // DIBV5 (17 — bitmap coverage via the same Global
                // semantics) and registered rich formats (HTML/RTF).
                // CF_BITMAP (2) is a GDI *handle* class: it cannot pass
                // through GlobalLock — documented limitation.
                let is_copyable = format == CF_UNICODETEXT.0 as u32
                    || format == 1 /* CF_TEXT */
                    || format == 7 /* CF_OEMTEXT */
                    || format == 8 /* CF_DIB */
                    || format == 17 /* CF_DIBV5 */
                    || is_registered_rich_format(format);
                if !is_copyable {
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

/// Writes Unicode text + history/cloud exclusion formats (RF-30b).
/// The `ExcludeClipboardContentFromMonitorProcessing`,
/// `CanIncludeInClipboardHistory` and `CanUploadToCloudClipboard` formats
/// are provided by Windows precisely for this scenario (ephemeral
/// content, never in the `Win+V` history).
pub fn set_text_excluding_history(text: &str) -> Result<(), ClipboardError> {
    open_with_retry(10)?;
    let result = unsafe {
        let inner = (|| -> Result<(), ClipboardError> {
            let _ = EmptyClipboard();
            // Exclusion formats — set BEFORE the text: on failure
            // here, the clipboard is left with no content (never publish
            // a text without its RF-30b hygiene flags).
            set_exclusion_flag("ExcludeClipboardContentFromMonitorProcessing")?;
            set_exclusion_flag("CanIncludeInClipboardHistory")?;
            set_exclusion_flag("CanUploadToCloudClipboard")?;

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
            // On success the system takes over the block; on failure,
            // THE block is ours: `GlobalFree` is mandatory (otherwise a
            // memory leak on every paste).
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hglobal.0))).map_err(|_| {
                let _ = GlobalFree(Some(hglobal));
                ClipboardError::SetFailed
            })?;
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
    // Same rule: failure → the block stays ours → immediate free.
    SetClipboardData(format, Some(HANDLE(hglobal.0))).map_err(|_| {
        let _ = GlobalFree(Some(hglobal));
        ClipboardError::SetFailed
    })?;
    Ok(())
}

/// Restores a snapshot (RF-30a). If `guard_fingerprint` is provided and the
/// current content differs from what WE pasted, the restoration is
/// cancelled: the user's concurrent copy is preserved (RF-30c).
pub fn restore(saved: &SavedClipboard, our_fingerprint: Option<u64>) -> Result<(), ClipboardError> {
    if let Some(fp) = our_fingerprint {
        if let Ok(current) = read_text_fingerprint() {
            if current != fp {
                // The user copied something else during the hold.
                return Ok(());
            }
        }
    }
    open_with_retry(10)?;

    unsafe {
        let inner = (|| -> Result<(), ClipboardError> {
            // Nothing recoverable was saved (e.g. a bitmap-only clip on
            // an earlier build, or an initially empty clipboard): do NOT
            // wipe the clipboard — leaving the transcription we pasted
            // in place beats an empty clipboard (RF-30a: nothing is
            // lost, not even replaced by emptiness).
            if saved.formats.is_empty() {
                return Ok(());
            }
            let _ = EmptyClipboard();
            // Tolerant per format: a failing format (alloc/lock/set)
            // frees its block without abandoning the others — partial
            // restoration is still better than a total loss of the
            // original content (RF-30a), the clipboard being already
            // held by US (fingerprint verified above).
            let mut failures = 0usize;
            for f in &saved.formats {
                let hglobal = match GlobalAlloc(GMEM_MOVEABLE, f.data.len().max(1)) {
                    Ok(h) => h,
                    Err(_) => {
                        failures += 1;
                        continue;
                    }
                };
                let ptr = GlobalLock(hglobal);
                if ptr.is_null() {
                    let _ = GlobalFree(Some(hglobal));
                    failures += 1;
                    continue;
                }
                std::ptr::copy_nonoverlapping(f.data.as_ptr(), ptr as *mut u8, f.data.len());
                let _ = GlobalUnlock(hglobal);
                if SetClipboardData(f.format, Some(HANDLE(hglobal.0))).is_err() {
                    let _ = GlobalFree(Some(hglobal));
                    failures += 1;
                }
            }
            if failures == saved.formats.len() && !saved.formats.is_empty() {
                return Err(ClipboardError::RestoreFailed);
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        inner
    }
}

/// Fingerprint of the current clipboard (text), for RF-30c detection.
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
            // Bounded scan: never walk past the allocation on a
            // malformed CF_UNICODETEXT (no NUL terminator inside the
            // buffer).
            let count =
                windows::Win32::System::Memory::GlobalSize(hglobal) / std::mem::size_of::<u16>();
            let mut len = 0usize;
            while len < count && *(ptr as *const u16).add(len) != 0 {
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

/// Complete transaction used by level 2 of the cascade: save →
/// write → Ctrl+V keystroke (performed by the caller) → deferred
/// restoration on a dedicated thread (RF-30: hold for slow applications,
/// timeout per profile).
pub struct ClipboardGuard;

impl ClipboardGuard {
    /// Writes the text and schedules the restoration after `hold_ms`.
    /// Returns the fingerprint of the pasted content (for the RF-30c check).
    pub fn paste_with_restore(text: &str, hold_ms: u32) -> Result<u64, ClipboardError> {
        let saved = save()?;
        set_text_excluding_history(text)?;
        let fp = read_text_fingerprint().ok();
        // Deferred restoration: a dedicated thread waits out the hold
        // window (applications slow to read the clipboard —
        // Outlook, RDP — require this hold, docs/01 §1.1).
        let to_restore = saved;
        let fp_for_restore = fp;
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(hold_ms.max(100) as u64));
            let _ = restore(&to_restore, fp_for_restore);
        });
        fp.ok_or(ClipboardError::SetFailed)
    }
}
