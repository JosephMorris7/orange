//! What each wheel shows. Pure functions (no Tauri) so they are unit-testable.

use orange_core::{Capabilities, Dep};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct WheelItem {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub enabled: bool,
    pub hint: Option<String>,
}

fn missing_hint(missing: &[Dep]) -> Option<String> {
    if missing.is_empty() {
        None
    } else {
        Some(format!("Needs {}", missing.iter().map(|d| d.name()).collect::<Vec<_>>().join(", ")))
    }
}

/// Target formats every selected file can be converted to (order follows the first file).
pub fn convert_items(paths: &[PathBuf], caps: &Capabilities) -> Vec<WheelItem> {
    let Some(first) = paths.first() else { return Vec::new() };
    let mut items: Vec<WheelItem> = orange_convert::options_for(first, caps)
        .into_iter()
        .map(|o| WheelItem { id: o.id, label: o.label, icon: None, enabled: o.available, hint: o.note })
        .collect();
    for p in &paths[1..] {
        let other = orange_convert::options_for(p, caps);
        items.retain(|i| other.iter().any(|o| o.id == i.id));
        for i in items.iter_mut() {
            if let Some(o) = other.iter().find(|o| o.id == i.id) {
                if !o.available {
                    i.enabled = false;
                    i.hint = o.note.clone();
                }
            }
        }
    }
    items
}

/// Tools that apply to this selection.
pub fn tool_items(paths: &[PathBuf], caps: &Capabilities) -> Vec<WheelItem> {
    orange_tools::tools_for(paths, caps)
        .into_iter()
        .map(|t| {
            let missing: Vec<Dep> = t.deps.iter().copied().filter(|d| !caps.has(*d)).collect();
            WheelItem { id: t.id, label: t.label, icon: Some(t.icon), enabled: missing.is_empty(), hint: missing_hint(&missing) }
        })
        .collect()
}

pub fn action_items(has_files: bool) -> Vec<WheelItem> {
    let mk = |id: &str, label: &str, icon: &str, hint: &str| WheelItem {
        id: id.into(),
        label: label.into(),
        icon: Some(icon.into()),
        enabled: has_files,
        hint: Some(hint.into()),
    };
    vec![
        mk("share", "SHARE", "share", "Open the Windows share sheet"),
        mk("float", "FLOAT", "layers", "Park on the floating shelf"),
        mk("copy", "COPY", "copy", "Copy files to the clipboard"),
        mk("move", "MOVE", "folder-move", "Move to a folder you pick"),
        mk("copy_path", "COPY PATH", "link", "Copy the full path"),
        mk("terminal", "TERMINAL", "terminal", "Open a terminal here"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn caps() -> Capabilities {
        Capabilities::from_paths(HashMap::new())
    }

    #[test]
    fn actions_are_the_six_documented_ones() {
        let ids: Vec<_> = action_items(true).into_iter().map(|i| i.id).collect();
        assert_eq!(ids, ["share", "float", "copy", "move", "copy_path", "terminal"]);
        assert!(action_items(false).iter().all(|i| !i.enabled));
    }

    #[test]
    fn convert_items_exclude_own_format_and_flag_missing_tools() {
        let items = convert_items(&[PathBuf::from("a.png")], &caps());
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.id != "png"));
        // with no ImageMagick/ffmpeg found nothing can be run, and the wheel says so
        assert!(items.iter().all(|i| !i.enabled && i.hint.is_some()));
    }

    #[test]
    fn mixed_selection_keeps_only_shared_targets() {
        let items = convert_items(&[PathBuf::from("a.png"), PathBuf::from("b.mp4")], &caps());
        let png_only = convert_items(&[PathBuf::from("a.png")], &caps());
        assert!(items.len() < png_only.len());
    }

    #[test]
    fn tools_follow_selection() {
        let one = tool_items(&[PathBuf::from("a.png")], &caps());
        assert!(one.iter().any(|t| t.id == "compress-image"));
        assert!(tool_items(&[], &caps()).is_empty());
    }
}
