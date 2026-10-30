//! Log follow mode: read a log file the way `tail -f` does.
//!
//! Reading is by offset: each poll costs one `stat`, and only bytes added since the last poll are
//! read, so following a multi-gigabyte log is as cheap as following a small one. A file that
//! suddenly got smaller was rotated or truncated; following then restarts from the top.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// How much of the end of a big file is shown when following starts.
pub const TAIL_BYTES: u64 = 1024 * 1024;
/// The most bytes read in one poll (a burst bigger than this is caught up over several polls).
const MAX_CHUNK: u64 = 8 * 1024 * 1024;

/// Is this the kind of file Log Follow is for?
pub fn is_log_file(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["log", "out", "err"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// What a poll found.
#[derive(Default, Debug, PartialEq)]
pub struct Update {
    pub lines: Vec<String>,
    /// The file shrank: everything shown so far is out of date and `lines` start from the top.
    pub reset: bool,
    /// Bytes at the start of a large file that were not shown (only when following starts).
    pub skipped_bytes: u64,
}

pub struct Follower {
    pub path: PathBuf,
    offset: u64,
    /// The end of the file so far, when it does not end in a line break yet.
    pending: Vec<u8>,
}

fn split_lines(pending: &mut Vec<u8>, fresh: &[u8]) -> Vec<String> {
    pending.extend_from_slice(fresh);
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(i) = pending[start..].iter().position(|b| *b == b'\n') {
        let mut line = &pending[start..start + i];
        if line.last() == Some(&b'\r') {
            line = &line[..line.len() - 1];
        }
        out.push(String::from_utf8_lossy(line).into_owned());
        start += i + 1;
    }
    pending.drain(..start);
    out
}

fn read_range(path: &Path, from: u64, len: u64) -> std::io::Result<Vec<u8>> {
    let mut f = std::fs::File::open(path)?;
    f.seek(SeekFrom::Start(from))?;
    let mut buf = Vec::new();
    f.take(len).read_to_end(&mut buf)?;
    Ok(buf)
}

impl Follower {
    /// Start following `path`, showing up to `tail` bytes from its end.
    pub fn open(path: &Path, tail: u64) -> std::io::Result<(Follower, Update)> {
        let len = std::fs::metadata(path)?.len();
        let start = len.saturating_sub(tail);
        let mut data = read_range(path, start, len - start)?;
        let mut skipped = 0;
        if start > 0 {
            // Begin at a line start: drop the cut line at the front.
            let cut = data.iter().position(|b| *b == b'\n').map_or(data.len(), |i| i + 1);
            skipped = start + cut as u64;
            data.drain(..cut);
        }
        // Reading continues from the end of what was just read.
        let mut f = Follower { path: path.to_path_buf(), offset: len, pending: Vec::new() };
        let lines = split_lines(&mut f.pending, &data);
        Ok((f, Update { lines, reset: false, skipped_bytes: skipped }))
    }

    /// Read what was added since the last call.
    pub fn poll(&mut self) -> std::io::Result<Update> {
        let len = std::fs::metadata(&self.path)?.len();
        let mut update = Update::default();
        if len < self.offset {
            // Rotated or truncated: start again from the top.
            self.offset = 0;
            self.pending.clear();
            update.reset = true;
        }
        if len == self.offset {
            return Ok(update);
        }
        let take = (len - self.offset).min(MAX_CHUNK);
        let fresh = read_range(&self.path, self.offset, take)?;
        self.offset += fresh.len() as u64;
        update.lines = split_lines(&mut self.pending, &fresh);
        Ok(update)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("notepp_log_{name}_{}.log", std::process::id()));
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn recognises_log_files() {
        assert!(is_log_file(Path::new("a/app.LOG")) && is_log_file(Path::new("x.out")) && is_log_file(Path::new("x.err")));
        assert!(!is_log_file(Path::new("x.txt")) && !is_log_file(Path::new("log")));
    }

    #[test]
    fn follows_appended_lines_and_holds_back_a_partial_one() {
        let p = temp("follow");
        std::fs::write(&p, "one\r\ntwo\n").unwrap();
        let (mut f, first) = Follower::open(&p, TAIL_BYTES).unwrap();
        assert_eq!(first.lines, ["one", "two"]);
        assert_eq!(f.poll().unwrap(), Update::default());

        let mut file = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        file.write_all(b"three\nfou").unwrap();
        assert_eq!(f.poll().unwrap().lines, ["three"]);
        file.write_all(b"r\n").unwrap();
        assert_eq!(f.poll().unwrap().lines, ["four"]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_big_file_starts_at_its_tail_on_a_line_boundary() {
        let p = temp("tail");
        let text: String = (0..100).map(|i| format!("line number {i:03}\n")).collect();
        std::fs::write(&p, &text).unwrap();
        let (mut f, u) = Follower::open(&p, 100).unwrap();
        assert!(u.skipped_bytes > 0);
        assert!(u.lines.len() >= 4 && u.lines.len() <= 6, "{:?}", u.lines);
        assert_eq!(u.lines.last().unwrap(), "line number 099");
        assert!(u.lines.iter().all(|l| l.starts_with("line number ")), "no cut line at the front");
        assert_eq!(f.poll().unwrap(), Update::default());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn a_smaller_file_means_rotation() {
        let p = temp("rotate");
        std::fs::write(&p, "old line one\nold line two\n").unwrap();
        let (mut f, _) = Follower::open(&p, TAIL_BYTES).unwrap();
        std::fs::write(&p, "new\n").unwrap();
        let u = f.poll().unwrap();
        assert!(u.reset);
        assert_eq!(u.lines, ["new"]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn invalid_utf8_and_a_missing_file_are_handled() {
        let p = temp("bytes");
        std::fs::write(&p, b"ok \xFF\xFE bytes\n").unwrap();
        let (_, u) = Follower::open(&p, TAIL_BYTES).unwrap();
        assert_eq!(u.lines.len(), 1);
        assert!(u.lines[0].starts_with("ok "));
        let _ = std::fs::remove_file(&p);
        assert!(Follower::open(&p, TAIL_BYTES).is_err());
    }
}
