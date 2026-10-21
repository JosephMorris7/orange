//! Creating and showing Orange's windows (wheel overlay, tool dialogs, shelf, main).

use crate::state::AppState;
use crate::win::overlay;
use serde_json::json;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Logical size of the wheel overlay at scale 1.0.
pub const WHEEL_SIZE: f64 = 640.0;

#[cfg(windows)]
pub fn hwnd_of(w: &WebviewWindow) -> isize {
    w.hwnd().map(|h| h.0 as isize).unwrap_or(0)
}
#[cfg(not(windows))]
pub fn hwnd_of(_w: &WebviewWindow) -> isize {
    0
}

/// Create the hidden overlay window at startup (so showing it later is instant).
pub fn create_wheel(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let w = WebviewWindowBuilder::new(app, "wheel", WebviewUrl::App("wheel.html".into()))
        .title("Orange wheel")
        .inner_size(WHEEL_SIZE, WHEEL_SIZE)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build()?;
    let _ = w.set_decorations(false);
    overlay::make_noactivate(hwnd_of(&w));
    Ok(w)
}

/// Show the wheel centred on the screen point (x, y). `paths` is only set when the wheel is opened
/// without a drag (tray "Convert…", `--demo-wheel`); then it takes focus so Esc works.
pub fn show_wheel(app: &AppHandle, mode: &str, x: i32, y: i32, paths: Option<Vec<String>>) {
    let st = app.state::<AppState>();
    let Some(w) = app.get_webview_window("wheel") else { return };
    let scale = st.settings().wheel_scale;
    // opened without a drag (tray / settings button): centre on the mouse pointer
    let (x, y) = match (&paths, app.cursor_position()) {
        (Some(_), Ok(p)) => (p.x as i32, p.y as i32),
        _ => (x, y),
    };
    st.wheel_gen.fetch_add(1, Ordering::SeqCst);
    *st.wheel_visible.lock().unwrap() = true;

    #[cfg(windows)]
    {
        let dpi = overlay::dpi_scale_at(x, y);
        let side = (WHEEL_SIZE * scale * dpi).round() as i32;
        let work = overlay::work_area_at(x, y);
        let (px, py) = match work {
            Some(r) => crate::geometry::place_centered((x, y), (side, side), r),
            None => (x - side / 2, y - side / 2),
        };
        log::info!(target: "wheel", "show {mode} cursor=({x},{y}) dpi={dpi} side={side} at=({px},{py}) work={work:?}");
        overlay::show_at(hwnd_of(&w), px, py, side, side);
        if paths.is_none() {
            show_drop_catcher(app, px, py, side);
        }
        if paths.is_some() {
            let _ = w.set_focus();
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        let _ = w.set_size(LogicalSize::new(WHEEL_SIZE * scale, WHEEL_SIZE * scale));
        let _ = w.center();
        let _ = w.show();
        if paths.is_some() {
            let _ = w.set_focus();
        }
    }
    let _ = w.emit("wheel:show", json!({"mode": mode, "x": x, "y": y, "scale": scale, "paths": paths}));
}

pub fn hide_wheel(app: &AppHandle) {
    let st = app.state::<AppState>();
    *st.wheel_visible.lock().unwrap() = false;
    #[cfg(windows)]
    let _ = app.run_on_main_thread(crate::win::droptarget::hide_catcher);
    if let Some(w) = app.get_webview_window("wheel") {
        #[cfg(windows)]
        overlay::hide(hwnd_of(&w));
        #[cfg(not(windows))]
        let _ = w.hide();
        let _ = w.emit("wheel:hide", ());
    }
}

/// Hide the wheel after `ms`, unless a newer wheel opened (or it was already closed) meanwhile.
pub fn hide_wheel_later(app: &AppHandle, gen: u64, ms: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let st = app.state::<AppState>();
        if st.wheel_gen.load(Ordering::SeqCst) == gen && *st.wheel_visible.lock().unwrap() {
            hide_wheel(&app);
        }
    });
}

pub fn show_main(app: &AppHandle) {
    let w = match app.get_webview_window("main") {
        Some(w) => w,
        None => match WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("Orange")
            .inner_size(980.0, 700.0)
            .min_inner_size(760.0, 560.0)
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                log::error!("main window: {e}");
                return;
            }
        },
    };
    let _ = w.show();
    let _ = w.unminimize();
    let _ = w.set_focus();
}

pub fn show_shelf(app: &AppHandle) {
    let w = match app.get_webview_window("shelf") {
        Some(w) => w,
        None => match WebviewWindowBuilder::new(app, "shelf", WebviewUrl::App("shelf.html".into()))
            .title("Orange shelf")
            .inner_size(300.0, 440.0)
            .always_on_top(true)
            .skip_taskbar(true)
            .decorations(false)
            .resizable(true)
            .build()
        {
            Ok(w) => w,
            Err(e) => {
                log::error!("shelf window: {e}");
                return;
            }
        },
    };
    let _ = w.show();
    let _ = w.set_focus();
}

/// Open a tool dialog window for `tool_id` on `paths`.
pub fn open_tool_window(app: &AppHandle, tool_id: &str, title: &str, big: bool, paths: &[String]) -> Result<(), String> {
    let st = app.state::<AppState>();
    let n = st.tool_windows.fetch_add(1, Ordering::SeqCst) + 1;
    let json = serde_json::to_string(paths).map_err(|e| e.to_string())?;
    let url = format!("tool.html?tool={}&paths={}", urlencoding::encode(tool_id), urlencoding::encode(&json));
    log::info!(target: "tool", "open {tool_id} with {} file(s)", paths.len());
    let (w, h) = if big { (1040.0, 720.0) } else { (560.0, 640.0) };
    let win = WebviewWindowBuilder::new(app, format!("tool-{n}"), WebviewUrl::App(url.as_str().into()))
        .title(format!("{title} — Orange"))
        .inner_size(w, h)
        .min_inner_size(480.0, 420.0)
        .center()
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_focus();
    Ok(())
}

/// While a file is being dragged, put an invisible native drop target over the wheel (see `win::droptarget`)
/// and forward what it sees to the page.
#[cfg(windows)]
fn show_drop_catcher(app: &AppHandle, x: i32, y: i32, side: i32) {
    use crate::win::{droptarget, DropEvent};
    let emitter = app.clone();
    let cb: std::sync::Arc<dyn Fn(DropEvent) + Send + Sync> = std::sync::Arc::new(move |ev: DropEvent| {
        let payload = match &ev {
            DropEvent::Enter { paths, x, y } => {
                log::info!(target: "drag", "native enter {} file(s) at ({x},{y})", paths.len());
                json!({"type": "enter", "paths": paths, "x": x, "y": y})
            }
            DropEvent::Over { x, y } => json!({"type": "over", "x": x, "y": y}),
            DropEvent::Leave => {
                log::info!(target: "drag", "native leave");
                json!({"type": "leave"})
            }
            DropEvent::Drop { paths, x, y } => {
                log::info!(target: "drag", "native drop {} file(s) at ({x},{y})", paths.len());
                json!({"type": "drop", "paths": paths, "x": x, "y": y})
            }
        };
        let _ = emitter.emit_to("wheel", "wheel:drag", payload);
    });
    let _ = app.run_on_main_thread(move || {
        match droptarget::ensure_catcher(cb) {
            Ok(info) => log::info!(target: "drag", "drop catcher ready: {info}"),
            Err(e) => log::warn!(target: "drag", "drop catcher failed: {e}"),
        }
        droptarget::show_catcher(x, y, side, side);
    });
}
