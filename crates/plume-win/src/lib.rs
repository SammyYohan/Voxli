//! `plume-win` — toute l'interop Win32 (convention docs/05 §6) : les autres
//! modules n'appellent jamais l'API Windows directement.
//!
//! Règle de sécurité du projet : `unsafe` interdit hors de ce crate et des
//! FFI moteurs ; chaque bloc y est documenté et justifié.
//!
//! Modules :
//! - `clipboard` : sauvegarde/restauration multi-format, hygiène de
//!   l'historique (RF-30) ;
//! - `sendinput` : frappe Unicode (niveau 3 de la cascade) ;
//! - `foreground` : fenêtre active, titre, exécutable, détection d'élévation ;
//! - `uia` : UI Automation — mot de passe, `ValuePattern` (niveau 1).

#[cfg(target_os = "windows")]
pub mod clipboard;
#[cfg(target_os = "windows")]
pub mod foreground;
#[cfg(target_os = "windows")]
pub mod sendinput;
#[cfg(target_os = "windows")]
pub mod uia;

#[cfg(target_os = "windows")]
pub use clipboard::{ClipboardError, ClipboardGuard};
#[cfg(target_os = "windows")]
pub use foreground::{foreground_info, ForegroundInfo};
#[cfg(target_os = "windows")]
pub use sendinput::{send_ctrl_v, type_unicode};
