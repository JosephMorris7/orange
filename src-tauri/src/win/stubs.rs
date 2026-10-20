//! Non-Windows stand-ins so the whole app crate builds and the UI can be exercised on Linux/macOS.
//! Same function signatures as the real modules.

pub mod hooks {
    use crate::session::RawEvent;
    use std::sync::mpsc::Sender;
    pub struct HookHandle;
    impl HookHandle {
        pub fn stop(self) {}
    }
    pub fn start(_tx: Sender<RawEvent>) -> Result<HookHandle, String> {
        Err("global drag detection is Windows-only; use the tray's \"Show wheel\" to try the interface".into())
    }
}

pub mod overlay {
    use crate::geometry::Rect;
    pub fn make_noactivate(_window: isize) {}
    pub fn strip_frame(_window: isize) {}
    pub fn show_at(_window: isize, _x: i32, _y: i32, _w: i32, _h: i32) {}
    pub fn hide(_window: isize) {}
    pub fn work_area_at(_x: i32, _y: i32) -> Option<Rect> {
        None
    }
    pub fn dpi_scale_at(_x: i32, _y: i32) -> f64 {
        1.0
    }
}

pub mod clipboard {
    use std::path::PathBuf;
    pub fn set_files(_owner: isize, _paths: &[PathBuf]) -> Result<(), String> {
        Err("copying files to the clipboard is Windows-only".into())
    }
    pub fn set_text(_owner: isize, text: &str) -> Result<(), String> {
        // Dev convenience: best-effort via common CLI tools.
        for (prog, args) in [("wl-copy", vec![]), ("xclip", vec!["-selection", "clipboard"]), ("pbcopy", vec![])] {
            if let Ok(mut c) = std::process::Command::new(prog).args(&args).stdin(std::process::Stdio::piped()).spawn() {
                use std::io::Write;
                if let Some(mut i) = c.stdin.take() {
                    let _ = i.write_all(text.as_bytes());
                }
                let _ = c.wait();
                return Ok(());
            }
        }
        Err("no clipboard tool found".into())
    }
    pub fn set_paths_text(owner: isize, paths: &[PathBuf]) -> Result<(), String> {
        set_text(owner, &crate::fileactions::copy_path_text(paths))
    }
}

pub mod shell {
    use std::path::{Path, PathBuf};
    pub fn reveal(path: &Path) -> Result<(), String> {
        let dir = crate::fileactions::terminal_dir(path);
        std::process::Command::new("xdg-open").arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
    pub fn open_terminal(_path: &Path) -> Result<(), String> {
        Err("opening a terminal here is Windows-only".into())
    }
    pub fn share(_owner: isize, _paths: &[PathBuf]) -> Result<(), String> {
        Err("the share sheet is Windows-only".into())
    }
}

pub mod ocr {
    use std::path::Path;
    #[derive(Clone, Copy, Debug, PartialEq)]
    pub struct Region {
        pub x: f64,
        pub y: f64,
        pub w: f64,
        pub h: f64,
    }
    pub fn detect_text(_path: &Path) -> Result<Vec<Region>, String> {
        Ok(Vec::new())
    }
    pub fn detect_faces(_path: &Path) -> Result<Vec<Region>, String> {
        Ok(Vec::new())
    }
}

pub mod droptarget {
    use super::super::DropEvent;
    use std::sync::Arc;
    pub fn ensure_catcher(_cb: Arc<dyn Fn(DropEvent) + Send + Sync>) -> Result<String, String> {
        Ok(String::new())
    }
    pub fn show_catcher(_x: i32, _y: i32, _w: i32, _h: i32) {}
    pub fn hide_catcher() {}
}
