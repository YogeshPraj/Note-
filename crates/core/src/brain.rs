//! Brain: one place for what the user learns at work (docs/brain-plan.md).
//!
//! One Markdown file, `quick-notes.md`, with a `# 2026-11-12 Thursday` heading per day. A quick capture becomes an entry at
//! its end: a `## HH:MM` heading, the text, and where it came from. Everything here is plain text in and out, so it is the same
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

/// The Brain's one file: `brain/brain.md`.
pub fn note_path(root: &Path) -> PathBuf {
    root.join("quick-notes.md")
}

/// Where a captured selection came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    /// The file's path, or the tab's title when it has no file.
    pub file: String,
    /// The first selected line, from 1.
    pub line: usize,
}

/// What to tell an AI agent so it can answer questions from the Brain and cite the notes it used.
pub const AGENT_PROMPT: &str = "You can use my Quick Notes through the Note++ tools: brain_overview (what it holds), brain_search (words, #tags, today, yesterday, week, month, date:2026-11), brain_read (the notes, in parts) and, if I allowed it, brain_add (a new entry). Answer my questions from my notes: search first, read the notes that matter, and say which notes and dates you used. If the notes do not say, tell me so instead of guessing. Never put anything into Quick Notes unless I ask.";

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

/// `note` with the heading of `when`'s day at its end, unless the last day in it already is that day. The note's own
/// line endings are kept.
pub fn with_day(note: &str, when: &DateTime) -> String {
    if parse::last_day(note) == Some(Date::of(when)) {
        return note.to_string();
    }
    let nl = if note.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = note.trim_end().replace("\r\n", "\n");
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str(&day_heading(when));
    out.push_str("\n\n");
    if nl == "\n" { out } else { out.replace('\n', nl) }
}

/// `note` with one more entry at its end, under the heading of `when`'s day (which is added when the note's last day
/// is another). The note's own line endings are kept.
pub fn with_entry(note: &str, when: &DateTime, text: &str, from: Option<&Source>) -> String {
    let note = with_day(note, when);
    let nl = if note.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = note.trim_end().replace("\r\n", "\n");
    out.push_str("\n\n");
    out.push_str(&entry_text(when, text, from));
    if nl == "\n" { out } else { out.replace('\n', nl) }
}

/// The smallest edit that turns `old` into `new` when `new` is `old` with something added at its end (as
/// [`with_entry`] makes it): where the change starts in `old`, and the text that goes from there to the end. An open
/// note is changed this way, so the user's caret and everything above stay put.
pub fn tail_edit<'a>(old: &str, new: &'a str) -> (usize, &'a str) {
    let mut same = old.bytes().zip(new.bytes()).take_while(|(a, b)| a == b).count();
    while !new.is_char_boundary(same) || !old.is_char_boundary(same) {
        same -= 1;
    }
    (same, &new[same..])
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn at(hour: u32, minute: u32) -> DateTime {
        DateTime { year: 2026, month: 11, day: 12, weekday: 4, hour, minute, second: 0 }
    }

    #[test]
    fn the_brain_is_one_file() {
        assert_eq!(note_path(Path::new("b")), Path::new("b").join("quick-notes.md"));
    }

    #[test]
    fn a_new_day_gets_its_heading_and_the_same_day_does_not() {
        let first = with_entry("", &at(9, 0), "one", None);
        let mut next = at(8, 15);
        next.day = 13;
        next.weekday = 5;
        let two = with_entry(&first, &next, "two", None);
        assert_eq!(two, "# 2026-11-12 Thursday\n\n## 09:00\none\n\n# 2026-11-13 Friday\n\n## 08:15\ntwo\n");
        assert_eq!(with_day(&two, &next), two, "today's heading is there already");
        assert_eq!(with_day("", &at(9, 0)), "# 2026-11-12 Thursday\n\n");
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
    fn adding_to_an_open_note_changes_only_its_end() {
        let old = "# 2026-11-12 Thursday\n\n## 09:00\none\n\n\n";
        let new = with_entry(old, &at(10, 0), "two", None);
        let (from, tail) = tail_edit(old, &new);
        assert_eq!((from, tail), (37, "## 10:00\ntwo\n"), "only what follows the common start is replaced");
        assert_eq!(format!("{}{tail}", &old[..from]), new);
        assert_eq!(tail_edit("", "abc"), (0, "abc"));
        assert_eq!(tail_edit("h\u{e9}", "h\u{e8}x"), (1, "\u{e8}x"), "never inside a character");
    }

    #[test]
    fn a_note_keeps_its_own_line_endings_and_what_the_user_wrote() {
        let note = with_entry("# 2026-11-12 Thursday\r\n\r\nmy own words\r\n\r\n", &at(8, 0), "x", None);
        assert_eq!(note, "# 2026-11-12 Thursday\r\n\r\nmy own words\r\n\r\n## 08:00\r\nx\r\n");
    }
}
