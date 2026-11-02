//! Project file discovery for Quick Open, and crash-safe file writing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

/// Write `bytes` to `path` so a crash or full disk mid-save can never leave a truncated file:
/// the data goes to a temporary file in the same folder, is flushed to disk, then replaces the
/// target in one step. Symbolic links, read-only files and folders that refuse a new file are
/// written in place instead, so those keep behaving as before.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path).ok();
    let in_place = meta.as_ref().is_some_and(|m| m.file_type().is_symlink() || m.permissions().readonly());
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else { return std::fs::write(path, bytes) };
    if in_place || (meta.is_none() && !dir.as_os_str().is_empty() && !dir.exists()) {
        return std::fs::write(path, bytes);
    }
    let dir = if dir.as_os_str().is_empty() { Path::new(".") } else { dir };
    let tmp = dir.join(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let staged = (|| {
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if staged.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return std::fs::write(path, bytes);
    }
    std::fs::rename(&tmp, path).or_else(|_| {
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(path, bytes)
    })
}

/// Directories never worth indexing (same list the Electron app skips).
const SKIP: [&str; 14] = ["node_modules", "target", ".git", ".svn", ".hg", "dist", "build", "out", ".next", ".cache", ".vscode", ".idea", "__pycache__", "bin"];

/// The nearest ancestor of `start` (up to 8 levels) containing `.git`, else `start`.
pub fn project_root(start: &Path) -> PathBuf {
    let mut dir = start;
    for _ in 0..8 {
        if dir.join(".git").exists() {
            return dir.to_path_buf();
        }
        match dir.parent() {
            Some(p) => dir = p,
            None => break,
        }
    }
    start.to_path_buf()
}

/// Breadth-first scan of `root`, shallow files first, up to `limit` files.
pub fn scan_files(root: &Path, limit: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut queue = std::collections::VecDeque::from([root.to_path_buf()]);
    while let Some(dir) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    queue.push_back(e.path());
                }
            } else {
                out.push(e.path());
                if out.len() >= limit {
                    return out;
                }
            }
        }
    }
    out
}

/// Most files Quick Open lists from a folder.
pub const INDEX_LIMIT: usize = 8000;
/// A listing older than this is refreshed in the background the next time it is used.
const INDEX_REFRESH: Duration = Duration::from_secs(30);

type Listing = Arc<Mutex<Option<Vec<PathBuf>>>>;

/// The files under a project folder for Quick Open, listed on a worker thread (walking a large tree takes seconds, and
/// must not freeze the window). Callers get the last finished listing; a stale or missing one is refreshed meanwhile.
#[derive(Debug, Default)]
pub struct FileIndex {
    root: Option<PathBuf>,
    files: Vec<PathBuf>,
    listed: Option<Instant>,
    running: Option<(PathBuf, Listing)>,
}

impl FileIndex {
    /// The files under `root` as last listed, starting a fresh listing when there is none or it is stale. With no
    /// earlier listing of `root`, waits up to `wait` for the first one; None if it is still running.
    pub fn files_under(&mut self, root: &Path, wait: Duration) -> Option<&[PathBuf]> {
        self.poll();
        let current = self.root.as_deref() == Some(root);
        if !current || self.listed.is_none_or(|t| t.elapsed() >= INDEX_REFRESH) {
            self.start(root);
        }
        let deadline = Instant::now() + wait;
        while self.root.as_deref() != Some(root) && self.running.is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
            self.poll();
        }
        (self.root.as_deref() == Some(root)).then_some(self.files.as_slice())
    }

    /// Start listing `root` in the background, unless that is already under way.
    pub fn start(&mut self, root: &Path) {
        if self.running.as_ref().is_some_and(|(r, _)| r == root) {
            return;
        }
        let listing: Listing = Arc::new(Mutex::new(None));
        let (out, dir) = (listing.clone(), root.to_path_buf());
        std::thread::spawn(move || {
            let files = scan_files(&dir, INDEX_LIMIT);
            *out.lock().unwrap_or_else(PoisonError::into_inner) = Some(files);
        });
        self.running = Some((root.to_path_buf(), listing));
    }

    /// Take a finished listing.
    pub fn poll(&mut self) {
        let Some((root, listing)) = &self.running else { return };
        let Some(files) = listing.lock().unwrap_or_else(PoisonError::into_inner).take() else { return };
        self.root = Some(root.clone());
        self.files = files;
        self.listed = Some(Instant::now());
        self.running = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_lists_in_the_background_and_answers_once_done() {
        let d = std::env::temp_dir().join(format!("notepp_index_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("a.txt"), "").unwrap();
        std::fs::write(d.join("src").join("b.rs"), "").unwrap();
        let mut index = FileIndex::default();
        let files = index.files_under(&d, Duration::from_secs(10)).map(<[PathBuf]>::len);
        assert_eq!(files, Some(2));
        // A listing of another folder does not answer for this one.
        let other = d.join("src");
        let mut fresh = FileIndex::default();
        fresh.start(&d);
        assert!(fresh.files_under(&other, Duration::ZERO).is_none_or(|f| f.len() == 1));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn scans_skips_and_finds_root() {
        let d = std::env::temp_dir().join("notepp_files_test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join(".git")).unwrap();
        std::fs::create_dir_all(d.join("src").join("deep")).unwrap();
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("a.txt"), "").unwrap();
        std::fs::write(d.join("src").join("b.rs"), "").unwrap();
        std::fs::write(d.join("src").join("deep").join("c.rs"), "").unwrap();
        std::fs::write(d.join("node_modules").join("x.js"), "").unwrap();

        assert_eq!(project_root(&d.join("src").join("deep")), d);
        let files = scan_files(&d, 100);
        let names: Vec<String> = files.iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, vec!["a.txt", "b.rs", "c.rs"]);
        assert_eq!(scan_files(&d, 2).len(), 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn atomic_write_replaces_and_leaves_no_temp_files() {
        let d = std::env::temp_dir().join("notepp_atomic_test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("doc.txt");
        atomic_write(&f, b"first").unwrap();
        atomic_write(&f, b"second, longer").unwrap();
        atomic_write(&f, b"3").unwrap();
        assert_eq!(std::fs::read(&f).unwrap(), b"3");
        let names: Vec<_> = std::fs::read_dir(&d).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names.len(), 1, "{names:?}");
        // A read-only file is still refused, as with a plain write.
        let mut p = std::fs::metadata(&f).unwrap().permissions();
        p.set_readonly(true);
        std::fs::set_permissions(&f, p).unwrap();
        assert!(atomic_write(&f, b"nope").is_err());
        assert_eq!(std::fs::read(&f).unwrap(), b"3");
        let mut p = std::fs::metadata(&f).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false, reason = "clears the DOS read-only attribute")]
        p.set_readonly(false);
        std::fs::set_permissions(&f, p).unwrap();
        // A missing folder is an error, not a silently created tree.
        assert!(atomic_write(&d.join("no").join("x.txt"), b"x").is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
