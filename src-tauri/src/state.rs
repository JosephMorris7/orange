//! Shared application state and the Tauri-backed event emitter.

use crate::jobs::{Emitter, Jobs};
use crate::session::Session;
use crate::settings::{self, Settings};
use orange_core::Capabilities;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter as _};

pub struct AppState {
    pub caps: Arc<Capabilities>,
    pub settings: Mutex<Settings>,
    pub settings_file: PathBuf,
    pub jobs: Arc<Jobs>,
    pub shelf: Mutex<Vec<String>>,
    pub session: Arc<Mutex<Session>>,
    /// bumped every time the wheel opens; lets a delayed "hide" ignore a newer wheel
    pub wheel_gen: AtomicU64,
    pub wheel_visible: Mutex<bool>,
    pub tool_windows: AtomicUsize,
}

impl AppState {
    pub fn new() -> AppState {
        let settings_file = settings::settings_path();
        let st = settings::load_from(&settings_file);
        let session = Session::new(st.bindings());
        AppState {
            caps: Arc::new(Capabilities::detect(&[])),
            settings: Mutex::new(st),
            settings_file,
            jobs: Jobs::new(),
            shelf: Mutex::new(Vec::new()),
            session: Arc::new(Mutex::new(session)),
            wheel_gen: AtomicU64::new(0),
            wheel_visible: Mutex::new(false),
            tool_windows: AtomicUsize::new(0),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }
}

/// Forwards job events to every window and raises a desktop notification when a job ends.
pub struct TauriEmitter {
    pub app: AppHandle,
}

impl Emitter for TauriEmitter {
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload.clone());
        if event == "job:done" {
            use tauri::Manager;
            use tauri_plugin_notification::NotificationExt;
            let notify = self.app.state::<AppState>().settings().notifications;
            if notify {
                let outputs = payload["outputs"].as_array().map(|a| a.len()).unwrap_or(0);
                let (title, body) = if let Some(err) = payload["error"].as_str() {
                    ("Orange couldn't finish".to_string(), err.to_string())
                } else if payload["cancelled"].as_bool() == Some(true) {
                    return;
                } else {
                    let first = payload["outputs"][0].as_str().and_then(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()));
                    ("Done".to_string(), match (outputs, first) {
                        (1, Some(n)) => n,
                        (n, _) => format!("{n} files created"),
                    })
                };
                let _ = self.app.notification().builder().title(title).body(body).show();
            }
        }
    }
}
