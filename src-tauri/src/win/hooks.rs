//! Global low-level mouse + keyboard hooks. Windows only (gated in `mod.rs`).
//!
//! The hook callbacks do the bare minimum (translate the event, push it on a channel) because Windows
//! silently removes a low-level hook whose callback takes too long. All decisions are made by
//! `session::Session` on a worker thread.

use crate::session::{Modifier, RawEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::OnceLock;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_MENU,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PostThreadMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG,
    MSLLHOOKSTRUCT, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

static TX: OnceLock<Sender<RawEvent>> = OnceLock::new();
/// True while the left mouse button is held; mouse-move events are only forwarded then.
static LEFT_DOWN: AtomicBool = AtomicBool::new(false);

/// An unassigned virtual-key code: pressing it between Alt-down and Alt-up stops Windows from
/// treating the Alt tap as "activate the menu bar" (the same trick AutoHotkey uses).
const MASK_KEY: u16 = 0xE8;

fn send(ev: RawEvent) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(ev);
    }
}

fn modifier_of(vk: u16) -> Option<Modifier> {
    let v = VIRTUAL_KEY(vk);
    if v == VK_SHIFT || v == VK_LSHIFT || v == VK_RSHIFT {
        Some(Modifier::Shift)
    } else if v == VK_CONTROL || v == VK_LCONTROL || v == VK_RCONTROL {
        Some(Modifier::Control)
    } else if v == VK_MENU || v == VK_LMENU || v == VK_RMENU {
        Some(Modifier::Alt)
    } else {
        None
    }
}

unsafe fn inject_mask_key() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(MASK_KEY), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for WH_MOUSE_LL with HC_ACTION, lParam points to a valid MSLLHOOKSTRUCT for the call's duration.
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        match wparam.0 as u32 {
            WM_LBUTTONDOWN => {
                LEFT_DOWN.store(true, Ordering::SeqCst);
                send(RawEvent::MouseDown { x: info.pt.x, y: info.pt.y });
            }
            WM_LBUTTONUP => {
                LEFT_DOWN.store(false, Ordering::SeqCst);
                send(RawEvent::MouseUp);
            }
            WM_MOUSEMOVE if LEFT_DOWN.load(Ordering::Relaxed) => send(RawEvent::MouseMove { x: info.pt.x, y: info.pt.y }),
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn key_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        // SAFETY: for WH_KEYBOARD_LL with HC_ACTION, lParam points to a valid KBDLLHOOKSTRUCT for the call's duration.
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = (k.flags.0 & LLKHF_INJECTED.0) != 0;
        let msg = wparam.0 as u32;
        let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        let vk = k.vkCode as u16;
        if !injected && LEFT_DOWN.load(Ordering::Relaxed) {
            match modifier_of(vk) {
                Some(m) if down => send(RawEvent::KeyDown(m)),
                Some(m) if up => {
                    send(RawEvent::KeyUp(m));
                    if m == Modifier::Alt {
                        inject_mask_key();
                    }
                }
                _ => {
                    if down && VIRTUAL_KEY(vk) == VK_ESCAPE {
                        send(RawEvent::Escape);
                    }
                }
            }
        } else if !injected && up {
            // keep the session's "held keys" bookkeeping right even when the button is not down
            if let Some(m) = modifier_of(vk) {
                send(RawEvent::KeyUp(m));
            }
        } else if !injected && down {
            if let Some(m) = modifier_of(vk) {
                send(RawEvent::KeyDown(m));
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

pub struct HookHandle {
    thread_id: u32,
    join: Option<std::thread::JoinHandle<()>>,
}

impl HookHandle {
    /// Remove the hooks and stop the hook thread.
    pub fn stop(mut self) {
        // SAFETY: posting WM_QUIT to our own hook thread's message queue.
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// Install both hooks on a dedicated thread with its own message loop. Events are sent to `tx`.
pub fn start(tx: Sender<RawEvent>) -> Result<HookHandle, String> {
    TX.set(tx).map_err(|_| "hooks already started".to_string())?;
    let (ready_tx, ready_rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    let join = std::thread::Builder::new()
        .name("orange-hooks".into())
        .spawn(move || {
            // SAFETY: standard hook installation + message pump on the thread that owns the hooks.
            unsafe {
                let module = GetModuleHandleW(None).unwrap_or_default();
                let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), module, 0);
                let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_proc), module, 0);
                match (mouse, keyboard) {
                    (Ok(m), Ok(k)) => {
                        let _ = ready_tx.send(Ok(GetCurrentThreadId()));
                        let mut msg = MSG::default();
                        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                            let _ = TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                        let _ = UnhookWindowsHookEx(m);
                        let _ = UnhookWindowsHookEx(k);
                    }
                    (m, k) => {
                        if let Ok(m) = m {
                            let _ = UnhookWindowsHookEx(m);
                        }
                        if let Ok(k) = k {
                            let _ = UnhookWindowsHookEx(k);
                        }
                        let _ = ready_tx.send(Err("could not install the input hooks".to_string()));
                    }
                }
            }
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv() {
        Ok(Ok(thread_id)) => Ok(HookHandle { thread_id, join: Some(join) }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("hook thread exited early".to_string()),
    }
}
