//! The two spots that need `std::os::windows` process extensions, isolated so the rest of the
//! Windows layer can be type-checked on other hosts. Windows only (gated in `mod.rs`).

use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// `explorer.exe /select,"<path>"`. Explorer parses its own command line, so the argument is passed
/// verbatim (quoted) rather than through Rust's argument escaping.
pub fn explorer_select(path: &Path) -> Result<(), String> {
    Command::new("explorer.exe").raw_arg(format!("/select,\"{}\"", path.to_string_lossy())).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Spawn `program args…` in `dir`; console programs get their own visible window.
pub fn spawn_in_dir(program: &str, args: &[String], dir: &Path, new_console: bool) -> Result<(), String> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(dir);
    if new_console {
        cmd.creation_flags(CREATE_NEW_CONSOLE);
    }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}
