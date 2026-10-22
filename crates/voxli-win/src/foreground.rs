//! Active window: title, executable, elevation detection (RF-29, RF-32).
//!
//! The target is remembered **at dictation start** (RF-32) — it is
//! `foreground_info` that feeds the `FocusSnapshot`.
//!
//! Hardened RF-29 rule: when the elevation of the target process cannot be
//! **proven** (access denied, unreadable token), the answer is
//! conservative — `elevated = true` — because refusing insertion is
//! reversible (the text stays in the buffer, RF-22) whereas writing into
//! an elevated window is not.

use std::os::windows::ffi::OsStringExt;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ForegroundInfo {
    pub hwnd: isize,
    pub title: String,
    /// Lowercase executable name (`notepad.exe`) — app identifier.
    pub app_id: String,
    /// `true` if the target window is elevated **or if the elevation
    /// cannot be verified** (conservative RF-29 rule) — out-of-scope
    /// insertion (ADR-06), never silent (`refused` status to the UI).
    pub elevated: bool,
}

/// Snapshot of the foreground window. `None` = no usable window
/// (desktop, lock screen).
pub fn foreground_info() -> Option<ForegroundInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        // Title
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; (len.max(0) as usize) + 1];
        let copied = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..copied.max(0) as usize]);

        // Process
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok();
        let process = match process {
            Some(p) => p,
            None => {
                // Access denied: elevated window (UIPI) — reported, not a crash.
                return Some(ForegroundInfo {
                    hwnd: hwnd.0 as isize,
                    title,
                    app_id: String::new(),
                    elevated: true,
                });
            }
        };
        let mut path_buf = [0u16; 1024];
        let mut size: u32 = path_buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            HANDLE(process.0),
            PROCESS_NAME_WIN32,
            PWSTR(path_buf.as_mut_ptr()),
            &mut size,
        );
        let app_id = if ok.is_ok() {
            std::path::Path::new(&std::ffi::OsString::from_wide(&path_buf[..size as usize]))
                .file_name()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default()
        } else {
            String::new()
        };
        // Elevation: the limited rquery alone is not enough (a
        // medium-privileged process can open an elevated process with this
        // reduced right). The token is authoritative, otherwise "elevated"
        // (doubt ⇒ refusal).
        let elevated = is_process_elevated(HANDLE(process.0));
        let _ = CloseHandle(HANDLE(process.0)); // no handle left open
        Some(ForegroundInfo {
            hwnd: hwnd.0 as isize,
            title,
            app_id,
            elevated,
        })
    }
}

/// The elevation of the process is carried by its token (`TokenElevation`).
/// Any impossible read → `true` (conservative, RF-29): only write into
/// windows whose NON-elevated state is proven.
unsafe fn is_process_elevated(process: HANDLE) -> bool {
    let mut token = HANDLE::default();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token).is_err() {
        return true;
    }
    let is_elevated = {
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut returned: u32 = 0;
        GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
            core::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
        .map(|_| elevation.TokenIsElevated != 0)
    };
    let _ = CloseHandle(token);
    is_elevated.unwrap_or(true)
}
