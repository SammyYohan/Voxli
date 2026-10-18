//! Fenêtre active : titre, exécutable, détection d'élévation (RF-29, RF-32).
//!
//! La cible est mémorisée **au démarrage de la dictée** (RF-32) — c'est
//! `foreground_info` qui alimente le `FocusSnapshot`.

use std::os::windows::ffi::OsStringExt;

use windows::core::PWSTR;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};

#[derive(Clone, Debug, PartialEq)]
pub struct ForegroundInfo {
    pub hwnd: isize,
    pub title: String,
    /// Nom d'exécutable en minuscules (`notepad.exe`) — identifiant d'app.
    pub app_id: String,
    /// `true` si le processus cible refuse `PROCESS_QUERY_LIMITED_INFORMATION`
    /// (f couche UIPI : fenêtre élevée) — insertion hors périmètre (ADR-06).
    pub elevated: bool,
}

/// Photographie de la fenêtre au premier plan. `None` = aucune fenêtre
/// exploitable (bureau, écran de verrouillage).
pub fn foreground_info() -> Option<ForegroundInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        // Titre
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; (len.max(0) as usize) + 1];
        let copied = GetWindowTextW(hwnd, &mut buf);
        let title = String::from_utf16_lossy(&buf[..copied.max(0) as usize]);

        // Processus
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok();
        let process = match process {
            Some(p) => p,
            None => {
                // Accès refusé : fenêtre élevée (UIPI) — signalée, pas plantée.
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
        Some(ForegroundInfo {
            hwnd: hwnd.0 as isize,
            title,
            app_id,
            elevated: false,
        })
    }
}
