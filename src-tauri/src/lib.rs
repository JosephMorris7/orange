#![cfg_attr(not(windows), allow(dead_code))]
//! Orange: drag a file, tap a key, drop it on a wheel. The Windows-specific parts live in `win/`.

mod commands;
mod fileactions;
mod geometry;
mod jobs;
mod logger;
mod session;
mod settings;
mod state;
mod wheel;
mod win;
mod windows;

use session::{Output, RawEvent};
use state::AppState;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};

/// Drag-session worker: hook events in, wheel show/hide out. Kept off the hook thread on purpose.
fn spawn_session_worker(app: AppHandle) {
    let (tx, rx) = mpsc::channel::<RawEvent>();
    match win::hooks::start(tx) {
        Ok(handle) => {
            log::info!(target: "hooks", "mouse and keyboard hooks installed");
            // lives for the whole process
            std::mem::forget(handle);
        }
        Err(e) => {
            log::error!(target: "hooks", "input hooks NOT started: {e}");
            return;
        }
    }
    std::thread::Builder::new()
        .name("orange-session".into())
        .spawn(move || {
            while let Ok(ev) = rx.recv() {
                let st = app.state::<AppState>();
                let out = st.session.lock().unwrap().handle(ev);
                if !matches!(ev, RawEvent::MouseMove { .. }) || out != Output::None {
                    log::info!(target: "session", "{ev:?} -> {out:?}");
                }
                match out {
                    Output::None => {}
                    Output::ShowWheel { mode, x, y } => {
                        let name = match mode {
                            session::Mode::Convert => "convert",
                            session::Mode::Tools => "tools",
                            session::Mode::Actions => "actions",
                        };
                        windows::show_wheel(&app, name, x, y, None);
                    }
                    Output::HideWheel => windows::hide_wheel(&app),
                    Output::EndSession { wheel_open } => {
                        if wheel_open {
                            // the OLE drop (if any) reaches the webview first; hide if nothing handled it
                            let gen = st.wheel_gen.load(Ordering::SeqCst);
                            windows::hide_wheel_later(&app, gen, 400);
                        }
                    }
                }
            }
        })
        .expect("spawn session worker");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let convert = MenuItemBuilder::with_id("convert", "Convert…").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let shelf = MenuItemBuilder::with_id("shelf", "Show float shelf").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit Orange").build(app)?;
    let menu = MenuBuilder::new(app).items(&[&convert, &settings, &shelf]).separator().item(&quit).build()?;
    let mut tray = TrayIconBuilder::with_id("orange").tooltip("Orange").menu(&menu).show_menu_on_left_click(false);
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.on_menu_event(|app, event| match event.id().as_ref() {
        "convert" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(files) = commands::pick_files(app.clone()).await {
                    if !files.is_empty() {
                        windows::show_wheel(&app, "convert", 0, 0, Some(files));
                    }
                }
            });
        }
        "settings" => windows::show_main(app),
        "shelf" => windows::show_shelf(app),
        "quit" => app.exit(0),
        _ => {}
    })
    .on_tray_icon_event(|tray, event| {
        if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = event {
            windows::show_main(tray.app_handle());
        }
    })
    .build(app)?;
    Ok(())
}

/// `Orange --demo-wheel [convert|tools|actions] [file…]` opens the wheel without any drag, for development.
fn demo_wheel_args() -> Option<(String, Vec<String>)> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == "--demo-wheel")?;
    let mut rest = args[i + 1..].iter().cloned();
    let first = rest.next();
    match first.as_deref() {
        Some(m @ ("convert" | "tools" | "actions")) => Some((m.to_string(), rest.collect())),
        Some(f) => Some(("convert".into(), std::iter::once(f.to_string()).chain(rest).collect())),
        None => Some(("convert".into(), Vec::new())),
    }
}

pub fn run() {
    logger::init(&settings::settings_path().with_file_name("orange.log"));
    let hidden = std::env::args().any(|a| a == "--hidden");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| windows::show_main(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::capabilities,
            commands::recheck_helpers,
            commands::install_libreoffice,
            commands::wheel_items,
            commands::wheel_commit,
            commands::wheel_cancel,
            commands::run_convert,
            commands::run_tool,
            commands::cancel_job,
            commands::tool_info,
            commands::preview_tool,
            commands::web_preview,
            commands::file_info,
            commands::read_metadata,
            commands::detect_regions,
            commands::open_tool_window,
            commands::run_action,
            commands::shelf_list,
            commands::shelf_add,
            commands::shelf_remove,
            commands::shelf_clear,
            commands::reveal_in_folder,
            commands::get_settings,
            commands::set_settings,
            commands::pick_files,
            commands::open_wheel,
            commands::drag_icon,
            commands::log_line,
        ])
        .on_window_event(|window, event| {
            // Orange stays resident in the tray: closing the settings window only hides it.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" || window.label() == "shelf" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let wheel = windows::create_wheel(&handle)?;
            wheel.on_webview_event(|e| {
                if let tauri::WebviewEvent::DragDrop(d) = e {
                    match d {
                        tauri::DragDropEvent::Enter { paths, position } => log::info!(target: "drag", "enter {} file(s) at {:?}", paths.len(), position),
                        tauri::DragDropEvent::Drop { paths, position } => log::info!(target: "drag", "drop {} file(s) at {:?}", paths.len(), position),
                        tauri::DragDropEvent::Leave => log::info!(target: "drag", "leave"),
                        _ => {}
                    }
                }
            });
            build_tray(&handle)?;
            spawn_session_worker(handle.clone());
            let first_run = !app.state::<AppState>().settings().first_run_done;
            if let Some((mode, files)) = demo_wheel_args() {
                windows::show_main(&handle);
                windows::show_wheel(&handle, &mode, 0, 0, Some(files));
            } else if first_run || !hidden {
                windows::show_main(&handle);
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Orange")
        .run(|_app, event| {
            // keep running in the tray when the last window closes
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
