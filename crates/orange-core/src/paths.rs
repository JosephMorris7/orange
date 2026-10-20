//! Output naming. Orange never overwrites: "photo.png" then "photo 2.png", "photo 3.png"...

use crate::ctx::Ctx;
use std::path::{Path, PathBuf};

/// Directory outputs for `input` should land in.
pub fn out_dir_for(input: &Path, ctx: &Ctx) -> PathBuf {
    match &ctx.out_dir {
        Some(d) => d.clone(),
        None => input.parent().map(|p| p.to_path_buf()).filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| PathBuf::from(".")),
    }
}

pub fn stem_of(path: &Path) -> String {
    path.file_stem().and_then(|s| s.to_str()).unwrap_or("file").to_string()
}

/// First non-existing `dir/stem.ext`, `dir/stem 2.ext`, ...
pub fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    let name = |n: u32| {
        let s = if n <= 1 { stem.to_string() } else { format!("{stem} {n}") };
        if ext.is_empty() { s } else { format!("{s}.{ext}") }
    };
    let mut n = 1;
    loop {
        let p = dir.join(name(n));
        if !p.exists() {
            return p;
        }
        n += 1;
    }
}

/// `<out_dir>/<stem>[ <suffix>].<ext>`, uniquified.
pub fn output_path(input: &Path, suffix: Option<&str>, ext: &str, ctx: &Ctx) -> PathBuf {
    let dir = out_dir_for(input, ctx);
    let stem = match suffix {
        Some(s) if !s.is_empty() => format!("{} {}", stem_of(input), s),
        _ => stem_of(input),
    };
    unique_path(&dir, &stem, ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uniquifies() {
        let d = tempfile::tempdir().unwrap();
        let a = unique_path(d.path(), "x", "png");
        assert_eq!(a.file_name().unwrap(), "x.png");
        std::fs::write(&a, b"1").unwrap();
        let b = unique_path(d.path(), "x", "png");
        assert_eq!(b.file_name().unwrap(), "x 2.png");
    }
}
