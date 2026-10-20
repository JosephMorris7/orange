//! Archive conversions (zip / tar / gz, rar and 7z as input) through 7-Zip.

use crate::util::*;
use orange_core::paths::{out_dir_for, stem_of, unique_path};
use orange_core::{exec, Ctx, Dep, Error, Result};
use std::path::{Path, PathBuf};

fn extract(sz: &Path, archive: &Path, dest: &Path, ctx: &Ctx) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let mut c = exec::command(sz);
    c.args(["x", "-y", "-bd"]).arg(format!("-o{}", s(dest))).arg(archive);
    exec::run(&mut c, "7-Zip", ctx)?;
    Ok(())
}

pub(crate) fn convert(input: &Path, from: &str, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let sz = ctx.caps.require(Dep::SevenZip)?;
    let tmp = ctx.temp_dir()?;
    let work = tmp.path();
    ctx.report(0.05, "Reading archive");
    let mut stem = stem_of(input);
    if let Some(s) = stem.strip_suffix(".tar") {
        stem = s.to_string();
    }
    let dir = out_dir_for(input, ctx);
    std::fs::create_dir_all(&dir)?;

    // tar -> gz is just compressing the tar
    let (ext, built): (&str, PathBuf) = if from == "tar" && to == "gz" {
        let out = work.join("out.tar.gz");
        let mut c = exec::command(sz);
        c.args(["a", "-tgzip", "-y", "-bd"]).arg(&out).arg(input);
        exec::run(&mut c, "7-Zip", ctx)?;
        ("tar.gz", out)
    } else {
        let mut root = work.join("x1");
        extract(sz, input, &root, ctx)?;
        ctx.check_cancel()?;
        // .tar.gz / .tgz unpack to a lone .tar: unpack that too
        let entries: Vec<PathBuf> = std::fs::read_dir(&root)?.flatten().map(|e| e.path()).collect();
        if entries.len() == 1 && entries[0].is_file() && ext_is(&entries[0], "tar") {
            let r2 = work.join("x2");
            extract(sz, &entries[0], &r2, ctx)?;
            root = r2;
        }
        ctx.report(0.5, "Packing");
        let pack = |fmt: &str, out: &Path| -> Result<()> {
            let mut c = exec::command(sz);
            c.args(["a", &format!("-t{fmt}"), "-y", "-bd"]).arg(out).arg("*").current_dir(&root);
            exec::run(&mut c, "7-Zip", ctx)?;
            Ok(())
        };
        match to {
            "zip" => {
                let out = work.join("out.zip");
                pack("zip", &out)?;
                ("zip", out)
            }
            "tar" => {
                let out = work.join("out.tar");
                pack("tar", &out)?;
                ("tar", out)
            }
            "gz" => {
                let tar = work.join("mid.tar");
                pack("tar", &tar)?;
                ctx.check_cancel()?;
                let out = work.join("out.tar.gz");
                let mut c = exec::command(sz);
                c.args(["a", "-tgzip", "-y", "-bd"]).arg(&out).arg(&tar);
                exec::run(&mut c, "7-Zip", ctx)?;
                ("tar.gz", out)
            }
            other => return Err(Error::Unsupported(format!("archive format {other}"))),
        }
    };
    let dest = unique_path(&dir, &stem, ext);
    if !built.is_file() {
        return Err(Error::msg("the archive was not created"));
    }
    move_file(&built, &dest)?;
    Ok(vec![dest])
}

fn ext_is(p: &Path, e: &str) -> bool {
    p.extension().and_then(|x| x.to_str()).map(|x| x.eq_ignore_ascii_case(e)).unwrap_or(false)
}
