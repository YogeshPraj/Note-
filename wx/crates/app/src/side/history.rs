//! Clipboard history: the texts copied recently, newest first.

use super::{Activate, Content, SIDE_W};

const MAX_ENTRIES: usize = 50;
const PREVIEW_CHARS: usize = 120;

/// One-line, length-limited form of an entry: line breaks become a return arrow.
pub fn preview(s: &str) -> String {
    let flat: String = s
        .chars()
        .map(|c| {
            if c == '\r' || c == '\n' {
                '\u{21B5}'
            } else if c == '\t' {
                ' '
            } else {
                c
            }
        })
        .collect();
    let mut out: String = flat.chars().take(PREVIEW_CHARS).collect();
    if flat.chars().count() > PREVIEW_CHARS {
        out.push('\u{2026}');
    }
    out
}

#[derive(Default)]
pub struct History {
    items: Vec<String>,
}

impl History {
    /// Remember a copied text (newest first, no repeats). False when it was empty or already the newest.
    pub fn record(&mut self, text: &str) -> bool {
        if text.is_empty() || self.items.first().map(String::as_str) == Some(text) {
            return false;
        }
        self.items.retain(|x| x != text);
        self.items.insert(0, text.to_string());
        self.items.truncate(MAX_ENTRIES);
        true
    }

    pub fn get(&self, row: usize) -> Option<&str> {
        self.items.get(row).map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl Content for History {
    fn columns(&self) -> Vec<(&'static str, i32)> {
        vec![("Clipboard History", SIDE_W - 24)]
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.items.iter().map(|s| vec![preview(s)]).collect()
    }

    fn activate(&self, row: usize) -> Option<Activate> {
        self.get(row).map(|t| Activate::Insert(t.to_string()))
    }

    fn remove(&mut self, row: usize) -> bool {
        if row < self.items.len() {
            self.items.remove(row);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first() {
        let mut h = History::default();
        h.record("a");
        h.record("b");
        assert_eq!((h.get(0), h.get(1)), (Some("b"), Some("a")));
    }

    #[test]
    fn repeating_the_newest_changes_nothing() {
        let mut h = History::default();
        assert!(h.record("a"));
        assert!(!h.record("a"));
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn an_older_repeat_moves_to_the_front_without_duplicating() {
        let mut h = History::default();
        h.record("a");
        h.record("b");
        assert!(h.record("a"));
        assert_eq!((h.len(), h.get(0)), (2, Some("a")));
    }

    #[test]
    fn empty_text_is_ignored() {
        let mut h = History::default();
        assert!(!h.record(""));
        assert_eq!(h.len(), 0);
    }

    #[test]
    fn the_history_is_capped() {
        let mut h = History::default();
        for i in 0..80 {
            h.record(&i.to_string());
        }
        assert_eq!(h.len(), MAX_ENTRIES);
        assert_eq!(h.get(0), Some("79"));
    }

    #[test]
    fn removing_a_row_drops_only_that_entry() {
        let mut h = History::default();
        h.record("a");
        h.record("b");
        assert!(h.remove(0));
        assert_eq!(h.get(0), Some("a"));
        assert!(!h.remove(5));
    }

    #[test]
    fn previews_are_one_line_and_limited() {
        assert_eq!(preview("a\r\nb\tc"), "a\u{21B5}\u{21B5}b c");
        let long = "x".repeat(200);
        let p = preview(&long);
        assert_eq!(p.chars().count(), PREVIEW_CHARS + 1);
        assert!(p.ends_with('\u{2026}'));
    }

    #[test]
    fn activating_a_row_inserts_the_full_text_not_the_preview() {
        let mut h = History::default();
        let long = "y".repeat(300);
        h.record(&long);
        assert_eq!(h.activate(0), Some(Activate::Insert(long)));
    }
}
