use serde::{Deserialize, Serialize};
use std::path::Path;

/// Broad file category. Decides which wheel entries a file gets.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Image,
    Video,
    Audio,
    Pdf,
    Document,
    Slides,
    Spreadsheet,
    Epub,
    Subtitle,
    Archive,
    Folder,
    Other,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Image => "Images",
            Kind::Video => "Video",
            Kind::Audio => "Audio",
            Kind::Pdf => "PDF",
            Kind::Document => "Documents",
            Kind::Slides => "Slides",
            Kind::Spreadsheet => "Spreadsheets",
            Kind::Epub => "EPUB",
            Kind::Subtitle => "Subtitles",
            Kind::Archive => "Archives",
            Kind::Folder => "Folder",
            Kind::Other => "Other",
        }
    }
}

/// Lower-cased extension without the dot ("" if none).
pub fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default()
}

pub fn kind_of_ext(ext: &str) -> Kind {
    match ext {
        "jpg" | "jpeg" | "png" | "webp" | "heic" | "heif" | "tif" | "tiff" | "avif" | "bmp" | "svg" => Kind::Image,
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "gif" | "m4v" | "flv" | "mpg" | "mpeg" | "3gp" | "ogv" | "mts" | "m2ts" | "vob" => Kind::Video,
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "oga" | "opus" | "aiff" | "aif" | "wma" | "aac" | "mka" | "m4b" | "m4r" | "weba" | "amr" | "ac3" | "mp2" => Kind::Audio,
        "pdf" => Kind::Pdf,
        "docx" | "doc" | "odt" | "md" | "markdown" | "txt" | "rtf" | "rtfd" | "html" | "htm" => Kind::Document,
        "ppt" | "pptx" | "odp" => Kind::Slides,
        "xlsx" | "xls" | "ods" | "csv" | "tsv" => Kind::Spreadsheet,
        "epub" => Kind::Epub,
        "srt" | "vtt" => Kind::Subtitle,
        "zip" | "tar" | "gz" | "tgz" | "rar" | "7z" => Kind::Archive,
        _ => Kind::Other,
    }
}

/// Guess a file's format from its first bytes. Returns the extension that format normally uses.
/// Only media, image and PDF signatures are recognised; anything else is `None`.
pub fn sniff_ext(head: &[u8]) -> Option<&'static str> {
    let starts = |sig: &[u8]| head.starts_with(sig);
    let at = |off: usize, sig: &[u8]| head.len() >= off + sig.len() && &head[off..off + sig.len()] == sig;
    if starts(b"ID3") {
        return Some("mp3");
    }
    if head.len() >= 2 && head[0] == 0xFF && head[1] & 0xF6 == 0xF0 {
        return Some("aac"); // ADTS frame (layer bits 00)
    }
    if head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 {
        return Some("mp3"); // MPEG audio frame sync
    }
    if at(4, b"ftyp") {
        let brand = if head.len() >= 12 { &head[8..12] } else { &[][..] };
        return Some(match brand {
            b"M4A " | b"M4B " | b"M4P " => "m4a",
            b"qt  " => "mov",
            _ => "mp4",
        });
    }
    if starts(b"RIFF") {
        if at(8, b"WAVE") {
            return Some("wav");
        }
        if at(8, b"AVI ") {
            return Some("avi");
        }
        if at(8, b"WEBP") {
            return Some("webp");
        }
    }
    if starts(b"fLaC") {
        return Some("flac");
    }
    if starts(b"OggS") {
        return Some("ogg");
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some("mkv"); // Matroska or WebM
    }
    if starts(&[0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11]) {
        return Some("wmv"); // ASF container (WMA or WMV)
    }
    if starts(b"FORM") && (at(8, b"AIFF") || at(8, b"AIFC")) {
        return Some("aiff");
    }
    if starts(b"FLV\x01") {
        return Some("flv");
    }
    if starts(&[0x00, 0x00, 0x01, 0xBA]) {
        return Some("mpg");
    }
    if head.len() > 188 && head[0] == 0x47 && head[188] == 0x47 && (head.len() <= 376 || head[376] == 0x47) {
        return Some("mpg"); // MPEG transport stream
    }
    if starts(b"GIF8") {
        return Some("gif");
    }
    if starts(&[0x89, b'P', b'N', b'G']) {
        return Some("png");
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if starts(b"BM") && head.len() > 14 && head[6..10] == [0, 0, 0, 0] {
        return Some("bmp");
    }
    if starts(b"%PDF") {
        return Some("pdf");
    }
    None
}

/// The extension to treat a file as: its own if Orange knows it, otherwise a guess from the file's contents
/// (so a song or video with an unusual or missing extension still gets the right wheel).
pub fn effective_ext(path: &Path) -> String {
    let ext = ext_of(path);
    if kind_of_ext(&ext) != Kind::Other || path.is_dir() {
        return ext;
    }
    use std::io::Read;
    let mut buf = [0u8; 512];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut buf)).unwrap_or(0);
    sniff_ext(&buf[..n]).map(str::to_string).unwrap_or(ext)
}

/// Detect the kind of a path (directories are `Kind::Folder`).
pub fn detect(path: &Path) -> Kind {
    if path.is_dir() {
        return Kind::Folder;
    }
    kind_of_ext(&effective_ext(path))
}

/// If every path has the same kind, return it.
pub fn common_kind(paths: &[impl AsRef<Path>]) -> Option<Kind> {
    let mut it = paths.iter().map(|p| detect(p.as_ref()));
    let first = it.next()?;
    if it.all(|k| k == first) {
        Some(first)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects() {
        assert_eq!(kind_of_ext("png"), Kind::Image);
        assert_eq!(kind_of_ext("gif"), Kind::Video);
        assert_eq!(kind_of_ext("xyz"), Kind::Other);
        assert_eq!(common_kind(&["a.png", "b.JPG"]), Some(Kind::Image));
        assert_eq!(common_kind(&["a.png", "b.mp3"]), None);
    }

    #[test]
    fn more_media_extensions_are_known() {
        for e in ["mpeg", "mpg", "3gp", "m4v", "flv", "ogv"] {
            assert_eq!(kind_of_ext(e), Kind::Video, "{e}");
        }
        for e in ["aac", "mka", "m4b", "weba", "amr", "ac3"] {
            assert_eq!(kind_of_ext(e), Kind::Audio, "{e}");
        }
    }

    #[test]
    fn sniffs_common_media_signatures() {
        assert_eq!(sniff_ext(b"ID3\x04\x00\x00\x00\x00\x00\x00"), Some("mp3"));
        assert_eq!(sniff_ext(&[0xFF, 0xFB, 0x90, 0x00]), Some("mp3"));
        assert_eq!(sniff_ext(&[0xFF, 0xF1, 0x50, 0x80]), Some("aac"));
        assert_eq!(sniff_ext(b"\x00\x00\x00\x20ftypM4A \x00\x00\x00\x00"), Some("m4a"));
        assert_eq!(sniff_ext(b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00"), Some("mp4"));
        assert_eq!(sniff_ext(b"\x00\x00\x00\x14ftypqt  \x00\x00\x00\x00"), Some("mov"));
        assert_eq!(sniff_ext(b"RIFF\x24\x00\x00\x00WAVEfmt "), Some("wav"));
        assert_eq!(sniff_ext(b"RIFF\x24\x00\x00\x00AVI LIST"), Some("avi"));
        assert_eq!(sniff_ext(b"fLaC\x00\x00\x00\x22"), Some("flac"));
        assert_eq!(sniff_ext(b"OggS\x00\x02"), Some("ogg"));
        assert_eq!(sniff_ext(&[0x1A, 0x45, 0xDF, 0xA3, 0x01]), Some("mkv"));
        assert_eq!(sniff_ext(b"%PDF-1.7"), Some("pdf"));
        assert_eq!(sniff_ext(b"just some text"), None);
        assert_eq!(sniff_ext(b""), None);
    }

    #[test]
    fn mpeg_transport_stream_needs_repeating_sync_bytes() {
        let mut ts = vec![0u8; 400];
        ts[0] = 0x47;
        ts[188] = 0x47;
        ts[376] = 0x47;
        assert_eq!(sniff_ext(&ts), Some("mpg"));
        ts[188] = 0;
        assert_eq!(sniff_ext(&ts), None);
    }

    #[test]
    fn unknown_extension_falls_back_to_the_contents() {
        let d = std::env::temp_dir().join(format!("orange-kind-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let song = d.join("Killin' 'em (From Scene).xyz");
        std::fs::write(&song, b"ID3\x04\x00\x00\x00\x00\x00\x00 and some audio").unwrap();
        assert_eq!(detect(&song), Kind::Audio);
        assert_eq!(effective_ext(&song), "mp3");
        let text = d.join("notes.xyz");
        std::fs::write(&text, b"hello world").unwrap();
        assert_eq!(detect(&text), Kind::Other);
        // a known extension is never second-guessed
        let known = d.join("really.mp3");
        std::fs::write(&known, b"%PDF-1.7").unwrap();
        assert_eq!(effective_ext(&known), "mp3");
        let _ = std::fs::remove_dir_all(&d);
    }
}
