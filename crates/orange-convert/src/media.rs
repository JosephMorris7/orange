//! Audio and video conversions via ffmpeg.

use crate::util::*;
use orange_core::{exec, Ctx, Dep, Result};
use std::path::{Path, PathBuf};

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|x| x.to_string()).collect()
}

fn duration(input: &Path, ctx: &Ctx) -> Option<f64> {
    let ffprobe = ctx.caps.get(Dep::Ffprobe)?;
    exec::probe_duration(ffprobe, input, ctx)
}

fn run(input: &Path, to: &str, codec_args: Vec<String>, ctx: &Ctx, label: &str) -> Result<Vec<PathBuf>> {
    let ffmpeg = ctx.caps.require(Dep::Ffmpeg)?;
    let tmp = ctx.temp_dir()?;
    let out = tmp.path().join(format!("out.{to}"));
    let mut args = v(&["-i"]);
    args.push(s(input));
    args.extend(codec_args);
    args.push(s(&out));
    ctx.report(0.0, label);
    let total = duration(input, ctx);
    exec::run_ffmpeg(ffmpeg, &args, total, ctx, 0.0, 0.97, label)?;
    Ok(vec![finish_file(&out, input, to, ctx)?])
}

/// Any audio (or the audio track of a video) -> audio format.
pub(crate) fn audio(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let codec: &[&str] = match to {
        "mp3" => &["-c:a", "libmp3lame", "-q:a", "2"],
        "m4a" => &["-c:a", "aac", "-b:a", "192k"],
        "wav" => &["-c:a", "pcm_s16le"],
        "flac" => &["-c:a", "flac"],
        "ogg" => &["-c:a", "libvorbis", "-q:a", "5"],
        "opus" => &["-c:a", "libopus", "-b:a", "128k"],
        "aiff" => &["-c:a", "pcm_s16be", "-f", "aiff"],
        _ => return Err(orange_core::Error::Unsupported(format!("audio format {to}"))),
    };
    let mut a = v(&["-vn", "-map", "0:a:0", "-map_metadata", "0"]);
    a.extend(v(codec));
    run(input, to, a, ctx, "Converting audio")
}

const EVEN: &str = "scale=trunc(iw/2)*2:trunc(ih/2)*2";

pub(crate) fn video(input: &Path, to: &str, ctx: &Ctx) -> Result<Vec<PathBuf>> {
    let a: Vec<String> = match to {
        "mp4" | "mov" => v(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", EVEN, "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart",
        ]),
        "mkv" => v(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", EVEN, "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "192k",
        ]),
        "webm" => v(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", EVEN, "-c:v", "libvpx-vp9", "-crf", "32", "-b:v", "0", "-deadline", "good", "-cpu-used",
            "4", "-row-mt", "1", "-pix_fmt", "yuv420p", "-c:a", "libopus", "-b:a", "128k",
        ]),
        "avi" => v(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", EVEN, "-c:v", "mpeg4", "-q:v", "3", "-pix_fmt", "yuv420p", "-c:a", "libmp3lame",
            "-q:a", "3",
        ]),
        "wmv" => v(&[
            "-map", "0:v:0", "-map", "0:a:0?", "-vf", EVEN, "-c:v", "wmv2", "-b:v", "4M", "-pix_fmt", "yuv420p", "-c:a", "wmav2", "-b:a",
            "160k",
        ]),
        // One-pass palette: split the stream, build an optimal palette, dither with it.
        "gif" => v(&[
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            "fps=15,scale='min(720,iw)':-2:flags=lanczos,split[s0][s1];[s0]palettegen=stats_mode=diff[p];[s1][p]paletteuse=dither=bayer:bayer_scale=5:diff_mode=rectangle",
            "-loop",
            "0",
        ]),
        _ => return Err(orange_core::Error::Unsupported(format!("video format {to}"))),
    };
    run(input, to, a, ctx, "Converting video")
}
