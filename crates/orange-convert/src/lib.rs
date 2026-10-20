//! orange-convert: the format-conversion engine.
//!
//! A data-driven registry of (source ext -> target ext) pairs. Each pair maps to a `Route`
//! (which converter runs) and the external tools it needs; the wheel entries, availability
//! notes, `option_count()` and `convert()` are all derived from that one table.

use orange_core::kind::ext_of;
use orange_core::{Capabilities, Ctx, Dep, Error, Kind, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

mod archive;
mod image;
mod media;
mod office;
mod pdf;
mod pdfwrite;
mod subtitle;
mod util;

pub mod cli;

/// One entry on the "convert" wheel.
#[derive(Serialize, Clone, Debug)]
pub struct FormatOption {
    /// target extension, lowercase: "png", "docx", "mp3"
    pub id: String,
    /// display label: "PNG", "DOCX", "MP3"
    pub label: String,
    /// external dependency needed, if any
    pub dep: Option<Dep>,
    /// false when `dep` is not installed (wheel shows it greyed with `note`)
    pub available: bool,
    /// e.g. "Install LibreOffice to enable"
    pub note: Option<String>,
}

// ------------------------------------------------------------------ registry

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    Image,
    ImageToPdf,
    Audio,
    Video,
    Pdf,
    Document,
    Slides,
    Sheet,
    Subtitle,
    Archive,
}

/// A family of sources that share a target list (targets listed most useful first).
struct Group {
    kind: Kind,
    sources: &'static [&'static str],
    targets: &'static [&'static str],
}

const GROUPS: &[Group] = &[
    Group {
        kind: Kind::Image,
        sources: &["jpg", "png", "webp", "heic", "tiff", "avif", "bmp", "svg"],
        targets: &["jpg", "png", "webp", "pdf", "tiff", "bmp", "avif", "heic"],
    },
    Group {
        kind: Kind::Audio,
        sources: &["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff", "wma"],
        targets: &["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff"],
    },
    Group {
        kind: Kind::Video,
        sources: &["mp4", "mov", "mkv", "webm", "avi", "wmv", "gif"],
        targets: &["mp4", "gif", "mp3", "webm", "mov", "mkv", "avi", "wmv"],
    },
    Group { kind: Kind::Pdf, sources: &["pdf"], targets: &["docx", "jpg", "png", "txt", "md", "pptx", "xlsx", "epub"] },
    Group {
        kind: Kind::Document,
        sources: &["docx", "md", "txt", "epub", "rtf", "html", "doc", "odt"],
        targets: &["pdf", "docx", "md", "txt", "html", "epub"],
    },
    Group { kind: Kind::Slides, sources: &["pptx", "ppt"], targets: &["pdf", "jpg", "png"] },
    Group {
        kind: Kind::Spreadsheet,
        sources: &["xlsx", "ods", "csv", "tsv", "xls"],
        targets: &["xlsx", "csv", "pdf", "ods", "tsv"],
    },
    Group { kind: Kind::Subtitle, sources: &["srt", "vtt"], targets: &["srt", "vtt", "txt"] },
    Group { kind: Kind::Archive, sources: &["zip", "tar", "gz", "rar", "7z"], targets: &["zip", "tar", "gz"] },
];

/// Canonical extension ("jpeg" -> "jpg", "tif" -> "tiff", ...).
fn normalize(ext: &str) -> String {
    let e = ext.trim().trim_start_matches('.').to_ascii_lowercase();
    match e.as_str() {
        "jpeg" | "jpe" => "jpg",
        "tif" => "tiff",
        "heif" => "heic",
        "htm" => "html",
        "markdown" => "md",
        "tgz" => "gz",
        "aif" => "aiff",
        "oga" => "ogg",
        _ => return e,
    }
    .to_string()
}

/// Which converter handles `from -> to` within a group (None = pair not offered).
fn route(g: &Group, from: &str, to: &str) -> Option<Route> {
    if from == to || !g.sources.contains(&from) || !g.targets.contains(&to) {
        return None;
    }
    Some(match g.kind {
        Kind::Image if to == "pdf" => Route::ImageToPdf,
        Kind::Image => Route::Image,
        Kind::Audio => Route::Audio,
        Kind::Video if to == "mp3" => {
            if from == "gif" {
                return None;
            }
            Route::Audio
        }
        Kind::Video => Route::Video,
        Kind::Pdf => Route::Pdf,
        Kind::Document => Route::Document,
        Kind::Slides => Route::Slides,
        Kind::Spreadsheet => Route::Sheet,
        Kind::Subtitle => Route::Subtitle,
        Kind::Archive => Route::Archive,
        _ => return None,
    })
}

/// External tools a pair needs (empty = pure Rust).
fn deps_of(r: Route, from: &str, to: &str) -> Vec<Dep> {
    match r {
        Route::Image => vec![Dep::Magick],
        Route::ImageToPdf => {
            if from == "jpg" {
                vec![]
            } else {
                vec![Dep::Magick]
            }
        }
        Route::Audio | Route::Video => vec![Dep::Ffmpeg],
        Route::Pdf => match to {
            "jpg" | "png" | "txt" | "md" => vec![Dep::Pdftoppm],
            "epub" => vec![Dep::Pdftoppm, Dep::Pandoc],
            "xlsx" => vec![Dep::Pdftoppm, Dep::Soffice],
            _ => vec![Dep::Soffice],
        },
        Route::Document => {
            let needs_lo = matches!(from, "rtf" | "doc") || to == "pdf";
            let needs_pandoc = if to == "pdf" { matches!(from, "md" | "epub") } else { !(matches!(from, "rtf" | "doc") && to == "docx") };
            let mut v = Vec::new();
            if needs_pandoc {
                v.push(Dep::Pandoc);
            }
            if needs_lo {
                v.push(Dep::Soffice);
            }
            v
        }
        Route::Slides => {
            if to == "pdf" {
                vec![Dep::Soffice]
            } else {
                vec![Dep::Soffice, Dep::Pdftoppm]
            }
        }
        Route::Sheet => {
            if matches!((from, to), ("csv", "tsv") | ("tsv", "csv")) {
                vec![]
            } else {
                vec![Dep::Soffice]
            }
        }
        Route::Subtitle => vec![],
        Route::Archive => vec![Dep::SevenZip],
    }
}

struct Pair {
    from: &'static str,
    to: &'static str,
    route: Route,
    deps: Vec<Dep>,
}

fn registry() -> &'static Vec<Pair> {
    static REG: OnceLock<Vec<Pair>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut v = Vec::new();
        for g in GROUPS {
            for &from in g.sources {
                for &to in g.targets {
                    if let Some(r) = route(g, from, to) {
                        v.push(Pair { from, to, route: r, deps: deps_of(r, from, to) });
                    }
                }
            }
        }
        v
    })
}

fn find_pair(from: &str, to: &str) -> Option<&'static Pair> {
    registry().iter().find(|p| p.from == from && p.to == to)
}

// ------------------------------------------------------------------ public API

/// All target formats for this file, most useful first. Never includes the file's own format.
pub fn options_for(path: &Path, caps: &Capabilities) -> Vec<FormatOption> {
    if path.is_dir() {
        return Vec::new();
    }
    let from = normalize(&ext_of(path));
    let mut pairs: Vec<&Pair> = registry().iter().filter(|p| p.from == from).collect();
    // each group lists its targets most useful first
    pairs.sort_by_key(|p| {
        GROUPS.iter().find(|g| g.sources.contains(&p.from)).and_then(|g| g.targets.iter().position(|t| *t == p.to)).unwrap_or(usize::MAX)
    });
    pairs
        .into_iter()
        .map(|p| {
            let missing = p.deps.iter().find(|d| !caps.has(**d)).copied();
            let dep = missing.or_else(|| p.deps.first().copied());
            FormatOption {
                id: p.to.to_string(),
                label: p.to.to_ascii_uppercase(),
                dep,
                available: missing.is_none(),
                note: missing.map(|d| format!("Install {} to enable", d.name())),
            }
        })
        .collect()
}

/// Total number of distinct (source format -> target format) pairs in the registry.
pub fn option_count() -> usize {
    registry().len()
}

/// Convert `input` to format `to`. Multi-output conversions (e.g. PDF -> PNG, one file per
/// page) return every file created. Outputs are never overwritten (see `orange_core::paths`).
pub fn convert(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let from = normalize(&ext_of(input));
    let to = normalize(to);
    let pair = find_pair(&from, &to).ok_or_else(|| Error::Unsupported(format!("cannot convert .{from} to .{to}")))?;
    if !input.is_file() {
        return Err(Error::msg(format!("input file not found: {}", input.display())));
    }
    for d in &pair.deps {
        ctx.caps.require(*d)?;
    }
    ctx.check_cancel()?;
    ctx.report(0.0, format!("Converting to {}", to.to_ascii_uppercase()));
    let outs = match pair.route {
        Route::Image => image::convert(input, &from, &to, ctx),
        Route::ImageToPdf => image::to_pdf(input, &from, ctx),
        Route::Audio => media::audio(input, &to, ctx),
        Route::Video => media::video(input, &to, ctx),
        Route::Pdf => pdf::convert(input, &to, ctx),
        Route::Document => office::document(input, &from, &to, ctx),
        Route::Slides => office::slides(input, &to, ctx),
        Route::Sheet => office::sheet(input, &from, &to, ctx),
        Route::Subtitle => subtitle::convert(input, &to, ctx),
        Route::Archive => archive::convert(input, &from, &to, ctx),
    }?;
    ctx.report(1.0, "Done");
    Ok(outs)
}

/// Kinds this crate can convert (for docs/UI).
pub fn supported_kinds() -> Vec<Kind> {
    GROUPS.iter().map(|g| g.kind).collect()
}
