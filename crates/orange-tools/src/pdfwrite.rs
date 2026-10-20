//! A tiny PDF writer for image pages (no ImageMagick PDF coder involved).
//! JPEG data is embedded as-is (DCTDecode); everything else as Flate-compressed RGB.

use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::Write;

pub(crate) enum PageImage {
    /// Baseline/progressive JPEG with 1 (gray) or 3 (RGB) components.
    Jpeg { w: u32, h: u32, comps: u8, data: Vec<u8> },
    /// Raw 8-bit RGB, 3 bytes per pixel.
    Rgb { w: u32, h: u32, data: Vec<u8> },
}

impl PageImage {
    fn dims(&self) -> (u32, u32) {
        match self {
            PageImage::Jpeg { w, h, .. } | PageImage::Rgb { w, h, .. } => (*w, *h),
        }
    }
}

/// Parse (width, height, components) from a JPEG stream.
pub(crate) fn jpeg_info(d: &[u8]) -> Option<(u32, u32, u8)> {
    if d.len() < 4 || d[0] != 0xFF || d[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= d.len() {
        if d[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = d[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        let sof = matches!(m, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
        if sof {
            if i + 9 >= d.len() {
                return None;
            }
            let h = u16::from_be_bytes([d[i + 5], d[i + 6]]) as u32;
            let w = u16::from_be_bytes([d[i + 7], d[i + 8]]) as u32;
            return Some((w, h, d[i + 9]));
        }
        if m == 0xDA {
            return None;
        }
        i += 2 + len;
    }
    None
}

/// EXIF orientation (1..=8) of a JPEG, if present.
pub(crate) fn jpeg_orientation(d: &[u8]) -> Option<u16> {
    let mut i = 2;
    while i + 4 <= d.len() && d[i] == 0xFF {
        let m = d[i + 1];
        if m == 0xDA {
            break;
        }
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        if m == 0xE1 && d.len() >= i + 10 && &d[i + 4..i + 10] == b"Exif\0\0" {
            let t = &d[i + 10..(i + 2 + len).min(d.len())];
            if t.len() < 8 {
                return None;
            }
            let le = &t[0..2] == b"II";
            let u16at = |o: usize| -> Option<u16> {
                let b = t.get(o..o + 2)?;
                Some(if le { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) })
            };
            let u32at = |o: usize| -> Option<u32> {
                let b = t.get(o..o + 4)?;
                Some(if le { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
            };
            let ifd = u32at(4)? as usize;
            let n = u16at(ifd)? as usize;
            for k in 0..n {
                let e = ifd + 2 + k * 12;
                if u16at(e)? == 0x0112 {
                    return u16at(e + 8);
                }
            }
            return None;
        }
        i += 2 + len;
    }
    None
}

/// Parse a binary PPM (P6, maxval 255) -> (w, h, rgb bytes).
pub(crate) fn parse_ppm(d: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut pos = 0usize;
    let tok = |pos: &mut usize| -> Option<String> {
        loop {
            while *pos < d.len() && d[*pos].is_ascii_whitespace() {
                *pos += 1;
            }
            if *pos < d.len() && d[*pos] == b'#' {
                while *pos < d.len() && d[*pos] != b'\n' {
                    *pos += 1;
                }
            } else {
                break;
            }
        }
        let st = *pos;
        while *pos < d.len() && !d[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
        (*pos > st).then(|| String::from_utf8_lossy(&d[st..*pos]).into_owned())
    };
    if tok(&mut pos)? != "P6" {
        return None;
    }
    let w: u32 = tok(&mut pos)?.parse().ok()?;
    let h: u32 = tok(&mut pos)?.parse().ok()?;
    let max: u32 = tok(&mut pos)?.parse().ok()?;
    if max != 255 {
        return None;
    }
    pos += 1;
    let need = (w as usize) * (h as usize) * 3;
    (d.len() >= pos + need).then(|| (w, h, d[pos..pos + need].to_vec()))
}

/// Pixels -> points. Images are treated as 96 dpi, the usual screen convention.
const PT_PER_PX: f64 = 0.75;

/// Page geometry for `build`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Layout {
    /// `None` = every page is sized to its image (plus margin).
    pub page: Option<(f64, f64)>,
    pub margin: f64,
    /// 0 = auto, 1 = portrait, 2 = landscape (fixed page sizes only)
    pub orientation: u8,
}

impl Layout {
    pub fn fit() -> Self {
        Layout { page: None, margin: 0.0, orientation: 0 }
    }
    /// (page_w, page_h, img_x, img_y, img_w, img_h) in points, origin bottom-left.
    fn place(&self, w: u32, h: u32) -> (f64, f64, f64, f64, f64, f64) {
        let (iw, ih) = (w as f64 * PT_PER_PX, h as f64 * PT_PER_PX);
        let m = self.margin.max(0.0);
        match self.page {
            None => (iw + 2.0 * m, ih + 2.0 * m, m, m, iw, ih),
            Some((pw0, ph0)) => {
                let landscape = match self.orientation {
                    1 => false,
                    2 => true,
                    _ => w > h,
                };
                let (pw, ph) = if landscape { (pw0.max(ph0), pw0.min(ph0)) } else { (pw0.min(ph0), pw0.max(ph0)) };
                let (aw, ah) = ((pw - 2.0 * m).max(1.0), (ph - 2.0 * m).max(1.0));
                let sc = (aw / iw).min(ah / ih);
                let (dw, dh) = (iw * sc, ih * sc);
                (pw, ph, (pw - dw) / 2.0, (ph - dh) / 2.0, dw, dh)
            }
        }
    }
}

pub(crate) fn build(pages: &[PageImage], layout: &Layout) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let n = pages.len();
    let total_objs = 2 + 3 * n;
    let obj = |out: &mut Vec<u8>, offsets: &mut Vec<usize>, body: &[u8]| {
        offsets.push(out.len());
        let id = offsets.len();
        out.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    obj(&mut out, &mut offsets, b"<< /Type /Catalog /Pages 2 0 R >>");
    let kids: String = (0..n).map(|i| format!("{} 0 R", 3 + 3 * i)).collect::<Vec<_>>().join(" ");
    obj(&mut out, &mut offsets, format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>").as_bytes());
    for (i, p) in pages.iter().enumerate() {
        let (w, h) = p.dims();
        let (pw, ph, ix, iy, iw, ih) = layout.place(w, h);
        let page_id = 3 + 3 * i;
        let img_id = page_id + 1;
        let content_id = page_id + 2;
        obj(
            &mut out,
            &mut offsets,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im0 {img_id} 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        let (dict, data): (String, Vec<u8>) = match p {
            PageImage::Jpeg { w, h, comps, data } => (
                format!(
                    "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /{} /BitsPerComponent 8 /Filter /DCTDecode /Length {}",
                    if *comps == 1 { "DeviceGray" } else { "DeviceRGB" },
                    data.len()
                ),
                data.clone(),
            ),
            PageImage::Rgb { w, h, data } => {
                let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
                let _ = z.write_all(data);
                let c = z.finish().unwrap_or_default();
                (
                    format!(
                        "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode /Length {}",
                        c.len()
                    ),
                    c,
                )
            }
        };
        let mut body = dict.into_bytes();
        body.extend_from_slice(b" >>\nstream\n");
        body.extend_from_slice(&data);
        body.extend_from_slice(b"\nendstream");
        obj(&mut out, &mut offsets, &body);
        let content = format!("q {iw:.2} 0 0 {ih:.2} {ix:.2} {iy:.2} cm /Im0 Do Q");
        obj(&mut out, &mut offsets, format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", total_objs + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", total_objs + 1).as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn builds_valid_structure() {
        let pdf = build(&[PageImage::Rgb { w: 2, h: 2, data: vec![255; 12] }], &Layout::fit());
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.contains("/Count 1"));
        assert!(s.contains("xref\n0 6"));
    }
    #[test]
    fn ppm_parse() {
        let mut d = b"P6\n# c\n2 1\n255\n".to_vec();
        d.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        assert_eq!(parse_ppm(&d).unwrap(), (2, 1, vec![1, 2, 3, 4, 5, 6]));
    }
}
