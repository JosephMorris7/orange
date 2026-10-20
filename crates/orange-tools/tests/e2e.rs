//! End-to-end tests: real ImageMagick / ffmpeg / qpdf / exiftool / 7-Zip. Each test skips itself
//! (passes with a note) when a required external tool is missing.

use orange_core::{Capabilities, Ctx, Dep};
use orange_tools::{all_tools, file_info, preview, read_metadata, run_tool, tools_for, web_preview};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

fn caps() -> Arc<Capabilities> {
    Arc::new(Capabilities::detect(&[]))
}

macro_rules! need {
    ($caps:expr, $($d:expr),+) => {
        $( if !$caps.has($d) { eprintln!("SKIP: {} not installed", $d.name()); return; } )+
    };
}

fn ctx_in(c: &Arc<Capabilities>, out: &Path) -> Ctx {
    Ctx::new(c.clone()).with_out_dir(Some(out.to_path_buf()))
}

fn dims(c: &Capabilities, p: &Path) -> (u64, u64) {
    let v = file_info(p, c).unwrap();
    (v["width"].as_u64().unwrap(), v["height"].as_u64().unwrap())
}

fn make_image(c: &Capabilities, path: &Path, w: u32, h: u32) {
    let m = c.get(Dep::Magick).unwrap();
    let st = Command::new(m).args(["-size", &format!("{w}x{h}"), "plasma:fractal"]).arg(path).status().unwrap();
    assert!(st.success());
}

fn make_video(c: &Capabilities, path: &Path, secs: u32, audio: bool) {
    let ff = c.get(Dep::Ffmpeg).unwrap();
    let mut a: Vec<String> = ["-v", "error", "-y", "-f", "lavfi", "-i"].iter().map(|s| s.to_string()).collect();
    a.push(format!("testsrc=size=320x180:rate=25:duration={secs}"));
    if audio {
        a.extend(["-f".into(), "lavfi".into(), "-i".into(), format!("sine=frequency=440:duration={secs}")]);
    }
    a.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into()]);
    if audio {
        a.extend(["-c:a".into(), "aac".into()]);
    }
    a.push(path.to_string_lossy().into_owned());
    assert!(Command::new(ff).args(&a).status().unwrap().success());
}

fn make_audio(c: &Capabilities, path: &Path, secs: u32) {
    let ff = c.get(Dep::Ffmpeg).unwrap();
    let st = Command::new(ff).args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("sine=frequency=330:duration={secs}")]).arg(path).status().unwrap();
    assert!(st.success());
}

fn duration(c: &Capabilities, p: &Path) -> f64 {
    file_info(p, c).unwrap()["duration"].as_f64().unwrap()
}

#[test]
fn registry_has_26_tools_and_params_are_consistent() {
    let t = all_tools();
    assert_eq!(t.len(), 26);
    for tool in &t {
        assert!(!tool.label.is_empty() && !tool.title.is_empty() && !tool.description.is_empty(), "{}", tool.id);
        assert!(tool.min_files >= 1);
        assert!(tool.label == tool.label.to_uppercase(), "label must be ALL CAPS: {}", tool.label);
        serde_json::to_value(tool).unwrap();
    }
}

#[test]
fn selection_filtering() {
    let c = caps();
    let ids = |v: Vec<orange_tools::ToolInfo>| v.into_iter().map(|t| t.id).collect::<Vec<_>>();
    let one = ids(tools_for(&[PathBuf::from("a.png")], &c));
    assert!(one.contains(&"crop-image".to_string()) && !one.contains(&"create-pdf".to_string()) && !one.contains(&"collage".to_string()));
    let two = ids(tools_for(&[PathBuf::from("a.png"), PathBuf::from("b.jpg")], &c));
    assert!(two.contains(&"create-pdf".to_string()) && two.contains(&"collage".to_string()) && !two.contains(&"edit-photo".to_string()));
    let mixed = ids(tools_for(&[PathBuf::from("a.png"), PathBuf::from("b.mp3")], &c));
    assert_eq!(mixed, vec!["create-archive".to_string()]);
}

#[test]
fn image_tools_geometry() {
    let c = caps();
    need!(c, Dep::Magick);
    let td = tempfile::tempdir().unwrap();
    let out = td.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let ctx = ctx_in(&c, &out);
    let img = td.path().join("p.jpg");
    make_image(&c, &img, 800, 500);

    let r = run_tool("crop-image", &[img.clone()], &json!({"x":100,"y":50,"width":300,"height":200}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (300, 200));
    assert!(r[0].file_name().unwrap().to_string_lossy().contains("Cropped"));
    let r = run_tool("crop-image", &[img.clone()], &json!({"mode":"ratio","ratio":"1:1"}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (500, 500));
    // out-of-range rectangles are clamped, not fatal
    let r = run_tool("crop-image", &[img.clone()], &json!({"x":700,"y":400,"width":900,"height":900}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (100, 100));

    let r = run_tool("resize-image", &[img.clone()], &json!({"mode":"width","width":400}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (400, 250));
    let r = run_tool("resize-image", &[img.clone()], &json!({"mode":"fit","width":200,"height":200}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (200, 125));

    let r = run_tool("edit-photo", &[img.clone()], &json!({"rotate":90,"vignette":50,"saturation":20}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (500, 800));

    let r = run_tool("add-background", &[img.clone()], &json!({"background":{"type":"solid","color":"#fbf6ef"},"ratio":"1:1","padding":50,"corner_radius":30,"shadow":{"blur":20,"opacity":0.4,"offset_y":10}}), &ctx).unwrap();
    let (w, h) = dims(&c, &r[0]);
    assert_eq!(w, h, "1:1 ratio");
    assert!(w >= 900);
    let r = run_tool("add-background", &[img.clone()], &json!({"background":{"type":"gradient","from":"#ff9a3c","to":"#7b2ff7","angle":45},"ratio":"16:9"}), &ctx).unwrap();
    let (w, h) = dims(&c, &r[0]);
    assert!((w as f64 / h as f64 - 16.0 / 9.0).abs() < 0.01);
}

#[test]
fn annotate_and_redact() {
    let c = caps();
    need!(c, Dep::Magick);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let img = td.path().join("p.png");
    make_image(&c, &img, 600, 400);
    let params = json!({"items":[
        {"type":"arrow","x1":50,"y1":350,"x2":300,"y2":200,"color":"#ff3b30","width":8},
        {"type":"rect","x":320,"y":50,"w":200,"h":120,"color":"#00e5ff","width":6,"fill":"#ffffff33"},
        {"type":"ellipse","x":50,"y":50,"w":200,"h":120,"color":"#ffeb3b","width":6},
        {"type":"text","x":40,"y":300,"text":"@100% done","size":40,"color":"#fff","bg":"#000000aa","bold":true},
        {"type":"callout","x":500,"y":300,"n":7,"size":70,"color":"#ff3b30"}]});
    let r = run_tool("annotate-photo", &[img.clone()], &params, &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (600, 400));
    assert_ne!(std::fs::read(&img).unwrap(), std::fs::read(&r[0]).unwrap());

    // solid redaction paints exactly black inside the region
    let r = run_tool("redact-photo", &[img.clone()], &json!({"regions":[{"x":100,"y":100,"w":200,"h":100}],"style":"solid","color":"#000000"}), &ctx).unwrap();
    let m = c.get(Dep::Magick).unwrap();
    let px = Command::new(m).arg(format!("{}[0]", r[0].display())).args(["-format", "%[hex:p{150,150}]", "info:"]).output().unwrap();
    let hex = String::from_utf8_lossy(&px.stdout).trim().to_string();
    assert!(hex.starts_with("000000"), "pixel inside a solid redaction must be black, got {hex}");
    // pixelating blur changes the region but keeps the size
    let r = run_tool("redact-photo", &[img.clone()], &json!({"regions":[{"x":100,"y":100,"w":200,"h":100}],"style":"blur","blur_radius":40}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (600, 400));
    // bad input is rejected
    assert!(run_tool("annotate-photo", &[img.clone()], &json!({"items":[{"type":"rect","color":"-write x"}]}), &ctx).is_err());
    assert!(run_tool("redact-photo", &[img], &json!({"regions":[],"style":"oops"}), &ctx).is_err());
}

#[test]
fn compress_image_hits_target() {
    let c = caps();
    need!(c, Dep::Magick);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let img = td.path().join("p.jpg");
    make_image(&c, &img, 1600, 1000);
    let orig = std::fs::metadata(&img).unwrap().len();
    let r = run_tool("compress-image", &[img.clone()], &json!({"mode":"target_size","target_kb":100}), &ctx).unwrap();
    assert!(std::fs::metadata(&r[0]).unwrap().len() <= 100 * 1024);
    let r = run_tool("compress-image", &[img.clone()], &json!({"mode":"reduce","percent":60}), &ctx).unwrap();
    assert!(std::fs::metadata(&r[0]).unwrap().len() as f64 <= orig as f64 * 0.4);
    let r = run_tool("compress-image", &[img], &json!({"mode":"preset","preset":"small"}), &ctx).unwrap();
    assert!(std::fs::metadata(&r[0]).unwrap().len() < orig);
}

#[test]
fn collage_and_pdf_from_images() {
    let c = caps();
    need!(c, Dep::Magick);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let imgs: Vec<PathBuf> = (0..3)
        .map(|i| {
            let p = td.path().join(format!("i{i}.{}", if i == 1 { "png" } else { "jpg" }));
            make_image(&c, &p, 400 + 100 * i, 300);
            p
        })
        .collect();
    let r = run_tool("collage", &imgs, &json!({"layout":"grid","columns":2,"cell_size":200,"spacing":10}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (2 * 200 + 3 * 10, 2 * 200 + 3 * 10));
    let r = run_tool("collage", &imgs, &json!({"layout":"row","cell_size":100,"spacing":0}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (300, 100));
    let r = run_tool("collage", &imgs, &json!({"layout":"featured","cell_size":100,"spacing":10}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]), (10 + 210 + 10 + 100 + 10, 230));

    let r = run_tool("create-pdf", &imgs, &json!({"page_size":"a4","margin":20}), &ctx).unwrap();
    let info = file_info(&r[0], &c).unwrap();
    assert_eq!(info["pages"], 3);
    let bytes = std::fs::read(&r[0]).unwrap();
    assert!(bytes.starts_with(b"%PDF-"));
    if let Some(q) = c.get(Dep::Qpdf) {
        let st = Command::new(q).arg("--check").arg(&r[0]).output().unwrap();
        assert!(st.status.success(), "qpdf --check: {}", String::from_utf8_lossy(&st.stdout));
    }
    assert!(run_tool("create-pdf", &imgs[..1], &json!({}), &ctx).is_err(), "needs 2+ images");
}

#[test]
fn previews_are_downscaled_and_scale_params() {
    let c = caps();
    need!(c, Dep::Magick);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let img = td.path().join("p.jpg");
    make_image(&c, &img, 2000, 1000);
    // crop rect given in source pixels (1000x500 from 2000x1000) must scale down with the preview
    let p = preview("crop-image", &img, &json!({"x":0,"y":0,"width":1000,"height":500}), &ctx, 400).unwrap();
    assert_eq!(dims(&c, &p), (200, 100));
    let p2 = preview("add-background", &img, &json!({"background":{"type":"solid","color":"#fff"},"padding":200}), &ctx, 400).unwrap();
    let (w, h) = dims(&c, &p2);
    assert_eq!((w, h), (400 + 2 * 40, 200 + 2 * 40));
    assert!(preview("compress-image", &img, &json!({}), &ctx, 400).is_err());
    let wp = web_preview(&img, &ctx, 500).unwrap();
    assert_eq!(dims(&c, &wp), (500, 250));
    for f in [p, p2, wp] {
        let _ = std::fs::remove_file(f);
    }
}

#[test]
fn metadata_roundtrip_and_strip() {
    let c = caps();
    need!(c, Dep::Magick, Dep::Exiftool);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let img = td.path().join("p.jpg");
    make_image(&c, &img, 300, 200);
    let r = run_tool("edit-metadata", &[img.clone()], &json!({"set":{"Make":"Orange","Model":"T1","Artist":"Me","GPSLatitude":-33.86,"GPSLongitude":151.21}}), &ctx).unwrap();
    let m = read_metadata(&r[0], &c).unwrap();
    assert_eq!(m["make"], "Orange");
    assert_eq!(m["artist"], "Me");
    assert!((m["gps"]["lat"].as_f64().unwrap() + 33.86).abs() < 0.01, "southern latitude keeps its sign: {m}");
    assert!((m["gps"]["lon"].as_f64().unwrap() - 151.21).abs() < 0.01);
    // original untouched
    assert_eq!(read_metadata(&img, &c).unwrap()["make"], Value::Null);
    let r2 = run_tool("edit-metadata", &[r[0].clone()], &json!({"strip":["gps"]}), &ctx).unwrap();
    let m2 = read_metadata(&r2[0], &c).unwrap();
    assert_eq!(m2["gps"], Value::Null);
    assert_eq!(m2["make"], "Orange");
    assert!(run_tool("edit-metadata", &[img.clone()], &json!({"set":{"-@":"x"}}), &ctx).is_err());
    assert!(run_tool("edit-metadata", &[img], &json!({}), &ctx).is_err());
}

#[test]
fn video_tools() {
    let c = caps();
    need!(c, Dep::Ffmpeg, Dep::Ffprobe);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let v1 = td.path().join("v1.mp4");
    let v2 = td.path().join("v2.mp4");
    make_video(&c, &v1, 6, true);
    make_video(&c, &v2, 3, false);

    let r = run_tool("trim-video", &[v1.clone()], &json!({"start":1,"end":3.5,"precise":true}), &ctx).unwrap();
    assert!((duration(&c, &r[0]) - 2.5).abs() < 0.2);
    assert!(run_tool("trim-video", &[v1.clone()], &json!({"start":5,"end":2}), &ctx).is_err());
    let r = run_tool("remove-audio", &[v1.clone()], &json!({}), &ctx).unwrap();
    assert_eq!(file_info(&r[0], &c).unwrap()["has_audio"], false);
    let r = run_tool("resize-video", &[v1.clone()], &json!({"height":"90"}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]).1, 90);
    let r = run_tool("compress-video", &[v1.clone()], &json!({"mode":"preset","preset":"small","max_height":"90"}), &ctx).unwrap();
    assert_eq!(dims(&c, &r[0]).1, 90);
    let r = run_tool("compress-video", &[v1.clone()], &json!({"mode":"target_mb","target_mb":0.15}), &ctx).unwrap();
    assert!(std::fs::metadata(&r[0]).unwrap().len() < 200_000);
    // clips with and without audio merge; length adds up and the result has audio
    let r = run_tool("merge-video", &[v1, v2], &json!({}), &ctx).unwrap();
    assert!((duration(&c, &r[0]) - 9.0).abs() < 0.3);
    assert_eq!(file_info(&r[0], &c).unwrap()["has_audio"], true);
}

#[test]
fn audio_tools() {
    let c = caps();
    need!(c, Dep::Ffmpeg, Dep::Ffprobe);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let a1 = td.path().join("a1.mp3");
    let a2 = td.path().join("a2.mp3");
    let a3 = td.path().join("a3.wav");
    make_audio(&c, &a1, 5);
    make_audio(&c, &a2, 3);
    make_audio(&c, &a3, 2);
    let r = run_tool("trim-audio", &[a1.clone()], &json!({"start":1,"end":3,"precise":true}), &ctx).unwrap();
    assert!((duration(&c, &r[0]) - 2.0).abs() < 0.2);
    let r = run_tool("merge-audio", &[a1.clone(), a2, a3.clone()], &json!({}), &ctx).unwrap();
    assert!((duration(&c, &r[0]) - 10.0).abs() < 0.3);
    let r = run_tool("compress-audio", &[a3], &json!({"bitrate_kbps":"64"}), &ctx).unwrap();
    assert_eq!(r[0].extension().unwrap(), "mp3", "lossless inputs become MP3");
    let r = run_tool("normalize-audio", &[a1], &json!({"target_lufs":-16}), &ctx).unwrap();
    assert!((duration(&c, &r[0]) - 5.0).abs() < 0.3);
}

#[test]
fn pdf_tools() {
    let c = caps();
    need!(c, Dep::Magick, Dep::Qpdf);
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let imgs: Vec<PathBuf> = (0..4)
        .map(|i| {
            let p = td.path().join(format!("i{i}.jpg"));
            make_image(&c, &p, 1400, 1000);
            p
        })
        .collect();
    let pdf = run_tool("create-pdf", &imgs, &json!({}), &ctx).unwrap().remove(0);
    assert_eq!(file_info(&pdf, &c).unwrap()["pages"], 4);

    let parts = run_tool("split-pdf", &[pdf.clone()], &json!({"mode":"each"}), &ctx).unwrap();
    assert_eq!(parts.len(), 4);
    let parts = run_tool("split-pdf", &[pdf.clone()], &json!({"mode":"every_n","n":3}), &ctx).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(file_info(&parts[0], &c).unwrap()["pages"], 3);
    assert_eq!(file_info(&parts[1], &c).unwrap()["pages"], 1);
    let parts = run_tool("split-pdf", &[pdf.clone()], &json!({"mode":"ranges","ranges":"1-2,4"}), &ctx).unwrap();
    assert_eq!(parts.len(), 2);
    assert!(run_tool("split-pdf", &[pdf.clone()], &json!({"mode":"ranges","ranges":"1-9"}), &ctx).is_err());
    assert!(run_tool("split-pdf", &[pdf.clone()], &json!({"mode":"ranges","ranges":"1;rm"}), &ctx).is_err());

    let merged = run_tool("merge-pdf", &[pdf.clone(), parts[0].clone()], &json!({}), &ctx).unwrap();
    assert_eq!(file_info(&merged[0], &c).unwrap()["pages"], 6);
    let rot = run_tool("rotate-pdf", &[pdf.clone()], &json!({"degrees":90,"pages":"1"}), &ctx).unwrap();
    let txt = Command::new(c.get(Dep::Qpdf).unwrap()).args(["--qdf", "--object-streams=disable"]).arg(&rot[0]).arg("-").output().unwrap();
    assert!(String::from_utf8_lossy(&txt.stdout).contains("/Rotate 90"));

    let locked = run_tool("protect-pdf", &[pdf.clone()], &json!({"action":"lock","password":"s3cret"}), &ctx).unwrap();
    assert!(run_tool("protect-pdf", &[locked[0].clone()], &json!({"action":"unlock","password":"wrong"}), &ctx).is_err());
    let open = run_tool("protect-pdf", &[locked[0].clone()], &json!({"action":"unlock","password":"s3cret"}), &ctx).unwrap();
    assert_eq!(file_info(&open[0], &c).unwrap()["pages"], 4);
    assert!(run_tool("protect-pdf", &[pdf.clone()], &json!({"action":"lock","password":""}), &ctx).is_err());

    // compress: result is valid and not larger; strong level beats the original on JPEG-heavy PDFs
    let orig = std::fs::metadata(&pdf).unwrap().len();
    let small = run_tool("compress-pdf", &[pdf.clone()], &json!({"level":"high"}), &ctx).unwrap();
    let sz = std::fs::metadata(&small[0]).unwrap().len();
    assert!(sz < orig, "compressed {sz} should be below {orig}");
    assert_eq!(file_info(&small[0], &c).unwrap()["pages"], 4);
}

#[test]
fn subtitles_and_archives() {
    let c = caps();
    let td = tempfile::tempdir().unwrap();
    let ctx = ctx_in(&c, td.path());
    let srt = td.path().join("s.srt");
    std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:03,000\nHello\n\n2\n00:00:04,000 --> 00:00:05,000\nWorld\n").unwrap();
    let r = run_tool("shift-subtitles", &[srt.clone()], &json!({"offset_ms":-2000}), &ctx).unwrap();
    let t = std::fs::read_to_string(&r[0]).unwrap();
    assert!(t.contains("00:00:00,000 --> 00:00:01,000") || t.contains("00:00:02,000 --> 00:00:03,000"));
    assert!(run_tool("shift-subtitles", &[td.path().join("x.txt")], &json!({}), &ctx).is_err());

    let dir = td.path().join("folder");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.txt"), "hi").unwrap();
    std::fs::write(dir.join("sub").join("b.txt"), "yo").unwrap();
    let outd = td.path().join("out");
    std::fs::create_dir(&outd).unwrap();
    let ctx2 = ctx_in(&c, &outd);
    let z = run_tool("create-archive", &[dir.clone(), srt.clone()], &json!({"format":"zip","name":"bundle"}), &ctx2).unwrap();
    assert_eq!(z[0].file_name().unwrap(), "bundle.zip");
    let mut zr = zip::ZipArchive::new(std::fs::File::open(&z[0]).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zr.len()).map(|i| zr.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    assert!(names.contains(&"folder/a.txt".to_string()) && names.contains(&"folder/sub/b.txt".to_string()) && names.contains(&"s.srt".to_string()), "{names:?}");
    let g = run_tool("create-archive", &[dir.clone()], &json!({"format":"gz"}), &ctx2).unwrap();
    assert_eq!(g[0].file_name().unwrap(), "folder.tar.gz");
    let mut ar = tar::Archive::new(flate2::read::GzDecoder::new(std::fs::File::open(&g[0]).unwrap()));
    let listed: Vec<String> = ar.entries().unwrap().map(|e| e.unwrap().path().unwrap().to_string_lossy().replace('\\', "/")).collect();
    assert!(listed.contains(&"folder/sub/b.txt".to_string()), "{listed:?}");
    if c.has(Dep::SevenZip) {
        let s = run_tool("create-archive", &[dir], &json!({"format":"7z","name":"seven"}), &ctx2).unwrap();
        assert!(std::fs::metadata(&s[0]).unwrap().len() > 0);
    }
    // names never overwrite
    let z2 = run_tool("create-archive", &[srt], &json!({"format":"zip","name":"bundle"}), &ctx2).unwrap();
    assert_eq!(z2[0].file_name().unwrap(), "bundle 2.zip");
}
