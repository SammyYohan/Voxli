//! Pastille d'état (RF-09) : petite fenêtre native non focusable qui suit
//! la machine à états. Elle ne porte **jamais de contenu** (ADR-02, RF-34) :
//! le texte affiché est le nom d'état traduit (via `pill.js`) et le niveau
//! sonore instantané. C'est une fenêtre Windows native (Tauri) — conforme à
//! la définition de l'écran « mince » exigée par RF-09.

use tauri::{AppHandle, Manager, PhysicalPosition, Wry};

/// Position de la pastille, en pixels physiques depuis le bord droit/dessous
/// du moniteur principal.
const MARGIN_X: i32 = 36;
const MARGIN_Y: i32 = 96;

pub struct PillCtl {
    app: AppHandle<Wry>,
    positioned: std::sync::atomic::AtomicBool,
}

impl Clone for PillCtl {
    /// Chaque clone repart sans position mémorisée — la pastille se place
    /// au premier affichage de sa propre fenêtre (une seule fois).
    fn clone(&self) -> Self {
        Self {
            app: AppHandle::clone(&self.app),
            positioned: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl PillCtl {
    pub fn new(app: AppHandle<Wry>) -> Self {
        Self {
            app,
            positioned: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Montre la pastille hors état `idle`, la masque à l'état `idle`.
    /// Sans contenu : la page reçoit elle-même les mêmes événements `state://…`.
    pub fn set_state(&self, state: &str) {
        let Some(win) = self.app.get_webview_window("pill") else {
            return;
        };
        position_once(&win, &self.positioned);
        if state == "idle" {
            let _ = win.hide();
        } else {
            let _ = win.show();
        }
    }
}

/// Place la pastille au bord inférieur droit du moniteur de la fenêtre, une
/// seule fois (le déplacement forcé à chaque état ferait danser l'œil).
fn position_once(win: &tauri::WebviewWindow<Wry>, done: &std::sync::atomic::AtomicBool) {
    use std::sync::atomic::Ordering;
    if done.load(Ordering::Acquire) {
        return;
    }
    if let Ok(Some(monitor)) = win.current_monitor() {
        let size = win.outer_size().unwrap_or(tauri::PhysicalSize {
            width: 180,
            height: 52,
        });
        let x = monitor.position().x + monitor.size().width as i32 - size.width as i32 - MARGIN_X;
        let y = monitor.position().y + monitor.size().height as i32 - size.height as i32 - MARGIN_Y;
        let _ = win.set_position(PhysicalPosition::new(x, y));
        done.store(true, Ordering::Release);
    }
}
