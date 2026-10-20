//! Text and face detection with the Windows built-in engines (offline). Windows only (gated in `mod.rs`).

use std::path::Path;
use windows::core::HSTRING;
use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
use windows::Media::FaceAnalysis::FaceDetector;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// A detected region in pixels of the *decoded image*.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

fn load_bitmap(path: &Path) -> windows::core::Result<SoftwareBitmap> {
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.get()?;
    let stream = file.OpenAsync(FileAccessMode::Read)?.get()?;
    let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
    decoder.GetSoftwareBitmapAsync()?.get()
}

fn init_com() {
    // SAFETY: initialises COM for this thread; "already initialised in another mode" is harmless here.
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

/// Lines of text, one rectangle per recognised line (words merged), padded slightly.
pub fn detect_text(path: &Path) -> Result<Vec<Region>, String> {
    init_com();
    let run = || -> windows::core::Result<Vec<Region>> {
        let bmp = load_bitmap(path)?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let result = engine.RecognizeAsync(&bmp)?.get()?;
        let mut out = Vec::new();
        for line in result.Lines()? {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for word in line.Words()? {
                let r = word.BoundingRect()?;
                x0 = x0.min(r.X as f64);
                y0 = y0.min(r.Y as f64);
                x1 = x1.max((r.X + r.Width) as f64);
                y1 = y1.max((r.Y + r.Height) as f64);
            }
            if x1 > x0 && y1 > y0 {
                let pad = ((y1 - y0) * 0.15).max(2.0);
                out.push(Region { x: (x0 - pad).max(0.0), y: (y0 - pad).max(0.0), w: x1 - x0 + 2.0 * pad, h: y1 - y0 + 2.0 * pad });
            }
        }
        Ok(out)
    };
    run().map_err(|e| format!("text detection failed: {e}"))
}

/// Face boxes, enlarged by 25 % so hairlines and chins are covered.
pub fn detect_faces(path: &Path) -> Result<Vec<Region>, String> {
    init_com();
    let run = || -> windows::core::Result<Vec<Region>> {
        if !FaceDetector::IsSupported()? {
            return Err(windows::core::Error::from(windows::Win32::Foundation::E_NOTIMPL));
        }
        let bmp = load_bitmap(path)?;
        let gray = SoftwareBitmap::Convert(&bmp, BitmapPixelFormat::Gray8)?;
        let detector = FaceDetector::CreateAsync()?.get()?;
        let faces = detector.DetectFacesAsync(&gray)?.get()?;
        let mut out = Vec::new();
        for f in faces {
            let b = f.FaceBox()?;
            let (w, h) = (b.Width as f64, b.Height as f64);
            let (px, py) = (w * 0.125, h * 0.125);
            out.push(Region { x: (b.X as f64 - px).max(0.0), y: (b.Y as f64 - py).max(0.0), w: w + 2.0 * px, h: h + 2.0 * py });
        }
        Ok(out)
    };
    run().map_err(|e| format!("face detection failed: {e}"))
}
