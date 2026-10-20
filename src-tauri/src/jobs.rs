//! Background jobs: run conversions/tools off the UI thread, stream progress, support cancel.

use orange_core::{CancelToken, Capabilities, Ctx, Error, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Where job events go (the Tauri app handle in production, a recorder in tests).
pub trait Emitter: Send + Sync + 'static {
    fn emit(&self, event: &str, payload: Value);
}

#[derive(Default)]
pub struct Jobs {
    next: AtomicU64,
    running: Mutex<HashMap<String, CancelToken>>,
}

impl Jobs {
    pub fn new() -> Arc<Jobs> {
        Arc::new(Jobs::default())
    }

    pub fn cancel(&self, id: &str) -> bool {
        match self.running.lock().unwrap().get(id) {
            Some(t) => {
                t.cancel();
                true
            }
            None => false,
        }
    }

    pub fn running_count(&self) -> usize {
        self.running.lock().unwrap().len()
    }

    /// Start `work` on a thread. Emits `job:progress` while running and one `job:done` at the end.
    /// Returns the job id immediately.
    pub fn spawn<F>(self: &Arc<Self>, emitter: Arc<dyn Emitter>, caps: Arc<Capabilities>, out_dir: Option<PathBuf>, work: F) -> String
    where
        F: FnOnce(&Ctx) -> Result<Vec<PathBuf>> + Send + 'static,
    {
        let id = format!("job-{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let cancel = CancelToken::new();
        self.running.lock().unwrap().insert(id.clone(), cancel.clone());
        let jobs = Arc::clone(self);
        let job_id = id.clone();
        std::thread::Builder::new()
            .name(format!("orange-{id}"))
            .spawn(move || {
                let progress_emitter = emitter.clone();
                let pid = job_id.clone();
                let mut ctx = Ctx::new(caps).with_out_dir(out_dir).with_progress(move |p| {
                    progress_emitter.emit("job:progress", json!({"job_id": pid, "fraction": p.fraction, "message": p.message}));
                });
                ctx.cancel = cancel;
                // a panic inside an external-tool wrapper must still end the job cleanly
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ctx)))
                    .unwrap_or_else(|_| Err(Error::msg("the job crashed unexpectedly")));
                jobs.running.lock().unwrap().remove(&job_id);
                let payload = match result {
                    Ok(outs) => json!({"job_id": job_id, "outputs": outs.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>(), "error": null, "cancelled": false}),
                    Err(Error::Cancelled) => json!({"job_id": job_id, "outputs": [], "error": null, "cancelled": true}),
                    Err(e) => json!({"job_id": job_id, "outputs": [], "error": e.to_string(), "cancelled": false}),
                };
                emitter.emit("job:done", payload);
            })
            .expect("spawn job thread");
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    struct Rec(Mutex<mpsc::Sender<(String, Value)>>);
    impl Emitter for Rec {
        fn emit(&self, event: &str, payload: Value) {
            let _ = self.0.lock().unwrap().send((event.to_string(), payload));
        }
    }
    fn rec() -> (Arc<dyn Emitter>, mpsc::Receiver<(String, Value)>) {
        let (tx, rx) = mpsc::channel();
        (Arc::new(Rec(Mutex::new(tx))), rx)
    }
    fn wait_done(rx: &mpsc::Receiver<(String, Value)>) -> (Vec<Value>, Value) {
        let mut progress = Vec::new();
        loop {
            let (ev, v) = rx.recv_timeout(Duration::from_secs(5)).expect("job:done");
            if ev == "job:done" {
                return (progress, v);
            }
            progress.push(v);
        }
    }
    fn caps() -> Arc<Capabilities> {
        Arc::new(Capabilities::default())
    }

    #[test]
    fn success_reports_progress_and_outputs() {
        let jobs = Jobs::new();
        let (em, rx) = rec();
        let id = jobs.spawn(em, caps(), None, |ctx| {
            ctx.report(0.5, "halfway");
            Ok(vec![PathBuf::from("out.png")])
        });
        let (progress, done) = wait_done(&rx);
        assert_eq!(done["job_id"], id.as_str());
        assert_eq!(done["outputs"][0], "out.png");
        assert_eq!(done["error"], Value::Null);
        assert_eq!(progress[0]["message"], "halfway");
        assert_eq!(progress[0]["job_id"], id.as_str());
        assert_eq!(jobs.running_count(), 0);
    }

    #[test]
    fn errors_are_reported() {
        let jobs = Jobs::new();
        let (em, rx) = rec();
        jobs.spawn(em, caps(), None, |_| Err(Error::msg("boom")));
        let (_, done) = wait_done(&rx);
        assert_eq!(done["error"], "boom");
        assert_eq!(done["cancelled"], false);
    }

    #[test]
    fn cancel_stops_the_job() {
        let jobs = Jobs::new();
        let (em, rx) = rec();
        let id = jobs.spawn(em, caps(), None, |ctx| {
            for _ in 0..500 {
                ctx.check_cancel()?;
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(vec![])
        });
        assert!(jobs.cancel(&id));
        let (_, done) = wait_done(&rx);
        assert_eq!(done["cancelled"], true);
        assert!(!jobs.cancel(&id), "already finished");
    }

    #[test]
    fn panics_become_errors_and_ids_are_unique() {
        let jobs = Jobs::new();
        let (em, rx) = rec();
        let a = jobs.spawn(em.clone(), caps(), None, |_| panic!("kaput"));
        let (_, done) = wait_done(&rx);
        assert!(done["error"].as_str().unwrap().contains("crashed"));
        let b = jobs.spawn(em, caps(), None, |_| Ok(vec![]));
        wait_done(&rx);
        assert_ne!(a, b);
    }
}
