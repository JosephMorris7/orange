//! Low-level window helpers for the wheel overlay. Windows only (gated in `mod.rs`).
//! Everything takes the raw HWND as `isize` so the Tauri crate's own `windows` version never mixes with ours.

use crate::geometry::Rect;
use std::ffi::c_void;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE, HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SW_HIDE, WS_CAPTION, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU,
    WS_THICKFRAME,
};

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// Make the window never take focus (a focus change would cancel the drag in the source app),
/// keep it out of Alt-Tab and always on top.
pub fn make_noactivate(window: isize) {
    // SAFETY: plain style read-modify-write on a window handle owned by this process.
    unsafe {
        let h = hwnd(window);
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        let add = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0) as isize;
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | add);
    }
    strip_frame(window);
}

/// Remove the title bar and borders. A tool-style window can get a thin caption from Windows even when the
/// toolkit asked for none, so this is applied at startup and again every time the wheel is shown.
pub fn strip_frame(window: isize) {
    // SAFETY: style read-modify-write plus a frame-changed notification on our own window.
    unsafe {
        let h = hwnd(window);
        let style = GetWindowLongPtrW(h, GWL_STYLE);
        let strip = (WS_CAPTION.0 | WS_THICKFRAME.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0) as isize;
        let new = (style & !strip) | WS_POPUP.0 as isize;
        if new != style {
            SetWindowLongPtrW(h, GWL_STYLE, new);
            let _ = SetWindowPos(h, HWND::default(), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
        }
    }
}

/// Show at the given physical-pixel rectangle without activating, above all other windows.
pub fn show_at(window: isize, x: i32, y: i32, w: i32, h: i32) {
    strip_frame(window);
    // SAFETY: SetWindowPos on our own top-level window.
    unsafe {
        let _ = SetWindowPos(hwnd(window), HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE | SWP_SHOWWINDOW);
    }
}

pub fn hide(window: isize) {
    // SAFETY: ShowWindow on our own top-level window.
    unsafe {
        let _ = ShowWindow(hwnd(window), SW_HIDE);
    }
}

/// Work area (monitor minus taskbar) of the monitor containing the point, in physical pixels.
pub fn work_area_at(x: i32, y: i32) -> Option<Rect> {
    // SAFETY: MonitorFromPoint never fails with DEFAULTTONEAREST; MONITORINFO.cbSize is set as required.
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(mon, &mut info).as_bool() {
            let r = info.rcWork;
            Some(Rect { left: r.left, top: r.top, right: r.right, bottom: r.bottom })
        } else {
            None
        }
    }
}

/// DPI scale (1.0 = 96 dpi) of the monitor containing the point.
pub fn dpi_scale_at(x: i32, y: i32) -> f64 {
    // SAFETY: out-parameters are valid for the call.
    unsafe {
        let mon = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let (mut dx, mut dy) = (0u32, 0u32);
        match GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) {
            Ok(()) if dx > 0 => dx as f64 / 96.0,
            _ => 1.0,
        }
    }
}
