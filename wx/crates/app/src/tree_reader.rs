//! Reading a folder of the workspace tree on a worker thread: a slow folder (a network share, a huge directory) must not
//! freeze the window. Most folders are read within a moment and fill in at once; the rest keep their placeholder until
//! the timer finds them read.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use wxdragon::prelude::{TreeCtrl, TreeItemId};

use crate::panels;

type Entries = Vec<(PathBuf, bool)>;
type Slot = Arc<Mutex<Option<Entries>>>;

/// How long expanding waits for a folder before leaving it to the background.
const QUICK_WAIT: Duration = Duration::from_millis(50);

thread_local! {
    /// Folders being read in the background: their tree item and path.
    static PENDING: RefCell<Vec<(TreeItemId, PathBuf, Slot)>> = const { RefCell::new(Vec::new()) };
}

fn take(slot: &Slot) -> Option<Entries> {
    slot.lock().unwrap_or_else(PoisonError::into_inner).take()
}

/// A folder of the tree is being expanded: read it, at once when that is quick, else in the background.
pub fn expand(tree: &TreeCtrl, item: &TreeItemId) {
    let Some(dir) = panels::path_of(tree, item) else { return };
    if !panels::needs_reading(tree, item) || PENDING.with(|p| p.borrow().iter().any(|(_, d, _)| *d == dir)) {
        return;
    }
    let slot: Slot = Arc::new(Mutex::new(None));
    let (out, read) = (slot.clone(), dir.clone());
    std::thread::spawn(move || {
        let entries = panels::list_dir(&read);
        *out.lock().unwrap_or_else(PoisonError::into_inner) = Some(entries);
    });
    let deadline = Instant::now() + QUICK_WAIT;
    while Instant::now() < deadline {
        if let Some(entries) = take(&slot) {
            return panels::fill(tree, item, entries);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    PENDING.with(|p| p.borrow_mut().push((item.clone(), dir, slot)));
}

/// Timer: fill in the folders that have been read since.
pub fn poll(tree: &TreeCtrl) {
    let done: Vec<(TreeItemId, Entries)> = PENDING.with(|p| {
        let mut ready = Vec::new();
        p.borrow_mut().retain(|(item, _, slot)| match take(slot) {
            Some(entries) => {
                ready.push((item.clone(), entries));
                false
            }
            None => true,
        });
        ready
    });
    for (item, entries) in done {
        panels::fill(tree, &item, entries);
    }
}

/// The tree is being rebuilt: its items are about to go, so results for them must not be filled in.
pub fn forget() {
    PENDING.with(|p| p.borrow_mut().clear());
}
