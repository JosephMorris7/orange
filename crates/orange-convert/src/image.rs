//! Image conversions: ImageMagick for raster/SVG, a built-in PDF writer for PDF export.

use crate::pdfwrite::{self, PageImage};
use crate::util::*;
use orange_core::{exec, Ctx, Dep, Error, Result};
use std::path::{Path, PathBuf};

/// Input argument for ImageMagick: first frame only for multi-frame capable formats.
fn input_arg(input: &Path, from: &str) -> String {
    let p = s(input);
    let p = if p.starts_with('-') { format!("./{p}") } else { p };
    match from {
        "tiff" | "webp" | "heic" | "avif" => format!("{p}[0]"),
        _ => p,
    }
}

fn base_cmd(magick: &Path, input: &Path, from: &str) -> std::process::Command {
    let mut cmd = exec::command(magick);
    if from == "svg" {
        cmd.args(["-background", "none", "-density", "192"]);
    }
    cmd.arg(input_arg(input, from));
    cmd.arg("-auto-orient");
    cmd
}

pub(crate) fn convert(input: &Path, from: &str, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let magick = ctx.caps.require(Dep::Magick)?;
    let tmp = ctx.temp_dir()?;
    let out = tmp.path().join(format!("out.{to}"));
    ctx.report(0.1, "Converting image");
    let mut cmd = base_cmd(magick, input, from);
    match to {
        "jpg" => {
            cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off", "-quality", "92"]);
        }
        "bmp" => {
            cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
        }
        "webp" => {
            cmd.args(["-quality", "90"]);
        }
        "avif" => {
            cmd.args(["-quality", "60"]);
        }
        "heic" => {
            cmd.args(["-quality", "85"]);
        }
        "tiff" => {
            cmd.args(["-compress", "lzw"]);
        }
        _ => {}
    }
    cmd.arg(&out);
    let res = exec::run(&mut cmd, "ImageMagick", ctx);
    let ok = match res {
        // some ImageMagick builds exit 0 but write PNG data under the .avif/.heic name: verify the container
        Ok(_) => out.is_file() && (!matches!(to, "avif" | "heic") || is_isobmff(&out)),
        Err(Error::Cancelled) => return Err(Error::Cancelled),
        Err(e) => {
            if to == "avif" && ctx.caps.has(Dep::Ffmpeg) {
                false
            } else if to == "heic" {
                return Err(Error::ToolFailed {
                    tool: "ImageMagick".into(),
                    detail: format!("this ImageMagick build cannot write HEIC ({e})"),
                });
            } else {
                return Err(e);
            }
        }
    };
    if !ok && to == "heic" {
        let _ = std::fs::remove_file(&out);
        return Err(Error::ToolFailed { tool: "ImageMagick".into(), detail: "this ImageMagick build cannot write HEIC".into() });
    }
    if !ok {
        // ImageMagick without an AV1 encoder: let ffmpeg write the AVIF.
        let _ = std::fs::remove_file(&out);
        let ffmpeg = ctx.caps.require(Dep::Ffmpeg)?;
        let args: Vec<String> = [
            "-i",
            &s(input),
            "-frames:v",
            "1",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p",
            "-c:v",
            "libaom-av1",
            "-crf",
            "28",
            "-b:v",
            "0",
            "-cpu-used",
            "6",
            "-still-picture",
            "1",
            &s(&out),
        ]
        .iter()
        .map(|x| x.to_string())
        .collect();
        exec::run_ffmpeg(ffmpeg, &args, None, ctx, 0.1, 0.8, "Converting image")?;
    }
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

/// Image -> PDF (one page), using the built-in writer.
pub(crate) fn to_pdf(input: &Path, from: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    ctx.report(0.1, "Building PDF");
    let tmp = ctx.temp_dir()?;
    let mut page: Option<PageImage> = None;
    if from == "jpg" {
        let data = std::fs::read(input)?;
        if let Some((w, h, comps)) = pdfwrite::jpeg_info(&data) {
            let upright = matches!(pdfwrite::jpeg_orientation(&data), None | Some(1));
            if upright && (comps == 1 || comps == 3) {
                page = Some(PageImage::Jpeg { w, h, comps, data });
            }
        }
    }
    let page = match page {
        Some(p) => p,
        None => {
            let magick = ctx.caps.require(Dep::Magick)?;
            let ppm = tmp.path().join("page.ppm");
            let mut cmd = base_cmd(magick, input, from);
            cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off", "-depth", "8"]);
            cmd.arg(format!("ppm:{}", s(&ppm)));
            exec::run(&mut cmd, "ImageMagick", ctx)?;
            ctx.check_cancel()?;
            let bytes = std::fs::read(&ppm)?;
            let (w, h, data) = pdfwrite::parse_ppm(&bytes).ok_or_else(|| Error::msg("could not decode the image"))?;
            PageImage::Rgb { w, h, data }
        }
    };
    let out = tmp.path().join("out.pdf");
    std::fs::write(&out, pdfwrite::build(&[page]))?;
    Ok(vec![finish_file(&out, input, "pdf", ctx)?])
}

/// True when the file starts like an ISO base media file (`....ftyp`), which AVIF and HEIC both are.
fn is_isobmff(path: &Path) -> bool {
    use std::io::Read;
    let mut b = [0u8; 12];
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut b)).is_ok() && &b[4..8] == b"ftyp"
}
