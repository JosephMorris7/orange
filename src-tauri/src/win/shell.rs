//! Shell integration: Share sheet, Explorer reveal, terminal. Windows only (gated in `mod.rs`).

use super::proc;
use crate::fileactions::{terminal_command, terminal_dir};
use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::core::{factory, implement, Interface, Result as WinResult, HSTRING};
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::Collections::{IIterable, IIterable_Impl, IIterator, IIterator_Impl};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile, StorageFolder};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::IDataTransferManagerInterop;

// ---------------------------------------------------------------- reveal / terminal

/// Open Explorer with `path` selected.
pub fn reveal(path: &Path) -> Result<(), String> {
    proc::explorer_select(path)
}

fn have_windows_terminal() -> bool {
    if let Some(path) = std::env::var_os("PATH") {
        if std::env::split_paths(&path).any(|d| d.join("wt.exe").is_file()) {
            return true;
        }
    }
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Microsoft").join("WindowsApps").join("wt.exe").is_file()).unwrap_or(false)
}

/// Open Windows Terminal (or PowerShell) in the folder of `path`.
pub fn open_terminal(path: &Path) -> Result<(), String> {
    let dir = terminal_dir(path);
    let (program, args) = terminal_command(&dir, have_windows_terminal());
    let new_console = program != "wt.exe";
    proc::spawn_in_dir(&program, &args, &dir, new_console)
}

// ---------------------------------------------------------------- Share sheet

/// Minimal read-only WinRT list so we can hand `DataPackage::SetStorageItems` an `IIterable`.
#[implement(IIterable<IStorageItem>)]
struct StorageItems(Vec<IStorageItem>);

#[implement(IIterator<IStorageItem>)]
struct StorageItemsIter {
    items: Vec<IStorageItem>,
    pos: Mutex<usize>,
}

impl IIterable_Impl<IStorageItem> for StorageItems_Impl {
    fn First(&self) -> WinResult<IIterator<IStorageItem>> {
        Ok(StorageItemsIter { items: self.0.clone(), pos: Mutex::new(0) }.into())
    }
}

impl IIterator_Impl<IStorageItem> for StorageItemsIter_Impl {
    fn Current(&self) -> WinResult<IStorageItem> {
        let pos = *self.pos.lock().unwrap();
        self.items.get(pos).cloned().ok_or_else(|| windows::core::Error::from(windows::Win32::Foundation::E_BOUNDS))
    }
    fn HasCurrent(&self) -> WinResult<bool> {
        Ok(*self.pos.lock().unwrap() < self.items.len())
    }
    fn MoveNext(&self) -> WinResult<bool> {
        let mut p = self.pos.lock().unwrap();
        if *p < self.items.len() {
            *p += 1;
        }
        Ok(*p < self.items.len())
    }
    fn GetMany(&self, items: &mut [Option<IStorageItem>]) -> WinResult<u32> {
        let mut p = self.pos.lock().unwrap();
        let mut n = 0u32;
        for slot in items.iter_mut() {
            match self.items.get(*p) {
                Some(it) => {
                    *slot = Some(it.clone());
                    *p += 1;
                    n += 1;
                }
                None => break,
            }
        }
        Ok(n)
    }
}

/// Paths for the share currently in flight (the DataRequested handler is registered once per window).
static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static REGISTERED_FOR: Mutex<Option<isize>> = Mutex::new(None);

fn storage_item(path: &Path) -> WinResult<IStorageItem> {
    let h = HSTRING::from(path.as_os_str());
    if path.is_dir() {
        StorageFolder::GetFolderFromPathAsync(&h)?.get()?.cast::<IStorageItem>()
    } else {
        StorageFile::GetFileFromPathAsync(&h)?.get()?.cast::<IStorageItem>()
    }
}

fn on_data_requested(args: Option<&DataRequestedEventArgs>) -> WinResult<()> {
    let Some(args) = args else { return Ok(()) };
    let request = args.Request()?;
    let deferral = request.GetDeferral()?;
    let data = request.Data()?;
    data.Properties()?.SetTitle(&HSTRING::from("Share from Orange"))?;
    let paths = PENDING.lock().unwrap().clone();
    let mut items = Vec::new();
    for p in &paths {
        items.push(storage_item(p)?);
    }
    let iterable: IIterable<IStorageItem> = StorageItems(items).into();
    data.SetStorageItems(&iterable, true)?;
    deferral.Complete()
}

/// Show the Windows share sheet for these files. Must be called on the thread that owns `owner`
/// (the Tauri main thread).
pub fn share(owner: isize, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("nothing to share".into());
    }
    let run = || -> WinResult<()> {
        let hwnd = HWND(owner as *mut c_void);
        let interop = factory::<DataTransferManager, IDataTransferManagerInterop>()?;
        *PENDING.lock().unwrap() = paths.to_vec();
        let mut registered = REGISTERED_FOR.lock().unwrap();
        if *registered != Some(owner) {
            // SAFETY: GetForWindow returns the DataTransferManager bound to our window.
            let dtm: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
            dtm.DataRequested(&TypedEventHandler::new(|_sender: &Option<DataTransferManager>, args: &Option<DataRequestedEventArgs>| on_data_requested(args.as_ref())))?;
            *registered = Some(owner);
        }
        drop(registered);
        // SAFETY: shows the system share UI for our window.
        unsafe { interop.ShowShareUIForWindow(hwnd) }
    };
    run().map_err(|e| format!("could not open the share sheet: {e}"))
}
