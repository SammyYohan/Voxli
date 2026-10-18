//! Application de bureau Plume — assemblage (docs/05 §1 et §3).
//!
//! Ici uniquement de la colle : le comportement métier vit dans plume-core,
//! les interopérations système dans plume-win/plume-audio/plume-asr.
//! Règles respectées :
//! - aucune écriture de configuration directe par le webview (ADR-02) ;
//! - un seul mécanisme d'enregistrement des raccourcis : le plugin
//!   `tauri-plugin-global-shortcut`, validé par plume-hotkeys ; échec
//!   d'enregistrement consigné, jamais de panique (ADR-05) ;
//! - logs uniquement via plume-diagnostics — aucun `println!`.

// Console Windows supprimée, y compris en debug : rien n'est écrit sur
// stdout (logs → fichier plume.log), une fenêtre noire à l'ouverture
// n'apportait que de la confusion.
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

use plume_config::Config;
use plume_core::InputEvent;
use plume_diagnostics::{Event, Logger};
use plume_hotkeys::{validate_before_register, Combo, HotkeyError, Key};
use plume_win::uia::Uia;

use commands::{lock_or_recover, UiShared};
use worker::WorkerMsg;

// ------------------------------------------------------------- démarrage ---

fn main() {
    let data_dir = Config::default_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let logger = Arc::new(Logger::file_logger(&data_dir.join("logs")));

    // Configuration : chargement validé + migrations ; fallback valeurs
    // sûres, consigné — jamais de refus de démarrage (valeur sûre par défaut).
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

    // Préchauffage UIA (COM) : le premier appel peut prendre plusieurs
    // centaines de millisecondes — mieux vaut le payer au démarrage qu'à la
    // première dictée (RF-19).
    if Uia::new().is_ok() {
        logger.log(Event::DeviceEvent {
            kind: "uia-warmup-ok".into(),
        });
    }

    let logger_run = Arc::clone(&logger);
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(move |app| {
            let config_path = Config::default_path();

            // Licence (RF-38) : vérification locale hors ligne avec la clé
            // publique embarquée — le produit démarre toujours, Pro ou pas.
            let app_data = Config::default_dir();
            let license = commands::license_from_file(&app_data.join("license.txt"));
            logger_run.log(Event::DeviceEvent {
                kind: format!("license-{}", license.state),
            });

            // Historique (RF-33) : mémoire par défaut ; disque chiffré avec
            // la feature + la licence Pro + le choix de configuration.
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
                .name("plume-pipeline".into())
                .spawn(move || worker::run(handle, worker_logger, rx))
                .map_err(|_| strings::ERR_WORKER.to_string())?;

            register_shortcuts(app.handle(), &config, &thread_logger)?;
            setup_tray(app.handle())?;
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
        // Aucun `println!` par convention : l'échec de la boucle de vie est
        // consigné, puis le processus sort avec un code non nul.
        logger.log(Event::Error {
            module: "lancement".into(),
            code: 1,
        });
        std::process::exit(1);
    }
}

// ------------------------------------------------- pastille RF-09 ---

/// Fenêtre pastille (RF-09 v2) : indicateur **permanent** en haut au centre
/// du moniteur principal. Au repos, un seul point discret (écoute armée :
/// le raccourci est prêt) ; pendant une prise, la pilule complète (état +
/// vumètre + texte en direct du streaming, amendement ADR-02). Transparente
/// et non interactive : le clic traverse vers la fenêtre dessous. Un
/// thread de fond re-sert position, échelle et premier plan (écrans de
/// toutes tailles, fenêtres plein écran « fenêtrées ») et détecte le
/// survol — la légende du raccourci est alors montrée sous la pilule.
fn setup_pill_window(app: &tauri::App<Wry>) -> tauri::Result<()> {
    if app.get_webview_window("pill").is_some() {
        return Ok(());
    }
    let win =
        tauri::WebviewWindowBuilder::new(app, "pill", tauri::WebviewUrl::App("pill.html".into()))
            .title(strings::PILL_WINDOW_TITLE)
            // 640×34 : la pilule ; 40 px dessous : la zone de la légende
            // (invisible hors survol, traverse au clic comme le reste).
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
    // Cliquer à travers l'indicateur : il ne doit jamais créer d'ombre
    // interactive par-dessus l'application en cours (RF-09).
    let _ = win.set_ignore_cursor_events(true);

    // Thread de fond : position/échelle/premier plan réassurés et survol
    // détecté (clic traversant ⇒ le webview ne sentira jamais la souris,
    // c'est le noyau qui surveille le curseur). Un échec de spawn ne
    // bloque que le confort (légende, réassurance) — l'indicateur reste.
    let handle = app.app_handle().clone();
    let win_maint = win.clone();
    std::thread::Builder::new()
        .name("pill-maintenance".into())
        .spawn(move || pill_maintenance_loop(handle, win_maint))
        .ok();
    Ok(())
}

/// Dimensions de la pastille : largeur logique, hauteur totale, marge haut.
const PILL_WIDTH: f64 = 640.0;
const PILL_HEIGHT: f64 = 74.0;
const PILL_TOP_MARGIN: f64 = 8.0;
/// Rayon de survol du point (px logiques) : entrée à 15, sortie à 20
/// (hystérésis — la légende ne clignote pas au bord du rayon).
const PILL_HOVER_IN: f64 = 15.0;
const PILL_HOVER_OUT: f64 = 20.0;

/// Place (et repose) la fenêtre en haut au centre du moniteur principal.
/// Largeur bornée à 85 % de l'écran : du plus ancien PC (1024 px) au 4K à
/// échelle 200 %, l'indicateur garde sa place et ses proportions (le DPI
/// est géré en convertissant les logiques en physiques puis en logiques).
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
    // L'attribut peut se perdre au changement d'affichage : re-sert
    // simple ici ; la remontée fine dans la bande est le métier de
    // `repin_topmost` (boucle de maintenance). Plein écran exclusif
    // (jeux) : Windows garde le dessus — limite système assumée (docs/03).
    let _ = win.set_always_on_top(true);
}

/// Renvoie la pastille tout en haut de la bande « topmost » : non-topmost
/// puis topmost — sans ce couple, `SetWindowPos(HWND_TOPMOST)` est un
/// no-op quand la fenêtre est déjà topmost (elle ne remonte pas dans sa
/// bande), et une fenêtre plein écran auto-topmost la recouvre à jamais.
/// Deux appels consécutifs en un tick : le compositeur les fusionne,
/// aucun clignotement perceptible.
fn repin_topmost(win: &tauri::WebviewWindow<Wry>) {
    let _ = win.set_always_on_top(false);
    let _ = win.set_always_on_top(true);
}

/// Boucle de maintenance : survol lu à ~120 ms (réponse vive de la
/// légende), position rejouées toutes les ~2,4 s, et **ré-épinglage**
/// « non-topmost puis topmost » toutes les ~0,5 s — Windows classe les
/// fenêtres par bandes et un répérage topmost simple est un no-op si la
/// pastille est déjà dedans ; une fenêtre plein écran qui se déclare
/// topmost (lecteur vidéo, navigateur F11, jeu fenêtré) prend le dessus
/// de la bande. Le réfrograde+repein la renvoie au-dessus de tout ce qui
/// n'est pas un plein écran exclusif (limite système, docs/03). Coût
/// minime : une lecture de curseur, deux appels SetWindowPos.
fn pill_maintenance_loop(app: AppHandle<Wry>, win: tauri::WebviewWindow<Wry>) {
    let mut tick: u32 = 0;
    let mut inside = false;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(120));
        tick = tick.wrapping_add(1);
        if tick % 20 == 0 {
            place_top_center(&app, &win);
        }
        if tick % 4 == 0 {
            repin_topmost(&win);
        }
        if hover_point(&app, &win, inside) != inside {
            inside = !inside;
            // Événement réservé à la pastille (aucun contenu dicté) :
            // entrée = la légende du raccourci apparaît, sortie = elle
            // s'efface.
            let _ = app.emit_to("pill", "pill://hover", inside);
        }
    }
}

/// Le curseur est-il à moins de 15 px (logiques) du point d'état ? Le
/// point est **centré** au repos (visuel idle), à gauche de la pilule
/// (marge 14 + demi-point 5) pendant les états actifs — la légende suit
/// le point lui-même, pas le rect de la fenêtre. Hystérésis : entrée à
/// 15 px, sortie à 20 px — jamais de clignotement au bord du rayon.
/// Coordonnées physiques des deux côtés (`cursor_position` est physique,
/// relative à l'origine de l'écran principal).
fn hover_point(app: &tauri::AppHandle<Wry>, win: &tauri::WebviewWindow<Wry>, inside: bool) -> bool {
    let Ok(cursor) = app.cursor_position() else {
        return false;
    };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else {
        return false;
    };
    // Échelle du moniteur principal : l'emplacement du point et le rayon
    // sont définis en px logiques, comme la mise en page CSS.
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);
    // Position du point selon la machine à états (mie de la scène = 17
    // px logiques) : centré au repos, à gauche sinon.
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

// ------------------------------------------------------- fenêtre principale ---

/// La fenêtre de réglages est masquée au lancement (app tray) ; la croix la
/// masque plutôt que de fermer le processus.
fn setup_main_window(app: &tauri::App<Wry>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window("main") {
        // La fenêtre masquée au lancement : la croix la cache au tray au
        // lieu de tuer le processus (l'app vit sur le tray).
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

fn setup_tray(app: &AppHandle<Wry>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "reglages", "Réglages…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quitter", "Quitter Plume", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("plume-tray")
        .tooltip("Plume — dictée locale (Ctrl+Alt+Espace)")
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
            // Clic gauche : ouvrir/masquer les réglages (menu doublé au clic droit).
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

// ------------------------------------------------- raccourcis globaux ---

/// RF-01 : re-registration à chaud après `configSet`. Tout est désenregistré
/// puis les combinaisons valides sont re-registrées ; un conflit système est
/// consigné, jamais de panique (ADR-05). Appelé uniquement depuis le thread
/// principal (les commandes tauri y tournent).
pub(crate) fn re_register_shortcuts(app: &AppHandle<Wry>, cfg: &Config) {
    if let Some(shared) = app.try_state::<UiShared>() {
        let _ = app.global_shortcut().unregister_all();
        let _ = register_shortcuts(app, cfg, &shared.logger);
    }
}

/// Un seul mécanisme d'enregistrement (plugin global-shortcut). Règles :
/// - combinaisons réservées refusées d'office avec consigne au journal
///   (plume-hotkeys) ;
/// - conflit `RegisterHotKey` → consigne, **jamais de panique** (ADR-05) ;
/// - Push-to-talk suit appui/relâchement ; les deux actions discrètes agissent
///   à l'appui.
fn register_shortcuts(app: &AppHandle<Wry>, cfg: &Config, logger: &Logger) -> tauri::Result<()> {
    // Push-to-talk (RF-01) : appui → enregistrement, relâchement → pipeline.
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
                            kind: format!("register-pushToTalk-infructueux:{err}").into(),
                        });
                        let _ = app.emit(
                            "notice://notice",
                            serde_json::json!({
                                "kind": "hotkey",
                                "message": strings::ERR_HOTKEY_P0,
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

    // « Coller à nouveau » (RF-22) : ré-insertion du dernier texte.
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
    // « Montrer le dernier texte » (Shift+Alt+X) : fenêtre de secours.
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

/// Action discrète : un seul envoi à l'appui (le relâchement est ignoré, le
/// plugin délivre les deux états).
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
            kind: format!("register-{name}-infructueux:{err}").into(),
        });
        let _ = app.emit(
            "notice://notice",
            serde_json::json!({
                "kind": "hotkey",
                "message": strings::ERR_HOTKEY_P0,
            }),
        );
    }
}

fn log_registration(logger: &Logger, combo: &Combo, purpose: &str) {
    logger.log(Event::DeviceEvent {
        kind: format!("raccourci-{purpose}={}", combo.canonical()),
    });
}

/// plume_hotkeys::Combo → plugin `Shortcut` (nommage physique des touches).
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

// ------------------------------------------------------------- divers ---

fn config_code(err: &plume_config::ConfigError) -> u32 {
    match err {
        plume_config::ConfigError::Io(_) => 1,
        plume_config::ConfigError::Json(_) => 2,
        plume_config::ConfigError::UnknownSchema { .. } => 3,
    }
}
