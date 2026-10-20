//! File inspection: image size (after EXIF orientation), media streams via ffprobe, PDF page count.

use crate::im;
use orange_core::kind::{self, Kind};
use orange_core::{exec, Capabilities, Ctx, Dep, Error, Result};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Default)]
pub struct Probe {
    pub duration: f64,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub has_audio: bool,
    pub has_video: bool,
}

pub fn probe(file: &Path, ctx: &Ctx) -> Result<Probe> {
    let ffprobe = ctx.caps.require(Dep::Ffprobe)?;
    let mut c = exec::command(ffprobe);
    c.args(["-v", "error", "-print_format", "json", "-show_streams", "-show_format"]).arg(im::abs(file));
    let out = exec::run(&mut c, "ffprobe", ctx)?;
    let v: Value = serde_json::from_str(&out.stdout)?;
    let mut p = Probe::default();
    p.duration = v["format"]["duration"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0.0);
    if let Some(streams) = v["streams"].as_array() {
        for s in streams {
            match s["codec_type"].as_str() {
                Some("video") if s["disposition"]["attached_pic"].as_i64() != Some(1) => {
                    if !p.has_video {
                        p.has_video = true;
                        p.width = s["width"].as_u64().map(|x| x as u32);
                        p.height = s["height"].as_u64().map(|x| x as u32);
                    }
                }
                Some("audio") => p.has_audio = true,
                _ => {}
            }
        }
    }
    Ok(p)
}

pub fn pdf_pages(path: &Path, ctx: &Ctx) -> Result<u32> {
    if let Ok(doc) = lopdf::Document::load(path) {
        let n = doc.get_pages().len();
        if n > 0 {
            return Ok(n as u32);
        }
    }
    let q = ctx.caps.require(Dep::Qpdf)?;
    let mut c = exec::command(q);
    c.arg("--warning-exit-0").arg("--show-npages").arg(im::abs(path));
    let out = exec::run(&mut c, "qpdf", ctx)?;
    out.stdout.trim().parse().map_err(|_| Error::msg("could not read the PDF page count"))
}

pub fn file_info(path: &Path, caps: &Capabilities) -> Result<Value> {
    let ctx = Ctx::new(Arc::new(caps.clone()));
    match kind::detect(path) {
        Kind::Image => {
            let (w, h) = im::oriented_dims(path, &ctx)?;
            Ok(json!({"kind":"image","width":w,"height":h,"format":im::format_name(path, &ctx),"size":std::fs::metadata(path)?.len()}))
        }
        k @ (Kind::Video | Kind::Audio) => {
            let p = probe(path, &ctx)?;
            let kind = if k == Kind::Audio || !p.has_video { "audio" } else { "video" };
            Ok(json!({"kind":kind,"duration":p.duration,"width":p.width,"height":p.height,"has_audio":p.has_audio,"size":std::fs::metadata(path)?.len()}))
        }
        Kind::Pdf => Ok(json!({"kind":"pdf","pages":pdf_pages(path, &ctx)?,"size":std::fs::metadata(path)?.len()})),
        k => Ok(json!({"kind": format!("{k:?}").to_lowercase(), "size": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)})),
    }
}
