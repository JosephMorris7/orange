//! Image tools (ImageMagick): compress, edit, annotate, background, crop, redact, resize,
//! collage and create-PDF, plus live previews. Final renders and previews share one code path.

use crate::im::{self, abs, in_arg, out_arg};
use crate::params::{bool_or, color, f64_or, i64_or, str_or, valid_color};
use crate::pdfwrite::{self, Layout, PageImage};
use crate::util::{finish_named, s};
use orange_core::kind::ext_of;
use orange_core::paths::stem_of;
use orange_core::{Ctx, Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

type Args = Vec<String>;

fn a<const N: usize>(items: [&str; N]) -> Args {
    items.iter().map(|x| x.to_string()).collect()
}

/// (extension, IM coder, quality args) for writing an edited copy of `p` in its own format.
fn keep_format(p: &Path) -> (&'static str, &'static str, Args) {
    match ext_of(p).as_str() {
        "png" => ("png", "png", vec![]),
        "webp" => ("webp", "webp", a(["-quality", "95"])),
        "tif" | "tiff" => ("tiff", "tiff", a(["-compress", "zip"])),
        "bmp" => ("bmp", "bmp", vec![]),
        "svg" => ("png", "png", vec![]),
        _ => ("jpg", "jpeg", a(["-quality", "95"])),
    }
}

fn suffix(id: &str) -> &'static str {
    match id {
        "compress-image" => "Compressed",
        "edit-photo" => "Edited",
        "annotate-photo" => "Annotated",
        "add-background" => "With Background",
        "crop-image" => "Cropped",
        "redact-photo" => "Redacted",
        "resize-image" => "Resized",
        _ => "Edited",
    }
}

pub(crate) fn run_each(id: &str, inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let mut outs = Vec::new();
    let n = inputs.len() as f32;
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / n, format!("{} ({}/{})", suffix(id), i + 1, inputs.len()));
        let td = ctx.temp_dir()?;
        let (tmp, ext) = match id {
            "compress-image" => compress(input, params, td.path(), ctx)?,
            "add-background" => {
                let out = td.path().join("out.png");
                add_background(input, params, &out, ctx)?;
                (out, "png")
            }
            _ => {
                let (ext, coder, q) = keep_format(input);
                let out = td.path().join(format!("out.{ext}"));
                let (w, h) = im::oriented_dims(input, ctx)?;
                let mut args = vec![in_arg(input), "-auto-orient".into()];
                args.extend(ops(id, params, w, h, ctx)?);
                args.extend(q);
                args.push(out_arg(coder, &out));
                im::run(&args, ctx)?;
                (out, ext)
            }
        };
        let name = format!("{} {}", stem_of(input), suffix(id));
        outs.push(finish_named(&tmp, input, &name, ext, ctx)?);
    }
    ctx.report(1.0, "Done");
    Ok(outs)
}

/// Operations between `-auto-orient` and the output for the single-invocation tools.
fn ops(id: &str, p: &Value, w: u32, h: u32, ctx: &Ctx) -> Result<Args> {
    match id {
        "edit-photo" => edit_ops(p, w, h),
        "crop-image" => crop_ops(p, w, h),
        "redact-photo" => redact_ops(p, w, h),
        "annotate-photo" => annotate_ops(p, ctx),
        "resize-image" => resize_ops(p),
        other => Err(Error::Unsupported(format!("not an image tool: {other}"))),
    }
}

// ---------------------------------------------------------------- edit-photo

fn edit_ops(p: &Value, w: u32, h: u32) -> Result<Args> {
    let g = |k: &str| f64_or(p, k, 0.0).clamp(-100.0, 100.0);
    let pos = |k: &str| f64_or(p, k, 0.0).clamp(0.0, 100.0);
    let mut o: Args = Vec::new();
    let rot = i64_or(p, "rotate", 0).rem_euclid(360);
    if ![0, 90, 180, 270].contains(&rot) {
        return Err(Error::BadParams("rotate must be 0, 90, 180 or 270".into()));
    }
    if rot != 0 {
        o.extend(a(["-rotate", &rot.to_string()]));
    }
    if bool_or(p, "flip_h", false) {
        o.push("-flop".into());
    }
    if bool_or(p, "flip_v", false) {
        o.push("-flip".into());
    }
    let (ex, br, ct, sat, warm) = (g("exposure"), g("brightness"), g("contrast"), g("saturation"), g("warmth"));
    if ex != 0.0 {
        o.extend(["-evaluate".into(), "Multiply".into(), format!("{:.4}", 2f64.powf(ex / 50.0))]);
    }
    if br != 0.0 || ct != 0.0 {
        o.extend(["-brightness-contrast".into(), format!("{}x{}", br as i64, ct as i64)]);
    }
    if sat != 0.0 {
        o.extend(["-modulate".into(), format!("100,{},100", (100.0 + sat) as i64)]);
    }
    if warm != 0.0 {
        let (c, amt) = if warm > 0.0 { ("#ff8c1a", warm) } else { ("#1a7bff", -warm) };
        o.extend(["-fill".into(), c.into(), "-colorize".into(), format!("{:.1}%", amt * 0.15)]);
    }
    match str_or(p, "filter", "none") {
        "none" | "" => {}
        "mono" => o.extend(a(["-colorspace", "Gray"])),
        "sepia" => o.extend(a(["-sepia-tone", "80%"])),
        "vivid" => o.extend(a(["-modulate", "100,135,100", "-sigmoidal-contrast", "3,50%"])),
        "fade" => o.extend(a(["-brightness-contrast", "10x-18", "-modulate", "100,80,100"])),
        "noir" => o.extend(a(["-colorspace", "Gray", "-sigmoidal-contrast", "7,50%"])),
        other => return Err(Error::BadParams(format!("unknown filter: {other}"))),
    }
    let (sharp, blur, vig) = (pos("sharpness"), pos("blur"), pos("vignette"));
    if sharp > 0.0 {
        o.extend(["-unsharp".into(), format!("0x1.2+{:.2}+0.02", sharp / 40.0)]);
    }
    if blur > 0.0 {
        o.extend(["-blur".into(), format!("0x{:.2}", blur / 100.0 * 0.02 * w.max(h) as f64)]);
    }
    if vig > 0.0 {
        // Multiply by a radial gradient: white in the centre fading to grey in the corners.
        let g = (100.0 - 0.9 * vig).round().max(5.0) as i64;
        o.extend(["(".into(), "-size".into(), format!("{w}x{h}"), format!("radial-gradient:white-gray{g}"), ")".into()]);
        o.extend(a(["-compose", "Multiply", "-composite", "-compose", "Over"]));
    }
    Ok(o)
}

// ---------------------------------------------------------------- crop / resize

fn parse_ratio(r: &str) -> Result<f64> {
    let (a, b) = r.split_once(':').ok_or_else(|| Error::BadParams(format!("bad ratio: {r}")))?;
    let (a, b): (f64, f64) = (a.trim().parse().map_err(|_| Error::BadParams(format!("bad ratio: {r}")))?, b.trim().parse().map_err(|_| Error::BadParams(format!("bad ratio: {r}")))?);
    if a <= 0.0 || b <= 0.0 {
        return Err(Error::BadParams(format!("bad ratio: {r}")));
    }
    Ok(a / b)
}

pub(crate) fn crop_rect(p: &Value, w: u32, h: u32) -> Result<(u32, u32, u32, u32)> {
    let (wf, hf) = (w as f64, h as f64);
    let (x, y, cw, ch) = if str_or(p, "mode", "") == "ratio" {
        let r = parse_ratio(str_or(p, "ratio", "1:1"))?;
        let (cw, ch) = if wf / hf > r { (hf * r, hf) } else { (wf, wf / r) };
        (((wf - cw) / 2.0).floor(), ((hf - ch) / 2.0).floor(), cw.floor(), ch.floor())
    } else {
        let width = crate::params::require_f64(p, "width")?;
        let height = crate::params::require_f64(p, "height")?;
        (f64_or(p, "x", 0.0), f64_or(p, "y", 0.0), width, height)
    };
    let x = x.round().clamp(0.0, wf - 1.0);
    let y = y.round().clamp(0.0, hf - 1.0);
    let cw = cw.round().clamp(1.0, wf - x);
    let ch = ch.round().clamp(1.0, hf - y);
    Ok((x as u32, y as u32, cw as u32, ch as u32))
}

fn crop_ops(p: &Value, w: u32, h: u32) -> Result<Args> {
    let (x, y, cw, ch) = crop_rect(p, w, h)?;
    Ok(vec!["-crop".into(), format!("{cw}x{ch}+{x}+{y}"), "+repage".into()])
}

fn resize_ops(p: &Value) -> Result<Args> {
    let geom = match str_or(p, "mode", "width") {
        "width" => format!("{}x", f64_or(p, "width", 1920.0).round().max(1.0) as u32),
        "height" => format!("x{}", f64_or(p, "height", 1080.0).round().max(1.0) as u32),
        "percent" => format!("{}%", f64_or(p, "percent", 50.0).clamp(1.0, 400.0)),
        "fit" => format!("{}x{}", f64_or(p, "width", 1920.0).round().max(1.0) as u32, f64_or(p, "height", 1080.0).round().max(1.0) as u32),
        other => return Err(Error::BadParams(format!("unknown resize mode: {other}"))),
    };
    Ok(vec!["-resize".into(), geom])
}

// ---------------------------------------------------------------- redact

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn regions(p: &Value, w: u32, h: u32) -> Result<Vec<Rect>> {
    let arr = p.get("regions").and_then(|r| r.as_array()).ok_or_else(|| Error::BadParams("missing 'regions'".into()))?;
    let mut out = Vec::new();
    for r in arr {
        let (mut x, mut y, mut rw, mut rh) = (f64_or(r, "x", 0.0), f64_or(r, "y", 0.0), f64_or(r, "w", 0.0), f64_or(r, "h", 0.0));
        if rw < 0.0 {
            x += rw;
            rw = -rw;
        }
        if rh < 0.0 {
            y += rh;
            rh = -rh;
        }
        let x0 = x.clamp(0.0, w as f64);
        let y0 = y.clamp(0.0, h as f64);
        let x1 = (x + rw).clamp(0.0, w as f64);
        let y1 = (y + rh).clamp(0.0, h as f64);
        if x1 - x0 >= 1.0 && y1 - y0 >= 1.0 {
            out.push(Rect { x: x0.floor(), y: y0.floor(), w: (x1 - x0).ceil(), h: (y1 - y0).ceil() });
        }
    }
    Ok(out)
}

fn redact_ops(p: &Value, w: u32, h: u32) -> Result<Args> {
    let regs = regions(p, w, h)?;
    let style = str_or(p, "style", "solid");
    let mut o: Args = Vec::new();
    match style {
        "solid" => {
            let c = color(p, "color", "#000000")?;
            o.extend(["-fill".into(), c, "-stroke".into(), "none".into()]);
            for r in &regs {
                o.extend(["-draw".into(), format!("rectangle {},{} {},{}", r.x, r.y, r.x + r.w - 1.0, r.y + r.h - 1.0)]);
            }
        }
        "blur" => {
            // Pixelating blur: irreversible, unlike a light gaussian blur.
            let strength = f64_or(p, "blur_radius", 30.0).clamp(4.0, 200.0);
            for r in &regs {
                let block = (strength * 0.6).max(6.0).min(r.w.min(r.h).max(2.0));
                let (bw, bh) = ((r.w / block).ceil().max(1.0) as u32, (r.h / block).ceil().max(1.0) as u32);
                o.extend([
                    "(".into(),
                    "+clone".into(),
                    "-crop".into(),
                    format!("{}x{}+{}+{}", r.w as u32, r.h as u32, r.x as u32, r.y as u32),
                    "+repage".into(),
                    "-scale".into(),
                    format!("{bw}x{bh}!"),
                    "-scale".into(),
                    format!("{}x{}!", r.w as u32, r.h as u32),
                    ")".into(),
                    "-gravity".into(),
                    "NorthWest".into(),
                    "-geometry".into(),
                    format!("+{}+{}", r.x as u32, r.y as u32),
                    "-composite".into(),
                ]);
            }
        }
        other => return Err(Error::BadParams(format!("unknown redact style: {other}"))),
    }
    // A redaction must not leak the original through EXIF thumbnails.
    o.push("-strip".into());
    Ok(o)
}

// ---------------------------------------------------------------- annotate

fn annotate_ops(p: &Value, ctx: &Ctx) -> Result<Args> {
    let items = p.get("items").and_then(|i| i.as_array()).ok_or_else(|| Error::BadParams("missing 'items'".into()))?;
    let (reg, bold) = im::fonts(ctx);
    let mut o: Args = Vec::new();
    for it in items {
        let kind = str_or(it, "type", "");
        let col = color(it, "color", "#ff3b30")?;
        let lw = f64_or(it, "width", 6.0).max(1.0);
        match kind {
            "arrow" => {
                let (x1, y1, x2, y2) = (f64_or(it, "x1", 0.0), f64_or(it, "y1", 0.0), f64_or(it, "x2", 0.0), f64_or(it, "y2", 0.0));
                let (dx, dy) = (x2 - x1, y2 - y1);
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1.0 {
                    continue;
                }
                let head = (lw * 3.5).max(14.0).min(len * 0.6);
                let (ux, uy) = (dx / len, dy / len);
                let ang = 26f64.to_radians();
                let rot = |sx: f64, sy: f64, a: f64| (sx * a.cos() - sy * a.sin(), sx * a.sin() + sy * a.cos());
                let (lx, ly) = rot(-ux, -uy, ang);
                let (rx, ry) = rot(-ux, -uy, -ang);
                let p1 = (x2 + lx * head, y2 + ly * head);
                let p2 = (x2 + rx * head, y2 + ry * head);
                let bx = x2 - ux * head * 0.6;
                let by = y2 - uy * head * 0.6;
                o.extend(["-stroke".into(), col.clone(), "-fill".into(), "none".into(), "-strokewidth".into(), format!("{lw}")]);
                o.extend(["-draw".into(), format!("line {:.1},{:.1} {:.1},{:.1}", x1, y1, bx, by)]);
                o.extend(["-fill".into(), col.clone(), "-strokewidth".into(), "1".into()]);
                o.extend(["-draw".into(), format!("polygon {:.1},{:.1} {:.1},{:.1} {:.1},{:.1}", x2, y2, p1.0, p1.1, p2.0, p2.1)]);
            }
            "rect" | "ellipse" => {
                let (mut x, mut y, mut w, mut h) = (f64_or(it, "x", 0.0), f64_or(it, "y", 0.0), f64_or(it, "w", 0.0), f64_or(it, "h", 0.0));
                if w < 0.0 {
                    x += w;
                    w = -w;
                }
                if h < 0.0 {
                    y += h;
                    h = -h;
                }
                if w < 1.0 || h < 1.0 {
                    continue;
                }
                let fill = match it.get("fill") {
                    Some(Value::String(f)) if !f.is_empty() => valid_color(f)?,
                    _ => "none".to_string(),
                };
                o.extend(["-stroke".into(), col.clone(), "-fill".into(), fill, "-strokewidth".into(), format!("{lw}")]);
                let prim = if kind == "rect" {
                    format!("rectangle {:.1},{:.1} {:.1},{:.1}", x, y, x + w, y + h)
                } else {
                    format!("ellipse {:.1},{:.1} {:.1},{:.1} 0,360", x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0)
                };
                o.extend(["-draw".into(), prim]);
            }
            "text" => {
                let text = str_or(it, "text", "");
                if text.is_empty() {
                    continue;
                }
                let size = f64_or(it, "size", 48.0).max(6.0);
                let (x, y) = (f64_or(it, "x", 0.0), f64_or(it, "y", 0.0));
                let tc = valid_color(str_or(it, "color", "#ffffff"))?;
                let bgc = match it.get("bg") {
                    Some(Value::String(b)) if !b.is_empty() => Some(valid_color(b)?),
                    _ => None,
                };
                let font = if bool_or(it, "bold", false) { bold.clone().or(reg.clone()) } else { reg.clone() };
                o.push("(".into());
                o.extend(["-background".into(), bgc.clone().unwrap_or_else(|| "none".into()), "-fill".into(), tc, "-stroke".into(), "none".into()]);
                if let Some(f) = &font {
                    o.extend(["-font".into(), f.clone()]);
                }
                o.extend(["-pointsize".into(), format!("{size}"), format!("label:{}", im::safe_text(text))]);
                if let Some(b) = &bgc {
                    o.extend(["-bordercolor".into(), b.clone(), "-border".into(), format!("{}", (size * 0.25).round() as u32)]);
                }
                o.push(")".into());
                o.extend(["-gravity".into(), "NorthWest".into(), "-geometry".into(), format!("+{}+{}", x.round().max(0.0) as i64, y.round().max(0.0) as i64), "-composite".into()]);
            }
            "callout" => {
                let size = f64_or(it, "size", 56.0).max(16.0);
                let n = i64_or(it, "n", 1);
                let (x, y) = (f64_or(it, "x", 0.0), f64_or(it, "y", 0.0));
                let d = size.round() as i64;
                let r = d as f64 / 2.0;
                o.push("(".into());
                o.extend(["-size".into(), format!("{d}x{d}"), "xc:none".into(), "-stroke".into(), "white".into(), "-strokewidth".into(), format!("{}", (size / 14.0).max(2.0)), "-fill".into(), col.clone()]);
                o.extend(["-draw".into(), format!("circle {:.1},{:.1} {:.1},{:.1}", r, r, r, 2.0)]);
                o.extend(["-stroke".into(), "none".into(), "-fill".into(), "white".into(), "-gravity".into(), "center".into()]);
                if let Some(f) = bold.clone().or(reg.clone()) {
                    o.extend(["-font".into(), f]);
                }
                o.extend(["-pointsize".into(), format!("{:.0}", size * 0.52), "-annotate".into(), "+0+0".into(), n.to_string()]);
                o.push(")".into());
                o.extend(["-gravity".into(), "NorthWest".into(), "-geometry".into(), format!("+{}+{}", (x - r).round() as i64, (y - r).round() as i64), "-composite".into()]);
            }
            other => return Err(Error::BadParams(format!("unknown annotation type: {other:?}"))),
        }
    }
    Ok(o)
}

// ---------------------------------------------------------------- shared compositing pieces

/// Args that give the current image rounded corners (alpha mask). Radius is clamped.
fn round_corners(r: u32) -> Args {
    if r == 0 {
        return vec![];
    }
    let mut v: Args = a(["-alpha", "set", "(", "+clone", "-alpha", "extract", "-draw"]);
    v.push(format!("fill black polygon 0,0 0,{r} {r},0 fill white circle {r},{r} {r},0"));
    v.extend(a(["(", "+clone", "-flip", ")", "-compose", "Multiply", "-composite", "(", "+clone", "-flop", ")", "-compose", "Multiply", "-composite", ")"]));
    v.extend(a(["-alpha", "off", "-compose", "CopyOpacity", "-composite", "-compose", "Over"]));
    v
}

fn add_background(input: &Path, p: &Value, out: &Path, ctx: &Ctx) -> Result<()> {
    let td = ctx.temp_dir()?;
    let (w, h) = im::oriented_dims(input, ctx)?;
    let pad = f64_or(p, "padding", 120.0).max(0.0).round() as u32;
    let radius = (f64_or(p, "corner_radius", 40.0).max(0.0).round() as u32).min(w.min(h) / 2);
    let fg = td.path().join("fg.png");
    let mut args: Args = vec![in_arg(input), "-auto-orient".into()];
    args.extend(round_corners(radius));
    if let Some(sh) = p.get("shadow").filter(|v| v.is_object()) {
        let blur = f64_or(sh, "blur", 30.0).max(0.0);
        let op = (f64_or(sh, "opacity", 0.35).clamp(0.0, 1.0) * 100.0).round() as i64;
        let off = f64_or(sh, "offset_y", 16.0).round() as i64;
        if op > 0 {
            args.extend(a(["(", "+clone", "-background", "black", "-shadow"]));
            args.push(format!("{op}x{:.1}+0+{off}", blur / 2.0));
            args.extend(a([")", "+swap", "-background", "none", "-layers", "merge", "+repage"]));
        }
    }
    args.push(out_arg("png", &fg));
    im::run(&args, ctx)?;
    let (fw, fh) = im::oriented_dims(&fg, ctx)?;
    let (mut cw, mut ch) = ((w + 2 * pad).max(fw), (h + 2 * pad).max(fh));
    let ratio = str_or(p, "ratio", "auto");
    if ratio != "auto" && !ratio.is_empty() {
        let r = parse_ratio(ratio)?;
        if (cw as f64 / ch as f64) < r {
            cw = (ch as f64 * r).ceil() as u32;
        } else {
            ch = (cw as f64 / r).ceil() as u32;
        }
    }
    let bg = p.get("background").cloned().unwrap_or_else(|| serde_json::json!({"type":"solid","color":"#ffffff"}));
    let mut fin = im::background_args(&bg, cw, ch)?;
    fin.extend(a(["-alpha", "off"]));
    fin.push(s(&abs(&fg)));
    fin.extend(a(["-gravity", "center", "-composite", "-depth", "8"]));
    fin.push(out_arg("png", out));
    im::run(&fin, ctx)
}

// ---------------------------------------------------------------- compress

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Jpeg,
    Webp,
    Png,
    Tiff,
}

fn encode(base: &Path, fmt: Fmt, q: u32, colors: Option<u32>, out: &Path, ctx: &Ctx) -> Result<u64> {
    let mut args: Args = vec![in_arg(base)];
    let coder = match fmt {
        Fmt::Jpeg => {
            args.extend(a(["-quality", &q.to_string(), "-sampling-factor", "4:2:0", "-interlace", "Plane"]));
            "jpeg"
        }
        Fmt::Webp => {
            args.extend(a(["-quality", &q.to_string(), "-define", "webp:method=4"]));
            "webp"
        }
        Fmt::Png => {
            if let Some(c) = colors {
                args.extend(a(["-dither", "FloydSteinberg", "-colors", &c.to_string()]));
            }
            args.extend(a(["-define", "png:compression-level=9", "-define", "png:compression-filter=5"]));
            "png"
        }
        Fmt::Tiff => {
            args.extend(a(["-compress", "jpeg", "-quality", &q.to_string()]));
            "tiff"
        }
    };
    args.push(out_arg(coder, out));
    im::run(&args, ctx)?;
    Ok(std::fs::metadata(out)?.len())
}

fn compress(input: &Path, p: &Value, td: &Path, ctx: &Ctx) -> Result<(PathBuf, &'static str)> {
    let orig = std::fs::metadata(input)?.len();
    let mode = str_or(p, "mode", "preset");
    let (fmt, ext) = match ext_of(input).as_str() {
        "png" => (Fmt::Png, "png"),
        "webp" => (Fmt::Webp, "webp"),
        "tif" | "tiff" => (Fmt::Tiff, "tiff"),
        _ => (Fmt::Jpeg, "jpg"),
    };
    let maxdim = i64_or(p, "max_dimension", 0).max(0) as u32;
    let target: Option<u64> = match mode {
        "target_size" => Some((f64_or(p, "target_kb", 500.0).max(1.0) * 1024.0) as u64),
        "reduce" => Some((orig as f64 * (1.0 - f64_or(p, "percent", 50.0).clamp(1.0, 95.0) / 100.0)) as u64),
        "preset" => None,
        other => return Err(Error::BadParams(format!("unknown compress mode: {other}"))),
    };
    let make_base = |dim: u32, dest: &Path| -> Result<()> {
        let mut args = vec![in_arg(input), "-auto-orient".into(), "-strip".into()];
        if dim > 0 {
            args.extend(["-resize".into(), format!("{dim}x{dim}>")]);
        }
        args.extend(a(["-define", "png:compression-level=1"]));
        args.push(out_arg("png", dest));
        im::run(&args, ctx)
    };
    let base = td.join("base.png");
    let out = td.join(format!("out.{ext}"));
    make_base(maxdim, &base)?;
    match target {
        None => {
            let preset = str_or(p, "preset", "balanced");
            match fmt {
                Fmt::Png => {
                    let colors = match preset {
                        "small" => Some(128),
                        _ => None,
                    };
                    if preset == "small" && maxdim == 0 {
                        make_base(2400, &base)?;
                    }
                    encode(&base, fmt, 0, colors, &out, ctx)?;
                }
                _ => {
                    let q = match preset {
                        "small" => 55,
                        "high" => 90,
                        "balanced" => 78,
                        other => return Err(Error::BadParams(format!("unknown preset: {other}"))),
                    };
                    if preset == "small" && maxdim == 0 {
                        make_base(2400, &base)?;
                    }
                    encode(&base, fmt, q, None, &out, ctx)?;
                }
            }
        }
        Some(t) => {
            let mut dim = if maxdim > 0 { maxdim } else { 0 };
            let (bw, bh) = im::oriented_dims(&base, ctx)?;
            let longest = bw.max(bh);
            let mut best: Option<u64> = None;
            // Up to 6 rounds: each round searches quality / colours; if the target still cannot be
            // met, the image is shrunk by 20 % and the search repeated.
            for round in 0..6 {
                ctx.check_cancel()?;
                let attempt = td.join("attempt.tmp");
                let size = match fmt {
                    Fmt::Png => {
                        let mut got = encode(&base, fmt, 0, None, &attempt, ctx)?;
                        for c in [256, 128, 64, 32, 16] {
                            if got <= t {
                                break;
                            }
                            got = encode(&base, fmt, 0, Some(c), &attempt, ctx)?;
                        }
                        got
                    }
                    _ => {
                        let (mut lo, mut hi) = (10u32, 95u32);
                        let mut found = false;
                        let mut size = encode(&base, fmt, lo, None, &attempt, ctx)?;
                        if size <= t {
                            found = true;
                            let keep = td.join("keep.tmp");
                            std::fs::copy(&attempt, &keep)?;
                            while hi - lo > 2 {
                                let mid = (lo + hi) / 2;
                                let sz = encode(&base, fmt, mid, None, &attempt, ctx)?;
                                if sz <= t {
                                    lo = mid;
                                    std::fs::copy(&attempt, &keep)?;
                                    size = sz;
                                } else {
                                    hi = mid;
                                }
                            }
                            std::fs::copy(&keep, &attempt)?;
                        }
                        let _ = found;
                        size
                    }
                };
                if best.map_or(true, |b| size < b) {
                    best = Some(size);
                    std::fs::copy(&attempt, &out)?;
                }
                if size <= t {
                    break;
                }
                let cur = if dim == 0 { longest } else { dim.min(longest) };
                dim = ((cur as f64) * 0.8).round() as u32;
                if dim < 64 || round == 5 {
                    break;
                }
                make_base(dim, &base)?;
            }
        }
    }
    Ok((out, ext))
}

// ---------------------------------------------------------------- collage

fn collage_layout(layout: &str, n: usize, cols_req: usize, cell: u32, sp: u32) -> Result<(u32, u32, Vec<(u32, u32, u32, u32)>)> {
    let mut cells = Vec::new();
    match layout {
        "grid" | "row" | "column" => {
            let cols = match layout {
                "row" => n,
                "column" => 1,
                _ => cols_req.clamp(1, n),
            };
            let rows = n.div_ceil(cols);
            let cw = cols as u32 * cell + (cols as u32 + 1) * sp;
            let chh = rows as u32 * cell + (rows as u32 + 1) * sp;
            for i in 0..n {
                let (r, c) = (i / cols, i % cols);
                let in_row = if r == rows - 1 { n - r * cols } else { cols };
                let lead = (cols - in_row) as u32 * (cell + sp) / 2;
                cells.push((sp + lead + c as u32 * (cell + sp), sp + r as u32 * (cell + sp), cell, cell));
            }
            Ok((cw, chh, cells))
        }
        "featured" => {
            let big = 2 * cell + sp;
            let k = (n - 1) as u32;
            cells.push((sp, sp, big, big));
            let hk = (big - (k - 1) * sp) / k;
            for j in 0..k {
                cells.push((sp + big + sp, sp + j * (hk + sp), cell, hk));
            }
            Ok((sp + big + sp + cell + sp, big + 2 * sp, cells))
        }
        other => Err(Error::BadParams(format!("unknown collage layout: {other}"))),
    }
}

pub(crate) fn collage(inputs: &[PathBuf], p: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let n = inputs.len();
    let layout = str_or(p, "layout", "grid");
    let cell = f64_or(p, "cell_size", 800.0).clamp(64.0, 4000.0) as u32;
    let sp = f64_or(p, "spacing", 24.0).clamp(0.0, 400.0) as u32;
    let rad = f64_or(p, "corner_radius", 24.0).clamp(0.0, 1000.0) as u32;
    let cols = i64_or(p, "columns", 2).max(1) as usize;
    let (cw, ch, cells) = collage_layout(layout, n, cols, cell, sp)?;
    let td = ctx.temp_dir()?;
    let mut paths = Vec::new();
    for (i, (input, (_, _, w, h))) in inputs.iter().zip(&cells).enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / (n as f32 + 1.0), format!("Preparing image {} of {n}", i + 1));
        let dest = td.path().join(format!("cell{i}.png"));
        let mut args: Args = vec![in_arg(input), "-auto-orient".into(), "-resize".into(), format!("{w}x{h}^"), "-gravity".into(), "center".into(), "-extent".into(), format!("{w}x{h}"), "+repage".into()];
        args.extend(round_corners(rad.min(w.min(h) / 2)));
        args.push(out_arg("png", &dest));
        im::run(&args, ctx)?;
        paths.push(dest);
    }
    let bg = p.get("background").cloned().unwrap_or_else(|| serde_json::json!({"type":"solid","color":"#ffffff"}));
    let mut fin = im::background_args(&bg, cw, ch)?;
    fin.extend(a(["-alpha", "off", "-gravity", "NorthWest"]));
    for (path, (x, y, _, _)) in paths.iter().zip(&cells) {
        fin.push(s(&abs(path)));
        fin.extend(["-geometry".into(), format!("+{x}+{y}"), "-composite".into()]);
    }
    let tmp = td.path().join("collage.png");
    fin.push(out_arg("png", &tmp));
    im::run(&fin, ctx)?;
    let first = &inputs[0];
    let dest = finish_named(&tmp, first, &format!("{} Collage", stem_of(first)), "png", ctx)?;
    ctx.report(1.0, "Done");
    Ok(vec![dest])
}

// ---------------------------------------------------------------- create-pdf

pub(crate) fn create_pdf(inputs: &[PathBuf], p: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let td = ctx.temp_dir()?;
    let mut pages = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / inputs.len() as f32, format!("Reading image {} of {}", i + 1, inputs.len()));
        let is_jpeg = matches!(ext_of(input).as_str(), "jpg" | "jpeg");
        let flat: Args = a(["-auto-orient", "-background", "white", "-alpha", "remove", "-alpha", "off", "-colorspace", "sRGB"]);
        if is_jpeg {
            let tmp = td.path().join(format!("p{i}.jpg"));
            let mut args = vec![in_arg(input)];
            args.extend(flat);
            args.extend(a(["-quality", "92"]));
            args.push(out_arg("jpeg", &tmp));
            im::run(&args, ctx)?;
            let data = std::fs::read(&tmp)?;
            let (w, h, comps) = pdfwrite::jpeg_info(&data).ok_or_else(|| Error::msg("unreadable JPEG produced"))?;
            pages.push(PageImage::Jpeg { w, h, comps, data });
        } else {
            let tmp = td.path().join(format!("p{i}.ppm"));
            let mut args = vec![in_arg(input)];
            args.extend(flat);
            args.extend(a(["-depth", "8"]));
            args.push(out_arg("ppm", &tmp));
            im::run(&args, ctx)?;
            let data = std::fs::read(&tmp)?;
            let (w, h, rgb) = pdfwrite::parse_ppm(&data).ok_or_else(|| Error::msg("unreadable bitmap produced"))?;
            pages.push(PageImage::Rgb { w, h, data: rgb });
        }
    }
    let page = match str_or(p, "page_size", "fit") {
        "fit" => None,
        "a4" => Some((595.28, 841.89)),
        "letter" => Some((612.0, 792.0)),
        other => return Err(Error::BadParams(format!("unknown page size: {other}"))),
    };
    let orientation = match str_or(p, "orientation", "auto") {
        "portrait" => 1,
        "landscape" => 2,
        _ => 0,
    };
    let layout = Layout { page, margin: f64_or(p, "margin", 0.0).clamp(0.0, 200.0), orientation };
    let bytes = pdfwrite::build(&pages, &layout);
    let tmp = td.path().join("out.pdf");
    std::fs::write(&tmp, bytes)?;
    let first = &inputs[0];
    let dest = finish_named(&tmp, first, &stem_of(first), "pdf", ctx)?;
    ctx.report(1.0, "Done");
    Ok(vec![dest])
}

// ---------------------------------------------------------------- previews

fn scale_params(id: &str, p: &Value, k: f64) -> Value {
    fn mul(v: &mut Value, key: &str, k: f64) {
        if let Some(n) = v.get(key).and_then(|x| x.as_f64()) {
            v[key] = serde_json::json!(n * k);
        }
    }
    let mut v = p.clone();
    match id {
        "crop-image" => {
            for key in ["x", "y", "width", "height"] {
                mul(&mut v, key, k);
            }
        }
        "redact-photo" => {
            mul(&mut v, "blur_radius", k);
            if let Some(arr) = v.get_mut("regions").and_then(|r| r.as_array_mut()) {
                for r in arr {
                    for key in ["x", "y", "w", "h"] {
                        mul(r, key, k);
                    }
                }
            }
        }
        "annotate-photo" => {
            if let Some(arr) = v.get_mut("items").and_then(|r| r.as_array_mut()) {
                for it in arr {
                    for key in ["x", "y", "x1", "y1", "x2", "y2", "w", "h", "width", "size"] {
                        mul(it, key, k);
                    }
                }
            }
        }
        "add-background" => {
            mul(&mut v, "padding", k);
            mul(&mut v, "corner_radius", k);
            if let Some(sh) = v.get_mut("shadow") {
                mul(sh, "blur", k);
                mul(sh, "offset_y", k);
            }
        }
        _ => {}
    }
    v
}

fn temp_png(prefix: &str) -> Result<PathBuf> {
    let f = tempfile::Builder::new().prefix(prefix).suffix(".png").tempfile()?;
    let (_, path) = f.keep().map_err(|e| Error::msg(e.to_string()))?;
    Ok(path)
}

pub(crate) fn preview(id: &str, input: &Path, p: &Value, ctx: &Ctx, max_dim: u32) -> Result<PathBuf> {
    if !matches!(id, "edit-photo" | "annotate-photo" | "add-background" | "crop-image" | "redact-photo") {
        return Err(Error::Unsupported(format!("no live preview for {id}")));
    }
    let max_dim = max_dim.clamp(64, 4000);
    let td = ctx.temp_dir()?;
    let (w, h) = im::oriented_dims(input, ctx)?;
    let k = (max_dim as f64 / w.max(h) as f64).min(1.0);
    let base = td.path().join("base.png");
    let mut args = vec![in_arg(input), "-auto-orient".into()];
    if k < 1.0 {
        args.extend(["-resize".into(), format!("{max_dim}x{max_dim}>")]);
    }
    args.push(out_arg("png", &base));
    im::run(&args, ctx)?;
    let (bw, bh) = im::oriented_dims(&base, ctx)?;
    let sp = scale_params(id, p, bw as f64 / w as f64);
    let out = temp_png("orange-preview-")?;
    let res = if id == "add-background" {
        add_background(&base, &sp, &out, ctx)
    } else {
        let mut a2 = vec![in_arg(&base)];
        a2.extend(ops(id, &sp, bw, bh, ctx)?);
        a2.extend(a(["-depth", "8"]));
        a2.push(out_arg("png", &out));
        im::run(&a2, ctx)
    };
    if let Err(e) = res {
        let _ = std::fs::remove_file(&out);
        return Err(e);
    }
    Ok(out)
}

pub(crate) fn web_preview(input: &Path, ctx: &Ctx, max_dim: u32) -> Result<PathBuf> {
    let max_dim = max_dim.clamp(64, 8000);
    let out = temp_png("orange-web-")?;
    let args = vec![in_arg(input), "-auto-orient".into(), "-resize".into(), format!("{max_dim}x{max_dim}>"), "-depth".into(), "8".into(), out_arg("png", &out)];
    if let Err(e) = im::run(&args, ctx) {
        let _ = std::fs::remove_file(&out);
        return Err(e);
    }
    Ok(out)
}
