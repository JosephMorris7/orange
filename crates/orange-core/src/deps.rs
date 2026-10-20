//! Locating the external tools Orange drives. Orange bundles portable builds
//! next to the executable (`bin/`), falls back to PATH, and (for LibreOffice /
//! 7-Zip) to well-known Windows install locations. Anything missing simply
//! disables the wheel entries that need it.

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Dep {
    Ffmpeg,
    Ffprobe,
    Magick,
    Pandoc,
    Soffice,
    Pdftoppm,
    SevenZip,
    Exiftool,
    Qpdf,
}

impl Dep {
    pub const ALL: [Dep; 9] = [
        Dep::Ffmpeg,
        Dep::Ffprobe,
        Dep::Magick,
        Dep::Pandoc,
        Dep::Soffice,
        Dep::Pdftoppm,
        Dep::SevenZip,
        Dep::Exiftool,
        Dep::Qpdf,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Dep::Ffmpeg => "ffmpeg",
            Dep::Ffprobe => "ffprobe",
            Dep::Magick => "ImageMagick",
            Dep::Pandoc => "pandoc",
            Dep::Soffice => "LibreOffice",
            Dep::Pdftoppm => "poppler (pdftoppm)",
            Dep::SevenZip => "7-Zip",
            Dep::Exiftool => "ExifTool",
            Dep::Qpdf => "qpdf",
        }
    }

    /// Executable base names to try, in order (without `.exe`).
    fn candidates(self) -> &'static [&'static str] {
        match self {
            Dep::Ffmpeg => &["ffmpeg"],
            Dep::Ffprobe => &["ffprobe"],
            // `magick` = ImageMagick 7. `convert` = ImageMagick 6 (never used on Windows:
            // that name is the system FAT->NTFS converter there).
            Dep::Magick => {
                if cfg!(windows) {
                    &["magick"]
                } else {
                    &["magick", "convert"]
                }
            }
            Dep::Pandoc => &["pandoc"],
            Dep::Soffice => &["soffice", "libreoffice"],
            Dep::Pdftoppm => &["pdftoppm"],
            Dep::SevenZip => &["7z", "7zz", "7za"],
            Dep::Exiftool => &["exiftool", "exiftool(-k)"],
            Dep::Qpdf => &["qpdf"],
        }
    }

    /// Extra absolute locations to probe (Windows installs).
    fn known_locations(self) -> Vec<PathBuf> {
        let mut v = Vec::new();
        if cfg!(windows) {
            let pf = std::env::var_os("ProgramFiles").map(PathBuf::from);
            let pf86 = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from);
            for base in [pf, pf86].into_iter().flatten() {
                match self {
                    Dep::Soffice => v.push(base.join("LibreOffice").join("program").join("soffice.exe")),
                    Dep::SevenZip => v.push(base.join("7-Zip").join("7z.exe")),
                    Dep::Pandoc => v.push(base.join("Pandoc").join("pandoc.exe")),
                    _ => {}
                }
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
                if self == Dep::Pandoc {
                    v.push(local.join("Pandoc").join("pandoc.exe"));
                }
            }
        }
        v
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn find_in_dir(dir: &Path, dep: Dep) -> Option<PathBuf> {
    for c in dep.candidates() {
        let p = dir.join(exe_name(c));
        if p.is_file() {
            return Some(p);
        }
        // bundled layouts like bin/ffmpeg/ffmpeg.exe or bin/7zip/7z.exe
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let sub = e.path();
                if sub.is_dir() {
                    for cand in [sub.join(exe_name(c)), sub.join("bin").join(exe_name(c)), sub.join("program").join(exe_name(c))] {
                        if cand.is_file() {
                            return Some(cand);
                        }
                    }
                }
            }
        }
    }
    None
}

fn find_in_path(dep: Dep) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for c in dep.candidates() {
            let p = dir.join(exe_name(c));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[derive(Clone, Debug, Default)]
pub struct Capabilities {
    paths: HashMap<Dep, PathBuf>,
}

impl Capabilities {
    /// Detect all dependencies. `extra_dirs` are searched first (bundled `bin/`).
    /// The env var `ORANGE_BIN_DIR` (a path list) is searched after `extra_dirs`.
    pub fn detect(extra_dirs: &[PathBuf]) -> Self {
        let mut dirs: Vec<PathBuf> = extra_dirs.to_vec();
        if let Some(v) = std::env::var_os("ORANGE_BIN_DIR") {
            dirs.extend(std::env::split_paths(&v));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(d) = exe.parent() {
                dirs.push(d.join("bin"));
                dirs.push(d.join("resources").join("bin"));
            }
        }
        let mut paths = HashMap::new();
        for dep in Dep::ALL {
            let found = dirs
                .iter()
                .find_map(|d| find_in_dir(d, dep))
                .or_else(|| find_in_path(dep))
                .or_else(|| dep.known_locations().into_iter().find(|p| p.is_file()));
            if let Some(p) = found {
                paths.insert(dep, p);
            }
        }
        Capabilities { paths }
    }

    /// For tests: construct from explicit paths.
    pub fn from_paths(paths: HashMap<Dep, PathBuf>) -> Self {
        Capabilities { paths }
    }

    pub fn get(&self, dep: Dep) -> Option<&Path> {
        self.paths.get(&dep).map(|p| p.as_path())
    }
    pub fn has(&self, dep: Dep) -> bool {
        self.paths.contains_key(&dep)
    }
    pub fn require(&self, dep: Dep) -> Result<&Path> {
        self.get(dep).ok_or_else(|| Error::MissingDep(dep.name().to_string()))
    }

    /// Command prefix for ImageMagick `identify`, working for both IM7 and IM6.
    /// Returns (program, leading args).
    pub fn magick_identify(&self) -> Result<(PathBuf, Vec<String>)> {
        let m = self.require(Dep::Magick)?;
        let stem = m.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem == "magick" {
            Ok((m.to_path_buf(), vec!["identify".into()]))
        } else {
            let sib = m.with_file_name(exe_name("identify"));
            if sib.is_file() {
                Ok((sib, vec![]))
            } else {
                Err(Error::MissingDep("ImageMagick identify".into()))
            }
        }
    }

    /// Serializable summary: dep id -> path or null.
    pub fn to_json(&self) -> serde_json::Value {
        let mut m = serde_json::Map::new();
        for dep in Dep::ALL {
            let key = serde_json::to_value(dep).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
            m.insert(
                key,
                match self.get(dep) {
                    Some(p) => serde_json::json!({ "name": dep.name(), "path": p.to_string_lossy(), "found": true }),
                    None => serde_json::json!({ "name": dep.name(), "path": null, "found": false }),
                },
            );
        }
        serde_json::Value::Object(m)
    }
}
