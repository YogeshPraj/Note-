//! Find in Files: walk a folder, filter names with wildcards, and search each text file. Runs on a worker thread in
//! the app, so it reports how far it got and stops when asked ([`SearchProgress`]).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::{Hit, Opts, compile, hits_in_text};
use crate::doc;
use crate::error::Error;

/// `*` and `?` wildcard match, case-insensitive.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    go(&p, &n)
}

/// Does `name` pass a filter list like `*.rs;*.toml;!*.min.js`?
pub fn passes_filter(filters: &str, name: &str) -> bool {
    let parts: Vec<&str> = filters.split([';', ',', ' ']).map(str::trim).filter(|s| !s.is_empty()).collect();
    let includes: Vec<&&str> = parts.iter().filter(|p| !p.starts_with('!')).collect();
    let excluded = parts.iter().filter_map(|p| p.strip_prefix('!')).any(|p| glob_match(p, name));
    !excluded && (includes.is_empty() || includes.iter().any(|p| glob_match(p, name)))
}

const SKIP_DIRS: [&str; 5] = ["node_modules", "target", ".git", ".svn", ".hg"];
pub const MAX_HITS: usize = 5000;
/// Most Find All results listed in the panel.
pub const MAX_LIST: usize = 20_000;
/// Files larger than this are not searched.
const MAX_FILE: u64 = 8_000_000;

/// Shared between a Find in Files worker and the window: how many files it has searched, and a request to stop.
#[derive(Debug, Default)]
pub struct SearchProgress {
    pub files: AtomicUsize,
    pub cancel: AtomicBool,
}

impl SearchProgress {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Search every file under `dir` whose name passes `filters`.
///
/// # Errors
/// [`Error::EmptyPattern`] for an empty search, or the pattern's error.
pub fn find_in_files(dir: &Path, filters: &str, o: &Opts) -> Result<Vec<Hit>, Error> {
    find_in_files_with(dir, filters, o, &SearchProgress::default()).map(|(hits, _)| hits)
}

/// Like [`find_in_files`], counting each searched file in `progress` and stopping as soon as it is cancelled. The flag
/// says whether the whole folder was searched (false when cancelled).
///
/// # Errors
/// [`Error::EmptyPattern`] for an empty search, or the pattern's error.
pub fn find_in_files_with(dir: &Path, filters: &str, o: &Opts, progress: &SearchProgress) -> Result<(Vec<Hit>, bool), Error> {
    // An empty pattern matches between every two characters of every file.
    if o.find.is_empty() {
        return Err(Error::EmptyPattern);
    }
    let re = compile(o)?;
    let mut hits = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            if progress.cancelled() {
                return Ok((hits, false));
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(e.path());
                }
            } else if passes_filter(filters, &name) && e.metadata().is_ok_and(|m| m.len() <= MAX_FILE) {
                progress.files.fetch_add(1, Ordering::Relaxed);
                let Some(text) = searchable_text(&e.path()) else { continue };
                hits.extend(hits_in_text(&text, &re, Some(&e.path())));
                if hits.len() >= MAX_HITS {
                    hits.truncate(MAX_HITS);
                    return Ok((hits, true));
                }
            }
        }
    }
    Ok((hits, true))
}

/// The text of a file worth searching: None for binaries (a NUL byte early on, unless it is UTF-16) and unreadable files.
fn searchable_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let head = &bytes[..bytes.len().min(8000)];
    let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
    if !utf16 && head.contains(&0) {
        return None;
    }
    Some(doc::decode(&bytes).0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(find: &str) -> Opts {
        Opts { find: find.into(), ..Default::default() }
    }

    #[test]
    fn glob_and_filters() {
        assert!(glob_match("*.rs", "Main.RS"));
        assert!(!glob_match("*.rs", "main.rss"));
        assert!(glob_match("a?c.*", "abc.txt"));
        assert!(passes_filter("*.*", "x.txt"));
        assert!(passes_filter("*.rs;*.toml", "Cargo.toml"));
        assert!(!passes_filter("*.rs;*.toml", "a.js"));
        assert!(!passes_filter("*.js;!*.min.js", "a.min.js"));
        assert!(passes_filter("", "anything"));
    }

    #[test]
    fn find_in_files_walks_and_filters() {
        let d = std::env::temp_dir().join("notepp_fif_native");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("a.txt"), "hello\nWorld here\n").unwrap();
        std::fs::write(d.join("sub").join("b.rs"), "nothing\nworld again").unwrap();
        std::fs::write(d.join("node_modules").join("c.txt"), "world").unwrap();
        std::fs::write(d.join("bin.dat"), b"world\0\0\0").unwrap();
        let hits = find_in_files(&d, "*.*", &opts("world")).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.line == 2 && h.path.as_ref().unwrap().ends_with("a.txt")));
        let only_rs = find_in_files(&d, "*.rs", &opts("world")).unwrap();
        assert_eq!(only_rs.len(), 1);
        // An empty search box lists nothing (it used to match at every character of every file).
        assert!(matches!(find_in_files(&d, "*.*", &opts("")), Err(Error::EmptyPattern)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn progress_counts_files_and_a_cancelled_search_stops() {
        let d = std::env::temp_dir().join(format!("notepp_fif_progress_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for i in 0..5 {
            std::fs::write(d.join(format!("{i}.txt")), "needle").unwrap();
        }
        let progress = SearchProgress::default();
        let (hits, complete) = find_in_files_with(&d, "*.txt", &opts("needle"), &progress).unwrap();
        assert_eq!((hits.len(), complete, progress.files.load(Ordering::Relaxed)), (5, true, 5));
        let stopped = SearchProgress::default();
        stopped.cancel.store(true, Ordering::Relaxed);
        let (hits, complete) = find_in_files_with(&d, "*.txt", &opts("needle"), &stopped).unwrap();
        assert_eq!((hits.len(), complete), (0, false));
        let _ = std::fs::remove_dir_all(&d);
    }
}
