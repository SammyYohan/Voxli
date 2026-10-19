//! Fenêtre active : titre, exécutable, détection d'élévation (RF-29, RF-32).
//!
//! La cible est mémorisée **au démarrage de la dictée** (RF-32) — c'est
//! `foreground_info` qui alimente le `FocusSnapshot`.
//!
//! Règle RF-29 durcie : quand l'élévation du processus cible ne peut pas
//! être **prouvée** (accès refusé, jeton illisible), la réponse est
//! conservatrice — `elevated = true` — parce que le refus d'insertion
//! est réversible (le texte reste au tampon, RF-22) alors qu'une
//! écriture dans une fenêtre élevée ne l'est pas.

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
    /// Nom d'exécutable en minuscules (`notepad.exe`) — identifiant d'app.
    pub app_id: String,
    /// `true` si la fenêtre cible est élevée **ou si l'élévation ne peut
    /// pas être vérifiée** (règle conservatrice RF-29) — insertion hors
    /// périmètre (ADR-06), jamais silencieuse (code `refused` à l'UI).
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
        // Élévation : la réponse de rquery limitée ne suffit pas (un
        // processus moyen peut ouvrir un processus élevé avec ce droit
        // réduit). Le jeton fait foi, sinon « élevé » (doute ⇒ refus).
        let elevated = is_process_elevated(HANDLE(process.0));
        let _ = CloseHandle(HANDLE(process.0)); // aucun handle laissé ouvert
        Some(ForegroundInfo {
            hwnd: hwnd.0 as isize,
            title,
            app_id,
            elevated,
        })
    }
}

/// L'élévation du processus est posée par son jeton (`TokenElevation`).
/// Toute lecture impossible → `true` (conservateur, RF-29) : n'écrire que
/// dans des fenêtres dont l'élévation NON elevée est prouvée.
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
