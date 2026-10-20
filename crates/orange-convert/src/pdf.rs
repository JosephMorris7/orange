//! PDF as a source: raster pages (pdftoppm), text (pdftotext), Office formats (LibreOffice), EPUB (pandoc).

use crate::office::{soffice, write_delimited};
use crate::util::*;
use orange_core::paths::{out_dir_for, stem_of, unique_path};
use orange_core::{exec, Ctx, Dep, Error, Result};
use std::path::{Path, PathBuf};

fn page_count(pdf: &Path, ctx: &Ctx) -> Option<usize> {
    if let Some(info) = ctx.caps.get(Dep::Pdftoppm).and_then(|p| sibling(p, "pdfinfo")) {
        let mut c = exec::command(&info);
        c.arg(pdf);
        if let Ok(o) = exec::run(&mut c, "pdfinfo", ctx) {
            if let Some(n) = o.stdout.lines().find_map(|l| l.strip_prefix("Pages:")).and_then(|v| v.trim().parse().ok()) {
                return Some(n);
            }
        }
    }
    if let Some(q) = ctx.caps.get(Dep::Qpdf) {
        let mut c = exec::command(q);
        c.arg("--show-npages").arg(pdf);
        if let Ok(o) = exec::run(&mut c, "qpdf", ctx) {
            return o.stdout.trim().parse().ok();
        }
    }
    None
}

/// Rasterise every page at 300 dpi. One page -> `<name>.<ext>`; several -> folder `<name> PNG|JPG`
/// holding `<name> 01.png`... Returns every file created.
pub(crate) fn rasterize(pdf: &Path, naming_input: &Path, to: &str, ctx: &Ctx, base: f32, span: f32) -> Result<Vec<PathBuf>> {
    let pdftoppm = ctx.caps.require(Dep::Pdftoppm)?;
    let tmp = ctx.temp_dir()?;
    let prefix = tmp.path().join("pg");
    let flag = if to == "jpg" { "-jpeg" } else { "-png" };
    let make = |range: Option<usize>| {
        let mut c = exec::command(pdftoppm);
        c.args(["-r", "300", flag]);
        if to == "jpg" {
            c.args(["-jpegopt", "quality=92"]);
        }
        if let Some(p) = range {
            c.arg("-f").arg(p.to_string()).arg("-l").arg(p.to_string());
        }
        c.arg(pdf).arg(&prefix);
        c
    };
    match page_count(pdf, ctx) {
        Some(n) if n > 1 => {
            for p in 1..=n {
                ctx.check_cancel()?;
                ctx.report(base + span * (p - 1) as f32 / n as f32 * 0.95, format!("Rendering page {p} of {n}"));
                exec::run(&mut make(Some(p)), "pdftoppm", ctx)?;
            }
        }
        _ => {
            ctx.report(base, "Rendering pages");
            exec::run(&mut make(None), "pdftoppm", ctx)?;
        }
    }
    let ext = if to == "jpg" { "jpg" } else { "png" };
    let mut pages: Vec<(u64, PathBuf)> = std::fs::read_dir(tmp.path())?
        .flatten()
        .map(|e| e.path())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            let rest = name.strip_prefix("pg-")?;
            let num = rest.split('.').next()?.parse::<u64>().ok()?;
            let e = p.extension()?.to_str()?.to_ascii_lowercase();
            (e == "png" || e == "jpg" || e == "jpeg").then_some((num, p))
        })
        .collect();
    pages.sort();
    if pages.is_empty() {
        return Err(Error::msg("no pages were rendered"));
    }
    let stem = stem_of(naming_input);
    let dir = out_dir_for(naming_input, ctx);
    std::fs::create_dir_all(&dir)?;
    let mut outs = Vec::new();
    if pages.len() == 1 {
        outs.push(finish_named(&pages[0].1, naming_input, &stem, ext, ctx)?);
    } else {
        let folder = unique_path(&dir, &format!("{stem} {}", ext.to_uppercase()), "");
        std::fs::create_dir_all(&folder)?;
        let width = pages.len().to_string().len().max(2);
        for (i, (_, p)) in pages.iter().enumerate() {
            let dest = folder.join(format!("{stem} {:0width$}.{ext}", i + 1));
            move_file(p, &dest)?;
            outs.push(dest);
        }
    }
    ctx.report(base + span, "Done");
    Ok(outs)
}

fn pdftotext(input: &Path, layout: bool, ctx: &Ctx, out: &Path) -> Result<()> {
    let tool = ctx.caps.require(Dep::Pdftoppm)?;
    let t = sibling(tool, "pdftotext").ok_or_else(|| Error::MissingDep("poppler (pdftotext)".into()))?;
    let mut c = exec::command(&t);
    c.args(["-enc", "UTF-8"]);
    if layout {
        c.arg("-layout");
    }
    c.arg(input).arg(out);
    exec::run(&mut c, "pdftotext", ctx)?;
    Ok(())
}

/// Heuristic plain text -> Markdown: joins wrapped lines, marks bullets, page breaks become rules.
pub(crate) fn text_to_markdown(text: &str) -> String {
    let mut out = String::new();
    for (pi, page) in text.split('\u{c}').enumerate() {
        if page.trim().is_empty() {
            continue;
        }
        if pi > 0 && !out.is_empty() {
            out.push_str("---\n\n");
        }
        for block in page.replace("\r\n", "\n").split("\n\n") {
            let lines: Vec<&str> = block.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
            if lines.is_empty() {
                continue;
            }
            let is_bullet = |l: &str| l.starts_with('\u{2022}') || l.starts_with("- ") || l.starts_with("* ");
            if lines.iter().all(|l| is_bullet(l)) {
                for l in &lines {
                    out.push_str(&format!("- {}\n", l.trim_start_matches(['\u{2022}', '-', '*']).trim()));
                }
                out.push('\n');
                continue;
            }
            let mut para = String::new();
            for l in &lines {
                if para.is_empty() {
                    para.push_str(l);
                } else if para.ends_with('-') && l.chars().next().map(|c| c.is_lowercase()).unwrap_or(false) {
                    para.pop();
                    para.push_str(l);
                } else {
                    para.push(' ');
                    para.push_str(l);
                }
            }
            let letters: Vec<char> = para.chars().filter(|c| c.is_alphabetic()).collect();
            let heading = lines.len() == 1 && para.chars().count() < 70 && letters.len() > 3 && letters.iter().all(|c| c.is_uppercase());
            if heading {
                out.push_str(&format!("## {para}\n\n"));
            } else {
                out.push_str(&para);
                out.push_str("\n\n");
            }
        }
    }
    out
}

pub(crate) fn convert(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let tmp = ctx.temp_dir()?;
    let work = tmp.path();
    ctx.report(0.05, "Reading PDF");
    match to {
        "jpg" | "png" => rasterize(input, input, to, ctx, 0.0, 1.0),
        "docx" => {
            let o = soffice(ctx, input, Some("writer_pdf_import"), "docx:MS Word 2007 XML", "docx", work)?;
            Ok(vec![finish_file(&o, input, "docx", ctx)?])
        }
        "pptx" => {
            let o = soffice(ctx, input, Some("impress_pdf_import"), "pptx", "pptx", work)?;
            Ok(vec![finish_file(&o, input, "pptx", ctx)?])
        }
        "txt" => {
            let out = work.join("out.txt");
            match pdftotext(input, true, ctx, &out) {
                Ok(()) => {}
                Err(Error::MissingDep(_)) if ctx.caps.has(Dep::Soffice) => {
                    let o = soffice(ctx, input, Some("writer_pdf_import"), "txt:Text", "txt", work)?;
                    std::fs::copy(o, &out)?;
                }
                Err(e) => return Err(e),
            }
            Ok(vec![finish_file(&out, input, "txt", ctx)?])
        }
        "md" => {
            let txt = work.join("raw.txt");
            pdftotext(input, false, ctx, &txt)?;
            let md = text_to_markdown(&read_text(&txt)?);
            let out = work.join("out.md");
            std::fs::write(&out, md)?;
            Ok(vec![finish_file(&out, input, "md", ctx)?])
        }
        "epub" => {
            let pandoc = ctx.caps.require(Dep::Pandoc)?;
            let txt = work.join("raw.txt");
            pdftotext(input, false, ctx, &txt)?;
            ctx.check_cancel()?;
            let md_path = work.join("book.md");
            std::fs::write(&md_path, text_to_markdown(&read_text(&txt)?))?;
            let out = work.join("out.epub");
            let mut c = exec::command(pandoc);
            c.args(["-f", "markdown-smart", "-t", "epub", "--metadata"]).arg(format!("title={}", stem_of(input)));
            c.arg(&md_path).arg("-o").arg(&out);
            exec::run(&mut c, "pandoc", ctx)?;
            Ok(vec![finish_file(&out, input, "epub", ctx)?])
        }
        "xlsx" => {
            // Table-ish extraction: layout text, columns split on runs of 2+ spaces, then LibreOffice builds the workbook.
            let txt = work.join("layout.txt");
            pdftotext(input, true, ctx, &txt)?;
            ctx.check_cancel()?;
            let mut rows: Vec<Vec<String>> = Vec::new();
            for line in read_text(&txt)?.replace('\u{c}', "\n").lines() {
                let mut cells = Vec::new();
                let mut cur = String::new();
                let mut spaces = 0;
                for ch in line.trim().chars() {
                    if ch == ' ' {
                        spaces += 1;
                        continue;
                    }
                    if spaces >= 2 {
                        cells.push(std::mem::take(&mut cur));
                    } else if spaces == 1 {
                        cur.push(' ');
                    }
                    spaces = 0;
                    cur.push(ch);
                }
                cells.push(cur);
                rows.push(cells);
            }
            let csv = work.join("table.csv");
            std::fs::write(&csv, write_delimited(&rows, '\t'))?;
            let o = soffice(ctx, &csv, Some("Text - txt - csv (StarCalc):9,34,76,1"), "xlsx:Calc MS Excel 2007 XML", "xlsx", work)?;
            Ok(vec![finish_file(&o, input, "xlsx", ctx)?])
        }
        other => Err(Error::Unsupported(format!("PDF -> {other}"))),
    }
}
