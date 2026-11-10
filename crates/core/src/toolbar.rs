//! The toolbar as plain data: which commands, in which order, with which separators. The shell chooses the icons
//! (wx draws Note++'s own `SVG` files, `WinUI` uses its Fluent symbols) and draws the buttons; the tool-tip is the
//! registry's unless a button says its own.

use crate::commands::{
    ID_CLOSE, ID_COPY, ID_CUT, ID_ENC_TOGGLE, ID_FIND, ID_LOG_FOLLOW, ID_NEW, ID_NEW_WB, ID_OPEN, ID_PASTE, ID_PREVIEW, ID_PRINT, ID_PW_TOOLBAR, ID_REDO,
    ID_REPLACE, ID_SAVE, ID_SAVEALL, ID_THEME, ID_UNDO, ID_WRAP, ID_ZOOMIN, ID_ZOOMOUT,
};
use crate::registry;

/// One place on the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    /// A button for a command; `tip` replaces the registry's tool-tip when set.
    Button {
        id: usize,
        tip: Option<&'static str>,
    },
    Separator,
}

const fn button(id: usize) -> Tool {
    Tool::Button { id, tip: None }
}

const fn tipped(id: usize, tip: &'static str) -> Tool {
    Tool::Button { id, tip: Some(tip) }
}

/// The toolbar, left to right.
pub const LAYOUT: &[Tool] = &[
    button(ID_NEW),
    button(ID_NEW_WB),
    button(ID_OPEN),
    button(ID_SAVE),
    button(ID_SAVEALL),
    button(ID_CLOSE),
    tipped(ID_PRINT, "Print"),
    tipped(ID_ENC_TOGGLE, "Encrypt this file (or remove its encryption)"),
    tipped(ID_PW_TOOLBAR, "Passwords (Bitwarden)"),
    Tool::Separator,
    button(ID_CUT),
    button(ID_COPY),
    button(ID_PASTE),
    Tool::Separator,
    button(ID_UNDO),
    button(ID_REDO),
    Tool::Separator,
    button(ID_FIND),
    button(ID_REPLACE),
    Tool::Separator,
    button(ID_ZOOMIN),
    button(ID_ZOOMOUT),
    Tool::Separator,
    button(ID_WRAP),
    button(ID_PREVIEW),
    tipped(ID_LOG_FOLLOW, "Log Follow: stream a .log / .out / .err file as it grows"),
    button(ID_THEME),
];

/// The tool-tip of button `id`: its own, or the registry's (name and shortcut).
pub fn tip(id: usize) -> String {
    let own = LAYOUT.iter().find_map(|t| match t {
        Tool::Button { id: i, tip: Some(tip) } if *i == id => Some((*tip).to_string()),
        _ => None,
    });
    own.or_else(|| registry::spec(id).map(registry::CommandSpec::tooltip)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_button_has_a_tip_and_appears_once() {
        let mut seen = std::collections::BTreeSet::new();
        for t in LAYOUT {
            if let Tool::Button { id, tip } = t {
                assert!(registry::spec(*id).is_some() || tip.is_some(), "{id} is neither registered nor given a tip");
                assert!(seen.insert(*id), "{id} appears twice");
            }
        }
    }

    #[test]
    fn separators_only_stand_between_buttons() {
        assert!(matches!(LAYOUT.first(), Some(Tool::Button { .. })));
        assert!(matches!(LAYOUT.last(), Some(Tool::Button { .. })));
        assert!(LAYOUT.windows(2).all(|w| !(w[0] == Tool::Separator && w[1] == Tool::Separator)));
    }

    #[test]
    fn a_button_may_name_its_own_tip_otherwise_the_registry_does() {
        assert_eq!(tip(ID_PRINT), "Print");
        assert!(!tip(ID_SAVE).is_empty());
        assert_ne!(tip(ID_SAVE), "Print");
    }
}
