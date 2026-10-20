//! LibreOffice (soffice) driver plus document / slide / spreadsheet conversions.

use crate::util::*;
use orange_core::{exec, Ctx, Dep, Error, Result};
use std::path::{Path, PathBuf};

fn file_url(p: &Path) -> String {
    let raw = s(p).replace('\\', "/");
    let mut enc = String::new();
    for ch in raw.chars() {
        match ch {
            ' ' => enc.push_str("%20"),
            '%' => enc.push_str("%25"),
            '#' => enc.push_str("%23"),
            _ => enc.push(ch),
        }
    }
    if enc.starts_with('/') {
        format!("file://{enc}")
    } else {
        format!("file:///{enc}")
    }
}

/// Run `soffice --convert-to`, with a throw-away profile so concurrent runs and locked profiles never interfere.
/// Returns the produced file (inside a temp dir owned by `work`).
pub(crate) fn soffice(ctx: &Ctx, input: &Path, infilter: Option<&str>, target: &str, out_ext: &str, work: &Path) -> Result<PathBuf> {
    let soffice = ctx.caps.require(Dep::Soffice)?;
    let profile = work.join("lo-profile");
    let outdir = work.join(format!("lo-out-{out_ext}"));
    std::fs::create_dir_all(&outdir)?;
    let mut cmd = exec::command(soffice);
    cmd.arg(format!("-env:UserInstallation={}", file_url(&profile)));
    cmd.args(["--headless", "--norestore", "--nolockcheck", "--nodefault", "--nologo"]);
    if let Some(f) = infilter {
        cmd.arg(format!("--infilter={f}"));
    }
    cmd.arg("--convert-to").arg(target).arg("--outdir").arg(&outdir).arg(input);
    let cap = exec::run(&mut cmd, "LibreOffice", ctx)?;
    find_with_ext(&outdir, out_ext).ok_or_else(|| Error::ToolFailed {
        tool: "LibreOffice".into(),
        detail: format!("no output produced. {}", cap.stderr.lines().chain(cap.stdout.lines()).rev().find(|l| !l.trim().is_empty()).unwrap_or("")),
    })
}

// ---------------------------------------------------------------- documents

fn pandoc_reader(from: &str) -> Option<&'static str> {
    Some(match from {
        "docx" => "docx",
        "md" => "markdown",
        "epub" => "epub",
        "html" => "html",
        "odt" => "odt",
        "txt" => "markdown+hard_line_breaks-smart",
        _ => return None,
    })
}

pub(crate) fn document(input: &Path, from: &str, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let tmp = ctx.temp_dir()?;
    let work = tmp.path();
    ctx.report(0.05, "Converting document");
    if to == "pdf" {
        let pdf = match from {
            "md" | "epub" => {
                // pandoc -> docx, then LibreOffice lays it out
                let docx = work.join("step.docx");
                pandoc(ctx, input, "docx", &docx, from, None)?;
                ctx.report(0.4, "Laying out pages");
                soffice(ctx, &docx, None, "pdf", "pdf", work)?
            }
            _ => soffice(ctx, input, None, "pdf", "pdf", work)?,
        };
        return Ok(vec![finish_file(&pdf, input, "pdf", ctx)?]);
    }
    let out = work.join(format!("out.{to}"));
    match pandoc_reader(from) {
        Some(_) => pandoc(ctx, input, to, &out, from, None)?,
        None => {
            // rtf / doc: LibreOffice reads them, pandoc (3.1) cannot
            if to == "docx" {
                let d = soffice(ctx, input, None, "docx:MS Word 2007 XML", "docx", work)?;
                return Ok(vec![finish_file(&d, input, "docx", ctx)?]);
            }
            let d = soffice(ctx, input, None, "docx:MS Word 2007 XML", "docx", work)?;
            ctx.check_cancel()?;
            let stem = orange_core::paths::stem_of(input);
            pandoc(ctx, &d, to, &out, "docx", Some(&stem))?;
        }
    }
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

fn pandoc(ctx: &Ctx, input: &Path, to: &str, out: &Path, from: &str, title: Option<&str>) -> Result<()> {
    let pandoc = ctx.caps.require(Dep::Pandoc)?;
    let reader = pandoc_reader(from).unwrap_or("docx");
    let mut cmd = exec::command(pandoc);
    cmd.arg("-f").arg(reader);
    match to {
        "docx" => cmd.args(["-t", "docx"]),
        "md" => cmd.args(["-t", "gfm", "--wrap=none"]),
        "txt" => cmd.args(["-t", "plain", "--wrap=none"]),
        "epub" => cmd.args(["-t", "epub"]),
        "html" => cmd.args(["-t", "html5", "-s"]),
        other => return Err(Error::Unsupported(format!("document format {other}"))),
    };
    if let Some(dir) = input.parent().filter(|p| !p.as_os_str().is_empty()) {
        cmd.arg("--resource-path").arg(dir);
    }
    if let Some(t) = title {
        cmd.arg("--metadata").arg(format!("title={t}"));
    } else if to == "html" {
        cmd.arg("--metadata").arg(format!("pagetitle={}", orange_core::paths::stem_of(input)));
    }
    cmd.arg(input).arg("-o").arg(out);
    exec::run(&mut cmd, "pandoc", ctx)?;
    Ok(())
}

// ---------------------------------------------------------------- slides

pub(crate) fn slides(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let tmp = ctx.temp_dir()?;
    ctx.report(0.05, "Rendering slides");
    let pdf = soffice(ctx, input, None, "pdf", "pdf", tmp.path())?;
    ctx.check_cancel()?;
    if to == "pdf" {
        return Ok(vec![finish_file(&pdf, input, "pdf", ctx)?]);
    }
    crate::pdf::rasterize(&pdf, input, to, ctx, 0.35, 0.65)
}

// ---------------------------------------------------------------- spreadsheets

const CSV_FILTER: &str = "Text - txt - csv (StarCalc)";

pub(crate) fn sheet(input: &Path, from: &str, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    if matches!((from, to), ("csv", "tsv") | ("tsv", "csv")) {
        return csv_tsv(input, from, to, ctx);
    }
    let tmp = ctx.temp_dir()?;
    ctx.report(0.05, "Converting spreadsheet");
    let infilter = match from {
        "csv" => Some(format!("{CSV_FILTER}:44,34,76,1")),
        "tsv" => Some(format!("{CSV_FILTER}:9,34,76,1")),
        _ => None,
    };
    let (target, ext) = match to {
        "xlsx" => ("xlsx:Calc MS Excel 2007 XML".to_string(), "xlsx"),
        "ods" => ("ods".to_string(), "ods"),
        "pdf" => ("pdf:calc_pdf_Export".to_string(), "pdf"),
        "csv" => (format!("csv:{CSV_FILTER}:44,34,76,1,,0,false,true,false"), "csv"),
        "tsv" => (format!("csv:{CSV_FILTER}:9,34,76,1,,0,false,true,false"), "csv"),
        other => return Err(Error::Unsupported(format!("spreadsheet format {other}"))),
    };
    let out = soffice(ctx, input, infilter.as_deref(), &target, ext, tmp.path())?;
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

pub(crate) fn parse_delimited(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' && field.is_empty() {
            in_q = true;
        } else if c == delim {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            any = false;
        } else {
            field.push(c);
        }
    }
    if any || !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows
}

pub(crate) fn write_delimited(rows: &[Vec<String>], delim: char) -> String {
    let mut out = String::new();
    for r in rows {
        let cells: Vec<String> = r
            .iter()
            .map(|f| {
                if f.contains(delim) || f.contains('"') || f.contains('\n') || f.contains('\r') {
                    format!("\"{}\"", f.replace('"', "\"\""))
                } else {
                    f.clone()
                }
            })
            .collect();
        out.push_str(&cells.join(&delim.to_string()));
        out.push('\n');
    }
    out
}

fn csv_tsv(input: &Path, from: &str, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let d = |e: &str| if e == "tsv" { '\t' } else { ',' };
    let rows = parse_delimited(&read_text(input)?, d(from));
    let tmp = ctx.temp_dir()?;
    let out = tmp.path().join(format!("out.{to}"));
    std::fs::write(&out, write_delimited(&rows, d(to)))?;
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_roundtrip() {
        let rows = parse_delimited("a,\"b,c\",\"d\"\"e\"\r\n1,2,3\n", ',');
        assert_eq!(rows, vec![vec!["a", "b,c", "d\"e"], vec!["1", "2", "3"]]);
        assert_eq!(write_delimited(&rows, '\t'), "a\tb,c\td\"e\n1\t2\t3\n".replace("d\"e", "\"d\"\"e\""));
    }
}
