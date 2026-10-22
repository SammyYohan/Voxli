//! Voxli desktop application — assembly (docs/05 §1 and §3).
//!
//! Glue only here: business behavior lives in voxli-core, and system
//! interop in voxli-win/voxli-audio/voxli-asr. Rules honored:
//! - no direct configuration writes by the webview (ADR-02);
//! - a single hotkey registration mechanism: the `tauri-plugin-global-shortcut`
//!   plugin, validated by voxli-hotkeys; registration failure is logged,
//!   never panic (ADR-05);
//! - logs only via voxli-diagnostics — no `println!`.

// Windows console removed, even in debug: nothing is written to stdout
// (logs go to the voxli.log file); a black window at launch only
// caused confusion.
#![windows_subsystem = "windows"]

mod commands;
mod strings;
mod worker;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use voxli_config::Config;
use voxli_core::InputEvent;
use voxli_diagnostics::{Event, Logger};
use voxli_hotkeys::{validate_before_register, Combo, HotkeyError, Key};
use voxli_win::uia::Uia;

use commands::{lock_or_recover, UiShared};
use worker::WorkerMsg;

// ------------------------------------------------------------- start-up ---

fn main() {
    let data_dir = Config::default_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let logger = Arc::new(Logger::file_logger(&data_dir.join("logs")));

    // Configuration: validated load + migrations; fallback to safe
    // values, logged — startup is never refused (safe default value).
    let config = match Config::load(&Config::default_path()) {
        Ok(c) => c,
        Err(err) => {
            logger.log(Event::Error {
                module: "config".into(),
                code: config_code(&err),
            });
            Config::default()
        }
    };

    let (tx, rx) = mpsc::channel::<WorkerMsg>();

    // UIA (COM) warm-up: the first call can take several hundred
    // milliseconds — better paid at startup than at the first dictation
    // (RF-19).
    if Uia::new().is_ok() {
        logger.log(Event::DeviceEvent {
            kind: "uia-warmup-ok".into(),
        });
    }

    let logger_run = Arc::clone(&logger);
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        // Opening donation links (Ko-fi / PayPal.me) in the system
        // browser: the capability's permissions are SCOPED to the two
        // exact URLs — no other open is allowed.
        .plugin(tauri_plugin_opener::init())
        .setup(move |app| {
            let config_path = Config::default_path();

            // License (RF-38): local offline verification against the
            // embedded public key — the product always starts, Pro or not.
            let app_data = Config::default_dir();
            let license = commands::license_from_file(&app_data.join("license.txt"));
            logger_run.log(Event::DeviceEvent {
                kind: format!("license-{}", license.state),
            });

            // History (RF-33): in memory by default; encrypted on disk
            // requires feature + Pro license + the configuration choice.
            let history = worker::build_history(
                &config,
                license.features.iter().any(|f| f == "history-on-disk"),
                &logger_run,
            );

            app.manage(UiShared {
                tx: tx.clone(),
                config: Mutex::new(config.clone()),
                config_path,
                last: Mutex::new(None),
                state: Mutex::new("idle".into()),
                engine: Mutex::new(String::new()),
                logger: Arc::clone(&logger_run),
                license: Mutex::new(license.clone()),
                history: Mutex::new(history),
                download_busy: Mutex::new(false),
                download_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            });

            setup_main_window(app)?;
            setup_pill_window(app)?;

            let handle: AppHandle<Wry> = app.handle().clone();
            let thread_logger = Arc::clone(&logger_run);
            thread_logger.log(Event::DeviceEvent {
                kind: "démarrage".into(),
            });
            let worker_logger = Arc::clone(&thread_logger);
            std::thread::Builder::new()
                .name("voxli-pipeline".into())
                .spawn(move || worker::run(handle, worker_logger, rx))
                .map_err(|_| strings::ERR_WORKER.to_string())?;

            register_shortcuts(app.handle(), &config, &thread_logger)?;
            setup_tray(app.handle(), &config)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config_get,
            commands::config_set,
            commands::state_get,
            commands::rescue_last,
            commands::rescue_paste_again,
            commands::rescue_retry,
            commands::rescue_close,
            commands::cancel_dictation,
            commands::diagnostics_export,
            commands::license_status,
            commands::license_import,
            commands::devices_list,
            commands::models_list,
            commands::models_download,
            commands::models_download_cancel,
            commands::models_remove,
            commands::engines_list,
            commands::history_query,
            commands::history_delete,
            commands::history_purge,
            commands::dictionaries_import,
            commands::dictionaries_export,
            commands::context_preview,
            commands::data_export,
        ])
        .run(tauri::generate_context!());

    if let Err(_err) = result {
        // No `println!` by convention: the run-loop failure is logged,
        // then the process exits with a non-zero code.
        logger.log(Event::Error {
            module: "lancement".into(),
            code: 1,
        });
        std::process::exit(1);
    }
}

// ------------------------------------------------- RF-09 pill ---

/// Pill window (RF-09 v2): a **permanent** indicator at the top center of
/// the primary monitor. At rest, a single discreet dot (listening armed:
/// the hotkey is ready); during capture, the full pill (state + VU meter +
/// live streaming text, ADR-02 amendment). Transparent and non-interactive:
/// a click passes through to the window underneath. A background thread
/// re-asserts position, scale and on-top (screens of every size, "windowed"
/// full-screen windows) and detects hover — the hotkey caption is then
/// shown under the pill.
fn setup_pill_window(app: &tauri::App<Wry>) -> tauri::Result<()> {
    if app.get_webview_window("pill").is_some() {
        return Ok(());
    }
    let win =
        tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("pill.html".into()))
            .title(strings::PILL_WINDOW_TITLE)
            // 640×34: the pill; 40 px below: the caption zone
            // (invisible unless hovered, click-through like the rest).
            .inner_size(640.0, 74.0)
            .decorations(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .focusable(false)
            .transparent(true)
            .shadow(false)
            .build()?;
    place_top_center(app.app_handle(), &win);
    // Click-through indicator: it must never cast an interactive shadow
    // over the app being used (RF-09).
    let _ = win.set_ignore_cursor_events(true);

    // Background thread: position/scale/on-top re-asserted and hover
    // detected (click-through ⇒ the webview will never feel the mouse;
    // the core watches the cursor). A spawn failure only costs comfort
    // (caption, re-assertion) — the indicator remains.
    let handle = app.app_handle().clone();
    let win_maint = win.clone();
    std::thread::Builder::new()
        .name("pill-maintenance".into())
        .spawn(move || pill_maintenance_loop(handle, win_maint))
        .ok();
    Ok(())
}

/// Pill dims: logical width, total height, top margin.
const PILL_WIDTH: f64 = 640.0;
const PILL_HEIGHT: f64 = 74.0;
const PILL_TOP_MARGIN: f64 = 8.0;
/// Dot hover radius (logical px): enter at 15, exit at 20
/// (hysteresis — the caption does not flicker at the radius edge).
const PILL_HOVER_IN: f64 = 15.0;
const PILL_HOVER_OUT: f64 = 20.0;

/// Place (and re-place) the window at the top center of the primary monitor.
/// Width clamped to 85 % of the screen: from the oldest PC (1024 px) to a
/// 200 %-scaled 4K, the indicator keeps its slot and proportions (DPI is
/// handled by converting logical to physical then back to logical).
fn place_top_center(app: &tauri::AppHandle<Wry>, win: &tauri::WebviewWindow<Wry>) {
    let Ok(Some(m)) = app.primary_monitor() else {
        return;
    };
    let scale = m.scale_factor();
    let screen_w = m.size().width as f64;
    let target_w = ((PILL_WIDTH * scale).round() as i32).min((screen_w * 0.85) as i32);
    if let Ok(size) = win.outer_size() {
        if (size.width as f64 - target_w as f64).abs() > 1.0 {
            let _ = win.set_size(tauri::LogicalSize::new(
                target_w as f64 / scale,
                PILL_HEIGHT,
            ));
        }
    }
    let x = m.position().x + (m.size().width as i32 - target_w).max(0) / 2;
    let y = m.position().y + PILL_TOP_MARGIN as i32;
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    // The attribute can be lost on display change: a plain re-assert
    // here; fine-grained topmost re-pinning in the band is the job of
    // `repin_topmost` (maintenance loop). Exclusive full screen (games):
    // Windows keeps it above — accepted system limit (docs/03).
    let _ = win.set_always_on_top(true);
}

/// Push the pill back to the very top of the "topmost" band: non-topmost
/// then topmost — without this pair, `SetWindowPos(HWND_TOPMOST)` is a
/// no-op when the window is already topmost (it does not move up within
/// its band), and a self-topmost full-screen window would cover it for
/// good. Two consecutive calls in one tick: the composer merges them,
/// no perceptible flicker.
fn repin_topmost(win: &tauri::WebviewWindow<Wry>) {
    let _ = win.set_always_on_top(false);
    let _ = win.set_always_on_top(true);
}

/// Maintenance loop: hover read at ~120 ms (lively caption response),
/// position replayed every ~2.4 s, and **re-pinning** "non-topmost then
/// topmost" every ~0.5 s — Windows ranks windows into bands and a plain
/// topmost re-pin is a no-op if the pill is already in its band; a
/// full-screen window that declares itself topmost (video player, F11
/// browser, windowed game) takes the top of the band. The demote+re-pin
/// puts it back above everything that is not exclusive full screen
/// (system limit, docs/03). Minimal cost: one cursor read, two
/// SetWindowPos calls.
fn pill_maintenance_loop(app: AppHandle<Wry>, win: tauri::WebviewWindow<Wry>) {
    let mut tick: u32 = 0;
    let mut inside = false;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(120));
        tick = tick.wrapping_add(1);
        if tick.is_multiple_of(20) {
            place_top_center(&app, &win);
        }
        if tick.is_multiple_of(4) {
            repin_topmost(&win);
        }
        if hover_point(&app, &win, inside) != inside {
            inside = !inside;
            // Event reserved for the pill (no dictated content):
            // enter = the hotkey caption appears, exit = it
            // clears.
            let _ = app.emit_to("pill", "pill://hover", inside);
        }
    }
}

/// Is the cursor within 15 px (logical) of the status dot? The dot is
/// **centered** at rest (idle look), on the left of the pill (margin 14 +
/// half-dot 5) in active states — the caption follows the dot itself,
/// not the window rect. Hysteresis: enter at 15 px, exit at 20 px — no
/// flicker at the radius edge. Physical coordinates on both sides
/// (`cursor_position` is physical, relative to the primary screen's
/// origin).
fn hover_point(app: &tauri::AppHandle<Wry>, win: &tauri::WebviewWindow<Wry>, inside: bool) -> bool {
    let Ok(cursor) = app.cursor_position() else {
        return false;
    };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else {
        return false;
    };
    // Primary monitor scale: the dot's location and the radius are
    // defined in logical px, like the CSS layout.
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    // Dot position per the state machine (top edge of the pill = 17
    // logical px): centered at rest, on the left otherwise.
    let state_now = {
        let shared = app.state::<UiShared>();
        let locked = lock_or_recover(&shared.state).clone();
        locked
    };
    let dot_x = if state_now == "idle" {
        pos.x as f64 + size.width as f64 / 2.0
    } else {
        pos.x as f64 + 19.0 * scale
    };
    let dot_y = pos.y as f64 + 17.0 * scale;
    let (r_in, r_out) = (PILL_HOVER_IN * scale, PILL_HOVER_OUT * scale);
    let dx = cursor.x - dot_x;
    let dy = cursor.y - dot_y;
    let d2 = dx * dx + dy * dy;
    if inside {
        d2 <= r_out * r_out
    } else {
        d2 <= r_in * r_in
    }
}

// ------------------------------------------------------- main window ---

/// Settings window: its close button hides it instead of exiting the
/// process (the app lives in the tray).
fn setup_main_window(app: &tauri::App<Wry>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        // The close button hides the window to the tray instead of
        // killing the process (the app lives in the tray).
        let hidden_window = w.clone();
        w.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hidden_window.hide();
            }
        });
    }
    Ok(())
}

// ------------------------------------------------------------- tray ---

fn setup_tray(app: &AppHandle<Wry>, cfg: &Config) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "reglages", "Réglages…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quitter", "Quitter Voxli", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("voxli-tray")
        .tooltip(tray_tooltip(cfg))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "reglages" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "quitter" => {
                app.exit(0);
            }
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder
        .on_tray_icon_event(|tray, event| {
            // Left click: open/hide the settings (menu duplicated on right click).
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let visible = w.is_visible().unwrap_or(false);
                    if visible {
                        let _ = w.hide();
                    } else {
                        let _ = w.show();
                        let _ = w.set_focus();
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

// ------------------------------------------------- global shortcuts ---

/// RF-01: hot re-registration after `configSet`. Everything is
/// unregistered, then valid combos are re-registered; a system conflict
/// is logged, never panic (ADR-05). Called only from the main thread
/// (tauri commands run there).
pub(crate) fn re_register_shortcuts(app: &AppHandle<Wry>, cfg: &Config) {
    if let Some(shared) = app.try_state::<UiShared>() {
        let _ = app.global_shortcut().unregister_all();
        let _ = register_shortcuts(app, cfg, &shared.logger);
    }
    // The tooltip follows the hotkey live (RF-01): re-read after
    // re-registration; the update is best-effort (never panic
    // over a label).
    if let Some(tray) = app.tray_by_id("voxli-tray") {
        let _ = tray.set_tooltip(Some(&tray_tooltip(cfg)));
    }
}

/// A single registration mechanism (global-shortcut plugin). Rules:
/// - reserved combos are refused outright, logged (voxli-hotkeys);
/// - a `RegisterHotKey` conflict → log entry, **never panic** (ADR-05);
/// - push-to-talk follows press/release; both discrete actions act
///   on press.
fn register_shortcuts(app: &AppHandle<Wry>, cfg: &Config, logger: &Logger) -> tauri::Result<()> {
    // Push-to-talk (RF-01): press → record, release → pipeline.
    if let Ok(combo) = Combo::parse(&cfg.hotkeys.push_to_talk) {
        log_registration(logger, &combo, "pushToTalk");
        if validate_before_register(&combo).is_ok() {
            match to_shortcut(&combo) {
                Ok(sc) => {
                    let res = app
                        .global_shortcut()
                        .on_shortcut(sc, |app, _shortcut, event| {
                            let ev = match event.state {
                                ShortcutState::Pressed => InputEvent::HotkeyPressed,
                                _ => InputEvent::HotkeyReleased,
                            };
                            let _ = app.state::<UiShared>().tx.send(WorkerMsg::Input(ev));
                        });
                    if let Err(err) = res {
                        logger.log(Event::Error {
                            module: "hotkeys".into(),
                            code: 4,
                        });
                        logger.log(Event::DeviceEvent {
                            kind: format!("register-pushToTalk-infructueux:{err}"),
                        });
                        let _ = app.emit(
                            "notice://notice",
                            serde_json::json!({
                                "kind": "hotkey",
                                "message": strings::ERR_HOTKEY_TAKEN,
                            }),
                        );
                    }
                }
                Err(_err) => logger.log(Event::Error {
                    module: "hotkeys".into(),
                    code: 1,
                }),
            }
        } else {
            logger.log(Event::Error {
                module: "hotkeys".into(),
                code: 2,
            });
        }
    } else {
        logger.log(Event::Error {
            module: "hotkeys".into(),
            code: 1,
        });
    }

    // "Paste again" (RF-22): re-insert the last text.
    register_discrete(
        app,
        &cfg.hotkeys.paste_last_transcript,
        "pasteLastTranscript",
        logger,
        |app| {
            let _ = app
                .state::<UiShared>()
                .tx
                .send(WorkerMsg::Input(InputEvent::PasteAgain));
        },
    );
    // "Show last text" (Shift+Alt+X): rescue window.
    register_discrete(
        app,
        &cfg.hotkeys.show_last_transcript,
        "showLastTranscript",
        logger,
        |app| {
            let _ = app.state::<UiShared>().tx.send(WorkerMsg::ShowRescue);
        },
    );
    Ok(())
}

/// Discrete action: a single send on press (release is ignored; the
/// plugin delivers both states).
fn register_discrete<F>(app: &AppHandle<Wry>, raw: &str, name: &str, logger: &Logger, action: F)
where
    F: Fn(&AppHandle<Wry>) + Send + Sync + 'static,
{
    let Ok(combo) = Combo::parse(raw) else {
        logger.log(Event::Error {
            module: "hotkeys".into(),
            code: 1,
        });
        return;
    };
    log_registration(logger, &combo, name);
    if validate_before_register(&combo).is_err() {
        logger.log(Event::Error {
            module: "hotkeys".into(),
            code: 2,
        });
        return;
    }
    let shortcut = match to_shortcut(&combo) {
        Ok(s) => s,
        Err(_) => {
            logger.log(Event::Error {
                module: "hotkeys".into(),
                code: 1,
            });
            return;
        }
    };
    let res = app
        .global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                action(app);
            }
        });
    if let Err(err) = res {
        logger.log(Event::Error {
            module: "hotkeys".into(),
            code: 4,
        });
        logger.log(Event::DeviceEvent {
            kind: format!("register-{name}-infructueux:{err}"),
        });
        let _ = app.emit(
            "notice://notice",
            serde_json::json!({
                "kind": "hotkey",
                "message": strings::ERR_HOTKEY_TAKEN,
            }),
        );
    }
}

/// Tray tooltip: the EFFECTIVE dictation hotkey (RF-01), canonicalized
/// when parseable, generic wording otherwise (never false info).
fn tray_tooltip(cfg: &Config) -> String {
    match Combo::parse(&cfg.hotkeys.push_to_talk) {
        Ok(combo) => format!("Voxli — dictée locale ({})", combo.canonical()),
        Err(_) => "Voxli — dictée locale".into(),
    }
}

fn log_registration(logger: &Logger, combo: &Combo, purpose: &str) {
    logger.log(Event::DeviceEvent {
        kind: format!("raccourci-{purpose}={}", combo.canonical()),
    });
}

/// voxli_hotkeys::Combo → plugin `Shortcut` (physical key naming).
fn to_shortcut(combo: &Combo) -> Result<Shortcut, HotkeyError> {
    let key = combo
        .key
        .ok_or_else(|| HotkeyError::Unparsable("aucune touche".into()))?;
    let code =
        code_of(key).ok_or_else(|| HotkeyError::Unparsable("touche non prise en charge".into()))?;
    let mut mods = Modifiers::empty();
    if combo.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if combo.shift {
        mods |= Modifiers::SHIFT;
    }
    if combo.alt {
        mods |= Modifiers::ALT;
    }
    if combo.win {
        mods |= Modifiers::META;
    }
    Ok(Shortcut::new(Some(mods), code))
}

fn code_of(k: Key) -> Option<Code> {
    match k {
        Key::Space => Some(Code::Space),
        Key::Tab => Some(Code::Tab),
        Key::Enter => Some(Code::Enter),
        Key::Backquote => Some(Code::Backquote),
        Key::Letter(c) => match c.to_ascii_uppercase() {
            'A' => Some(Code::KeyA),
            'B' => Some(Code::KeyB),
            'C' => Some(Code::KeyC),
            'D' => Some(Code::KeyD),
            'E' => Some(Code::KeyE),
            'F' => Some(Code::KeyF),
            'G' => Some(Code::KeyG),
            'H' => Some(Code::KeyH),
            'I' => Some(Code::KeyI),
            'J' => Some(Code::KeyJ),
            'K' => Some(Code::KeyK),
            'L' => Some(Code::KeyL),
            'M' => Some(Code::KeyM),
            'N' => Some(Code::KeyN),
            'O' => Some(Code::KeyO),
            'P' => Some(Code::KeyP),
            'Q' => Some(Code::KeyQ),
            'R' => Some(Code::KeyR),
            'S' => Some(Code::KeyS),
            'T' => Some(Code::KeyT),
            'U' => Some(Code::KeyU),
            'V' => Some(Code::KeyV),
            'W' => Some(Code::KeyW),
            'X' => Some(Code::KeyX),
            'Y' => Some(Code::KeyY),
            'Z' => Some(Code::KeyZ),
            _ => None,
        },
        Key::Digit(d) => match d {
            0 => Some(Code::Digit0),
            1 => Some(Code::Digit1),
            2 => Some(Code::Digit2),
            3 => Some(Code::Digit3),
            4 => Some(Code::Digit4),
            5 => Some(Code::Digit5),
            6 => Some(Code::Digit6),
            7 => Some(Code::Digit7),
            8 => Some(Code::Digit8),
            9 => Some(Code::Digit9),
            _ => None,
        },
        Key::Function(n) => match n {
            1 => Some(Code::F1),
            2 => Some(Code::F2),
            3 => Some(Code::F3),
            4 => Some(Code::F4),
            5 => Some(Code::F5),
            6 => Some(Code::F6),
            7 => Some(Code::F7),
            8 => Some(Code::F8),
            9 => Some(Code::F9),
            10 => Some(Code::F10),
            11 => Some(Code::F11),
            12 => Some(Code::F12),
            13 => Some(Code::F13),
            14 => Some(Code::F14),
            15 => Some(Code::F15),
            16 => Some(Code::F16),
            17 => Some(Code::F17),
            18 => Some(Code::F18),
            19 => Some(Code::F19),
            20 => Some(Code::F20),
            21 => Some(Code::F21),
            22 => Some(Code::F22),
            23 => Some(Code::F23),
            24 => Some(Code::F24),
            _ => None,
        },
    }
}

// ------------------------------------------------------------- misc ---

fn config_code(err: &voxli_config::ConfigError) -> u32 {
    match err {
        voxli_config::ConfigError::Io(_) => 1,
        voxli_config::ConfigError::Json(_) => 2,
        voxli_config::ConfigError::UnknownSchema { .. } => 3,
    }
}
