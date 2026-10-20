//! SRT <-> VTT and plain-text export, pure Rust.

use crate::util::*;
use orange_core::{Ctx, Error, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cue {
    pub start: u64,
    pub end: u64,
    pub text: String,
}

fn parse_ts(t: &str) -> Option<u64> {
    let t = t.trim();
    let parts: Vec<&str> = t.split(':').collect();
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (h.parse::<u64>().ok()?, m.parse::<u64>().ok()?, *s),
        [m, s] => (0, m.parse::<u64>().ok()?, *s),
        _ => return None,
    };
    let sec = sec.replace(',', ".");
    let (whole, frac) = sec.split_once('.').unwrap_or((&sec, "0"));
    let ms: u64 = format!("{:0<3}", &frac[..frac.len().min(3)]).parse().ok()?;
    Some(((h * 60 + m) * 60 + whole.parse::<u64>().ok()?) * 1000 + ms)
}

fn fmt_ts(ms: u64, sep: char) -> String {
    format!("{:02}:{:02}:{:02}{sep}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Parses both SRT and WebVTT (header, NOTE/STYLE blocks and cue settings are dropped).
pub(crate) fn parse(text: &str) -> Vec<Cue> {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[ti].split_once("-->") else { continue };
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (parse_ts(a), parse_ts(b)) else { continue };
        cues.push(Cue { start, end, text: lines[ti + 1..].join("\n").trim().to_string() });
    }
    cues
}

pub(crate) fn to_srt(cues: &[Cue]) -> String {
    let mut o = String::new();
    for (i, c) in cues.iter().enumerate() {
        o.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, fmt_ts(c.start, ','), fmt_ts(c.end, ','), c.text));
    }
    o
}

pub(crate) fn to_vtt(cues: &[Cue]) -> String {
    let mut o = String::from("WEBVTT\n\n");
    for c in cues {
        o.push_str(&format!("{} --> {}\n{}\n\n", fmt_ts(c.start, '.'), fmt_ts(c.end, '.'), c.text));
    }
    o
}

fn strip_markup(t: &str) -> String {
    let mut out = String::new();
    let mut depth_tag = false;
    let mut depth_brace = false;
    for ch in t.chars() {
        match ch {
            '<' => depth_tag = true,
            '>' if depth_tag => depth_tag = false,
            '{' => depth_brace = true,
            '}' if depth_brace => depth_brace = false,
            _ if depth_tag || depth_brace => {}
            _ => out.push(ch),
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&nbsp;", " ")
}

pub(crate) fn to_txt(cues: &[Cue]) -> String {
    let mut o = String::new();
    for c in cues {
        let t = strip_markup(&c.text);
        if !t.trim().is_empty() {
            o.push_str(t.trim());
            o.push('\n');
        }
    }
    o
}

pub(crate) fn convert(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let cues = parse(&read_text(input)?);
    if cues.is_empty() {
        return Err(Error::msg("no subtitle cues found in this file"));
    }
    let body = match to {
        "srt" => to_srt(&cues),
        "vtt" => to_vtt(&cues),
        "txt" => to_txt(&cues),
        other => return Err(Error::Unsupported(format!("subtitle format {other}"))),
    };
    let tmp = ctx.temp_dir()?;
    let out = tmp.path().join(format!("out.{to}"));
    std::fs::write(&out, body)?;
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn srt_vtt_roundtrip() {
        let srt = "1\n00:00:01,500 --> 00:00:03,000\nHello <i>there</i>\n\n2\n00:01:00,000 --> 00:01:02,250\nBye\n";
        let cues = parse(srt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start, 1500);
        let vtt = to_vtt(&cues);
        assert!(vtt.starts_with("WEBVTT"));
        assert!(vtt.contains("00:00:01.500 --> 00:00:03.000"));
        assert_eq!(parse(&vtt), cues);
        assert_eq!(to_srt(&parse(&vtt)), to_srt(&cues));
        assert_eq!(to_txt(&cues), "Hello there\nBye\n");
    }
    #[test]
    fn vtt_short_timestamps_and_settings() {
        let vtt = "WEBVTT\n\nNOTE x\n\nintro\n01:02.500 --> 01:04.000 align:start\nHi\n";
        let c = parse(vtt);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].start, c[0].end), (62500, 64000));
    }
}
