//! Running external tools: hidden console window on Windows, captured output,
//! cooperative cancellation and ffmpeg progress parsing.

use crate::ctx::Ctx;
use crate::error::{Error, Result};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct Captured {
    pub stdout: String,
    pub stderr: String,
}

/// A `Command` for `program` that will not flash a console window on Windows.
pub fn command(program: &Path) -> Command {
    let mut c = Command::new(program);
    hide_window(&mut c);
    c
}

#[cfg(windows)]
pub fn hide_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}
#[cfg(not(windows))]
pub fn hide_window(_cmd: &mut Command) {}

fn tail(s: &str, n: usize) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Run to completion; non-zero exit => `Error::ToolFailed` with the stderr tail.
/// Cancellation kills the child and returns `Error::Cancelled`.
pub fn run(cmd: &mut Command, tool: &str, ctx: &Ctx) -> Result<Captured> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| Error::ToolFailed { tool: tool.into(), detail: format!("could not start: {e}") })?;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = out.as_mut() {
            let mut buf = Vec::new();
            let _ = o.read_to_end(&mut buf);
            s = String::from_utf8_lossy(&buf).into_owned();
        }
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = err.as_mut() {
            let mut buf = Vec::new();
            let _ = o.read_to_end(&mut buf);
            s = String::from_utf8_lossy(&buf).into_owned();
        }
        s
    });
    let status = loop {
        if ctx.cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        match child.try_wait()? {
            Some(s) => break s,
            None => std::thread::sleep(Duration::from_millis(30)),
        }
    };
    let stdout = t_out.join().unwrap_or_default();
    let stderr = t_err.join().unwrap_or_default();
    if !status.success() {
        let detail = if stderr.trim().is_empty() { tail(&stdout, 6) } else { tail(&stderr, 6) };
        return Err(Error::ToolFailed { tool: tool.into(), detail });
    }
    Ok(Captured { stdout, stderr })
}

/// Run ffmpeg with progress reporting. `args` must NOT include `-progress`/`-nostats`
/// (added here) and must end with the output file. `total_secs` is the expected output
/// duration (from ffprobe) used to compute the fraction; `base`..`base+span` is the
/// slice of the overall job this run covers.
pub fn run_ffmpeg(ffmpeg: &Path, args: &[String], total_secs: Option<f64>, ctx: &Ctx, base: f32, span: f32, label: &str) -> Result<()> {
    let mut cmd = command(ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-nostdin", "-y", "-progress", "pipe:1", "-nostats"]);
    cmd.args(args);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| Error::ToolFailed { tool: "ffmpeg".into(), detail: format!("could not start: {e}") })?;
    let stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let ctx2 = ctx.clone();
    let label = label.to_string();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
            if let Some(v) = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms=")) {
                if let (Ok(us), Some(total)) = (v.trim().parse::<f64>(), total_secs) {
                    if total > 0.0 {
                        let f = ((us / 1_000_000.0) / total).clamp(0.0, 1.0) as f32;
                        ctx2.report(base + span * f, label.clone());
                    }
                }
            }
        }
    });
    let t_err = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        String::from_utf8_lossy(&b).into_owned()
    });
    let status = loop {
        if ctx.cancel.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Cancelled);
        }
        match child.try_wait()? {
            Some(s) => break s,
            None => std::thread::sleep(Duration::from_millis(30)),
        }
    };
    let _ = reader.join();
    let err = t_err.join().unwrap_or_default();
    if !status.success() {
        return Err(Error::ToolFailed { tool: "ffmpeg".into(), detail: tail(&err, 6) });
    }
    ctx.report(base + span, label_done());
    Ok(())
}

fn label_done() -> String {
    String::new()
}

/// Duration of a media file in seconds via ffprobe, if available.
pub fn probe_duration(ffprobe: &Path, file: &Path, ctx: &Ctx) -> Option<f64> {
    let mut c = command(ffprobe);
    c.args(["-v", "error", "-show_entries", "format=duration", "-of", "default=nw=1:nk=1"]).arg(file);
    let out = run(&mut c, "ffprobe", ctx).ok()?;
    out.stdout.trim().parse::<f64>().ok()
}
