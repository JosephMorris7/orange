//! A tiny file logger so problems on someone else's PC can be diagnosed:
//! `%APPDATA%\Orange\orange.log` (cleared when it grows past 512 KB). No network, nothing leaves the PC.

use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

struct FileLogger {
    file: Mutex<std::fs::File>,
    start: Instant,
}

impl log::Log for FileLogger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        if let Ok(mut f) = self.file.lock() {
            let _ = writeln!(f, "[{:>9.3}] {:<5} {:<8} {}", self.start.elapsed().as_secs_f64(), r.level(), r.target(), r.args());
            let _ = f.flush();
        }
    }
    fn flush(&self) {}
}

pub fn init(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(path).map(|m| m.len() > 512 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(path);
    }
    let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(path) else { return };
    let logger = FileLogger { file: Mutex::new(file), start: Instant::now() };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    log::info!(target: "app", "---- Orange {} started ----", env!("CARGO_PKG_VERSION"));
    std::panic::set_hook(Box::new(|info| log::error!(target: "panic", "{info}")));
}
