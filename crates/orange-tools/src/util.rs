//! Small helpers shared by the converters.

use orange_core::paths::{out_dir_for, stem_of, unique_path};
use orange_core::{Ctx, Error, Result};
use std::path::{Path, PathBuf};

/// Rename, falling back to copy + delete (temp dir may live on another drive).
pub(crate) fn move_file(from: &Path, to: &Path) -> Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to)?;
    let _ = std::fs::remove_file(from);
    Ok(())
}

/// Move a finished temp file next to the input (or into `ctx.out_dir`) as `<stem>.<ext>`, never overwriting.
pub(crate) fn finish_file(tmp: &Path, input: &Path, ext: &str, ctx: &Ctx) -> Result<PathBuf> {
    finish_named(tmp, input, &stem_of(input), ext, ctx)
}

pub(crate) fn finish_named(tmp: &Path, input: &Path, stem: &str, ext: &str, ctx: &Ctx) -> Result<PathBuf> {
    if !tmp.is_file() || std::fs::metadata(tmp)?.len() == 0 {
        return Err(Error::msg("the conversion produced no output"));
    }
    let dir = out_dir_for(input, ctx);
    std::fs::create_dir_all(&dir)?;
    let dest = unique_path(&dir, stem, ext);
    move_file(tmp, &dest)?;
    Ok(dest)
}

/// Sibling executable of a found tool (e.g. `pdftotext` next to `pdftoppm`).
pub(crate) fn sibling(tool: &Path, name: &str) -> Option<PathBuf> {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let p = tool.with_file_name(file);
    p.is_file().then_some(p)
}

pub(crate) fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// First file in `dir` with the given extension.
pub(crate) fn find_with_ext(dir: &Path, ext: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case(ext)).unwrap_or(false))
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Read a text file, tolerating a BOM and invalid UTF-8.
pub(crate) fn read_text(p: &Path) -> Result<String> {
    let b = std::fs::read(p)?;
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&b);
    Ok(String::from_utf8_lossy(b).into_owned())
}
