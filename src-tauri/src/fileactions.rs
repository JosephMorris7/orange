//! Platform-independent pieces of the file actions (clipboard payloads, move planning, terminal commands).
//! The Windows-only plumbing that uses these lives in `win/`.

use orange_core::paths::unique_path;
use std::path::{Path, PathBuf};

/// One path per line, CRLF separated (what Notepad and most Windows apps expect).
pub fn copy_path_text(paths: &[PathBuf]) -> String {
    paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\r\n")
}

fn utf16z(s: &str, out: &mut Vec<u8>) {
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out.extend_from_slice(&0u16.to_le_bytes());
}

/// Bytes of a `CF_HDROP` clipboard payload: a `DROPFILES` header followed by a double-NUL terminated
/// list of UTF-16 paths. Pasting it in Explorer copies the files.
pub fn build_dropfiles(paths: &[PathBuf]) -> Vec<u8> {
    const HEADER: u32 = 20; // sizeof(DROPFILES)
    let mut out = Vec::new();
    out.extend_from_slice(&HEADER.to_le_bytes()); // pFiles: offset of the file list
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.x
    out.extend_from_slice(&0i32.to_le_bytes()); // pt.y
    out.extend_from_slice(&0u32.to_le_bytes()); // fNC
    out.extend_from_slice(&1u32.to_le_bytes()); // fWide: UTF-16
    for p in paths {
        utf16z(&p.to_string_lossy(), &mut out);
    }
    out.extend_from_slice(&0u16.to_le_bytes()); // list terminator
    out
}

/// (from, to) pairs for moving `paths` into `dest`. Names never overwrite (`name 2.ext`); items already in
/// `dest` are skipped; a folder cannot be moved into itself.
pub fn plan_moves(paths: &[PathBuf], dest: &Path) -> Vec<(PathBuf, PathBuf)> {
    let mut plan = Vec::new();
    for p in paths {
        if p.parent() == Some(dest) {
            continue;
        }
        if p.is_dir() && dest.starts_with(p) {
            continue;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
        let (stem, ext) = if p.is_dir() {
            (name.clone(), String::new())
        } else {
            match name.rsplit_once('.') {
                Some((s, e)) if !s.is_empty() => (s.to_string(), e.to_string()),
                _ => (name.clone(), String::new()),
            }
        };
        let mut target = unique_path(dest, &stem, &ext);
        // two sources with the same name in one batch must not collide with each other either
        while plan.iter().any(|(_, t): &(PathBuf, PathBuf)| *t == target) {
            let n = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            target = unique_path(dest, &format!("{n} copy"), &ext);
        }
        plan.push((p.clone(), target));
    }
    plan
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.path().is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), &t)?;
        }
    }
    Ok(())
}

/// Rename, or copy + delete when the destination is on another drive.
pub fn move_item(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    if from.is_dir() {
        copy_dir(from, to)?;
        std::fs::remove_dir_all(from)
    } else {
        std::fs::copy(from, to)?;
        std::fs::remove_file(from)
    }
}

/// The folder a terminal should open in: the folder itself, or a file's parent.
pub fn terminal_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(|p| p.to_path_buf()).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from("."))
    }
}

/// (program, args) to open a terminal in `dir`: Windows Terminal when installed, else PowerShell.
pub fn terminal_command(dir: &Path, have_windows_terminal: bool) -> (String, Vec<String>) {
    let d = dir.to_string_lossy().into_owned();
    if have_windows_terminal {
        ("wt.exe".into(), vec!["-d".into(), d])
    } else {
        let quoted = d.replace('\'', "''");
        ("powershell.exe".into(), vec!["-NoExit".into(), "-Command".into(), format!("Set-Location -LiteralPath '{quoted}'")])
    }
}

/// Arguments for `explorer.exe` that open the folder with `path` selected.
pub fn reveal_args(path: &Path) -> Vec<String> {
    vec![format!("/select,{}", path.to_string_lossy())]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_text() {
        let t = copy_path_text(&[PathBuf::from(r"C:\a\b.png"), PathBuf::from(r"C:\c d\e.txt")]);
        assert_eq!(t, "C:\\a\\b.png\r\nC:\\c d\\e.txt");
        assert_eq!(copy_path_text(&[]), "");
    }

    #[test]
    fn dropfiles_layout() {
        let b = build_dropfiles(&[PathBuf::from("C:\\a.txt"), PathBuf::from("D:\\é.png")]);
        assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(b[16..20].try_into().unwrap()), 1, "fWide");
        let units: Vec<u16> = b[20..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        // "C:\a.txt\0" "D:\é.png\0" "\0"
        let s = String::from_utf16(&units[..units.len() - 1]).unwrap();
        assert_eq!(s, "C:\\a.txt\0D:\\é.png\0");
        assert_eq!(*units.last().unwrap(), 0);
        assert_eq!(units[units.len() - 2], 0, "double NUL terminator");
        assert_eq!(b.len() % 2, 0);
    }

    #[test]
    fn move_planning_never_overwrites() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("src");
        let dst = d.path().join("dst");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::write(src.join("a.txt"), "1").unwrap();
        std::fs::write(dst.join("a.txt"), "existing").unwrap();
        let plan = plan_moves(&[src.join("a.txt")], &dst);
        assert_eq!(plan[0].1.file_name().unwrap(), "a 2.txt");
        move_item(&plan[0].0, &plan[0].1).unwrap();
        assert!(!src.join("a.txt").exists());
        assert_eq!(std::fs::read_to_string(dst.join("a.txt")).unwrap(), "existing");
        assert_eq!(std::fs::read_to_string(dst.join("a 2.txt")).unwrap(), "1");
    }

    #[test]
    fn move_planning_edge_cases() {
        let d = tempfile::tempdir().unwrap();
        let (a, b) = (d.path().join("a"), d.path().join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.txt"), "1").unwrap();
        std::fs::write(b.join("x.txt"), "2").unwrap();
        // already in the destination: skipped
        assert!(plan_moves(&[a.join("x.txt")], &a).is_empty());
        // same name from two folders in one batch: distinct targets
        let dest = d.path().join("out");
        std::fs::create_dir_all(&dest).unwrap();
        let plan = plan_moves(&[a.join("x.txt"), b.join("x.txt")], &dest);
        assert_eq!(plan.len(), 2);
        assert_ne!(plan[0].1, plan[1].1);
        // a folder cannot move into itself
        assert!(plan_moves(&[a.clone()], &a.join("inner")).is_empty());
    }

    #[test]
    fn moves_folders() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("f");
        std::fs::create_dir_all(f.join("sub")).unwrap();
        std::fs::write(f.join("sub").join("n.txt"), "x").unwrap();
        let dest = d.path().join("dest");
        std::fs::create_dir_all(&dest).unwrap();
        let plan = plan_moves(&[f.clone()], &dest);
        move_item(&plan[0].0, &plan[0].1).unwrap();
        assert!(dest.join("f").join("sub").join("n.txt").is_file());
        assert!(!f.exists());
    }

    #[test]
    fn terminal() {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("a.txt");
        std::fs::write(&file, "x").unwrap();
        assert_eq!(terminal_dir(&file), d.path());
        assert_eq!(terminal_dir(d.path()), d.path());
        let (p, a) = terminal_command(Path::new(r"C:\Users\me\My Files"), true);
        assert_eq!((p.as_str(), a), ("wt.exe", vec!["-d".to_string(), r"C:\Users\me\My Files".to_string()]));
        let (p, a) = terminal_command(Path::new(r"C:\it's here"), false);
        assert_eq!(p, "powershell.exe");
        assert_eq!(a[2], r"Set-Location -LiteralPath 'C:\it''s here'");
    }

    #[test]
    fn reveal() {
        assert_eq!(reveal_args(Path::new(r"C:\x\y z.png")), vec![r"/select,C:\x\y z.png".to_string()]);
    }
}
