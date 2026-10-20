//! User settings, persisted as JSON at `%APPDATA%\Orange\settings.json`.

use crate::session::{Bindings, Modifier};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Keys {
    pub convert: String,
    pub tools: String,
    pub actions: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OutputCfg {
    /// "beside" = next to each source file, "folder" = always into `folder`
    pub mode: String,
    pub folder: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub keys: Keys,
    pub output: OutputCfg,
    pub launch_at_login: bool,
    pub notifications: bool,
    pub wheel_scale: f64,
    /// "system" | "light" | "dark"
    pub theme: String,
    pub first_run_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            keys: Keys { convert: "Shift".into(), tools: "Control".into(), actions: "Alt".into() },
            output: OutputCfg { mode: "beside".into(), folder: None },
            launch_at_login: false,
            notifications: true,
            wheel_scale: 1.0,
            theme: "system".into(),
            first_run_done: false,
        }
    }
}

impl Settings {
    /// Repair anything out of range so the rest of the app can trust the values.
    pub fn sanitized(mut self) -> Settings {
        let d = Settings::default();
        self.wheel_scale = if self.wheel_scale.is_finite() { self.wheel_scale.clamp(0.8, 1.4) } else { 1.0 };
        if !matches!(self.theme.as_str(), "system" | "light" | "dark") {
            self.theme = "system".into();
        }
        if !matches!(self.output.mode.as_str(), "beside" | "folder") {
            self.output.mode = "beside".into();
        }
        if self.output.mode == "folder" && self.output.folder.as_deref().map_or(true, |f| f.trim().is_empty()) {
            self.output.mode = "beside".into();
        }
        // three distinct, valid modifier keys
        let parse = |s: &str| Modifier::parse(s);
        let (c, t, a) = (parse(&self.keys.convert), parse(&self.keys.tools), parse(&self.keys.actions));
        match (c, t, a) {
            (Some(c), Some(t), Some(a)) if c != t && t != a && c != a => {
                self.keys = Keys { convert: c.name().into(), tools: t.name().into(), actions: a.name().into() };
            }
            _ => self.keys = d.keys,
        }
        self
    }

    pub fn bindings(&self) -> Bindings {
        let s = self.clone().sanitized();
        Bindings {
            convert: Modifier::parse(&s.keys.convert).unwrap_or(Modifier::Shift),
            tools: Modifier::parse(&s.keys.tools).unwrap_or(Modifier::Control),
            actions: Modifier::parse(&s.keys.actions).unwrap_or(Modifier::Alt),
        }
    }

    /// Output directory override for jobs (`None` = beside the source files).
    pub fn out_dir(&self) -> Option<PathBuf> {
        if self.output.mode == "folder" {
            self.output.folder.as_ref().map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
        } else {
            None
        }
    }
}

pub fn settings_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Orange").join("settings.json")
}

pub fn load_from(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Settings>(&t).ok())
        .unwrap_or_default()
        .sanitized()
}

pub fn save_to(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(s)?)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        let d = Settings::default();
        assert_eq!(d.clone().sanitized(), d);
        let b = d.bindings();
        assert_eq!((b.convert, b.tools, b.actions), (Modifier::Shift, Modifier::Control, Modifier::Alt));
    }

    #[test]
    fn repairs_bad_values() {
        let mut s = Settings::default();
        s.wheel_scale = 9.0;
        s.theme = "neon".into();
        s.output = OutputCfg { mode: "folder".into(), folder: None };
        s.keys.tools = "Shift".into(); // duplicate of convert
        let s = s.sanitized();
        assert_eq!(s.wheel_scale, 1.4);
        assert_eq!(s.theme, "system");
        assert_eq!(s.output.mode, "beside");
        assert_eq!(s.keys, Settings::default().keys);
    }

    #[test]
    fn accepts_rebinding_and_aliases() {
        let mut s = Settings::default();
        s.keys = Keys { convert: "Alt".into(), tools: "Ctrl".into(), actions: "Shift".into() };
        let b = s.sanitized().bindings();
        assert_eq!((b.convert, b.tools, b.actions), (Modifier::Alt, Modifier::Control, Modifier::Shift));
    }

    #[test]
    fn roundtrip_and_partial_files() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a").join("settings.json");
        let mut s = Settings::default();
        s.notifications = false;
        s.output = OutputCfg { mode: "folder".into(), folder: Some("D:\\Out".into()) };
        save_to(&p, &s).unwrap();
        assert_eq!(load_from(&p), s);
        assert_eq!(s.out_dir(), Some(PathBuf::from("D:\\Out")));
        std::fs::write(&p, r#"{"theme":"dark"}"#).unwrap();
        let l = load_from(&p);
        assert_eq!(l.theme, "dark");
        assert!(l.notifications);
        std::fs::write(&p, "not json").unwrap();
        assert_eq!(load_from(&p), Settings::default());
    }
}
