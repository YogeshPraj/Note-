//! Brain: one place for what the user learns at work (docs/brain-plan.md).
//!
//! A folder of Markdown notes, one per day plus one per topic. A quick capture becomes an entry in the day's note: a
//! `## HH:MM` heading, the text, and where it came from. Everything here is plain text in and out, so it is the same
//! on every shell and is unit tested; reading and writing the files is in [`store`].

use std::path::{Path, PathBuf};

use crate::edit::DateTime;

mod date;
mod parse;
mod search;
pub mod store;

pub use date::Date;
pub use parse::{Entry, entries, links_in, tags_in};
pub use search::{Hit, Note, Query, parse_query, search, tag_counts};

/// The brain's folder inside the profile folder `config`.
pub fn root(config: &Path) -> PathBuf {
    config.join("brain")
}

/// The note of `day`: `daily/2026/2026-11-12.md`.
pub fn day_path(root: &Path, day: Date) -> PathBuf {
    root.join("daily").join(format!("{:04}", day.year)).join(format!("{day}.md"))
}

/// A topic's file name: what the user typed, without the characters a file name cannot hold. None when nothing is left.
pub fn topic_file(name: &str) -> Option<String> {
    let clean: String = name.trim().chars().filter(|c| !matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).collect();
    let clean = clean.trim().trim_matches('.').trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

/// The note of the topic called `name`: `topics/<name>.md`. None when the name holds nothing a file can be called.
pub fn topic_path(root: &Path, name: &str) -> Option<PathBuf> {
    topic_file(name).map(|f| root.join("topics").join(format!("{f}.md")))
}

/// Where a captured selection came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The file's path, or the tab's title when it has no file.
    pub file: String,
    /// The first selected line, from 1.
    pub line: usize,
}

/// The first line of a day's note: `# 2026-11-12 Thursday`.
pub fn day_heading(when: &DateTime) -> String {
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let name = DAYS.get(crate::num::to_usize(when.weekday)).copied().unwrap_or("");
    format!("# {} {name}", Date::of(when)).trim_end().to_string()
}

/// An entry as it is written: the time, the text, and where it came from.
pub fn entry_text(when: &DateTime, text: &str, from: Option<&Source>) -> String {
    let mut out = format!("## {:02}:{:02}\n{}\n", when.hour, when.minute, text.trim().replace("\r\n", "\n"));
    if let Some(s) = from {
        out.push_str(&format!("from: {}:{}\n", s.file, s.line));
    }
    out
}

/// The day's `note` with one more entry at its end. An empty note first gets the day's heading. The note's own line
/// endings are kept.
pub fn with_entry(note: &str, when: &DateTime, text: &str, from: Option<&Source>) -> String {
    let nl = if note.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = note.trim_end().replace("\r\n", "\n");
    if out.is_empty() {
        out = day_heading(when);
    }
    out.push_str("\n\n");
    out.push_str(&entry_text(when, text, from));
    if nl == "\n" { out } else { out.replace('\n', nl) }
}

/// What a new topic note starts with.
pub fn topic_start(name: &str) -> String {
    format!("# {}\n\n", name.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn at(hour: u32, minute: u32) -> DateTime {
        DateTime { year: 2026, month: 11, day: 12, weekday: 4, hour, minute, second: 0 }
    }

    #[test]
    fn notes_have_their_places() {
        let root = Path::new("b");
        assert_eq!(day_path(root, Date { year: 2026, month: 11, day: 2 }), Path::new("b").join("daily").join("2026").join("2026-11-02.md"));
        assert_eq!(topic_path(root, " Azure: deploy? "), Some(Path::new("b").join("topics").join("Azure deploy.md")));
        assert_eq!(topic_path(root, " ../.. "), None, "a name cannot leave the folder");
        assert_eq!(topic_path(root, "a/b\\c"), Some(Path::new("b").join("topics").join("abc.md")));
        assert_eq!(topic_path(root, "???"), None);
    }

    #[test]
    fn the_first_entry_of_a_day_starts_the_note() {
        let note = with_entry("", &at(14, 32), "  swap needs the flag #deploy \n", None);
        assert_eq!(note, "# 2026-11-12 Thursday\n\n## 14:32\nswap needs the flag #deploy\n");
    }

    #[test]
    fn later_entries_follow_and_say_where_they_came_from() {
        let first = with_entry("", &at(9, 5), "one", None);
        let src = Source { file: "D:\\work\\deploy.ps1".into(), line: 120 };
        let both = with_entry(&first, &at(16, 0), "two\r\nlines", Some(&src));
        assert_eq!(both, "# 2026-11-12 Thursday\n\n## 09:05\none\n\n## 16:00\ntwo\nlines\nfrom: D:\\work\\deploy.ps1:120\n");
    }

    #[test]
    fn a_note_keeps_its_own_line_endings_and_what_the_user_wrote() {
        let note = with_entry("# 2026-11-12 Thursday\r\n\r\nmy own words\r\n\r\n", &at(8, 0), "x", None);
        assert_eq!(note, "# 2026-11-12 Thursday\r\n\r\nmy own words\r\n\r\n## 08:00\r\nx\r\n");
    }
}
