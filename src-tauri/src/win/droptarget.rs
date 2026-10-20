//! A native OLE drop target for the wheel overlay. Windows only (gated in `mod.rs`).
//!
//! The web view's own file-drop handling never saw the drag in testing, so the overlay window itself registers
//! as the drop target and reads the dragged file paths (CF_HDROP) directly. Events are reported in physical
//! pixels relative to the window's client area.

use super::DropEvent;
use std::ffi::c_void;
use std::sync::Arc;
use windows::core::{implement, Result};
use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, POINTL, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop, DROPEFFECT, DROPEFFECT_COPY,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, RegisterClassExW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow,
    HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use std::sync::atomic::{AtomicIsize, Ordering};

type Callback = Arc<dyn Fn(DropEvent) + Send + Sync>;

const CF_HDROP: u16 = 15;

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

#[implement(IDropTarget)]
struct Target {
    window: isize,
    cb: Callback,
}

impl Target {
    fn client(&self, pt: &POINTL) -> (i32, i32) {
        let mut p = POINT { x: pt.x, y: pt.y };
        // SAFETY: coordinate conversion for our own window handle.
        unsafe {
            let _ = ScreenToClient(hwnd(self.window), &mut p);
        }
        (p.x, p.y)
    }
}

/// File paths carried by a drag, empty when it carries no files.
fn paths_of(data: Option<&IDataObject>) -> Vec<String> {
    let Some(data) = data else { return Vec::new() };
    let fmt = FORMATETC { cfFormat: CF_HDROP, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    // SAFETY: GetData fills a medium that is released below; the HDROP is only read while the medium is alive.
    unsafe {
        let Ok(mut medium) = data.GetData(&fmt) else { return Vec::new() };
        let drop = HDROP(medium.u.hGlobal.0);
        let count = DragQueryFileW(drop, 0xFFFF_FFFF, None);
        let mut out = Vec::new();
        for i in 0..count {
            let len = DragQueryFileW(drop, i, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let n = DragQueryFileW(drop, i, Some(&mut buf)) as usize;
            out.push(String::from_utf16_lossy(&buf[..n.min(buf.len())]));
        }
        ReleaseStgMedium(&mut medium);
        out
    }
}

impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let paths = paths_of(pdataobj);
        let (x, y) = self.client(pt);
        // SAFETY: OLE passes a valid out pointer.
        unsafe { *pdweffect = DROPEFFECT_COPY };
        (self.cb)(DropEvent::Enter { paths, x, y });
        Ok(())
    }

    fn DragOver(&self, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let (x, y) = self.client(pt);
        // SAFETY: OLE passes a valid out pointer.
        unsafe { *pdweffect = DROPEFFECT_COPY };
        (self.cb)(DropEvent::Over { x, y });
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        (self.cb)(DropEvent::Leave);
        Ok(())
    }

    fn Drop(&self, pdataobj: Option<&IDataObject>, _keys: MODIFIERKEYS_FLAGS, pt: &POINTL, pdweffect: *mut DROPEFFECT) -> Result<()> {
        let paths = paths_of(pdataobj);
        let (x, y) = self.client(pt);
        // Copy, never move: the source file must stay where it is.
        // SAFETY: OLE passes a valid out pointer.
        unsafe { *pdweffect = DROPEFFECT_COPY };
        (self.cb)(DropEvent::Drop { paths, x, y });
        Ok(())
    }
}

// ---- the catcher window ------------------------------------------------------------------------------
//
// The web view's own windows belong to another process, and Windows does not look past them for the wheel
// window's drop target (the pointer showed a "no entry" sign). So while a file is being dragged, a nearly
// invisible native window of ours is placed exactly over the wheel; it is the window Windows finds under the
// pointer, and it is the drop target.

static CATCHER: AtomicIsize = AtomicIsize::new(0);

unsafe extern "system" fn catcher_proc(h: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(h, msg, w, l)
}

/// Create the catcher window once (on the UI thread) and register it as a drop target.
pub fn ensure_catcher(cb: Callback) -> std::result::Result<String, String> {
    if CATCHER.load(Ordering::SeqCst) != 0 {
        return Ok("catcher already exists".into());
    }
    // SAFETY: window class and window creation on the UI thread; the handle is kept for the process lifetime.
    unsafe {
        let ole = OleInitialize(None);
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let inst = HINSTANCE(module.0);
        let class = w!("OrangeDropCatcher");
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(catcher_proc),
            hInstance: inst,
            lpszClassName: class,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        let h = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("Orange drop catcher"),
            WS_POPUP,
            0,
            0,
            10,
            10,
            None,
            None,
            inst,
            None,
        )
        .map_err(|e| e.to_string())?;
        // Alpha 1 of 255: invisible, but still found by hit-testing (fully transparent would be skipped).
        SetLayeredWindowAttributes(h, COLORREF(0), 1, LWA_ALPHA).map_err(|e| e.to_string())?;
        let window = h.0 as isize;
        let target: IDropTarget = Target { window, cb }.into();
        let reg = RegisterDragDrop(h, &target);
        let info = format!("catcher={:#x} ole_init={:?} register={:?} thread={}", window, ole, reg, GetCurrentThreadId());
        reg.map_err(|e| e.to_string())?;
        CATCHER.store(window, Ordering::SeqCst);
        Ok(info)
    }
}

/// Put the catcher exactly over the given physical-pixel rectangle, above the wheel.
pub fn show_catcher(x: i32, y: i32, w: i32, h: i32) {
    let c = CATCHER.load(Ordering::SeqCst);
    if c == 0 {
        return;
    }
    // SAFETY: SetWindowPos on our own window.
    unsafe {
        let _ = SetWindowPos(hwnd(c), HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

pub fn hide_catcher() {
    let c = CATCHER.load(Ordering::SeqCst);
    if c == 0 {
        return;
    }
    // SAFETY: ShowWindow on our own window.
    unsafe {
        let _ = ShowWindow(hwnd(c), SW_HIDE);
    }
}
