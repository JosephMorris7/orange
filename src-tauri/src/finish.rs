//! The "finish moment": a small card by the tray when a job is done, with the size saved,
//! a "Show in folder" button and a short-lived "Undo".
//!
//! A small transparent window shows `finish.html`. Rust works out WHAT to say (from the `job:done` event) and
//! WHERE to put it (the bottom-right corner of the work area, above the taskbar); the page does the drawing.
//! Undo only ever deletes files that this very job created, never the originals (see `undo_files`).

use crate::geometry::Rect;
#[cfg(windows)]
use crate::win::overlay;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

const LABEL: &str = "finish";
/// Logical size of the window; the card inside leaves room for its glow.
const WIN_W: f64 = 460.0;
const WIN_H: f64 = 206.0;
/// Undo only removes files modified this recently, as one more guard against touching anything old.
const UNDO_WINDOW: Duration = Duration::from_secs(20 * 60);

/// What the page shows.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Card {
    /// "Report.docx → Report.pdf", or "3 files created".
    pub title: String,
    /// "Saved 38 MB · 62% smaller", or just the size of the result. May be empty.
    pub detail: String,
    /// The file "Show in folder" selects.
    pub first_output: String,
    pub can_undo: bool,
}

struct Current {
    card: Card,
    outputs: Vec<PathBuf>,
    inputs: Vec<PathBuf>,
}

static CURRENT: Mutex<Option<Current>> = Mutex::new(None);

/// "412 KB", "38 MB", "1.4 GB".
pub fn format_bytes(n: u64) -> String {
    const KB: f64 = 1024.0;
    let f = n as f64;
    if f < KB {
        format!("{n} B")
    } else if f < KB * KB {
        format!("{:.0} KB", f / KB)
    } else if f < KB * KB * KB {
        let mb = f / (KB * KB);
        if mb >= 10.0 { format!("{mb:.0} MB") } else { format!("{mb:.1} MB") }
    } else {
        format!("{:.1} GB", f / (KB * KB * KB))
    }
}

/// The last part of a path, whichever kind of slash it uses.
fn file_name(p: &str) -> String {
    p.rsplit(['\\', '/']).find(|s| !s.is_empty()).unwrap_or(p).to_string()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

/// Build the card (and what Undo may touch) from a successful `job:done` payload.
fn describe(payload: &Value) -> Option<Current> {
    let outputs = strs(&payload["outputs"]);
    let inputs = strs(&payload["inputs"]);
    if outputs.is_empty() || !payload["error"].is_null() || payload["cancelled"].as_bool() == Some(true) {
        return None;
    }
    let title = match (inputs.len(), outputs.len()) {
        (1, 1) => format!("{} → {}", file_name(&inputs[0]), file_name(&outputs[0])),
        (_, 1) => file_name(&outputs[0]),
        (_, n) => format!("{n} files created"),
    };
    let (in_b, out_b) = (payload["in_bytes"].as_u64().unwrap_or(0), payload["out_bytes"].as_u64().unwrap_or(0));
    let detail = if in_b > 0 && out_b > 0 && out_b < in_b {
        let pct = ((in_b - out_b) as f64 / in_b as f64 * 100.0).round() as u64;
        if pct >= 1 { format!("Saved {} · {pct}% smaller", format_bytes(in_b - out_b)) } else { format_bytes(out_b) }
    } else if out_b > 0 {
        format_bytes(out_b)
    } else {
        String::new()
    };
    let out_paths: Vec<PathBuf> = outputs.iter().map(PathBuf::from).collect();
    let in_paths: Vec<PathBuf> = inputs.iter().map(PathBuf::from).collect();
    // a result that is one of the originals (an edit in place) can never be undone by deleting it
    let can_undo = !out_paths.iter().any(|o| in_paths.contains(o));
    Some(Current { card: Card { title, detail, first_output: outputs[0].clone(), can_undo }, outputs: out_paths, inputs: in_paths })
}

/// Bottom-right corner of `work`, `margin` pixels in from the edges.
pub fn corner_position(work: Rect, size: (i32, i32), margin: i32) -> (i32, i32) {
    ((work.right - size.0 - margin).max(work.left), (work.bottom - size.1 - margin).max(work.top))
}

/// Delete the files a job created. Never deletes anything that is not a plain file, anything that was one of the
/// job's inputs, or anything not modified within `UNDO_WINDOW`. Returns how many files were removed.
pub fn undo_files(outputs: &[PathBuf], inputs: &[PathBuf], now: SystemTime) -> usize {
    let mut removed = 0;
    for o in outputs {
        if inputs.contains(o) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(o) else { continue };
        if !meta.is_file() {
            continue;
        }
        let recent = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).map_or(false, |age| age <= UNDO_WINDOW);
        if recent && std::fs::remove_file(o).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Show the card for a finished job (called from the job thread). Does nothing for failed or cancelled jobs.
pub fn show(app: &AppHandle, payload: &Value) {
    let Some(cur) = describe(payload) else { return };
    let card = cur.card.clone();
    *CURRENT.lock().unwrap() = Some(cur);
    // a card is already up: just swap its contents
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.emit("finish:show", card);
        return;
    }
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("finish.html".into()))
        .title("Orange")
        .inner_size(WIN_W, WIN_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build();
    match built {
        Ok(w) => {
            #[cfg(windows)]
            overlay::make_noactivate(crate::windows::hwnd_of(&w));
            let _ = &w;
        }
        Err(e) => log::warn!(target: "finish", "could not create the card window: {e}"),
    }
}

fn place_and_show(app: &AppHandle) {
    let Some(w) = app.get_webview_window(LABEL) else { return };
    let (mut x, mut y, mut pw, mut ph) = (0, 0, WIN_W.round() as i32, WIN_H.round() as i32);
    if let Some(mon) = app.primary_monitor().ok().flatten() {
        let sf = mon.scale_factor();
        let wa = mon.work_area();
        let work = Rect { left: wa.position.x, top: wa.position.y, right: wa.position.x + wa.size.width as i32, bottom: wa.position.y + wa.size.height as i32 };
        pw = (WIN_W * sf).round() as i32;
        ph = (WIN_H * sf).round() as i32;
        (x, y) = corner_position(work, (pw, ph), (8.0 * sf).round() as i32);
    }
    log::info!(target: "finish", "card at ({x},{y}) size {pw}x{ph}");
    #[cfg(windows)]
    overlay::show_at(crate::windows::hwnd_of(&w), x, y, pw, ph);
    #[cfg(not(windows))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = w.set_position(PhysicalPosition::new(x, y));
        let _ = w.set_size(PhysicalSize::new(pw as u32, ph as u32));
        let _ = w.show();
    }
}

/// The page has loaded: place and show the window, and hand over what to say.
#[tauri::command]
pub fn finish_ready(app: AppHandle) -> Option<Card> {
    let card = CURRENT.lock().unwrap().as_ref().map(|c| c.card.clone());
    if card.is_some() {
        place_and_show(&app);
    }
    card
}

/// The card was dismissed (or timed out): remove the window.
#[tauri::command]
pub fn finish_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}

/// "Undo": delete the files the last finished job created. Returns how many were removed.
#[tauri::command]
pub fn finish_undo() -> usize {
    let cur = CURRENT.lock().unwrap().take();
    match cur {
        Some(c) if c.card.can_undo => {
            let n = undo_files(&c.outputs, &c.inputs, SystemTime::now());
            log::info!(target: "finish", "undo removed {n} file(s)");
            n
        }
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn done(inputs: &[&str], outputs: &[&str], in_b: u64, out_b: u64) -> Value {
        json!({"job_id": "job-1", "inputs": inputs, "outputs": outputs, "in_bytes": in_b, "out_bytes": out_b, "error": null, "cancelled": false})
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_bytes(900), "900 B");
        assert_eq!(format_bytes(412 * 1024), "412 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024 + 512 * 1024), "3.5 MB");
        assert_eq!(format_bytes(38 * 1024 * 1024), "38 MB");
        assert_eq!(format_bytes(1500 * 1024 * 1024), "1.5 GB");
    }

    #[test]
    fn one_file_in_one_file_out_shows_the_arrow_and_the_saving() {
        let c = describe(&done(&["C:\\a\\Report.docx"], &["C:\\a\\Report.pdf"], 100 * 1024 * 1024, 38 * 1024 * 1024)).unwrap().card;
        assert_eq!(c.title, "Report.docx → Report.pdf");
        assert_eq!(c.detail, "Saved 62 MB · 62% smaller");
        assert_eq!(c.first_output, "C:\\a\\Report.pdf");
        assert!(c.can_undo);
    }

    #[test]
    fn bigger_results_just_show_their_size() {
        let c = describe(&done(&["a.png"], &["a.pdf"], 1000, 5 * 1024 * 1024)).unwrap().card;
        assert_eq!(c.detail, "5.0 MB");
        let c = describe(&done(&["a.png"], &["a.webp"], 1000, 0)).unwrap().card;
        assert_eq!(c.detail, "");
    }

    #[test]
    fn many_files_and_failures() {
        assert_eq!(describe(&done(&["a", "b", "c"], &["x", "y", "z"], 0, 0)).unwrap().card.title, "3 files created");
        assert_eq!(describe(&done(&["a", "b"], &["only.zip"], 0, 0)).unwrap().card.title, "only.zip");
        assert!(describe(&done(&["a"], &[], 0, 0)).is_none());
        assert!(describe(&json!({"outputs": ["x"], "error": "boom", "cancelled": false})).is_none());
        assert!(describe(&json!({"outputs": ["x"], "error": null, "cancelled": true})).is_none());
    }

    #[test]
    fn an_edit_in_place_cannot_be_undone() {
        let c = describe(&done(&["C:\\a\\p.png"], &["C:\\a\\p.png"], 10, 8)).unwrap().card;
        assert!(!c.can_undo);
    }

    #[test]
    fn sits_in_the_bottom_right_corner() {
        let work = Rect { left: 0, top: 0, right: 1920, bottom: 1032 };
        assert_eq!(corner_position(work, (460, 206), 8), (1452, 818));
        let tiny = Rect { left: 0, top: 0, right: 300, bottom: 100 };
        assert_eq!(corner_position(tiny, (460, 206), 8), (0, 0));
    }

    #[test]
    fn undo_removes_only_fresh_results_never_originals() {
        let dir = tempfile::tempdir().unwrap();
        let orig = dir.path().join("orig.png");
        let result = dir.path().join("orig.webp");
        let folder = dir.path().join("sub");
        std::fs::write(&orig, b"original").unwrap();
        std::fs::write(&result, b"result").unwrap();
        std::fs::create_dir(&folder).unwrap();
        let now = SystemTime::now();
        // an output that is also an input, a folder, and a missing file are all left alone
        let n = undo_files(&[orig.clone(), folder.clone(), dir.path().join("missing.webp"), result.clone()], &[orig.clone()], now);
        assert_eq!(n, 1);
        assert!(orig.exists() && folder.exists());
        assert!(!result.exists());
    }

    #[test]
    fn undo_leaves_old_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("old.pdf");
        std::fs::write(&f, b"x").unwrap();
        let far_future = SystemTime::now() + Duration::from_secs(3 * 3600);
        assert_eq!(undo_files(&[f.clone()], &[], far_future), 0);
        assert!(f.exists());
    }
}
