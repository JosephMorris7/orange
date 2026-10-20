//! create-archive: ZIP / TAR / TAR.GZ in pure Rust, 7z through 7-Zip.

use crate::im::abs;
use crate::params::str_or;
use crate::util::s;
use orange_core::paths::{out_dir_for, stem_of, unique_path};
use orange_core::{exec, Ctx, Dep, Error, Result};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Every file under the inputs as (absolute path, archive-relative name with `/` separators).
fn collect(inputs: &[PathBuf]) -> Result<Vec<(PathBuf, String)>> {
    let mut used: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for input in inputs {
        let top = input.file_name().and_then(|n| n.to_str()).unwrap_or("item").to_string();
        let mut top_name = top.clone();
        let mut k = 2;
        while used.contains(&top_name) {
            top_name = match top.rsplit_once('.') {
                Some((stem, ext)) if input.is_file() => format!("{stem} ({k}).{ext}"),
                _ => format!("{top} ({k})"),
            };
            k += 1;
        }
        used.insert(top_name.clone());
        if input.is_dir() {
            walk(input, &top_name, &mut out)?;
        } else {
            out.push((abs(input), top_name));
        }
    }
    Ok(out)
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(PathBuf, String)>) -> Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    let mut empty = true;
    for e in entries {
        empty = false;
        let p = e.path();
        let name = format!("{prefix}/{}", e.file_name().to_string_lossy());
        if p.is_dir() {
            walk(&p, &name, out)?;
        } else if p.is_file() {
            out.push((abs(&p), name));
        }
    }
    if empty {
        out.push((abs(dir), format!("{prefix}/")));
    }
    Ok(())
}

pub(crate) fn create(inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let format = str_or(params, "format", "zip");
    let first = &inputs[0];
    let requested = str_or(params, "name", "").trim();
    let clean: String = requested.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')).collect();
    let name = if clean.is_empty() || (clean == "Archive" && inputs.len() == 1) { stem_of(first) } else { clean };
    let dir = out_dir_for(first, ctx);
    std::fs::create_dir_all(&dir)?;
    let ext = match format {
        "zip" => "zip",
        "tar" => "tar",
        "gz" => "tar.gz",
        "7z" => "7z",
        other => return Err(Error::BadParams(format!("unknown archive format: {other}"))),
    };
    let dest = unique_path(&dir, &name, ext);
    let files = collect(inputs)?;
    ctx.report(0.05, "Packing");
    let result = match format {
        "zip" => write_zip(&dest, &files, ctx),
        "tar" => write_tar(&dest, &files, false, ctx),
        "gz" => write_tar(&dest, &files, true, ctx),
        _ => write_7z(&dest, inputs, ctx),
    };
    if let Err(e) = result {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }
    ctx.report(1.0, "Done");
    Ok(vec![dest])
}

fn write_zip(dest: &Path, files: &[(PathBuf, String)], ctx: &Ctx) -> Result<()> {
    use std::io::Write;
    let f = File::create(dest)?;
    let mut zw = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
    let n = files.len().max(1) as f32;
    for (i, (path, name)) in files.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(0.05 + 0.9 * i as f32 / n, name.clone());
        if name.ends_with('/') {
            zw.add_directory(name.clone(), opts).map_err(|e| Error::msg(e.to_string()))?;
            continue;
        }
        let mut o = opts;
        if let Ok(m) = std::fs::metadata(path).and_then(|m| m.modified()) {
            if let Ok(dt) = zip::DateTime::try_from({ let o = time::OffsetDateTime::from(m); time::PrimitiveDateTime::new(o.date(), o.time()) }) {
                o = o.last_modified_time(dt);
            }
        }
        zw.start_file(name.clone(), o).map_err(|e| Error::msg(e.to_string()))?;
        let mut src = File::open(path)?;
        std::io::copy(&mut src, &mut zw)?;
        zw.flush()?;
    }
    zw.finish().map_err(|e| Error::msg(e.to_string()))?;
    Ok(())
}

fn write_tar(dest: &Path, files: &[(PathBuf, String)], gz: bool, ctx: &Ctx) -> Result<()> {
    let f = File::create(dest)?;
    let n = files.len().max(1) as f32;
    let fill = |b: &mut tar::Builder<Box<dyn std::io::Write>>| -> Result<()> {
        for (i, (path, name)) in files.iter().enumerate() {
            ctx.check_cancel()?;
            ctx.report(0.05 + 0.9 * i as f32 / n, name.clone());
            if name.ends_with('/') {
                b.append_dir(name.trim_end_matches('/'), path)?;
            } else {
                b.append_path_with_name(path, name)?;
            }
        }
        Ok(())
    };
    let w: Box<dyn std::io::Write> = if gz { Box::new(flate2::write::GzEncoder::new(f, flate2::Compression::default())) } else { Box::new(f) };
    let mut b = tar::Builder::new(w);
    fill(&mut b)?;
    let mut w = b.into_inner()?;
    std::io::Write::flush(&mut w)?;
    drop(w);
    Ok(())
}

fn write_7z(dest: &Path, inputs: &[PathBuf], ctx: &Ctx) -> Result<()> {
    let z = ctx.caps.require(Dep::SevenZip)?;
    let mut c = exec::command(z);
    c.args(["a", "-t7z", "-y"]).arg(s(&abs(dest)));
    for p in inputs {
        c.arg(s(&abs(p)));
    }
    exec::run(&mut c, "7-Zip", ctx).map(|_| ())
}
