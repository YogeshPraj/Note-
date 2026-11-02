//! Find in Files on a worker thread. The panel says how many files have been searched, its Find All button becomes
//! Stop, and the hits are listed when the worker is done; the window stays usable meanwhile.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError};

use notepp_core::search::{self, Hit, Opts, SearchProgress};

use crate::app::App;
use crate::findpanel;
use crate::state::FindInFilesJob;

impl App {
    /// Find All in Files: search the panel's folder, or stop the search that is running. Returns the panel's note.
    pub(crate) fn in_files_toggle(&mut self, opts: &Opts) -> String {
        if let Some(job) = &self.results.job {
            job.progress.cancel.store(true, Ordering::Relaxed);
            return "Stopping...".into();
        }
        let (dir, filters) = findpanel::read_dir_filter();
        if dir.as_os_str().is_empty() || !dir.is_dir() {
            return "Choose an existing directory first".into();
        }
        let progress = Arc::new(SearchProgress::default());
        let outcome = Arc::new(Mutex::new(None));
        let (p, out, d, o) = (progress.clone(), outcome.clone(), dir.clone(), opts.clone());
        std::thread::spawn(move || {
            let r = search::find_in_files_with(&d, &filters, &o, &p);
            *out.lock().unwrap_or_else(PoisonError::into_inner) = Some(r);
        });
        self.results.job = Some(FindInFilesJob { dir, progress, outcome });
        findpanel::set_searching(true);
        "Searching...".into()
    }

    /// Timer: show how far the running search is, and list its hits once it is done.
    pub(crate) fn in_files_poll(&mut self) {
        let Some(job) = &self.results.job else { return };
        let done = job.outcome.lock().unwrap_or_else(PoisonError::into_inner).take();
        let Some(outcome) = done else {
            let files = job.progress.files.load(Ordering::Relaxed);
            let what = if job.progress.cancel.load(Ordering::Relaxed) { "Stopping" } else { "Searching" };
            return findpanel::set_status(&format!("{what}... {} files searched", notepp_core::workspace::commafy(files)));
        };
        let Some(job) = self.results.job.take() else { return };
        findpanel::set_searching(false);
        let note = match outcome {
            Ok((hits, complete)) => self.list_in_files_hits(job.dir, hits, complete),
            Err(e) => crate::app::error_note(&e),
        };
        findpanel::set_status(&note);
    }

    /// Stop a running search (the panel is closing).
    pub(crate) fn in_files_cancel(&self) {
        if let Some(job) = &self.results.job {
            job.progress.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Put a finished search's hits in the results list; the note says how many, and whether it was stopped.
    fn list_in_files_hits(&mut self, dir: PathBuf, hits: Vec<Hit>, complete: bool) -> String {
        let files: std::collections::HashSet<_> = hits.iter().filter_map(|h| h.path.as_ref()).collect();
        let limit = if hits.len() >= search::MAX_HITS { " (limit reached)" } else { "" };
        let stopped = if complete { "" } else { " (stopped)" };
        let note = format!("{} hits in {} files{limit}{stopped}", hits.len(), files.len());
        let lines: Vec<String> = hits
            .iter()
            .map(|h| {
                let p = h.path.as_deref().unwrap_or(Path::new(""));
                format!("{} ({}):  {}", p.strip_prefix(&dir).unwrap_or(p).display(), h.line, h.text)
            })
            .collect();
        self.results.hits = hits;
        self.results.root = dir;
        findpanel::set_results(lines);
        note
    }
}
