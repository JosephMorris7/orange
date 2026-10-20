//! Audio tools (ffmpeg).

use crate::im::abs;
use crate::info::probe;
use crate::params::{f64_or, i64_or, str_or};
use crate::util::{finish_named, s};
use orange_core::kind::ext_of;
use orange_core::paths::stem_of;
use orange_core::{exec, Ctx, Dep, Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

/// Encoder arguments for an output extension; `kbps` applies to lossy codecs.
fn encoder(ext: &str, kbps: u32) -> Option<Vec<String>> {
    let b = format!("{kbps}k");
    Some(match ext {
        "mp3" => v(&["-c:a", "libmp3lame", "-b:a", &b]),
        "m4a" | "aac" => v(&["-c:a", "aac", "-b:a", &b]),
        "ogg" | "oga" => v(&["-c:a", "libvorbis", "-b:a", &b]),
        "opus" => v(&["-c:a", "libopus", "-b:a", &b]),
        "wav" => v(&["-c:a", "pcm_s16le"]),
        "flac" => v(&["-c:a", "flac"]),
        "aiff" | "aif" => v(&["-c:a", "pcm_s16be"]),
        "wma" => v(&["-c:a", "wmav2", "-b:a", &b]),
        _ => return None,
    })
}

pub(crate) fn run_each(id: &str, inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let ff = ctx.caps.require(Dep::Ffmpeg)?.to_path_buf();
    let n = inputs.len() as f32;
    let mut outs = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        let pr = probe(input, ctx)?;
        let src = ext_of(input);
        let td = ctx.temp_dir()?;
        let mut args: Vec<String> = vec!["-i".into(), s(&abs(input))];
        let (suffix, ext, dur): (&str, String, f64);
        match id {
            "compress-audio" => {
                let kbps = str_or(params, "bitrate_kbps", "0").parse::<u32>().unwrap_or(0);
                let kbps = if kbps == 0 { i64_or(params, "bitrate_kbps", 128).clamp(32, 320) as u32 } else { kbps.clamp(32, 320) };
                ext = if matches!(src.as_str(), "wav" | "flac" | "aiff" | "aif" | "wma") { "mp3".into() } else { src.clone() };
                suffix = "Compressed";
                dur = pr.duration;
                args.extend(v(&["-vn", "-map", "0:a:0", "-map_metadata", "0"]));
                args.extend(encoder(&ext, kbps).ok_or_else(|| Error::Unsupported(format!("cannot write .{ext}")))?);
            }
            "trim-audio" => {
                let (start, end) = super::video::trim_range(params, &pr)?;
                suffix = "Trimmed";
                ext = if encoder(&src, 192).is_some() { src.clone() } else { "mp3".into() };
                dur = end - start;
                if !crate::params::bool_or(params, "precise", false) && ext == src {
                    args = vec!["-ss".into(), format!("{start:.3}"), "-i".into(), s(&abs(input)), "-t".into(), format!("{:.3}", end - start), "-vn".into(), "-c".into(), "copy".into()];
                } else {
                    args = vec!["-i".into(), s(&abs(input)), "-ss".into(), format!("{start:.3}"), "-t".into(), format!("{:.3}", end - start), "-vn".into()];
                    args.extend(encoder(&ext, 192).unwrap());
                }
            }
            "normalize-audio" => {
                let lufs = f64_or(params, "target_lufs", -16.0).clamp(-40.0, -3.0);
                suffix = "Normalized";
                ext = if encoder(&src, 192).is_some() { src.clone() } else { "mp3".into() };
                dur = pr.duration;
                args.extend(v(&["-vn", "-map", "0:a:0", "-map_metadata", "0", "-af", &format!("loudnorm=I={lufs}:TP=-1.5:LRA=11"), "-ar", "48000"]));
                args.extend(encoder(&ext, 192).unwrap());
            }
            other => return Err(Error::Unsupported(format!("not an audio tool: {other}"))),
        }
        let tmp = td.path().join(format!("out.{ext}"));
        args.push(s(&tmp));
        exec::run_ffmpeg(&ff, &args, Some(dur), ctx, i as f32 / n, 1.0 / n, &format!("{suffix} ({}/{})", i + 1, inputs.len()))?;
        outs.push(finish_named(&tmp, input, &format!("{} {}", stem_of(input), suffix), &ext, ctx)?);
    }
    ctx.report(1.0, "Done");
    Ok(outs)
}

pub(crate) fn merge(inputs: &[PathBuf], _params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let ff = ctx.caps.require(Dep::Ffmpeg)?.to_path_buf();
    let mut args: Vec<String> = Vec::new();
    let mut total = 0.0;
    for p in inputs {
        let pr = probe(p, ctx)?;
        if !pr.has_audio {
            return Err(Error::BadParams(format!("{} has no audio", p.display())));
        }
        total += pr.duration;
        args.extend(["-i".into(), s(&abs(p))]);
    }
    let first: &Path = &inputs[0];
    let src = ext_of(first);
    let ext = if matches!(src.as_str(), "wma") || encoder(&src, 192).is_none() { "mp3".to_string() } else { src };
    let mut chains = Vec::new();
    let mut cin = String::new();
    for i in 0..inputs.len() {
        chains.push(format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}]"));
        cin.push_str(&format!("[a{i}]"));
    }
    chains.push(format!("{cin}concat=n={}:v=0:a=1[a]", inputs.len()));
    args.extend(["-filter_complex".into(), chains.join(";"), "-map".into(), "[a]".into()]);
    args.extend(encoder(&ext, 192).unwrap());
    let td = ctx.temp_dir()?;
    let tmp = td.path().join(format!("out.{ext}"));
    args.push(s(&tmp));
    exec::run_ffmpeg(&ff, &args, Some(total), ctx, 0.0, 1.0, "Merging audio")?;
    let dest = finish_named(&tmp, first, &format!("{} Merged", stem_of(first)), &ext, ctx)?;
    Ok(vec![dest])
}
