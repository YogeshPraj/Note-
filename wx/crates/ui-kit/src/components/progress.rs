//! Long work with a progress window. The job runs on a worker thread; if it takes more than a moment a modal window
//! shows how far it got, and its Cancel button asks the job to stop. Use it for anything that would otherwise keep
//! the UI thread busy for longer than a blink.
//!
//! ```ignore
//! let sums = progress::run(&frame, "SHA-256", "Reading the files...", move |p| {
//!     p.set(50, "Half way");
//!     if p.cancelled() { return None; }
//!     Some(42)
//! });
//! ```

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use wxdragon::prelude::*;

/// How long a job may run before the window appears (short jobs never flash one).
const SHOW_AFTER: Duration = Duration::from_millis(300);

/// What a job tells the window, and what the window tells the job.
#[derive(Debug, Default)]
pub struct Progress {
    percent: AtomicU32,
    text: Mutex<String>,
    cancel: AtomicBool,
}

impl Progress {
    /// How far the job is (0 to 100) and what it is doing.
    pub fn set(&self, percent: u32, text: &str) {
        self.percent.store(percent.min(100), Ordering::Relaxed);
        text.clone_into(&mut self.text.lock().unwrap_or_else(PoisonError::into_inner));
    }

    /// Did the user press Cancel? The job should stop as soon as it can.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Run `job` on a worker thread, with a progress window over `parent` titled `title` while it takes long. Returns the
/// job's result, or None when the user cancelled (the job is waited for either way).
pub fn run<T: Send + 'static>(parent: &impl WxWidget, title: &str, first: &str, job: impl FnOnce(&Progress) -> T + Send + 'static) -> Option<T> {
    let progress = Arc::new(Progress::default());
    progress.set(0, first);
    let worker = progress.clone();
    let handle = std::thread::spawn(move || job(&worker));
    let start = Instant::now();
    while !handle.is_finished() && start.elapsed() < SHOW_AFTER {
        std::thread::sleep(Duration::from_millis(10));
    }
    if !handle.is_finished() {
        let dialog = ProgressDialog::builder(parent, title, first, 100).can_abort().show_elapsed_time().build();
        while !handle.is_finished() {
            let text = progress.text.lock().unwrap_or_else(PoisonError::into_inner).clone();
            // Kept below the maximum, which would hide the window before the job has returned.
            let at = crate::num::to_i32(progress.percent.load(Ordering::Relaxed).min(99));
            if !dialog.update(at, Some(&text)) {
                progress.cancel.store(true, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let result = handle.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    (!progress.cancelled()).then_some(result)
}
