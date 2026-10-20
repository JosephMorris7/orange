//! Subtitle timing shift for SRT and VTT (pure Rust).

use crate::params::f64_or;
use crate::util::{finish_named, read_text};
use orange_core::kind::ext_of;
use orange_core::paths::stem_of;
use orange_core::{Ctx, Error, Result};
use serde_json::Value;
use std::path::PathBuf;

/// "01:02:03,456", "01:02:03.456", "02:03.456" -> milliseconds
fn parse_time(t: &str) -> Option<i64> {
    let t = t.trim().replace(',', ".");
    let (hms, frac) = t.split_once('.').unwrap_or((&t, "0"));
    let parts: Vec<&str> = hms.split(':').collect();
    let (h, m, s) = match parts.as_slice() {
        [h, m, s] => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?, s.parse::<i64>().ok()?),
        [m, s] => (0, m.parse::<i64>().ok()?, s.parse::<i64>().ok()?),
        _ => return None,
    };
    let mut f = frac.to_string();
    f.truncate(3);
    while f.len() < 3 {
        f.push('0');
    }
    Some(((h * 60 + m) * 60 + s) * 1000 + f.parse::<i64>().ok()?)
}

fn fmt_time(ms: i64, sep: char) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}{sep}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

pub fn shift(text: &str, offset_ms: i64, vtt: bool) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let sep = if vtt { '.' } else { ',' };
    let mut out: Vec<String> = Vec::new();
    let mut cue_no = 0;
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else {
            if !block.trim().is_empty() {
                out.push(block.trim_end_matches('\n').to_string());
            }
            continue;
        };
        let (l, r) = lines[ti].split_once("-->").unwrap();
        let r = r.trim();
        let (end_tok, rest) = match r.split_once(char::is_whitespace) {
            Some((e, rest)) => (e, format!(" {}", rest.trim())),
            None => (r, String::new()),
        };
        let (Some(a), Some(b)) = (parse_time(l), parse_time(end_tok)) else {
            out.push(block.to_string());
            continue;
        };
        let (a, b) = (a + offset_ms, b + offset_ms);
        if b <= 0 {
            continue; // cue now ends before the start of the file
        }
        cue_no += 1;
        let mut blk: Vec<String> = Vec::new();
        for (i, line) in lines.iter().enumerate() {
            if i == ti {
                blk.push(format!("{} --> {}{}", fmt_time(a, sep), fmt_time(b, sep), rest));
            } else if i < ti && !vtt && line.trim().chars().all(|c| c.is_ascii_digit()) && !line.trim().is_empty() {
                blk.push(cue_no.to_string());
            } else {
                blk.push((*line).to_string());
            }
        }
        out.push(blk.join("\n"));
    }
    let mut s = out.join("\n\n");
    s.push('\n');
    s
}

pub(crate) fn run_each(inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let offset = f64_or(params, "offset_ms", 0.0).round() as i64;
    let mut outs = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        ctx.report(i as f32 / inputs.len() as f32, "Shifting subtitles");
        let ext = ext_of(input);
        if ext != "srt" && ext != "vtt" {
            return Err(Error::BadParams(format!("{} is not an SRT or VTT file", input.display())));
        }
        let text = read_text(input)?;
        let shifted = shift(&text, offset, ext == "vtt");
        let td = ctx.temp_dir()?;
        let tmp = td.path().join(format!("out.{ext}"));
        std::fs::write(&tmp, shifted)?;
        outs.push(finish_named(&tmp, input, &format!("{} Shifted", stem_of(input)), &ext, ctx)?);
    }
    ctx.report(1.0, "Done");
    Ok(outs)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shifts_srt() {
        let src = "1\n00:00:01,000 --> 00:00:03,500\nHello\n\n2\n00:00:05,000 --> 00:00:06,000\nWorld\n";
        let out = shift(src, 1500, false);
        assert!(out.contains("00:00:02,500 --> 00:00:05,000"));
        assert!(out.contains("00:00:06,500 --> 00:00:07,500"));
    }
    #[test]
    fn drops_cues_that_fall_before_zero_and_renumbers() {
        let src = "1\n00:00:01,000 --> 00:00:02,000\nA\n\n2\n00:00:05,000 --> 00:00:06,000\nB\n";
        let out = shift(src, -3000, false);
        assert!(!out.contains('A'));
        assert!(out.starts_with("1\n00:00:02,000 --> 00:00:03,000\nB"));
    }
    #[test]
    fn vtt_keeps_header_and_settings() {
        let src = "WEBVTT\n\n00:01.000 --> 00:02.000 align:start\nHi\n";
        let out = shift(src, 1000, true);
        assert!(out.starts_with("WEBVTT"));
        assert!(out.contains("00:00:02.000 --> 00:00:03.000 align:start"));
    }
}
