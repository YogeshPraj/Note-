//! Project file discovery for Quick Open.

use std::path::{Path, PathBuf};

/// Directories never worth indexing (same list the Electron app skips).
const SKIP: [&str; 14] = [
    "node_modules", "target", ".git", ".svn", ".hg", "dist", "build", "out", ".next", ".cache", ".vscode", ".idea", "__pycache__",
    "bin",
];

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
        entries.sort_by_key(|e| e.file_name());
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
