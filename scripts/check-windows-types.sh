#!/usr/bin/env bash
# Type-checks the Windows-only layer (src-tauri/src/win/*) against the real `windows` 0.58 API on a
# non-Windows host. It cannot RUN anything; it only proves the calls match the Windows API signatures.
# Method: copy the `windows*` crates from the cargo registry, switch their `cfg(windows)` gates on,
# and compile the win/*.rs files in a throwaway harness crate.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="${WORK:-$ROOT/target/winck}"
mkdir -p "$WORK/vendor" "$WORK/harness/src"

# 1. fetch pristine sources
cat > "$WORK/fetch.toml" <<'T'
[package]
name = "fetch"
version = "0.0.0"
edition = "2021"`n[workspace]`n[dependencies]`nwindows = "0.58"
T
mkdir -p "$WORK/fetchcrate/src" && cp "$WORK/fetch.toml" "$WORK/fetchcrate/Cargo.toml" && : > "$WORK/fetchcrate/src/lib.rs"
(cd "$WORK/fetchcrate" && cargo fetch >/dev/null 2>&1 || cargo fetch)
REG="$(ls -d "$HOME"/.cargo/registry/src/*/ | head -1)"
for c in windows-0.58.0 windows-core-0.58.0 windows-result-0.2.0 windows-strings-0.1.0 windows-targets-0.52.6; do
  rm -rf "$WORK/vendor/$c"; cp -r "$REG/$c" "$WORK/vendor/$c"
done

# 2. turn the windows-only gates on
V="$WORK/vendor"
sed -i 's/^#!\[cfg(windows)\]//' "$V/windows-0.58.0/src/lib.rs" "$V/windows-strings-0.1.0/src/lib.rs"
for c in windows-core-0.58.0 windows-result-0.2.0; do
  find "$V/$c/src" -name '*.rs' -exec sed -i 's/#\[cfg(windows)\]/#[cfg(all())]/g; s/#\[cfg(not(windows))\]/#[cfg(any())]/g' {} +
done
# std::os::windows is not available on the host: stub the three uses in hstring.rs
python3 - "$V/windows-strings-0.1.0/src/hstring.rs" <<'PY'
import sys
p = sys.argv[1]; s = open(p).read()
s = s.replace("std::os::windows::ffi::OsStringExt::from_wide(self.as_wide())", "{ let _ = self; std::ffi::OsString::new() }")
s = s.replace("std::os::windows::ffi::OsStrExt::encode_wide(value)", "{ let _ = value; core::iter::empty::<u16>() }")
s = s.replace("std::os::windows::ffi::OsStrExt::encode_wide(other)", "{ let _ = other; core::iter::empty::<u16>() }")
open(p, "w").write(s)
PY

# 3. the harness
cat > "$WORK/harness/Cargo.toml" <<T
[package]
name = "orange-winck"
version = "0.0.0"
edition = "2021"
publish = false
[workspace]
[dependencies]
orange-core = { path = "$ROOT/crates/orange-core" }
serde = { version = "1", features = ["derive"] }
windows-core = "0.58"
windows = { version = "0.58", features = ["implement","Win32_Foundation","Win32_Graphics_Gdi","Win32_System_Com","Win32_System_Com_StructuredStorage","Win32_System_DataExchange","Win32_System_LibraryLoader","Win32_System_Memory","Win32_System_Ole", "Win32_System_SystemServices","Win32_System_Threading","Win32_System_WinRT","Win32_UI_HiDpi","Win32_UI_Input_KeyboardAndMouse","Win32_UI_Shell","Win32_UI_Shell_Common","Win32_UI_WindowsAndMessaging","ApplicationModel_DataTransfer","Foundation","Foundation_Collections","Globalization","Graphics_Imaging","Media_FaceAnalysis","Media_Ocr","Storage","Storage_Streams"] }
[patch.crates-io]
windows = { path = "$V/windows-0.58.0" }
windows-core = { path = "$V/windows-core-0.58.0" }
windows-result = { path = "$V/windows-result-0.2.0" }
windows-strings = { path = "$V/windows-strings-0.1.0" }
windows-targets = { path = "$V/windows-targets-0.52.6" }
T
S="$ROOT/src-tauri/src"
cat > "$WORK/harness/src/lib.rs" <<L
#![allow(dead_code, unused_imports)]
#[path = "$S/session.rs"] pub mod session;
#[path = "$S/geometry.rs"] pub mod geometry;
#[path = "$S/fileactions.rs"] pub mod fileactions;
pub mod win {
    #[derive(Debug, Clone)]
    pub enum DropEvent { Enter { paths: Vec<String>, x: i32, y: i32 }, Over { x: i32, y: i32 }, Leave, Drop { paths: Vec<String>, x: i32, y: i32 } }
    #[path = "$S/win/droptarget.rs"] pub mod droptarget;
    #[path = "$S/win/hooks.rs"] pub mod hooks;
    #[path = "$S/win/overlay.rs"] pub mod overlay;
    #[path = "$S/win/clipboard.rs"] pub mod clipboard;
    pub mod proc {
        use std::path::Path;
        pub fn explorer_select(_p: &Path) -> Result<(), String> { Ok(()) }
        pub fn spawn_in_dir(_p: &str, _a: &[String], _d: &Path, _n: bool) -> Result<(), String> { Ok(()) }
    }
    #[path = "$S/win/shell.rs"] pub mod shell;
    #[path = "$S/win/ocr.rs"] pub mod ocr;
}
L
cd "$WORK/harness"
if CARGO_TARGET_DIR="$WORK/target" cargo check 2>"$WORK/check.log"; then
  echo "Windows layer type-checks against windows 0.58 (not executed)."
else
  grep -E "^error" -A12 "$WORK/check.log" | head -80; echo "TYPE CHECK FAILED"; exit 1
fi
