//! orange-tools: the 26 file tools. STUB: the `tools` workstream fills this in.
//! Signatures and the `ToolInfo` / `ParamSpec` shapes are the contract (see docs/CONTRACT.md).

use orange_core::{Capabilities, Ctx, Dep, Error, Kind, Result};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub mod cli;
mod archive;
mod audio;
mod im;
mod image_tools;
mod info;
mod meta;
mod params;
mod pdf;
mod pdfwrite;
mod registry;
mod subtitle;
mod util;
mod video;

/// Which dialog the frontend should show for a tool.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ToolUi {
    /// Run immediately with default params (no dialog).
    None,
    /// Generic form generated from `params`.
    Form,
    Crop,
    EditPhoto,
    Annotate,
    AddBackground,
    Redact,
    Collage,
    Metadata,
    /// Media trim (start/end on a timeline).
    Trim,
    /// Reorderable list of the selected files (merge tools, create-pdf).
    Reorder,
    /// PDF page chooser (split / rotate).
    Pages,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParamKind {
    Number { min: f64, max: f64, step: f64, default: f64, unit: Option<String> },
    Select { options: Vec<(String, String)>, default: String },
    Toggle { default: bool },
    Text { default: String, placeholder: Option<String> },
    Color { default: String },
}

#[derive(Serialize, Clone, Debug)]
pub struct ParamSpec {
    pub key: String,
    pub label: String,
    #[serde(flatten)]
    pub kind: ParamKind,
    pub help: Option<String>,
    /// Only show this field while another field has one of these values.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_when: Option<ShowWhen>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ShowWhen {
    pub key: String,
    pub values: Vec<String>,
}

#[derive(Serialize, Clone, Debug)]
pub struct ToolInfo {
    /// stable id, kebab-case: "compress-image"
    pub id: String,
    /// short wheel label: "COMPRESS"
    pub label: String,
    /// icon name understood by the frontend icon set (see CONTRACT.md)
    pub icon: String,
    pub title: String,
    pub description: String,
    /// every selected file must be one of these kinds
    pub kinds: Vec<Kind>,
    pub min_files: usize,
    pub max_files: Option<usize>,
    pub ui: ToolUi,
    pub params: Vec<ParamSpec>,
    /// dependency required (tool hidden/greyed when missing)
    pub deps: Vec<Dep>,
}

/// Every tool Orange ships. Exactly 26 entries.
pub fn all_tools() -> Vec<ToolInfo> {
    registry::all()
}

/// Tools applicable to this selection: every file must be one of the tool's kinds
/// (folders only where `Kind::Folder` is listed) and the count within min/max.
/// Availability of dependencies is left to the caller (see `ToolInfo::deps`).
pub fn tools_for(paths: &[PathBuf], _caps: &Capabilities) -> Vec<ToolInfo> {
    if paths.is_empty() {
        return Vec::new();
    }
    let kinds: Vec<Kind> = paths.iter().map(|p| orange_core::kind::detect(p)).collect();
    registry::all()
        .into_iter()
        .filter(|t| {
            paths.len() >= t.min_files
                && t.max_files.map_or(true, |m| paths.len() <= m)
                && kinds.iter().all(|k| t.kinds.contains(k))
        })
        .collect()
}

/// Run a tool. `params` follows the per-tool JSON schema in docs/CONTRACT.md.
/// Returns every file created (never overwrites inputs).
pub fn run_tool(id: &str, inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let info = registry::all().into_iter().find(|t| t.id == id).ok_or_else(|| Error::Unsupported(format!("unknown tool: {id}")))?;
    if inputs.len() < info.min_files {
        return Err(Error::BadParams(format!("{} needs at least {} file(s)", info.title, info.min_files)));
    }
    if let Some(m) = info.max_files {
        if inputs.len() > m {
            return Err(Error::BadParams(format!("{} accepts at most {} file(s)", info.title, m)));
        }
    }
    for d in &info.deps {
        ctx.caps.require(*d)?;
    }
    for p in inputs {
        if !p.exists() {
            return Err(Error::BadParams(format!("file not found: {}", p.display())));
        }
    }
    ctx.check_cancel()?;
    match id {
        "compress-image" | "edit-photo" | "annotate-photo" | "add-background" | "crop-image" | "redact-photo" | "resize-image" => {
            image_tools::run_each(id, inputs, params, ctx)
        }
        "create-pdf" => image_tools::create_pdf(inputs, params, ctx),
        "collage" => image_tools::collage(inputs, params, ctx),
        "edit-metadata" => meta::edit(inputs, params, ctx),
        "compress-video" | "trim-video" | "remove-audio" | "resize-video" => video::run_each(id, inputs, params, ctx),
        "merge-video" => video::merge(inputs, params, ctx),
        "compress-audio" | "trim-audio" | "normalize-audio" => audio::run_each(id, inputs, params, ctx),
        "merge-audio" => audio::merge(inputs, params, ctx),
        "compress-pdf" | "split-pdf" | "rotate-pdf" | "protect-pdf" => pdf::run_each(id, inputs, params, ctx),
        "merge-pdf" => pdf::merge(inputs, params, ctx),
        "shift-subtitles" => subtitle::run_each(inputs, params, ctx),
        "create-archive" => archive::create(inputs, params, ctx),
        other => Err(Error::Unsupported(format!("unknown tool: {other}"))),
    }
}

/// Render a downscaled (max `max_dim` px on the long edge) PNG preview of an *image* tool applied to
/// `input` into a temp file the caller owns. Used for live previews in tool dialogs.
pub fn preview(id: &str, input: &Path, params: &Value, ctx: &Ctx, max_dim: u32) -> Result<PathBuf> {
    image_tools::preview(id, input, params, ctx, max_dim)
}

/// Convert any image (HEIC, TIFF, AVIF, ...) to a browser-displayable PNG/JPEG of at most `max_dim` px
/// (EXIF orientation applied); returns the temp file.
pub fn web_preview(input: &Path, ctx: &Ctx, max_dim: u32) -> Result<PathBuf> {
    image_tools::web_preview(input, ctx, max_dim)
}

/// Dimensions + basic info for images and media (ffprobe) as JSON:
/// image: {"kind":"image","width":..,"height":..,"format":"JPEG"}   (width/height are after EXIF auto-orient)
/// media: {"kind":"video"|"audio","duration":secs,"width":..,"height":..,"has_audio":bool}
/// pdf:   {"kind":"pdf","pages":n}
pub fn file_info(path: &Path, caps: &Capabilities) -> Result<Value> {
    info::file_info(path, caps)
}

/// Read editable metadata (see CONTRACT.md "metadata").
pub fn read_metadata(path: &Path, caps: &Capabilities) -> Result<Value> {
    meta::read(path, caps)
}
