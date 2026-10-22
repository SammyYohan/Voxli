//! `voxli-win` — all Win32 interop (convention docs/05 §6): the other
//! modules never call the Windows API directly.
//!
//! Project safety rule: `unsafe` is forbidden outside this crate and the
//! engine FFI; each block is expected to be documented and justified.
//!
//! Modules:
//! - `clipboard`: multi-format save/restore, clipboard history hygiene (RF-30);
//! - `sendinput`: Unicode typing (level 3 of the cascade);
//! - `foreground`: active window, title, executable, elevation detection;
//! - `uia`: UI Automation — password fields, `ValuePattern` (level 1).

#[cfg(target_os = "windows")]
pub mod clipboard;
#[cfg(target_os = "windows")]
pub mod foreground;
#[cfg(target_os = "windows")]
pub mod sendinput;
#[cfg(target_os = "windows")]
pub mod uia;

/// NeMo-Speech.cpp SDK binding (feature `nemo`): streaming in-process
/// speech recognition. The installed archive provides the C ABI.
#[cfg(feature = "nemo")]
pub mod nemo;

#[cfg(target_os = "windows")]
pub use clipboard::{ClipboardError, ClipboardGuard};
#[cfg(target_os = "windows")]
pub use foreground::{foreground_info, ForegroundInfo};
#[cfg(target_os = "windows")]
pub use sendinput::{send_ctrl_v, type_unicode};
