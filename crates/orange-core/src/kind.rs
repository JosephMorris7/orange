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
        "mp4" | "mov" | "mkv" | "webm" | "avi" | "wmv" | "gif" | "m4v" | "flv" => Kind::Video,
        "mp3" | "m4a" | "wav" | "flac" | "ogg" | "oga" | "opus" | "aiff" | "aif" | "wma" | "aac" => Kind::Audio,
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

/// Detect the kind of a path (directories are `Kind::Folder`).
pub fn detect(path: &Path) -> Kind {
    if path.is_dir() {
        return Kind::Folder;
    }
    kind_of_ext(&ext_of(path))
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
}
