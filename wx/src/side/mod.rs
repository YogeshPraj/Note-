//! The side list on the right of the editor: one list control, four things it can show.
//!
//! Each thing is a [`Content`] (a strategy): it says what the columns and rows are and what activating a row
//! does. The host ([`Side`]) only tracks which one is shown. Nothing here touches the toolkit, so it is all
//! unit tested; `panels.rs` fills the actual list control from it.

pub mod characters;
pub mod history;
pub mod targets;

use self::characters::Characters;
use self::history::History;
use self::targets::Targets;

/// Width of the side list.
pub const SIDE_W: i32 = 250;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Characters,
    Clipboard,
    Documents,
    Functions,
}

/// What the application should do when a row is activated.
#[derive(Debug, PartialEq, Eq)]
pub enum Activate {
    /// Insert this text at the caret.
    Insert(String),
    /// Switch to document (tab) number n.
    Document(usize),
    /// Go to this 0-based line.
    Goto(usize),
}

/// One thing the side list can show.
pub trait Content {
    fn columns(&self) -> Vec<(&'static str, i32)>;
    fn rows(&self) -> Vec<Vec<String>>;
    fn activate(&self, row: usize) -> Option<Activate>;
    /// Delete a row; true if the content supports that and the row existed.
    fn remove(&mut self, _row: usize) -> bool {
        false
    }
}

/// Which content is shown, and the state each keeps.
pub struct Side {
    pub mode: Option<Mode>,
    pub characters: Characters,
    pub history: History,
    pub documents: Targets,
    pub functions: Targets,
}

impl Default for Side {
    fn default() -> Self {
        Side {
            mode: None,
            characters: Characters,
            history: History::default(),
            documents: Targets::new(vec![("Name", 100), ("Path", SIDE_W - 124)]),
            functions: Targets::new(vec![("Function", SIDE_W - 74), ("Line", 50)]),
        }
    }
}

impl Side {
    pub fn content(&self) -> Option<&dyn Content> {
        Some(match self.mode? {
            Mode::Characters => &self.characters,
            Mode::Clipboard => &self.history,
            Mode::Documents => &self.documents,
            Mode::Functions => &self.functions,
        })
    }

    pub fn content_mut(&mut self) -> Option<&mut dyn Content> {
        Some(match self.mode? {
            Mode::Characters => &mut self.characters,
            Mode::Clipboard => &mut self.history,
            Mode::Documents => &mut self.documents,
            Mode::Functions => &mut self.functions,
        })
    }

    /// Show `want`, or hide the list when it already shows that. True when the list is now visible.
    pub fn toggle(&mut self, want: Mode) -> bool {
        if self.mode == Some(want) {
            self.mode = None;
        } else {
            self.mode = Some(want);
        }
        self.mode.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_the_shown_content_hides_the_list() {
        let mut s = Side::default();
        assert!(s.toggle(Mode::Characters));
        assert_eq!(s.mode, Some(Mode::Characters));
        assert!(!s.toggle(Mode::Characters));
        assert_eq!(s.mode, None);
    }

    #[test]
    fn toggling_another_content_switches_to_it() {
        let mut s = Side::default();
        s.toggle(Mode::Characters);
        assert!(s.toggle(Mode::Documents));
        assert_eq!(s.mode, Some(Mode::Documents));
    }

    #[test]
    fn nothing_is_shown_to_start_with() {
        assert!(Side::default().content().is_none());
    }

    #[test]
    fn each_mode_has_its_own_columns() {
        let mut s = Side::default();
        let cols = |s: &mut Side, m| {
            s.mode = Some(m);
            s.content().unwrap().columns().iter().map(|c| c.0).collect::<Vec<_>>()
        };
        assert_eq!(cols(&mut s, Mode::Characters), ["Value", "Hex", "Char"]);
        assert_eq!(cols(&mut s, Mode::Clipboard), ["Clipboard History"]);
        assert_eq!(cols(&mut s, Mode::Documents), ["Name", "Path"]);
        assert_eq!(cols(&mut s, Mode::Functions), ["Function", "Line"]);
    }

    #[test]
    fn only_the_clipboard_history_lets_rows_be_deleted() {
        let mut s = Side::default();
        s.history.record("one");
        s.mode = Some(Mode::Clipboard);
        assert!(s.content_mut().unwrap().remove(0));
        s.mode = Some(Mode::Characters);
        assert!(!s.content_mut().unwrap().remove(0));
    }
}
