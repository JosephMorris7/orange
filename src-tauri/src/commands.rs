//! Every `invoke(...)` the UI can make (see docs/CONTRACT.md).

use crate::fileactions;
use crate::settings::Settings;
use crate::state::{AppState, TauriEmitter};
use crate::win::{clipboard, ocr, shell};
use crate::windows::{self, hwnd_of};
use crate::wheel::{self, WheelItem};
use orange_core::Ctx;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

type R<T> = Result<T, String>;

fn pbs(paths: &[String]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

fn ctx(st: &AppState) -> Ctx {
    Ctx::new(st.caps.clone()).with_out_dir(st.settings().out_dir())
}

fn emitter(app: &AppHandle) -> Arc<TauriEmitter> {
    Arc::new(TauriEmitter { app: app.clone() })
}

#[tauri::command]
pub fn app_info() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "conversion_option_count": orange_convert::option_count(),
        "tool_count": orange_tools::all_tools().len(),
        "action_count": 6,
    })
}

#[tauri::command]
pub fn capabilities(state: State<'_, AppState>) -> Value {
    state.caps.to_json()
}

#[tauri::command]
pub async fn wheel_items(state: State<'_, AppState>, mode: String, paths: Vec<String>) -> R<Vec<WheelItem>> {
    let p = pbs(&paths);
    match mode.as_str() {
        "convert" => Ok(wheel::convert_items(&p, &state.caps)),
        "tools" => Ok(wheel::tool_items(&p, &state.caps)),
        "actions" => Ok(wheel::action_items(!p.is_empty())),
        other => Err(format!("unknown wheel mode: {other}")),
    }
}

#[tauri::command]
pub async fn wheel_commit(app: AppHandle, state: State<'_, AppState>, mode: String, id: String, paths: Vec<String>) -> R<Value> {
    if paths.is_empty() {
        windows::hide_wheel(&app);
        return Err("no files".into());
    }
    match mode.as_str() {
        "convert" => {
            windows::hide_wheel(&app);
            let job_id = start_convert(&app, &state, paths, id);
            Ok(json!({"kind": "job", "job_id": job_id}))
        }
        "tools" => {
            windows::hide_wheel(&app);
            let info = orange_tools::all_tools().into_iter().find(|t| t.id == id).ok_or_else(|| format!("unknown tool: {id}"))?;
            if info.ui == orange_tools::ToolUi::None {
                let job_id = start_tool(&app, &state, id, paths, json!({}));
                Ok(json!({"kind": "job", "job_id": job_id}))
            } else {
                open_tool_window_impl(&app, &id, &paths)?;
                Ok(json!({"kind": "dialog"}))
            }
        }
        "actions" => {
            // the share sheet needs the overlay as its owner window, so hide afterwards
            let keep_for_share = id == "share";
            if !keep_for_share {
                windows::hide_wheel(&app);
            }
            let r = run_action_impl(&app, &state, &id, &paths).await;
            if keep_for_share {
                windows::hide_wheel(&app);
            }
            r.map(|_| json!({"kind": "done"}))
        }
        other => Err(format!("unknown wheel mode: {other}")),
    }
}

#[tauri::command]
pub fn wheel_cancel(app: AppHandle) {
    windows::hide_wheel(&app);
}

fn start_convert(app: &AppHandle, state: &AppState, paths: Vec<String>, to: String) -> String {
    let files = pbs(&paths);
    state.jobs.spawn(emitter(app), state.caps.clone(), state.settings().out_dir(), move |ctx| {
        let mut outs = Vec::new();
        let n = files.len().max(1) as f32;
        for (i, f) in files.iter().enumerate() {
            ctx.check_cancel()?;
            ctx.report(i as f32 / n, format!("Converting {}", f.file_name().map(|s| s.to_string_lossy()).unwrap_or_default()));
            outs.extend(orange_convert::convert(f, &to, ctx)?);
        }
        ctx.report(1.0, "Done");
        Ok(outs)
    })
}

fn start_tool(app: &AppHandle, state: &AppState, tool_id: String, paths: Vec<String>, params: Value) -> String {
    let files = pbs(&paths);
    state.jobs.spawn(emitter(app), state.caps.clone(), state.settings().out_dir(), move |ctx| orange_tools::run_tool(&tool_id, &files, &params, ctx))
}

#[tauri::command]
pub async fn run_convert(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>, to: String) -> R<String> {
    if paths.is_empty() {
        return Err("no files".into());
    }
    Ok(start_convert(&app, &state, paths, to))
}

#[tauri::command]
pub async fn run_tool(app: AppHandle, state: State<'_, AppState>, tool_id: String, paths: Vec<String>, params: Value) -> R<String> {
    if paths.is_empty() {
        return Err("no files".into());
    }
    Ok(start_tool(&app, &state, tool_id, paths, params))
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: String) {
    state.jobs.cancel(&job_id);
}

#[tauri::command]
pub fn tool_info(tool_id: String) -> R<Value> {
    let t = orange_tools::all_tools().into_iter().find(|t| t.id == tool_id).ok_or_else(|| format!("unknown tool: {tool_id}"))?;
    serde_json::to_value(t).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn preview_tool(state: State<'_, AppState>, tool_id: String, path: String, params: Value, max_dim: Option<u32>) -> R<String> {
    let c = ctx(&state);
    orange_tools::preview(&tool_id, Path::new(&path), &params, &c, max_dim.unwrap_or(900)).map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn web_preview(state: State<'_, AppState>, path: String, max_dim: Option<u32>) -> R<String> {
    let c = ctx(&state);
    orange_tools::web_preview(Path::new(&path), &c, max_dim.unwrap_or(1600)).map(|p| p.to_string_lossy().into_owned()).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn file_info(state: State<'_, AppState>, path: String) -> R<Value> {
    orange_tools::file_info(Path::new(&path), &state.caps).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn read_metadata(state: State<'_, AppState>, path: String) -> R<Value> {
    orange_tools::read_metadata(Path::new(&path), &state.caps).map_err(|e| e.to_string())
}

/// Text lines or faces, as rectangles in the *source image's* pixels.
/// The Windows engines decode the picture themselves, so we hand them a browser-safe, orientation-corrected
/// copy and scale the result back to the original's pixel grid.
#[tauri::command]
pub async fn detect_regions(state: State<'_, AppState>, path: String, what: String) -> R<Vec<Value>> {
    let c = ctx(&state);
    let src = Path::new(&path);
    let info = orange_tools::file_info(src, &state.caps).map_err(|e| e.to_string())?;
    let (sw, sh) = (info["width"].as_f64().unwrap_or(0.0), info["height"].as_f64().unwrap_or(0.0));
    let tmp = orange_tools::web_preview(src, &c, 2400).map_err(|e| e.to_string())?;
    let tinfo = orange_tools::file_info(&tmp, &state.caps).map_err(|e| e.to_string())?;
    let (tw, th) = (tinfo["width"].as_f64().unwrap_or(0.0), tinfo["height"].as_f64().unwrap_or(0.0));
    let found = match what.as_str() {
        "text" => ocr::detect_text(&tmp),
        "faces" => ocr::detect_faces(&tmp),
        other => Err(format!("unknown region kind: {other}")),
    };
    let _ = std::fs::remove_file(&tmp);
    let regions = found?;
    if tw <= 0.0 || th <= 0.0 || sw <= 0.0 || sh <= 0.0 {
        return Ok(Vec::new());
    }
    let (kx, ky) = (sw / tw, sh / th);
    Ok(regions.into_iter().map(|r| json!({"x": r.x * kx, "y": r.y * ky, "w": r.w * kx, "h": r.h * ky})).collect())
}

fn open_tool_window_impl(app: &AppHandle, tool_id: &str, paths: &[String]) -> R<()> {
    use orange_tools::ToolUi::*;
    let info = orange_tools::all_tools().into_iter().find(|t| t.id == tool_id).ok_or_else(|| format!("unknown tool: {tool_id}"))?;
    let big = matches!(info.ui, Crop | EditPhoto | Annotate | AddBackground | Redact | Collage | Trim | Pages);
    windows::open_tool_window(app, tool_id, &info.title, big, paths)
}

#[tauri::command]
pub async fn open_tool_window(app: AppHandle, tool_id: String, paths: Vec<String>) -> R<()> {
    open_tool_window_impl(&app, &tool_id, &paths)
}

async fn run_action_impl(app: &AppHandle, state: &AppState, action: &str, paths: &[String]) -> R<Value> {
    let files = pbs(paths);
    if files.is_empty() {
        return Err("no files".into());
    }
    let owner = app.get_webview_window("wheel").map(|w| hwnd_of(&w)).unwrap_or(0);
    match action {
        "share" => {
            let (tx, rx) = std::sync::mpsc::channel();
            let f = files.clone();
            app.run_on_main_thread(move || {
                let _ = tx.send(shell::share(owner, &f));
            })
            .map_err(|e| e.to_string())?;
            rx.recv().map_err(|e| e.to_string())??;
            Ok(json!({"ok": true}))
        }
        "float" => {
            let list = {
                let mut s = state.shelf.lock().unwrap();
                for p in paths {
                    if !s.contains(p) {
                        s.push(p.clone());
                    }
                }
                s.clone()
            };
            let _ = app.emit("shelf:changed", &list);
            windows::show_shelf(app);
            Ok(json!({"ok": true, "message": format!("{} on the shelf", list.len())}))
        }
        "copy" => {
            clipboard::set_files(owner, &files)?;
            Ok(json!({"ok": true, "message": format!("Copied {} file(s)", files.len())}))
        }
        "copy_path" => {
            clipboard::set_paths_text(owner, &files)?;
            Ok(json!({"ok": true, "message": "Path copied"}))
        }
        "terminal" => {
            shell::open_terminal(&files[0])?;
            Ok(json!({"ok": true}))
        }
        "move" => {
            let (tx, rx) = std::sync::mpsc::channel();
            app.dialog().file().set_title("Move to…").pick_folder(move |f| {
                let _ = tx.send(f);
            });
            let dest = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten()).await.map_err(|e| e.to_string())?;
            let Some(dest) = dest else { return Ok(json!({"ok": true, "message": "Cancelled"})) };
            let dest = dest.into_path().map_err(|e| e.to_string())?;
            let mut moved = 0;
            for (from, to) in fileactions::plan_moves(&files, &dest) {
                fileactions::move_item(&from, &to).map_err(|e| format!("{}: {e}", from.display()))?;
                moved += 1;
            }
            Ok(json!({"ok": true, "message": format!("Moved {moved} item(s)")}))
        }
        other => Err(format!("unknown action: {other}")),
    }
}

#[tauri::command]
pub async fn run_action(app: AppHandle, state: State<'_, AppState>, action: String, paths: Vec<String>) -> R<Value> {
    run_action_impl(&app, &state, &action, &paths).await
}

fn shelf_emit(app: &AppHandle, list: &[String]) {
    let _ = app.emit("shelf:changed", list);
}

#[tauri::command]
pub fn shelf_list(state: State<'_, AppState>) -> Vec<String> {
    state.shelf.lock().unwrap().clone()
}

#[tauri::command]
pub fn shelf_add(app: AppHandle, state: State<'_, AppState>, paths: Vec<String>) -> Vec<String> {
    let mut s = state.shelf.lock().unwrap();
    for p in paths {
        if !s.contains(&p) {
            s.push(p);
        }
    }
    shelf_emit(&app, &s);
    s.clone()
}

#[tauri::command]
pub fn shelf_remove(app: AppHandle, state: State<'_, AppState>, path: String) -> Vec<String> {
    let mut s = state.shelf.lock().unwrap();
    s.retain(|p| *p != path);
    shelf_emit(&app, &s);
    s.clone()
}

#[tauri::command]
pub fn shelf_clear(app: AppHandle, state: State<'_, AppState>) -> Vec<String> {
    let mut s = state.shelf.lock().unwrap();
    s.clear();
    shelf_emit(&app, &s);
    Vec::new()
}

#[tauri::command]
pub fn reveal_in_folder(path: String) -> R<()> {
    shell::reveal(Path::new(&path))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> R<Settings> {
    let clean = settings.sanitized();
    {
        let mut cur = state.settings.lock().unwrap();
        let autostart_changed = cur.launch_at_login != clean.launch_at_login;
        *cur = clean.clone();
        state.session.lock().unwrap().set_bindings(clean.bindings());
        crate::settings::save_to(&state.settings_file, &clean).map_err(|e| format!("could not save settings: {e}"))?;
        if autostart_changed {
            use tauri_plugin_autostart::ManagerExt;
            let al = app.autolaunch();
            let r = if clean.launch_at_login { al.enable() } else { al.disable() };
            if let Err(e) = r {
                log::warn!("autostart: {e}");
            }
        }
    }
    let _ = app.emit("settings:changed", &clean);
    Ok(clean)
}

#[tauri::command]
pub async fn pick_files(app: AppHandle) -> R<Vec<String>> {
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().set_title("Choose files").pick_files(move |f| {
        let _ = tx.send(f);
    });
    let picked = tauri::async_runtime::spawn_blocking(move || rx.recv().ok().flatten()).await.map_err(|e| e.to_string())?;
    Ok(picked.unwrap_or_default().into_iter().filter_map(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned()).collect())
}

/// Open the wheel without a drag (the settings window's "Choose files…" button, the tray menu).
#[tauri::command]
pub fn open_wheel(app: AppHandle, mode: String, paths: Vec<String>) -> R<()> {
    if !matches!(mode.as_str(), "convert" | "tools" | "actions") {
        return Err(format!("unknown wheel mode: {mode}"));
    }
    windows::show_wheel(&app, &mode, 0, 0, Some(paths));
    Ok(())
}

/// A small PNG on disk to use as the drag preview when files are dragged out of the shelf.
#[tauri::command]
pub fn drag_icon() -> R<String> {
    let path = std::env::temp_dir().join("orange-drag-icon.png");
    if !path.is_file() {
        std::fs::write(&path, include_bytes!("../icons/128x128.png")).map_err(|e| e.to_string())?;
    }
    Ok(path.to_string_lossy().into_owned())
}

/// Lets the web UI write into the same log file (used by the wheel to record drag events).
#[tauri::command]
pub fn log_line(msg: String) {
    log::info!(target: "ui", "{msg}");
}
