//! ImageMagick helpers. All pixel tools go through here so final renders and previews are identical.
//! Works with ImageMagick 7 (`magick ...`) and 6 (`convert ...`): only options common to both are used.

use crate::util::s;
use orange_core::{exec, Ctx, Dep, Error, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub fn abs(p: &Path) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Input argument: absolute path, first frame only. The `[0]` also stops names that contain
/// `[...]` from being parsed as a frame/crop spec.
pub fn in_arg(p: &Path) -> String {
    format!("{}[0]", s(&abs(p)))
}

/// Output argument with an explicit coder so the extension never decides the format.
pub fn out_arg(coder: &str, p: &Path) -> String {
    format!("{coder}:{}", s(&abs(p)))
}

pub fn magick(ctx: &Ctx) -> Result<Command> {
    let p = ctx.caps.require(Dep::Magick)?;
    Ok(exec::command(p))
}

/// Run ImageMagick with `args` (everything after the program name).
pub fn run(args: &[String], ctx: &Ctx) -> Result<()> {
    let mut c = magick(ctx)?;
    c.args(args);
    exec::run(&mut c, "ImageMagick", ctx).map(|_| ())
}

/// (width, height) after EXIF auto-orient.
pub fn oriented_dims(p: &Path, ctx: &Ctx) -> Result<(u32, u32)> {
    let mut c = magick(ctx)?;
    c.arg(in_arg(p)).args(["-auto-orient", "-format", "%w %h", "info:"]);
    let out = exec::run(&mut c, "ImageMagick", ctx)?;
    let mut it = out.stdout.split_whitespace();
    let w = it.next().and_then(|v| v.parse().ok());
    let h = it.next().and_then(|v| v.parse().ok());
    match (w, h) {
        (Some(w), Some(h)) if w > 0 && h > 0 => Ok((w, h)),
        _ => Err(Error::msg(format!("could not read image size: {}", p.display()))),
    }
}

/// Original format name (e.g. "JPEG") via identify.
pub fn format_name(p: &Path, ctx: &Ctx) -> Option<String> {
    let mut c = magick(ctx).ok()?;
    c.arg(in_arg(p)).args(["-format", "%m", "info:"]);
    exec::run(&mut c, "ImageMagick", ctx).ok().map(|o| o.stdout.trim().to_string())
}

static FONTS: OnceLock<(Option<String>, Option<String>)> = OnceLock::new();

/// (regular, bold) font names that exist for ImageMagick on this machine; `None` = use IM's default.
pub fn fonts(ctx: &Ctx) -> (Option<String>, Option<String>) {
    FONTS
        .get_or_init(|| {
            let mut c = match magick(ctx) {
                Ok(c) => c,
                Err(_) => return (None, None),
            };
            c.args(["-list", "font"]);
            let out = match exec::run(&mut c, "ImageMagick", ctx) {
                Ok(o) => o.stdout,
                Err(_) => return (None, None),
            };
            let have: Vec<&str> = out.lines().filter_map(|l| l.trim().strip_prefix("Font:")).map(|f| f.trim()).collect();
            let pick = |cands: &[&str]| cands.iter().find(|c| have.contains(c)).map(|c| c.to_string());
            (
                pick(&["Segoe-UI", "Segoe-UI-Regular", "Arial", "Arial-Regular", "DejaVu-Sans", "Carlito-Regular", "Liberation-Sans", "Helvetica"]),
                pick(&["Segoe-UI-Bold", "Arial-Bold", "DejaVu-Sans-Bold", "Carlito-Bold", "Liberation-Sans-Bold", "Helvetica-Bold"]),
            )
        })
        .clone()
}

/// Make text safe for `label:` / `-annotate`: `%` escapes are expanded by IM and a leading `@` reads a file.
pub fn safe_text(t: &str) -> String {
    let t = t.replace('%', "%%");
    if t.starts_with('@') {
        format!("\\{t}")
    } else {
        t
    }
}

/// Args that create a `w`x`h` background image from a background spec (see CONTRACT "add-background").
pub fn background_args(bg: &serde_json::Value, w: u32, h: u32) -> Result<Vec<String>> {
    use crate::params::{f64_or, str_or, valid_color};
    let kind = str_or(bg, "type", "solid");
    let size = format!("{w}x{h}");
    match kind {
        "solid" => {
            let c = valid_color(str_or(bg, "color", "#ffffff"))?;
            Ok(vec!["-size".into(), size, format!("xc:{c}")])
        }
        "gradient" => {
            let from = valid_color(str_or(bg, "from", "#ff9a3c"))?;
            let to = valid_color(str_or(bg, "to", "#ff3c8e"))?;
            let angle = f64_or(bg, "angle", 135.0);
            Ok(vec!["-define".into(), format!("gradient:angle={}", angle.round() as i64), "-size".into(), size.clone(), format!("gradient:{from}-{to}")])
        }
        "image" => {
            let path = str_or(bg, "path", "");
            if path.is_empty() || !Path::new(path).is_file() {
                return Err(Error::BadParams("background image not found".into()));
            }
            Ok(vec![in_arg(Path::new(path)), "-auto-orient".into(), "-resize".into(), format!("{size}^"), "-gravity".into(), "center".into(), "-extent".into(), size])
        }
        other => Err(Error::BadParams(format!("unknown background type: {other}"))),
    }
}
