//! Platform layer. The real implementations only compile on Windows; elsewhere `stubs` provide the same API.
//! (The Windows files are type-checked against the `windows` crate signatures by
//! `scripts/check-windows-types.sh` and by CI on windows-latest, but can only be *run* on a real Windows PC.)

#[cfg(windows)]
pub mod clipboard;
#[cfg(windows)]
pub mod droptarget;
#[cfg(windows)]
pub mod hooks;
#[cfg(windows)]
pub mod ocr;
#[cfg(windows)]
pub mod overlay;
#[cfg(windows)]
mod proc;
#[cfg(windows)]
pub mod shell;

#[cfg(not(windows))]
mod stubs;
#[cfg(not(windows))]
#[allow(unused_imports)]
pub use self::stubs::{clipboard, droptarget, hooks, ocr, overlay, shell};

/// A file drag over the wheel window, as seen by the native drop target. Positions are physical pixels
/// relative to the window's client area.
#[derive(Debug, Clone)]
pub enum DropEvent {
    Enter { paths: Vec<String>, x: i32, y: i32 },
    Over { x: i32, y: i32 },
    Leave,
    Drop { paths: Vec<String>, x: i32, y: i32 },
}
