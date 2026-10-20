//! Clipboard writes (files and text). Windows only (gated in `mod.rs`).

use crate::fileactions::{build_dropfiles, copy_path_text};
use std::ffi::c_void;
use std::path::PathBuf;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HWND};
use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE, GMEM_ZEROINIT};

const CF_UNICODETEXT: u32 = 13;
const CF_HDROP: u32 = 15;
const DROPEFFECT_COPY: u32 = 1;

/// Put `bytes` on the (already open) clipboard under `format`. On success the clipboard owns the memory.
unsafe fn put(format: u32, bytes: &[u8]) -> Result<(), String> {
    let hmem = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes.len().max(1)).map_err(|e| e.to_string())?;
    let p = GlobalLock(hmem);
    if p.is_null() {
        let _ = GlobalFree(hmem);
        return Err("could not lock clipboard memory".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
    let _ = GlobalUnlock(hmem);
    match SetClipboardData(format, HANDLE(hmem.0)) {
        Ok(_) => Ok(()),
        Err(e) => {
            let _ = GlobalFree(hmem);
            Err(e.to_string())
        }
    }
}

/// Open the clipboard (another app may hold it briefly), run `f`, always close it.
fn with_clipboard(owner: isize, f: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
    // SAFETY: OpenClipboard/EmptyClipboard/CloseClipboard pairing; `f` only calls `put` while open.
    unsafe {
        let hwnd = HWND(owner as *mut c_void);
        let mut opened = false;
        for _ in 0..20 {
            if OpenClipboard(hwnd).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        if !opened {
            return Err("the clipboard is busy; try again".into());
        }
        let r = EmptyClipboard().map_err(|e| e.to_string()).and_then(|_| f());
        let _ = CloseClipboard();
        r
    }
}

/// Copy files to the clipboard so Explorer can paste them.
pub fn set_files(owner: isize, paths: &[PathBuf]) -> Result<(), String> {
    let drop = build_dropfiles(paths);
    with_clipboard(owner, || unsafe {
        put(CF_HDROP, &drop)?;
        // tell Explorer this is a copy, not a cut
        let fmt = RegisterClipboardFormatW(w!("Preferred DropEffect"));
        if fmt != 0 {
            put(fmt, &DROPEFFECT_COPY.to_le_bytes())?;
        }
        Ok(())
    })
}

/// Copy text (UTF-16) to the clipboard.
pub fn set_text(owner: isize, text: &str) -> Result<(), String> {
    let mut bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    bytes.extend_from_slice(&[0, 0]);
    with_clipboard(owner, || unsafe { put(CF_UNICODETEXT, &bytes) })
}

/// Copy file paths as text, one per line.
pub fn set_paths_text(owner: isize, paths: &[PathBuf]) -> Result<(), String> {
    set_text(owner, &copy_path_text(paths))
}
