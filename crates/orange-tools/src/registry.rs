//! The 26 tools: static metadata (ids, labels, icons, kinds, dialogs, generic form params).

use crate::{ParamKind, ParamSpec, ShowWhen, ToolInfo, ToolUi};
use orange_core::{Dep, Kind};

fn num(key: &str, label: &str, min: f64, max: f64, step: f64, default: f64, unit: Option<&str>, help: Option<&str>) -> ParamSpec {
    ParamSpec { key: key.into(), label: label.into(), kind: ParamKind::Number { min, max, step, default, unit: unit.map(String::from) }, help: help.map(String::from), show_when: None }
}
fn select(key: &str, label: &str, options: &[(&str, &str)], default: &str, help: Option<&str>) -> ParamSpec {
    ParamSpec {
        key: key.into(),
        label: label.into(),
        kind: ParamKind::Select { options: options.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect(), default: default.into() },
        help: help.map(String::from),
        show_when: None,
    }
}
fn text(key: &str, label: &str, default: &str, placeholder: Option<&str>, help: Option<&str>) -> ParamSpec {
    ParamSpec { key: key.into(), label: label.into(), kind: ParamKind::Text { default: default.into(), placeholder: placeholder.map(String::from) }, help: help.map(String::from), show_when: None }
}

#[allow(clippy::too_many_arguments)]
trait When {
    fn when(self, key: &str, values: &[&str]) -> Self;
}
impl When for ParamSpec {
    fn when(mut self, key: &str, values: &[&str]) -> Self {
        self.show_when = Some(ShowWhen { key: key.into(), values: values.iter().map(|v| v.to_string()).collect() });
        self
    }
}

#[allow(clippy::too_many_arguments)]
fn tool(id: &str, label: &str, icon: &str, title: &str, description: &str, kinds: &[Kind], min: usize, max: Option<usize>, ui: ToolUi, params: Vec<ParamSpec>, deps: &[Dep]) -> ToolInfo {
    ToolInfo {
        id: id.into(),
        label: label.into(),
        icon: icon.into(),
        title: title.into(),
        description: description.into(),
        kinds: kinds.to_vec(),
        min_files: min,
        max_files: max,
        ui,
        params,
        deps: deps.to_vec(),
    }
}

pub fn all() -> Vec<ToolInfo> {
    use Kind::*;
    let img = [Image];
    let vid = [Video];
    let aud = [Audio];
    let pdf = [Pdf];
    let mg = [Dep::Magick];
    let ff = [Dep::Ffmpeg, Dep::Ffprobe];
    let qp = [Dep::Qpdf];
    vec![
        // ---- images ----
        tool("compress-image", "COMPRESS", "compress", "Compress images", "Reduce file size to a target size, a percentage, or a preset. Optionally limit the longest side.", &img, 1, None, ToolUi::Form,
            vec![
                select("mode", "Method", &[("preset", "Preset"), ("target_size", "Target file size"), ("reduce", "Reduce by percentage")], "preset", None),
                select("preset", "Preset", &[("small", "Smallest"), ("balanced", "Balanced"), ("high", "High quality")], "balanced", Some("Used when the method is Preset.")).when("mode", &["preset"]),
                num("target_kb", "Target size", 10.0, 50000.0, 10.0, 500.0, Some("KB"), Some("Used when the method is Target file size.")).when("mode", &["target_size"]),
                num("percent", "Reduce by", 5.0, 95.0, 5.0, 50.0, Some("%"), Some("Used when the method is Reduce by percentage.")).when("mode", &["reduce"]),
                num("max_dimension", "Longest side (0 = keep)", 0.0, 8000.0, 10.0, 0.0, Some("px"), None),
            ], &mg),
        tool("edit-metadata", "METADATA", "tag", "Edit metadata", "Edit camera, date, location and author tags, or strip location / all metadata.", &img, 1, None, ToolUi::Metadata, vec![], &[Dep::Exiftool]),
        tool("edit-photo", "EDIT", "sliders", "Edit photo", "Exposure, colour, sharpness, effects, rotate and flip with a live preview.", &img, 1, Some(1), ToolUi::EditPhoto, vec![], &mg),
        tool("annotate-photo", "ANNOTATE", "pencil", "Annotate photo", "Add arrows, shapes, text and numbered callouts.", &img, 1, Some(1), ToolUi::Annotate, vec![], &mg),
        tool("add-background", "ADD BG", "image-plus", "Add a background", "Place the image on a colour, gradient or another image with spacing, rounded corners and a shadow.", &img, 1, None, ToolUi::AddBackground, vec![], &mg),
        tool("crop-image", "CROP", "crop", "Crop images", "Crop to a region or aspect ratio, or enter exact output dimensions.", &img, 1, None, ToolUi::Crop, vec![], &mg),
        tool("redact-photo", "REDACT", "eye-off", "Redact photo", "Cover faces, text or any region with a solid colour or a pixelating blur. Metadata is stripped.", &img, 1, Some(1), ToolUi::Redact, vec![], &mg),
        tool("resize-image", "RESIZE", "resize", "Resize images", "Scale by width, height, percentage or to fit inside a box. Aspect ratio is kept.", &img, 1, None, ToolUi::Form,
            vec![
                select("mode", "Resize by", &[("width", "Width"), ("height", "Height"), ("percent", "Percentage"), ("fit", "Fit inside box")], "width", None),
                num("width", "Width", 1.0, 20000.0, 1.0, 1920.0, Some("px"), None).when("mode", &["width", "fit"]),
                num("height", "Height", 1.0, 20000.0, 1.0, 1080.0, Some("px"), None).when("mode", &["height", "fit"]),
                num("percent", "Percentage", 1.0, 400.0, 1.0, 50.0, Some("%"), None).when("mode", &["percent"]),
            ], &mg),
        tool("create-pdf", "TO PDF", "file-plus", "Create a PDF", "Combine the selected images into one PDF, one page per image, in the order shown.", &img, 2, None, ToolUi::Reorder,
            vec![
                select("page_size", "Page size", &[("fit", "Fit to image"), ("a4", "A4"), ("letter", "US Letter")], "fit", None),
                select("orientation", "Orientation", &[("auto", "Automatic"), ("portrait", "Portrait"), ("landscape", "Landscape")], "auto", Some("Ignored for Fit to image.")),
                num("margin", "Margin", 0.0, 100.0, 1.0, 0.0, Some("pt"), None),
            ], &mg),
        tool("collage", "COLLAGE", "grid", "Make a collage", "Combine images in a grid, row, column or featured layout.", &img, 2, Some(30), ToolUi::Collage, vec![], &mg),
        // ---- video ----
        tool("compress-video", "COMPRESS", "compress", "Compress video", "Re-encode to H.264 with a smaller size: a preset or an approximate target size.", &vid, 1, None, ToolUi::Form,
            vec![
                select("mode", "Method", &[("preset", "Preset"), ("target_mb", "Target file size")], "preset", None),
                select("preset", "Preset", &[("small", "Smallest"), ("balanced", "Balanced"), ("high", "High quality")], "balanced", None).when("mode", &["preset"]),
                num("target_mb", "Target size", 1.0, 20000.0, 1.0, 25.0, Some("MB"), Some("Approximate; used when the method is Target file size.")).when("mode", &["target_mb"]),
                select("max_height", "Limit height", &[("0", "Keep original"), ("2160", "2160p"), ("1080", "1080p"), ("720", "720p"), ("480", "480p")], "0", None),
            ], &ff),
        tool("trim-video", "TRIM", "scissors", "Trim video", "Keep only the part between a start and an end time.", &vid, 1, Some(1), ToolUi::Trim, vec![], &ff),
        tool("merge-video", "MERGE", "merge", "Merge videos", "Join videos end to end in the order shown. Clips are scaled to the first clip's size.", &vid, 2, None, ToolUi::Reorder, vec![], &ff),
        tool("remove-audio", "MUTE", "volume-x", "Remove audio", "Save a copy of the video without its sound track (no re-encoding).", &vid, 1, None, ToolUi::None, vec![], &ff),
        tool("resize-video", "RESIZE", "resize", "Resize video", "Scale the video to a new height, keeping the aspect ratio.", &vid, 1, None, ToolUi::Form,
            vec![select("height", "Height", &[("2160", "2160p"), ("1440", "1440p"), ("1080", "1080p"), ("720", "720p"), ("480", "480p"), ("360", "360p")], "720", None)], &ff),
        // ---- audio ----
        tool("compress-audio", "COMPRESS", "gauge", "Compress audio", "Re-encode at a lower bitrate. Lossless files become MP3.", &aud, 1, None, ToolUi::Form,
            vec![select("bitrate_kbps", "Bitrate", &[("64", "64 kbps"), ("96", "96 kbps"), ("128", "128 kbps"), ("192", "192 kbps"), ("256", "256 kbps")], "128", None)], &ff),
        tool("trim-audio", "TRIM", "scissors", "Trim audio", "Keep only the part between a start and an end time.", &aud, 1, Some(1), ToolUi::Trim, vec![], &ff),
        tool("merge-audio", "MERGE", "merge", "Merge audio", "Join audio files end to end in the order shown.", &aud, 2, None, ToolUi::Reorder, vec![], &ff),
        tool("normalize-audio", "LEVEL", "music", "Normalize volume", "Even out loudness to a target level (EBU R128 loudnorm).", &aud, 1, None, ToolUi::Form,
            vec![num("target_lufs", "Target loudness", -30.0, -5.0, 1.0, -16.0, Some("LUFS"), Some("-16 suits most streaming and podcasts."))], &ff),
        // ---- PDF ----
        tool("compress-pdf", "COMPRESS", "compress", "Compress PDF", "Re-compress embedded JPEG images and optimize structure. Text stays selectable. Works best on scans and photo-heavy PDFs.", &pdf, 1, None, ToolUi::Form,
            vec![select("level", "Level", &[("low", "Light (best quality)"), ("medium", "Balanced"), ("high", "Strong (smallest)")], "medium", None)], &[]),
        tool("split-pdf", "SPLIT", "split", "Split PDF", "Split into single pages, chunks of N pages, or custom page ranges.", &pdf, 1, Some(1), ToolUi::Pages, vec![], &qp),
        tool("merge-pdf", "MERGE", "merge", "Merge PDFs", "Combine PDFs into one document in the order shown.", &pdf, 2, None, ToolUi::Reorder, vec![], &qp),
        tool("rotate-pdf", "ROTATE", "rotate", "Rotate PDF pages", "Rotate all pages or selected pages.", &pdf, 1, None, ToolUi::Pages, vec![], &qp),
        tool("protect-pdf", "LOCK", "lock", "Lock or unlock PDF", "Add a password to a PDF, or remove the password from one you can open.", &pdf, 1, None, ToolUi::Form,
            vec![
                select("action", "Action", &[("lock", "Lock with password"), ("unlock", "Remove password")], "lock", None),
                text("password", "Password", "", Some("Enter password"), None).when("action", &["lock", "unlock"]),
            ], &qp),
        // ---- other ----
        tool("shift-subtitles", "SHIFT", "clock", "Shift subtitle timing", "Move every cue earlier or later (SRT or VTT).", &[Subtitle], 1, None, ToolUi::Form,
            vec![num("offset_ms", "Shift by", -600000.0, 600000.0, 100.0, 1000.0, Some("ms"), Some("Positive = later, negative = earlier."))], &[]),
        tool("create-archive", "ZIP", "archive", "Create an archive", "Pack the selected files and folders into one ZIP, TAR, TAR.GZ or 7z archive.", &[Image, Video, Audio, Pdf, Document, Slides, Spreadsheet, Epub, Subtitle, Archive, Folder, Other], 1, None, ToolUi::Form,
            vec![
                select("format", "Format", &[("zip", "ZIP"), ("tar", "TAR"), ("gz", "TAR.GZ"), ("7z", "7z (needs 7-Zip)")], "zip", None),
                text("name", "Archive name", "Archive", None, None),
            ], &[]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exactly_26_unique() {
        let t = all();
        assert_eq!(t.len(), 26);
        let mut ids: Vec<_> = t.iter().map(|x| x.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 26);
    }
}
