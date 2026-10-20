//! PDF tools: qpdf for structure (split / merge / rotate / lock), lopdf + image for image recompression.

use crate::im::abs;
use crate::info::pdf_pages;
use crate::params::{i64_or, str_or};
use crate::util::{finish_named, s};
use lopdf::{Document, Object};
use orange_core::paths::stem_of;
use orange_core::{exec, Ctx, Dep, Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn qpdf(ctx: &Ctx, args: &[String]) -> Result<()> {
    let q = ctx.caps.require(Dep::Qpdf)?;
    let mut c = exec::command(q);
    c.arg("--warning-exit-0").args(args);
    exec::run(&mut c, "qpdf", ctx).map(|_| ())
}

pub(crate) fn run_each(id: &str, inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let mut outs = Vec::new();
    let n = inputs.len() as f32;
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / n, format!("Working on {}", input.file_name().and_then(|f| f.to_str()).unwrap_or("PDF")));
        let produced = match id {
            "compress-pdf" => compress(input, params, ctx)?,
            "split-pdf" => split(input, params, ctx)?,
            "rotate-pdf" => rotate(input, params, ctx)?,
            "protect-pdf" => protect(input, params, ctx)?,
            other => return Err(Error::Unsupported(format!("not a PDF tool: {other}"))),
        };
        outs.extend(produced);
    }
    ctx.report(1.0, "Done");
    Ok(outs)
}

// ---------------------------------------------------------------- compress

/// Recompress JPEG (DCTDecode) images in place and shrink oversized ones. Returns how many were replaced.
fn recompress_images(doc: &mut Document, quality: u8, max_dim: u32) -> usize {
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::FilterType;
    use image::{ImageFormat, ImageEncoder};
    let ids: Vec<_> = doc.objects.keys().cloned().collect();
    let mut replaced = 0;
    for id in ids {
        let Some(Object::Stream(st)) = doc.objects.get_mut(&id) else { continue };
        let is_image = st.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Image".as_slice());
        if !is_image {
            continue;
        }
        let filter_ok = match st.dict.get(b"Filter") {
            Ok(Object::Name(n)) => n == b"DCTDecode",
            Ok(Object::Array(a)) => a.len() == 1 && a[0].as_name().ok() == Some(b"DCTDecode".as_slice()),
            _ => false,
        };
        let cs = st.dict.get(b"ColorSpace").ok().and_then(|o| o.as_name().ok()).map(|n| n.to_vec());
        let gray = cs.as_deref() == Some(b"DeviceGray".as_slice());
        let rgb = cs.as_deref() == Some(b"DeviceRGB".as_slice());
        let masked = st.dict.has(b"Decode") || st.dict.has(b"Mask") || matches!(st.dict.get(b"ImageMask"), Ok(Object::Boolean(true)));
        if !filter_ok || !(gray || rgb) || masked {
            continue;
        }
        let Ok(img) = image::load_from_memory_with_format(&st.content, ImageFormat::Jpeg) else { continue };
        let (w, h) = (img.width(), img.height());
        let longest = w.max(h);
        let img = if longest > max_dim {
            let k = max_dim as f64 / longest as f64;
            img.resize(((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), FilterType::Lanczos3)
        } else {
            img
        };
        let mut buf = Vec::new();
        let enc = JpegEncoder::new_with_quality(&mut buf, quality);
        let ok = if gray {
            let g = img.to_luma8();
            enc.write_image(g.as_raw(), g.width(), g.height(), image::ExtendedColorType::L8).is_ok()
        } else {
            let c = img.to_rgb8();
            enc.write_image(c.as_raw(), c.width(), c.height(), image::ExtendedColorType::Rgb8).is_ok()
        };
        if !ok || buf.len() >= st.content.len() {
            continue;
        }
        let (nw, nh) = (img.width(), img.height());
        st.dict.set("Width", Object::Integer(nw as i64));
        st.dict.set("Height", Object::Integer(nh as i64));
        st.dict.set("Length", Object::Integer(buf.len() as i64));
        st.content = buf;
        replaced += 1;
    }
    replaced
}

fn compress(input: &Path, params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let (quality, max_dim) = match str_or(params, "level", "medium") {
        "low" => (82, 3000),
        "medium" => (68, 1800),
        "high" => (50, 1200),
        other => return Err(Error::BadParams(format!("unknown level: {other}"))),
    };
    let td = ctx.temp_dir()?;
    let stage1 = td.path().join("stage1.pdf");
    let mut doc = Document::load(input).map_err(|e| Error::msg(format!("could not open the PDF: {e}")))?;
    if doc.is_encrypted() {
        return Err(Error::BadParams("the PDF is password-protected; unlock it first".into()));
    }
    let replaced = recompress_images(&mut doc, quality, max_dim);
    doc.compress();
    doc.save(&stage1).map_err(|e| Error::msg(format!("could not write the PDF: {e}")))?;
    let mut best = stage1.clone();
    if ctx.caps.has(Dep::Qpdf) {
        let stage2 = td.path().join("stage2.pdf");
        let r = qpdf(ctx, &["--object-streams=generate".into(), "--compress-streams=y".into(), "--recompress-flate".into(), "--compression-level=9".into(), s(&abs(&stage1)), s(&stage2)]);
        if r.is_ok() && std::fs::metadata(&stage2).map(|m| m.len()).unwrap_or(u64::MAX) < std::fs::metadata(&best)?.len() {
            best = stage2;
        }
    }
    // never hand back something bigger than what we were given
    if std::fs::metadata(&best)?.len() >= std::fs::metadata(input)?.len() {
        ctx.report(0.95, "No further savings possible; saved an optimized copy");
        std::fs::copy(input, td.path().join("orig.pdf"))?;
        best = td.path().join("orig.pdf");
    } else {
        ctx.report(0.95, format!("Recompressed {replaced} image(s)"));
    }
    Ok(vec![finish_named(&best, input, &format!("{} Compressed", stem_of(input)), "pdf", ctx)?])
}

// ---------------------------------------------------------------- split / rotate / lock

fn valid_page_spec(spec: &str) -> bool {
    !spec.is_empty() && spec.chars().all(|c| c.is_ascii_digit() || matches!(c, ',' | '-' | 'z' | 'r'))
}

fn split(input: &Path, params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let pages = pdf_pages(input, ctx)?;
    let stem = stem_of(input);
    // (spec, name suffix)
    let mut parts: Vec<(String, String)> = Vec::new();
    match str_or(params, "mode", "each") {
        "each" => {
            for p in 1..=pages {
                parts.push((p.to_string(), format!("Page {p}")));
            }
        }
        "every_n" => {
            let n = i64_or(params, "n", 1).max(1) as u32;
            let (mut start, mut k) = (1, 1);
            while start <= pages {
                let end = (start + n - 1).min(pages);
                parts.push((format!("{start}-{end}"), format!("Part {k}")));
                start = end + 1;
                k += 1;
            }
        }
        "ranges" => {
            let ranges = str_or(params, "ranges", "");
            if !valid_page_spec(ranges) {
                return Err(Error::BadParams("enter page ranges like 1-3,5,7-9".into()));
            }
            for r in ranges.split(',').filter(|r| !r.is_empty()) {
                let (a, b) = match r.split_once('-') {
                    Some((a, b)) => (a.parse::<u32>().ok(), if b.is_empty() { Some(pages) } else { b.parse::<u32>().ok() }),
                    None => (r.parse::<u32>().ok(), r.parse::<u32>().ok()),
                };
                let (Some(a), Some(b)) = (a, b) else { return Err(Error::BadParams(format!("bad range: {r}"))) };
                if a < 1 || b < a || b > pages {
                    return Err(Error::BadParams(format!("range {r} is outside 1-{pages}")));
                }
                let name = if a == b { format!("Page {a}") } else { format!("Pages {a}-{b}") };
                parts.push((format!("{a}-{b}"), name));
            }
        }
        other => return Err(Error::BadParams(format!("unknown split mode: {other}"))),
    }
    if parts.is_empty() {
        return Err(Error::BadParams("nothing to split".into()));
    }
    let td = ctx.temp_dir()?;
    let mut outs = Vec::new();
    for (i, (spec, name)) in parts.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / parts.len() as f32, format!("Writing {name}"));
        let tmp = td.path().join(format!("part{i}.pdf"));
        qpdf(ctx, &[s(&abs(input)), "--pages".into(), ".".into(), spec.clone(), "--".into(), s(&tmp)])?;
        outs.push(finish_named(&tmp, input, &format!("{stem} {name}"), "pdf", ctx)?);
    }
    Ok(outs)
}

fn rotate(input: &Path, params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let deg = i64_or(params, "degrees", 90).rem_euclid(360);
    if ![90, 180, 270].contains(&deg) {
        return Err(Error::BadParams("degrees must be 90, 180 or 270".into()));
    }
    let pages = str_or(params, "pages", "all");
    let spec = if pages == "all" || pages.is_empty() { "1-z".to_string() } else { pages.replace(' ', "") };
    if !valid_page_spec(&spec) {
        return Err(Error::BadParams("enter pages like 1,3-5".into()));
    }
    let td = ctx.temp_dir()?;
    let tmp = td.path().join("rotated.pdf");
    qpdf(ctx, &[format!("--rotate=+{deg}:{spec}"), s(&abs(input)), s(&tmp)])?;
    Ok(vec![finish_named(&tmp, input, &format!("{} Rotated", stem_of(input)), "pdf", ctx)?])
}

fn protect(input: &Path, params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let pw = str_or(params, "password", "");
    if pw.is_empty() {
        return Err(Error::BadParams("enter a password".into()));
    }
    let td = ctx.temp_dir()?;
    let tmp = td.path().join("out.pdf");
    let (args, suffix): (Vec<String>, &str) = match str_or(params, "action", "lock") {
        "lock" => (vec!["--encrypt".into(), pw.into(), pw.into(), "256".into(), "--".into(), s(&abs(input)), s(&tmp)], "Locked"),
        "unlock" => (vec![format!("--password={pw}"), "--decrypt".into(), s(&abs(input)), s(&tmp)], "Unlocked"),
        other => return Err(Error::BadParams(format!("unknown action: {other}"))),
    };
    qpdf(ctx, &args).map_err(|e| match e {
        Error::ToolFailed { detail, .. } if detail.to_lowercase().contains("password") => Error::BadParams("that password does not open this PDF".into()),
        other => other,
    })?;
    Ok(vec![finish_named(&tmp, input, &format!("{} {}", stem_of(input), suffix), "pdf", ctx)?])
}

pub(crate) fn merge(inputs: &[PathBuf], _params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let td = ctx.temp_dir()?;
    let tmp = td.path().join("merged.pdf");
    let mut args: Vec<String> = vec!["--empty".into(), "--pages".into()];
    args.extend(inputs.iter().map(|p| s(&abs(p))));
    args.push("--".into());
    args.push(s(&tmp));
    ctx.report(0.1, "Merging");
    qpdf(ctx, &args)?;
    let first = &inputs[0];
    Ok(vec![finish_named(&tmp, first, &format!("{} Merged", stem_of(first)), "pdf", ctx)?])
}
