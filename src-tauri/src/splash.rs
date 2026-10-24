//! The startup animation: the Orange wheel spins up in the middle of the screen and flies into
//! Orange's tray icon.
//!
//! A full-screen, transparent, click-through window shows `splash.html`. Rust only works out WHERE the tray
//! icon is; the page does all the drawing. If Windows cannot tell us (the icon sits in the hidden overflow
//! flyout, or the taskbar is hidden) we aim at where the notification area normally is.

use crate::geometry::Rect;
#[cfg(windows)]
use crate::win::overlay;
use serde::Serialize;
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const LABEL: &str = "splash";
/// The window is closed after this long even if the page never reports back.
const SAFETY_CLOSE_MS: u64 = 7000;

/// What the page needs, in physical pixels relative to the splash window's top-left corner.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Plan {
    pub target_x: i32,
    pub target_y: i32,
    pub width: i32,
    pub height: i32,
}

/// Screen position the animation is aimed at, where it will be placed and whether it was a guess.
#[derive(Clone, Copy, Debug)]
struct Placement {
    monitor: Rect,
    plan: Plan,
}

static PENDING: Mutex<Option<Placement>> = Mutex::new(None);
/// What the page tells the thread that is running the animation.
enum Msg {
    /// The wheel has finished spinning: open Orange's window now, the wheel is about to fly to the tray.
    Opened,
    /// The animation is over.
    Done,
}
static CHANNEL: Mutex<Option<Sender<Msg>>> = Mutex::new(None);

/// Centre of the tray icon if Windows gave a sensible rectangle for it, otherwise `None`.
/// `icon` is `(left, top, right, bottom)` in physical screen pixels.
fn icon_centre(icon: Option<(i32, i32, i32, i32)>, monitor: Rect) -> Option<(i32, i32)> {
    let (l, t, r, b) = icon?;
    if r - l < 4 || b - t < 4 || r - l > 200 || b - t > 200 {
        return None; // empty or implausible
    }
    let c = ((l + r) / 2, (t + b) / 2);
    let inside = c.0 >= monitor.left && c.0 < monitor.right && c.1 >= monitor.top && c.1 < monitor.bottom;
    inside.then_some(c)
}

/// Where the notification area normally is, judged from the taskbar: the strip between the monitor and its
/// work area. `dpi` is the monitor's scale (1.0 = 96 dpi).
fn guess_notification_area(monitor: Rect, work: Rect, dpi: f64) -> (i32, i32) {
    let px = |v: f64| (v * dpi).round() as i32;
    let mid = |a: i32, b: i32| (a + b) / 2;
    if work.bottom < monitor.bottom {
        // taskbar along the bottom: clock and tray are at the right end
        (monitor.right - px(130.0), mid(work.bottom, monitor.bottom))
    } else if work.top > monitor.top {
        (monitor.right - px(130.0), mid(monitor.top, work.top))
    } else if work.left > monitor.left {
        (mid(monitor.left, work.left), monitor.bottom - px(130.0))
    } else if work.right < monitor.right {
        (mid(work.right, monitor.right), monitor.bottom - px(130.0))
    } else {
        // taskbar hidden: head for the bottom-right corner
        (monitor.right - px(60.0), monitor.bottom - px(30.0))
    }
}

/// Final aim point in physical screen pixels.
pub fn pick_target(icon: Option<(i32, i32, i32, i32)>, monitor: Rect, work: Rect, dpi: f64) -> (i32, i32) {
    icon_centre(icon, monitor).unwrap_or_else(|| guess_notification_area(monitor, work, dpi))
}

fn show_window(w: &WebviewWindow, m: Rect) {
    #[cfg(windows)]
    {
        overlay::show_at(crate::windows::hwnd_of(w), m.left, m.top, m.width(), m.height());
    }
    #[cfg(not(windows))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = w.set_position(PhysicalPosition::new(m.left, m.top));
        let _ = w.set_size(PhysicalSize::new(m.width() as u32, m.height() as u32));
        let _ = w.show();
    }
}

fn close_window(app: &AppHandle) {
    *PENDING.lock().unwrap() = None;
    if let Some(w) = app.get_webview_window(LABEL) {
        let _ = w.close();
    }
}

/// Work out the plan for the primary monitor (the one with the taskbar and tray).
fn compute(app: &AppHandle) -> Option<Placement> {
    let mon = app.primary_monitor().ok().flatten()?;
    let (p, s) = (mon.position(), mon.size());
    let monitor = Rect { left: p.x, top: p.y, right: p.x + s.width as i32, bottom: p.y + s.height as i32 };
    let wa = mon.work_area();
    let work = Rect { left: wa.position.x, top: wa.position.y, right: wa.position.x + wa.size.width as i32, bottom: wa.position.y + wa.size.height as i32 };
    let icon = app.tray_by_id("orange").and_then(|t| t.rect().ok().flatten()).map(|r| {
        let pos = r.position.to_physical::<i32>(mon.scale_factor());
        let size = r.size.to_physical::<i32>(mon.scale_factor());
        (pos.x, pos.y, pos.x + size.width, pos.y + size.height)
    });
    let (tx, ty) = pick_target(icon, monitor, work, mon.scale_factor());
    log::info!(target: "splash", "tray icon rect={icon:?} monitor={monitor:?} work={work:?} -> target=({tx},{ty})");
    Some(Placement { monitor, plan: Plan { target_x: tx - monitor.left, target_y: ty - monitor.top, width: monitor.width(), height: monitor.height() } })
}

/// Play the animation after `delay_ms`. `show_ui` says whether Orange's own window should open as part of it.
///
/// The order is: the wheel appears in the middle of the screen, spins, and the name "Orange" fades in under it.
/// When the spin ends the page reports `splash_opened`, Orange's window opens at once, and the wheel drops
/// into the tray. If the animation cannot run at all, the window still opens.
/// The page calls `splash_ready` when it has loaded, which shows the splash window and starts the animation.
/// Runs on its own thread so it never stalls start-up.
pub fn start(app: &AppHandle, delay_ms: u64, show_ui: bool) {
    let app = app.clone();
    log::info!(target: "splash", "animation v3: will start in {delay_ms} ms (open the window during it: {show_ui})");
    let _ = std::thread::Builder::new().name("orange-splash".into()).spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
        let mut ui_shown = false;
        let mut open_ui = |app: &AppHandle| {
            if show_ui && !ui_shown {
                ui_shown = true;
                crate::windows::show_main(app);
            }
        };
        play(&app, show_ui, &mut open_ui);
        open_ui(&app);
    });
}

/// Runs the whole animation and returns when it is over (or has failed).
fn play(app: &AppHandle, show_ui: bool, open_ui: &mut dyn FnMut(&AppHandle)) {
    let Some(placement) = compute(app) else {
        log::warn!(target: "splash", "no primary monitor, skipping");
        return;
    };
    *PENDING.lock().unwrap() = Some(placement);
    let (tx, rx) = channel::<Msg>();
    *CHANNEL.lock().unwrap() = Some(tx);
    let built = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("splash.html".into()))
        .title("Orange")
        .inner_size(400.0, 300.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .build();
    match built {
        Ok(w) => {
            let _ = w.set_ignore_cursor_events(true);
            #[cfg(windows)]
            overlay::make_noactivate(crate::windows::hwnd_of(&w));
        }
        Err(e) => {
            log::warn!(target: "splash", "could not create the window: {e}");
            *PENDING.lock().unwrap() = None;
            *CHANNEL.lock().unwrap() = None;
            return;
        }
    }
    // Load Orange's window in the background while the wheel spins, so it appears fully drawn (no white flash).
    if show_ui {
        crate::windows::prepare_main(app);
    }
    // The page reports back twice; give up after a while if it never does.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(SAFETY_CLOSE_MS);
    loop {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        match rx.recv_timeout(left) {
            Ok(Msg::Opened) => {
                log::info!(target: "splash", "the spin is over: opening Orange's window while the wheel drops into the tray");
                open_ui(app)
            }
            Ok(Msg::Done) | Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    *CHANNEL.lock().unwrap() = None;
    close_window(app);
}

/// The page has loaded: place and show the window, and hand over where to fly to.
#[tauri::command]
pub async fn splash_ready(app: AppHandle) -> Result<Plan, String> {
    let placement = {
        // the tray may have settled since the window was prepared, so look again
        let fresh = compute(&app);
        let mut slot = PENDING.lock().unwrap();
        let chosen = fresh.or(*slot).ok_or_else(|| "no splash pending".to_string())?;
        *slot = Some(chosen);
        chosen
    };
    let w = app.get_webview_window(LABEL).ok_or_else(|| "no splash window".to_string())?;
    show_window(&w, placement.monitor);
    Ok(placement.plan)
}

/// The wheel has finished spinning: open Orange's window now.
#[tauri::command]
pub async fn splash_opened() {
    if let Some(tx) = CHANNEL.lock().unwrap().as_ref() {
        let _ = tx.send(Msg::Opened);
    }
}

/// The animation has finished: remove the window.
#[tauri::command]
pub async fn splash_done(app: AppHandle) {
    if let Some(tx) = CHANNEL.lock().unwrap().take() {
        let _ = tx.send(Msg::Done);
    }
    close_window(&app);
}

#[cfg(test)]
mod tests {
    use super::*;
    const MON: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1080 };
    const WORK_BOTTOM: Rect = Rect { left: 0, top: 0, right: 1920, bottom: 1032 };

    #[test]
    fn uses_the_icon_when_windows_reports_it() {
        let t = pick_target(Some((1800, 1040, 1824, 1064)), MON, WORK_BOTTOM, 1.0);
        assert_eq!(t, (1812, 1052));
    }

    #[test]
    fn ignores_empty_or_absurd_rectangles() {
        for bad in [(0, 0, 0, 0), (10, 10, 12, 12), (0, 0, 1920, 1080)] {
            let t = pick_target(Some(bad), MON, WORK_BOTTOM, 1.0);
            assert_eq!(t, (1790, 1056), "rect {bad:?} should fall back");
        }
    }

    #[test]
    fn ignores_a_rectangle_off_the_primary_monitor() {
        assert_eq!(pick_target(Some((3000, 1040, 3024, 1064)), MON, WORK_BOTTOM, 1.0), (1790, 1056));
    }

    #[test]
    fn falls_back_to_the_notification_area_for_a_bottom_taskbar() {
        assert_eq!(pick_target(None, MON, WORK_BOTTOM, 1.0), (1790, 1056));
        // 150% scaling moves the guess in proportion
        let mon = Rect { left: 0, top: 0, right: 2880, bottom: 1620 };
        let work = Rect { left: 0, top: 0, right: 2880, bottom: 1572 };
        assert_eq!(pick_target(None, mon, work, 1.5), (2685, 1596));
    }

    #[test]
    fn handles_a_top_left_and_right_taskbar_and_a_hidden_one() {
        let top = Rect { left: 0, top: 48, right: 1920, bottom: 1080 };
        assert_eq!(guess_notification_area(MON, top, 1.0), (1790, 24));
        let left = Rect { left: 48, top: 0, right: 1920, bottom: 1080 };
        assert_eq!(guess_notification_area(MON, left, 1.0), (24, 950));
        let right = Rect { left: 0, top: 0, right: 1872, bottom: 1080 };
        assert_eq!(guess_notification_area(MON, right, 1.0), (1896, 950));
        assert_eq!(guess_notification_area(MON, MON, 1.0), (1860, 1050));
    }

    #[test]
    fn a_monitor_with_a_non_zero_origin_still_works() {
        let mon = Rect { left: 1920, top: 0, right: 3840, bottom: 1080 };
        let work = Rect { left: 1920, top: 0, right: 3840, bottom: 1032 };
        assert_eq!(pick_target(None, mon, work, 1.0), (3710, 1056));
    }
}
