use crate::deps::Capabilities;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Progress {
    /// 0.0..=1.0 for the whole job
    pub fraction: f32,
    pub message: String,
}

#[derive(Clone, Default, Debug)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst)
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Everything a conversion / tool run needs.
#[derive(Clone)]
pub struct Ctx {
    pub caps: Arc<Capabilities>,
    /// Where outputs go. `None` = next to each input file (Orange default).
    pub out_dir: Option<PathBuf>,
    pub progress: Arc<dyn Fn(Progress) + Send + Sync>,
    pub cancel: CancelToken,
}

impl Ctx {
    pub fn new(caps: Arc<Capabilities>) -> Self {
        Ctx { caps, out_dir: None, progress: Arc::new(|_| {}), cancel: CancelToken::new() }
    }
    pub fn with_out_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.out_dir = dir;
        self
    }
    pub fn with_progress(mut self, f: impl Fn(Progress) + Send + Sync + 'static) -> Self {
        self.progress = Arc::new(f);
        self
    }
    pub fn report(&self, fraction: f32, message: impl Into<String>) {
        (self.progress)(Progress { fraction: fraction.clamp(0.0, 1.0), message: message.into() });
    }
    pub fn check_cancel(&self) -> Result<()> {
        if self.cancel.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
    /// A fresh temp dir that is deleted on drop.
    pub fn temp_dir(&self) -> Result<tempfile::TempDir> {
        Ok(tempfile::Builder::new().prefix("orange-").tempdir()?)
    }
}
