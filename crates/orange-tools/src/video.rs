//! Video tools (ffmpeg).

use crate::im::abs;
use crate::info::{probe, Probe};
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

fn x264(crf: &str) -> Vec<String> {
    v(&["-c:v", "libx264", "-preset", "medium", "-crf", crf, "-pix_fmt", "yuv420p"])
}

pub(crate) fn run_each(id: &str, inputs: &[PathBuf], params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let ff = ctx.caps.require(Dep::Ffmpeg)?.to_path_buf();
    let n = inputs.len() as f32;
    let mut outs = Vec::new();
    for (i, input) in inputs.iter().enumerate() {
        ctx.check_cancel()?;
        let (base, span) = (i as f32 / n, 1.0 / n);
        let pr = probe(input, ctx)?;
        if !pr.has_video {
            return Err(Error::BadParams(format!("{} has no video stream", input.display())));
        }
        let td = ctx.temp_dir()?;
        let src = ext_of(input);
        let mut args: Vec<String> = vec!["-i".into(), s(&abs(input))];
        let (suffix, ext, dur): (String, String, Option<f64>);
        match id {
            "compress-video" => {
                suffix = "Compressed".into();
                ext = "mp4".into();
                dur = Some(pr.duration);
                let max_h = str_or(params, "max_height", "0").parse::<i64>().unwrap_or(0).max(0);
                let max_h = if max_h == 0 { i64_or(params, "max_height", 0) } else { max_h };
                let vf = if max_h > 0 { format!("scale=-2:min(ih\\,{max_h})") } else { "scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string() };
                args.extend(v(&["-map", "0:v:0", "-map", "0:a:0?", "-vf", &vf]));
                if str_or(params, "mode", "preset") == "target_mb" {
                    let mb = f64_or(params, "target_mb", 25.0).max(0.5);
                    if pr.duration <= 0.0 {
                        return Err(Error::BadParams("cannot target a size: unknown duration".into()));
                    }
                    let total_kbps = mb * 8.0 * 1024.0 * 1024.0 / pr.duration / 1000.0;
                    let audio = if pr.has_audio { 96.0 } else { 0.0 };
                    let vk = (total_kbps * 0.93 - audio).max(80.0);
                    args.extend(v(&["-c:v", "libx264", "-preset", "medium", "-pix_fmt", "yuv420p"]));
                    args.extend(["-b:v".into(), format!("{vk:.0}k"), "-maxrate".into(), format!("{:.0}k", vk * 1.5), "-bufsize".into(), format!("{:.0}k", vk * 3.0)]);
                    args.extend(v(&["-c:a", "aac", "-b:a", "96k"]));
                } else {
                    let crf = match str_or(params, "preset", "balanced") {
                        "small" => "30",
                        "high" => "22",
                        "balanced" => "26",
                        other => return Err(Error::BadParams(format!("unknown preset: {other}"))),
                    };
                    args.extend(x264(crf));
                    args.extend(v(&["-c:a", "aac", "-b:a", "128k"]));
                }
                args.extend(v(&["-movflags", "+faststart"]));
            }
            "trim-video" => {
                let (start, end) = trim_range(params, &pr)?;
                suffix = "Trimmed".into();
                dur = Some(end - start);
                let precise = crate::params::bool_or(params, "precise", false);
                if !precise && src != "gif" {
                    ext = src.clone();
                    // input seeking + stream copy: fast, cuts on the nearest keyframe
                    args = vec!["-ss".into(), format!("{start:.3}"), "-i".into(), s(&abs(input)), "-t".into(), format!("{:.3}", end - start)];
                    args.extend(v(&["-map", "0", "-c", "copy", "-avoid_negative_ts", "make_zero"]));
                } else {
                    ext = if src == "gif" { "mp4".into() } else { src.clone() };
                    args = vec!["-i".into(), s(&abs(input)), "-ss".into(), format!("{start:.3}"), "-t".into(), format!("{:.3}", end - start)];
                    args.extend(v(&["-map", "0:v:0", "-map", "0:a:0?", "-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2"]));
                    args.extend(x264("18"));
                    args.extend(v(&["-c:a", "aac", "-b:a", "192k"]));
                }
            }
            "remove-audio" => {
                suffix = "Muted".into();
                ext = src.clone();
                dur = Some(pr.duration);
                args.extend(v(&["-map", "0:v", "-c", "copy", "-an"]));
            }
            "resize-video" => {
                let h = str_or(params, "height", "720").parse::<i64>().unwrap_or(0);
                let h = if h == 0 { i64_or(params, "height", 720) } else { h };
                if !(64..=8640).contains(&h) {
                    return Err(Error::BadParams("height must be between 64 and 8640".into()));
                }
                suffix = format!("{h}p");
                ext = "mp4".into();
                dur = Some(pr.duration);
                args.extend(v(&["-map", "0:v:0", "-map", "0:a:0?", "-vf", &format!("scale=-2:{h}")]));
                args.extend(x264("22"));
                args.extend(v(&["-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart"]));
            }
            other => return Err(Error::Unsupported(format!("not a video tool: {other}"))),
        }
        let tmp = td.path().join(format!("out.{ext}"));
        args.push(s(&tmp));
        exec::run_ffmpeg(&ff, &args, dur, ctx, base, span, &format!("{} ({}/{})", suffix, i + 1, inputs.len()))?;
        outs.push(finish_named(&tmp, input, &format!("{} {}", stem_of(input), suffix), &ext, ctx)?);
    }
    ctx.report(1.0, "Done");
    Ok(outs)
}

pub(crate) fn trim_range(params: &Value, pr: &Probe) -> Result<(f64, f64)> {
    let start = f64_or(params, "start", 0.0).max(0.0);
    let end = f64_or(params, "end", pr.duration);
    let end = if pr.duration > 0.0 { end.min(pr.duration) } else { end };
    if end <= start {
        return Err(Error::BadParams("the end time must be after the start time".into()));
    }
    Ok((start, end))
}

pub(crate) fn merge(inputs: &[PathBuf], _params: &Value, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let ff = ctx.caps.require(Dep::Ffmpeg)?.to_path_buf();
    let probes: Vec<Probe> = inputs.iter().map(|p| probe(p, ctx)).collect::<Result<_>>()?;
    if let Some(i) = probes.iter().position(|p| !p.has_video) {
        return Err(Error::BadParams(format!("{} has no video stream", inputs[i].display())));
    }
    let (w, h) = (probes[0].width.unwrap_or(1280) / 2 * 2, probes[0].height.unwrap_or(720) / 2 * 2);
    let mut args: Vec<String> = Vec::new();
    for p in inputs {
        args.extend(["-i".into(), s(&abs(p))]);
    }
    let mut silent_idx = inputs.len();
    let mut chains: Vec<String> = Vec::new();
    let mut concat_in = String::new();
    for (i, pr) in probes.iter().enumerate() {
        chains.push(format!(
            "[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30,format=yuv420p[v{i}]"
        ));
        if pr.has_audio {
            chains.push(format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}]"));
        } else {
            args.extend(["-f".into(), "lavfi".into(), "-t".into(), format!("{:.3}", pr.duration.max(0.1)), "-i".into(), "anullsrc=r=48000:cl=stereo".into()]);
            chains.push(format!("[{silent_idx}:a]aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}]"));
            silent_idx += 1;
        }
        concat_in.push_str(&format!("[v{i}][a{i}]"));
    }
    chains.push(format!("{concat_in}concat=n={}:v=1:a=1[v][a]", inputs.len()));
    let td = ctx.temp_dir()?;
    let tmp = td.path().join("out.mp4");
    args.extend(["-filter_complex".into(), chains.join(";")]);
    args.extend(v(&["-map", "[v]", "-map", "[a]"]));
    args.extend(x264("22"));
    args.extend(v(&["-c:a", "aac", "-b:a", "160k", "-movflags", "+faststart"]));
    args.push(s(&tmp));
    let total: f64 = probes.iter().map(|p| p.duration).sum();
    exec::run_ffmpeg(&ff, &args, Some(total), ctx, 0.0, 1.0, "Merging videos")?;
    let first: &Path = &inputs[0];
    let dest = finish_named(&tmp, first, &format!("{} Merged", stem_of(first)), "mp4", ctx)?;
    Ok(vec![dest])
}
