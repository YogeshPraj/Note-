//! The activity log: what agents did, for the user to look at afterwards. A line says when, who, which tool, on what, and how
//! it went. Never what a document says: the log must not become a copy of the user's text.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::session::config_dir;

/// When the log is this long it starts again, and the old one is kept once.
const MAX_BYTES: u64 = 1024 * 1024;
/// The most of a failure's message that goes in.
const MAX_FAILURE: usize = 120;

pub fn log_path() -> PathBuf {
    config_dir().join("mcp-activity.log")
}

/// One line of the log, the time given.
pub fn line(time: &str, client: &str, tool: &str, target: &str, failure: Option<&str>) -> String {
    let single = |s: &str, most: usize| -> String { s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(most).collect() };
    let outcome = failure.map_or_else(|| "ok".to_string(), |f| format!("refused: {}", single(f, MAX_FAILURE)));
    let on = if target.is_empty() { String::new() } else { format!(" on \"{}\"", single(target, MAX_FAILURE)) };
    format!("{time}  {}  {tool}{on}  {outcome}\n", single(client, 60))
}

/// One line about something the user did or decided.
pub fn note(time: &str, who: &str, what: &str, target: &str) -> String {
    let single = |s: &str| -> String { s.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_FAILURE).collect() };
    let about = if target.is_empty() { String::new() } else { format!(" \"{}\"", single(target)) };
    format!("{time}  {}  {}{about}\n", single(who), single(what))
}

/// Add a line to the log at `path`. A log that cannot be written must not stop the work, so a failure is passed over.
pub fn append_to(path: &Path, text: &str) {
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(path, path.with_extension("log.old"));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Add a line to the log.
pub fn append(text: &str) {
    append_to(&log_path(), text);
}

/// The log, newest last, with what the last start-over kept.
pub fn read() -> String {
    let path = log_path();
    let old = std::fs::read_to_string(path.with_extension("log.old")).unwrap_or_default();
    old + &std::fs::read_to_string(path).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_says_who_did_what_and_how_it_went() {
        assert_eq!(line("T", "claude", "edit_text", "notes.txt", None), "T  claude  edit_text on \"notes.txt\"  ok\n");
        assert_eq!(line("T", "claude", "list_tabs", "", None), "T  claude  list_tabs  ok\n");
        assert_eq!(line("T", "c", "x", "", Some("no")), "T  c  x  refused: no\n");
    }

    #[test]
    fn a_decision_reads_as_one() {
        assert_eq!(note("T", "claude", "refused by the user", ""), "T  claude  refused by the user\n");
        assert_eq!(note("T", "the user", "allowed agents", "diary.txt"), "T  the user  allowed agents \"diary.txt\"\n");
    }

    #[test]
    fn a_line_stays_one_line_of_reasonable_length() {
        let l = line("T", "a\nb", "x", "t\r\nu", Some(&"long ".repeat(100)));
        assert_eq!(l.matches('\n').count(), 1);
        assert!(l.len() < 300);
    }

    #[test]
    fn the_log_starts_again_and_keeps_the_old_one_once() {
        let dir = std::env::temp_dir().join(format!("notepp-activity-{}", std::process::id()));
        let path = dir.join("mcp-activity.log");
        let _ = std::fs::remove_dir_all(&dir);
        append_to(&path, "first\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\n");
        std::fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap();
        append_to(&path, "fresh\n");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "fresh\n");
        assert!(path.with_extension("log.old").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
