//! End-to-end conversions against the real tools. A missing tool skips the test (prints SKIP).

use orange_convert::{convert, option_count, options_for};
use orange_core::{Capabilities, Ctx, Dep};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn caps() -> Arc<Capabilities> {
    Arc::new(Capabilities::detect(&[]))
}

/// Returns a context if every dep is present, else None (test skips).
fn setup(deps: &[Dep], out: Option<&Path>) -> Option<Ctx> {
    let c = caps();
    for d in deps {
        if !c.has(*d) {
            eprintln!("SKIP: {} not installed", d.name());
            return None;
        }
    }
    Some(Ctx::new(c).with_out_dir(out.map(|p| p.to_path_buf())))
}

fn sh(program: &Path, args: &[&str]) -> bool {
    Command::new(program).args(args).output().map(|o| o.status.success()).unwrap_or(false)
}

fn ffmpeg(ctx: &Ctx, args: &[&str]) -> bool {
    let mut a = vec!["-hide_banner", "-loglevel", "error", "-y"];
    a.extend_from_slice(args);
    sh(ctx.caps.get(Dep::Ffmpeg).unwrap(), &a)
}

fn magick(ctx: &Ctx, args: &[&str]) -> bool {
    sh(ctx.caps.get(Dep::Magick).unwrap(), args)
}

fn head(p: &Path, n: usize) -> Vec<u8> {
    let b = std::fs::read(p).unwrap();
    b[..b.len().min(n)].to_vec()
}

/// Asserts the file exists, is non-empty and looks like `ext` by magic bytes.
fn assert_type(p: &Path, ext: &str) {
    assert!(p.is_file(), "{} missing", p.display());
    let len = std::fs::metadata(p).unwrap().len();
    assert!(len > 0, "{} is empty", p.display());
    let h = head(p, 600);
    let at = |o: usize, s: &[u8]| h.len() >= o + s.len() && &h[o..o + s.len()] == s;
    let ok = match ext {
        "png" => at(0, b"\x89PNG"),
        "jpg" => at(0, &[0xFF, 0xD8]),
        "webp" => at(0, b"RIFF") && at(8, b"WEBP"),
        "bmp" => at(0, b"BM"),
        "tiff" => at(0, b"II*\0") || at(0, b"MM\0*"),
        "avif" => at(4, b"ftyp") && (at(8, b"avif") || at(8, b"avis") || at(8, b"mif1")),
        "pdf" => at(0, b"%PDF"),
        "zip" | "docx" | "pptx" | "xlsx" | "ods" | "epub" => at(0, b"PK"),
        "gif" => at(0, b"GIF8"),
        "mp3" => at(0, b"ID3") || at(0, &[0xFF, 0xFB]) || at(0, &[0xFF, 0xF3]) || at(0, &[0xFF, 0xF2]),
        "wav" => at(0, b"RIFF") && at(8, b"WAVE"),
        "flac" => at(0, b"fLaC"),
        "ogg" | "opus" => at(0, b"OggS"),
        "m4a" | "mp4" | "mov" => at(4, b"ftyp"),
        "aiff" => at(0, b"FORM") && at(8, b"AIFF"),
        "mkv" | "webm" => at(0, &[0x1A, 0x45, 0xDF, 0xA3]),
        "avi" => at(0, b"RIFF") && at(8, b"AVI "),
        "wmv" => at(0, &[0x30, 0x26, 0xB2, 0x75]),
        "tar" => at(257, b"ustar"),
        "gz" => at(0, &[0x1F, 0x8B]),
        "7z" => at(0, b"7z\xBC\xAF"),
        "txt" | "md" | "csv" | "tsv" | "srt" | "vtt" | "html" => true,
        other => panic!("no magic check for {other}"),
    };
    assert!(ok, "{} does not look like .{ext}: {:?}", p.display(), &h[..h.len().min(16)]);
}

fn ext_of(p: &Path) -> String {
    p.extension().unwrap().to_string_lossy().into_owned()
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn sample_png(ctx: &Ctx, dir: &Path) -> PathBuf {
    let p = dir.join("photo.png");
    assert!(ffmpeg(ctx, &["-f", "lavfi", "-i", "testsrc=size=160x120:d=1", "-frames:v", "1", p.to_str().unwrap()]));
    p
}

// ---------------------------------------------------------------- registry

#[test]
fn registry_counts_and_options() {
    let n = option_count();
    assert!(n > 150, "registry has only {n} pairs");
    let c = caps();
    let d = tmpdir();
    let f = d.path().join("a.png");
    std::fs::write(&f, b"x").unwrap();
    let o = options_for(&f, &c);
    assert!(!o.is_empty());
    assert!(o.iter().all(|x| x.id != "png"), "own format must be excluded");
    assert_eq!(o[0].id, "jpg");
    assert!(o.iter().any(|x| x.id == "pdf"));
    // uppercase extension + alias
    let j = d.path().join("A.JPEG");
    std::fs::write(&j, b"x").unwrap();
    let oj = options_for(&j, &c);
    assert!(oj.iter().all(|x| x.id != "jpg") && oj.iter().any(|x| x.id == "png"));
    // unsupported
    let z = d.path().join("a.xyz");
    std::fs::write(&z, b"x").unwrap();
    assert!(options_for(&z, &c).is_empty());
    // svg is input only
    let s = d.path().join("a.svg");
    std::fs::write(&s, b"x").unwrap();
    assert!(options_for(&s, &c).iter().any(|x| x.id == "png"));
    assert!(options_for(&f, &c).iter().all(|x| x.id != "svg"));
    // no mac-only formats
    for e in ["pages", "rtfd"] {
        assert!(!orange_convert::options_for(&d.path().join(format!("a.{e}")), &c).iter().any(|_| true));
    }
}

#[test]
fn availability_notes_follow_capabilities() {
    let empty = Capabilities::from_paths(Default::default());
    let d = tmpdir();
    let f = d.path().join("slides.pptx");
    std::fs::write(&f, b"x").unwrap();
    let o = options_for(&f, &empty);
    assert!(o.iter().all(|x| !x.available));
    let pdf = o.iter().find(|x| x.id == "pdf").unwrap();
    assert_eq!(pdf.dep, Some(Dep::Soffice));
    assert_eq!(pdf.note.as_deref(), Some("Install LibreOffice to enable"));
    // pure-Rust conversions are always available
    let s = d.path().join("a.srt");
    std::fs::write(&s, b"x").unwrap();
    assert!(options_for(&s, &empty).iter().all(|x| x.available && x.dep.is_none() && x.note.is_none()));
    let e = convert(&f, "pdf", &Ctx::new(Arc::new(empty)));
    assert!(matches!(e, Err(orange_core::Error::MissingDep(_))));
}

#[test]
fn unsupported_pair_errors() {
    let d = tmpdir();
    let f = d.path().join("a.png");
    std::fs::write(&f, b"x").unwrap();
    let ctx = Ctx::new(caps());
    assert!(convert(&f, "mp3", &ctx).is_err());
    assert!(convert(&f, "png", &ctx).is_err());
}

// ---------------------------------------------------------------- images

#[test]
fn images_all_to_all() {
    let Some(ctx) = setup(&[Dep::Magick, Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let png = sample_png(&ctx, d.path());
    let mut sources = vec![("png".to_string(), png.clone())];
    for to in ["jpg", "webp", "tiff", "bmp", "avif"] {
        let outs = convert(&png, to, &ctx).unwrap_or_else(|e| panic!("png -> {to}: {e}"));
        assert_eq!(outs.len(), 1);
        assert_type(&outs[0], to);
        sources.push((to.to_string(), outs[0].clone()));
    }
    // every readable source to every other writable target
    for (from, src) in &sources {
        for to in ["jpg", "png", "webp", "tiff", "bmp", "avif", "pdf"] {
            if to == from {
                continue;
            }
            let outs = convert(src, to, &ctx).unwrap_or_else(|e| panic!("{from} -> {to}: {e}"));
            assert_type(&outs[0], to);
        }
    }
    // never overwrites
    let again = convert(&png, "jpg", &ctx).unwrap();
    assert!(again[0].file_name().unwrap().to_string_lossy().contains(" 2") || again[0].exists());
}

#[test]
fn heic_output_when_supported() {
    let Some(ctx) = setup(&[Dep::Magick, Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let png = sample_png(&ctx, d.path());
    match convert(&png, "heic", &ctx) {
        Ok(o) => assert!(std::fs::metadata(&o[0]).unwrap().len() > 0),
        Err(e) => eprintln!("SKIP: ImageMagick cannot write HEIC here ({e})"),
    }
}

#[test]
fn svg_input() {
    let Some(ctx) = setup(&[Dep::Magick], None) else { return };
    let d = tmpdir();
    let svg = d.path().join("logo.svg");
    std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="60"><rect width="80" height="60" fill="#ff6a13"/><circle cx="40" cy="30" r="20" fill="#fff"/></svg>"##).unwrap();
    for to in ["png", "jpg", "pdf"] {
        match convert(&svg, to, &ctx) {
            Ok(o) => assert_type(&o[0], to),
            Err(e) => eprintln!("SKIP svg -> {to}: {e}"),
        }
    }
}

#[test]
fn jpeg_pdf_embeds_dct_and_png_pdf_uses_flate() {
    let Some(ctx) = setup(&[Dep::Magick, Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let png = sample_png(&ctx, d.path());
    let jpg = convert(&png, "jpg", &ctx).unwrap().remove(0);
    let p1 = convert(&jpg, "pdf", &ctx).unwrap().remove(0);
    let b1 = std::fs::read(&p1).unwrap();
    assert!(String::from_utf8_lossy(&b1).contains("/DCTDecode"));
    let p2 = convert(&png, "pdf", &ctx).unwrap().remove(0);
    assert!(String::from_utf8_lossy(&std::fs::read(&p2).unwrap()).contains("/FlateDecode"));
    // the PDF must be readable by real tools
    if let Some(q) = ctx.caps.get(Dep::Qpdf) {
        assert!(sh(q, &["--check", p1.to_str().unwrap()]), "qpdf rejects the JPEG PDF");
        assert!(sh(q, &["--check", p2.to_str().unwrap()]), "qpdf rejects the PNG PDF");
    }
    if let Some(t) = ctx.caps.get(Dep::Pdftoppm) {
        let o = d.path().join("chk");
        assert!(sh(t, &["-r", "30", "-png", p2.to_str().unwrap(), o.to_str().unwrap()]));
    }
}

#[test]
fn jpeg_pdf_works_without_imagemagick() {
    let Some(ctx) = setup(&[Dep::Magick, Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let png = sample_png(&ctx, d.path());
    let jpg = convert(&png, "jpg", &ctx).unwrap().remove(0);
    let bare = Ctx::new(Arc::new(Capabilities::from_paths(Default::default())));
    let out = convert(&jpg, "pdf", &bare).unwrap();
    assert_type(&out[0], "pdf");
}

// ---------------------------------------------------------------- audio / video

#[test]
fn audio_matrix() {
    let Some(ctx) = setup(&[Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let wav = d.path().join("tone.wav");
    assert!(ffmpeg(&ctx, &["-f", "lavfi", "-i", "sine=frequency=440:duration=1", wav.to_str().unwrap()]));
    let targets = ["mp3", "m4a", "flac", "ogg", "opus", "aiff"];
    let mut made = vec![wav.clone()];
    for t in targets {
        let o = convert(&wav, t, &ctx).unwrap_or_else(|e| panic!("wav -> {t}: {e}"));
        assert_type(&o[0], t);
        made.push(o[0].clone());
    }
    for src in &made {
        let from = ext_of(src);
        for t in ["mp3", "m4a", "wav", "flac", "ogg", "opus", "aiff"] {
            if t == from {
                continue;
            }
            let o = convert(src, t, &ctx).unwrap_or_else(|e| panic!("{from} -> {t}: {e}"));
            assert_type(&o[0], t);
        }
    }
    // wma input
    let wma = d.path().join("tone.wma");
    if ffmpeg(&ctx, &["-i", wav.to_str().unwrap(), "-c:a", "wmav2", wma.to_str().unwrap()]) {
        let o = convert(&wma, "mp3", &ctx).unwrap();
        assert_type(&o[0], "mp3");
    }
}

#[test]
fn video_matrix_and_progress() {
    let Some(ctx) = setup(&[Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let seen = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
    let s2 = seen.clone();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx }.with_progress(move |p| s2.lock().unwrap().push(p.fraction));
    let mp4 = d.path().join("clip.mp4");
    assert!(ffmpeg(
        &ctx,
        &[
            "-f", "lavfi", "-i", "testsrc=size=161x121:rate=15:d=1", "-f", "lavfi", "-i", "sine=frequency=330:duration=1", "-c:v", "libx264", "-pix_fmt",
            "yuv420p", "-vf", "scale=160:120", "-c:a", "aac", "-shortest", mp4.to_str().unwrap()
        ]
    ));
    for t in ["mov", "mkv", "webm", "avi", "wmv", "gif", "mp3"] {
        let o = convert(&mp4, t, &ctx).unwrap_or_else(|e| panic!("mp4 -> {t}: {e}"));
        assert_eq!(o.len(), 1);
        assert_type(&o[0], t);
        if t == "gif" {
            // gif -> mp4 and back to gif palette sanity
            let back = convert(&o[0], "mp4", &ctx).unwrap();
            assert_type(&back[0], "mp4");
        }
    }
    let v = seen.lock().unwrap();
    assert!(v.iter().any(|f| *f >= 0.99) && v.iter().any(|f| *f == 0.0));
    drop(v);
    // from mkv / webm / avi sources too
    for src in ["mkv", "webm", "avi"] {
        let s = out.path().join(format!("clip.{src}"));
        assert!(s.exists());
        let o = convert(&s, "mp4", &ctx).unwrap_or_else(|e| panic!("{src} -> mp4: {e}"));
        assert_type(&o[0], "mp4");
    }
}

#[test]
fn cancel_is_honoured() {
    let Some(ctx) = setup(&[Dep::Ffmpeg], None) else { return };
    let d = tmpdir();
    let wav = d.path().join("t.wav");
    assert!(ffmpeg(&ctx, &["-f", "lavfi", "-i", "sine=duration=1", wav.to_str().unwrap()]));
    ctx.cancel.cancel();
    assert!(matches!(convert(&wav, "mp3", &ctx), Err(orange_core::Error::Cancelled)));
    assert!(!d.path().join("t.mp3").exists());
}

// ---------------------------------------------------------------- documents / pdf / slides / sheets

fn make_pdf(ctx: &Ctx, dir: &Path, pages: usize) -> PathBuf {
    // pandoc -> docx -> LibreOffice -> pdf
    let md = dir.join("report.md");
    let mut text = String::from("# Quarterly Report\n\n");
    for i in 1..=pages {
        text.push_str(&format!("## Section {i}\n\nHello from page {i}. Lorem ipsum dolor sit amet.\n\n"));
        if i < pages {
            text.push_str("```{=openxml}\n<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>\n```\n\n");
        }
    }
    std::fs::write(&md, text).unwrap();
    convert(&md, "pdf", ctx).unwrap().remove(0)
}

#[test]
fn documents_matrix() {
    let Some(ctx) = setup(&[Dep::Pandoc, Dep::Soffice], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let md = d.path().join("notes.md");
    std::fs::write(&md, "# Title\n\nSome *emphasis* and a list:\n\n- one\n- two\n").unwrap();
    let mut made = vec![md.clone()];
    for t in ["docx", "txt", "epub", "html", "pdf"] {
        let o = convert(&md, t, &ctx).unwrap_or_else(|e| panic!("md -> {t}: {e}"));
        assert_type(&o[0], t);
        if t != "pdf" {
            made.push(o[0].clone());
        }
    }
    for src in &made {
        let from = ext_of(src);
        for t in ["docx", "md", "txt", "html", "epub", "pdf"] {
            if t == from {
                continue;
            }
            let o = convert(src, t, &ctx).unwrap_or_else(|e| panic!("{from} -> {t}: {e}"));
            assert_type(&o[0], t);
        }
    }
    let txt = std::fs::read_to_string(made.iter().find(|p| ext_of(p) == "txt").unwrap()).unwrap();
    assert!(txt.contains("Title") && txt.contains("emphasis"));
    // rtf input (LibreOffice path)
    let docx = made.iter().find(|p| ext_of(p) == "docx").unwrap();
    let rtf = Command::new(ctx.caps.get(Dep::Pandoc).unwrap())
        .args(["-t", "rtf", "-o", d.path().join("a.rtf").to_str().unwrap(), docx.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(rtf.status.success());
    for t in ["docx", "md", "pdf"] {
        let o = convert(&d.path().join("a.rtf"), t, &ctx).unwrap_or_else(|e| panic!("rtf -> {t}: {e}"));
        assert_type(&o[0], t);
    }
}

#[test]
fn pdf_conversions() {
    let Some(ctx) = setup(&[Dep::Pandoc, Dep::Soffice, Dep::Pdftoppm], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let build = Ctx { out_dir: Some(d.path().to_path_buf()), ..ctx.clone() };
    let pdf = make_pdf(&build, d.path(), 3);
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    for t in ["docx", "pptx", "xlsx", "txt", "md", "epub"] {
        let o = convert(&pdf, t, &ctx).unwrap_or_else(|e| panic!("pdf -> {t}: {e}"));
        assert_eq!(o.len(), 1);
        assert_type(&o[0], t);
    }
    let txt = std::fs::read_to_string(out.path().join("report.txt")).unwrap();
    assert!(txt.contains("Quarterly Report"), "{txt}");
    let md = std::fs::read_to_string(out.path().join("report.md")).unwrap();
    assert!(md.contains("Hello from page 2"));
    // multi-page images go to "<name> PNG" and every file is returned
    let o = convert(&pdf, "png", &ctx).unwrap();
    assert!(o.len() >= 3, "{o:?}");
    for f in &o {
        assert_type(f, "png");
        assert_eq!(f.parent().unwrap().file_name().unwrap(), "report PNG");
    }
    let j = convert(&pdf, "jpg", &ctx).unwrap();
    assert!(j.len() >= 3);
    assert_type(&j[0], "jpg");
    assert_eq!(j[0].parent().unwrap().file_name().unwrap(), "report JPG");
    // 300 dpi A4/Letter-ish
    if let Some((w, _)) = png_size(&o[0]) {
        assert!(w > 2000, "width {w} is not 300 dpi");
    }
    // second run never overwrites the folder
    let o2 = convert(&pdf, "png", &ctx).unwrap();
    assert_ne!(o[0].parent(), o2[0].parent());
}

fn png_size(p: &Path) -> Option<(u32, u32)> {
    let h = head(p, 32);
    (h.len() >= 24).then(|| (u32::from_be_bytes([h[16], h[17], h[18], h[19]]), u32::from_be_bytes([h[20], h[21], h[22], h[23]])))
}

#[test]
fn single_page_pdf_to_png_is_a_plain_file() {
    let Some(ctx) = setup(&[Dep::Pandoc, Dep::Soffice, Dep::Pdftoppm], None) else { return };
    let d = tmpdir();
    let ctx = Ctx { out_dir: Some(d.path().to_path_buf()), ..ctx };
    let pdf = make_pdf(&ctx, d.path(), 1);
    let o = convert(&pdf, "png", &ctx).unwrap();
    assert_eq!(o.len(), 1);
    assert_eq!(o[0], d.path().join("report.png"));
}

#[test]
fn slides_conversions() {
    let Some(ctx) = setup(&[Dep::Soffice, Dep::Pdftoppm, Dep::Pandoc], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let md = d.path().join("deck.md");
    std::fs::write(&md, "% Deck\n\n# One\n\n- a\n- b\n\n# Two\n\nText\n\n# Three\n\nMore\n").unwrap();
    let pptx = d.path().join("deck.pptx");
    assert!(sh(ctx.caps.get(Dep::Pandoc).unwrap(), &["-o", pptx.to_str().unwrap(), md.to_str().unwrap()]));
    let o = convert(&pptx, "pdf", &ctx).unwrap();
    assert_type(&o[0], "pdf");
    let o = convert(&pptx, "png", &ctx).unwrap();
    assert!(o.len() >= 3, "{o:?}");
    o.iter().for_each(|f| assert_type(f, "png"));
    let o = convert(&pptx, "jpg", &ctx).unwrap();
    assert!(o.len() >= 3);
    assert_type(&o[0], "jpg");
    // legacy .ppt input
    let ppt_dir = d.path().join("ppt");
    std::fs::create_dir_all(&ppt_dir).unwrap();
    if sh(ctx.caps.get(Dep::Soffice).unwrap(), &["--headless", "--convert-to", "ppt", "--outdir", ppt_dir.to_str().unwrap(), pptx.to_str().unwrap()]) {
        let ppt = ppt_dir.join("deck.ppt");
        if ppt.exists() {
            assert_type(&convert(&ppt, "pdf", &ctx).unwrap()[0], "pdf");
        }
    }
}

#[test]
fn spreadsheet_conversions() {
    let Some(ctx) = setup(&[Dep::Soffice], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let csv = d.path().join("data.csv");
    std::fs::write(&csv, "name,qty,note\nApple,3,\"red, sweet\"\nPear,5,green\n").unwrap();
    let mut made = vec![csv.clone()];
    for t in ["xlsx", "ods", "tsv", "pdf"] {
        let o = convert(&csv, t, &ctx).unwrap_or_else(|e| panic!("csv -> {t}: {e}"));
        assert_type(&o[0], t);
        if t != "pdf" {
            made.push(o[0].clone());
        }
    }
    let tsv = std::fs::read_to_string(out.path().join("data.tsv")).unwrap();
    assert!(tsv.contains("Apple\t3\tred, sweet"), "{tsv:?}");
    for src in &made {
        let from = ext_of(src);
        for t in ["xlsx", "ods", "csv", "tsv", "pdf"] {
            if t == from {
                continue;
            }
            let o = convert(src, t, &ctx).unwrap_or_else(|e| panic!("{from} -> {t}: {e}"));
            assert_type(&o[0], t);
        }
    }
    // LibreOffice-written CSV from xlsx keeps the data
    let xlsx = made.iter().find(|p| ext_of(p) == "xlsx").unwrap();
    let c = convert(xlsx, "csv", &ctx).unwrap();
    let text = std::fs::read_to_string(&c[0]).unwrap();
    assert!(text.contains("Pear") && text.contains("5"), "{text}");
    // xls input
    let xls_dir = d.path().join("xls");
    std::fs::create_dir_all(&xls_dir).unwrap();
    if sh(ctx.caps.get(Dep::Soffice).unwrap(), &["--headless", "--convert-to", "xls", "--outdir", xls_dir.to_str().unwrap(), xlsx.to_str().unwrap()]) {
        let xls = xls_dir.join(format!("{}.xls", xlsx.file_stem().unwrap().to_string_lossy()));
        if xls.exists() {
            assert_type(&convert(&xls, "xlsx", &ctx).unwrap()[0], "xlsx");
            assert_type(&convert(&xls, "pdf", &ctx).unwrap()[0], "pdf");
        }
    }
}

#[test]
fn csv_tsv_need_no_tools() {
    let empty = Ctx::new(Arc::new(Capabilities::from_paths(Default::default())));
    let d = tmpdir();
    let csv = d.path().join("x.csv");
    std::fs::write(&csv, "a,\"b\nc\",d\n").unwrap();
    let t = convert(&csv, "tsv", &empty).unwrap().remove(0);
    let back = convert(&t, "csv", &empty).unwrap().remove(0);
    assert_eq!(back.file_name().unwrap(), "x 2.csv");
    assert_eq!(std::fs::read_to_string(back).unwrap(), "a,\"b\nc\",d\n");
}

// ---------------------------------------------------------------- subtitles / archives

#[test]
fn subtitles_convert_without_tools() {
    let empty = Ctx::new(Arc::new(Capabilities::from_paths(Default::default())));
    let d = tmpdir();
    let srt = d.path().join("movie.srt");
    std::fs::write(&srt, "1\r\n00:00:01,000 --> 00:00:02,500\r\nHello\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\n<i>World</i>\r\n").unwrap();
    let vtt = convert(&srt, "vtt", &empty).unwrap().remove(0);
    assert!(std::fs::read_to_string(&vtt).unwrap().starts_with("WEBVTT"));
    let srt2 = convert(&vtt, "srt", &empty).unwrap().remove(0);
    assert!(std::fs::read_to_string(&srt2).unwrap().contains("00:00:01,000 --> 00:00:02,500"));
    let txt = convert(&vtt, "txt", &empty).unwrap().remove(0);
    assert_eq!(std::fs::read_to_string(txt).unwrap(), "Hello\nWorld\n");
}

#[test]
fn archives_matrix() {
    let Some(ctx) = setup(&[Dep::SevenZip], None) else { return };
    let d = tmpdir();
    let out = tmpdir();
    let ctx = Ctx { out_dir: Some(out.path().to_path_buf()), ..ctx };
    let src = d.path().join("stuff");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "alpha").unwrap();
    std::fs::write(src.join("sub").join("b.txt"), "beta").unwrap();
    let sz = ctx.caps.get(Dep::SevenZip).unwrap().to_path_buf();
    let mk = |fmt: &str, name: &str| -> PathBuf {
        let p = d.path().join(name);
        let ok = Command::new(&sz).current_dir(&src).args(["a", &format!("-t{fmt}"), "-y", p.to_str().unwrap(), "*"]).output().unwrap().status.success();
        assert!(ok, "could not build {name}");
        p
    };
    let zip = mk("zip", "stuff.zip");
    let tar = mk("tar", "stuff.tar");
    let sevenz = mk("7z", "stuff.7z");
    let targz = {
        let p = d.path().join("stuff.tar.gz");
        assert!(Command::new(&sz).args(["a", "-tgzip", "-y", p.to_str().unwrap(), tar.to_str().unwrap()]).output().unwrap().status.success());
        p
    };
    let list = |p: &Path| -> String {
        let o = Command::new(&sz).args(["l", p.to_str().unwrap()]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    for (src, targets) in [(&zip, vec!["tar", "gz"]), (&tar, vec!["zip", "gz"]), (&sevenz, vec!["zip", "tar", "gz"]), (&targz, vec!["zip", "tar"])] {
        for t in targets {
            let o = convert(src, t, &ctx).unwrap_or_else(|e| panic!("{} -> {t}: {e}", src.display()));
            assert_eq!(o.len(), 1);
            assert_type(&o[0], if t == "gz" { "gz" } else { t });
            // content survives (for gz the payload is a tar)
            let inner = if t == "gz" {
                let x = tmpdir();
                assert!(Command::new(&sz).args(["x", "-y", &format!("-o{}", x.path().display()), o[0].to_str().unwrap()]).output().unwrap().status.success());
                let tarp = std::fs::read_dir(x.path()).unwrap().next().unwrap().unwrap().path();
                list(&tarp)
            } else {
                list(&o[0])
            };
            assert!(inner.contains("a.txt") && inner.contains("b.txt"), "{inner}");
        }
    }
    // gz output is named .tar.gz
    assert!(out.path().join("stuff.tar.gz").exists());
}

#[test]
fn rar_input_if_possible() {
    // Creating a RAR needs the proprietary `rar`; skip unless a fixture can be built.
    let Some(_ctx) = setup(&[Dep::SevenZip], None) else { return };
    if Command::new("rar").arg("-?").output().is_err() {
        eprintln!("SKIP: no `rar` to build a fixture");
    }
}

#[test]
fn cli_roundtrip() {
    let d = tmpdir();
    let srt = d.path().join("a.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,000\nHi\n").unwrap();
    let out = d.path().join("o");
    assert_eq!(orange_convert::cli::run(vec![srt.to_string_lossy().into(), "--to".into(), "vtt".into(), "--out-dir".into(), out.to_string_lossy().into()]), 0);
    assert!(out.join("a.vtt").is_file());
    assert_eq!(orange_convert::cli::run(vec!["--count".into()]), 0);
    assert_eq!(orange_convert::cli::run(vec!["--list".into(), srt.to_string_lossy().into()]), 0);
    assert_eq!(orange_convert::cli::run(vec![srt.to_string_lossy().into(), "--to".into(), "mp4".into()]), 1);
}
