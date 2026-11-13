//! The right-click menus: a tab's, the editor's, the minimap's and the connected agents'. Each is a [`MenuSpec`] built from the
//! workspace's state, so any shell can show it (see `docs/winui3-plan.md`, phase 1); choosing an item runs its command.

use std::collections::BTreeMap;

use crate::commands as c;
use crate::menu::MenuSpec;
use crate::sci::Send;
use crate::workspace::{TAB_COLORS, TabKind, Workspace};

/// What a tab's menu depends on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabFacts {
    pub has_path: bool,
    pub is_text: bool,
    pub pinned: bool,
    pub favourite: bool,
    pub color: Option<(u8, u8, u8)>,
    pub can_dock: bool,
    pub can_float: bool,
    /// Docked or floating: Dock Back has something to do.
    pub out_of_strip: bool,
}

impl TabFacts {
    /// The facts of the active tab.
    pub fn of<S: Send>(ws: &Workspace<S>) -> TabFacts {
        let (i, t) = (ws.active, &ws.tabs[ws.active]);
        TabFacts {
            has_path: t.path.is_some(),
            is_text: t.kind == TabKind::Text,
            pinned: t.pinned,
            favourite: ws.active_is_favourite(),
            color: t.color,
            can_dock: ws.can_dock(i),
            can_float: ws.can_float(i),
            out_of_strip: ws.docked_index() == Some(i) || ws.current_float().is_some(),
        }
    }
}

fn dock_menu(f: &TabFacts) -> MenuSpec {
    let sides = ["Left", "Right", "Top", "Bottom"].iter().enumerate().fold(MenuSpec::new(), |m, (i, side)| m.item_if(c::ID_DOCK_BASE + i, side, f.can_dock));
    sides.sep().item_if(c::ID_FLOAT_TAB, "Undock", f.can_float)
}

fn colour_menu(f: &TabFacts) -> MenuSpec {
    let preset = TAB_COLORS.iter().any(|(_, rgb)| Some(*rgb) == f.color);
    let colours =
        TAB_COLORS.iter().enumerate().fold(MenuSpec::new(), |m, (i, (name, rgb))| m.swatch(c::ID_TAB_COLOR_BASE + i, name, *rgb, f.color == Some(*rgb)));
    match f.color {
        Some(rgb) if !preset => colours.sep().swatch(c::ID_TAB_COLOR_CUSTOM, "Custom colour...", rgb, true),
        _ => colours.sep().item(c::ID_TAB_COLOR_CUSTOM, "Custom colour..."),
    }
    .sep()
    .item_if(c::ID_TAB_COLOR_NONE, "No colour", f.color.is_some())
}

/// A tab's right-click menu.
pub fn tab_menu(f: &TabFacts) -> MenuSpec {
    MenuSpec::new()
        .item(c::ID_TAB_PIN, if f.pinned { "Unpin Tab" } else { "Pin Tab" })
        .item(c::ID_FAV_TOGGLE, if f.favourite { "Remove from Favourites" } else { "Add to Favourites" })
        .sep()
        .item(c::ID_CLOSE, "Close")
        .item(c::ID_CLOSE_ALL, "Close All")
        .item(c::ID_CLOSE_OTHERS, "Close All But This")
        .item(c::ID_CLOSE_LEFT, "Close All to the Left")
        .item(c::ID_CLOSE_RIGHT, "Close All to the Right")
        .sep()
        .sub("Tab Colour", colour_menu(f))
        .sub("Dock", dock_menu(f))
        .item_if(c::ID_UNDOCK, "Dock Back", f.out_of_strip)
        .sep()
        .item_if(c::ID_CP_FULLPATH, "Copy Full Path", f.has_path)
        .item_if(c::ID_OPEN_FOLDER_EXPLORER, "Open Containing Folder", f.has_path)
        .sep()
        .item_if(c::ID_TAB_CMP_FILE, "Compare...", f.is_text)
        .item_if(c::ID_TAB_CMP_CLIP, "Compare with Clipboard", f.is_text)
        .sep()
        .item_if(c::ID_TAB_REVEAL, "Reveal in File Tree", f.has_path)
}

/// The lines of a right-click menu file (`contextmenu.txt`): a command by its palette name, `-` for a separator, `#` for a
/// remark. Each command comes with its id and default shortcut text; a name that is no command is passed over.
pub fn editor_entries(text: &str) -> Vec<Option<(&'static str, usize, &'static str)>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            if l == "-" {
                Some(None)
            } else {
                crate::registry::palette().find(|s| s.palette.eq_ignore_ascii_case(l)).map(|s| Some((s.palette, s.id, s.hint())))
            }
        })
        .collect()
}

/// The editor's right-click menu from its file's entries, with the user's shortcuts (`shortcuts`: command id to keys)
/// over the defaults.
pub fn editor_menu(entries: &[Option<(&str, usize, &str)>], shortcuts: &BTreeMap<String, String>) -> MenuSpec {
    let mut m = MenuSpec::new();
    for e in entries {
        m = match e {
            None => m.sep(),
            Some((name, id, default)) => {
                let label = name.split_once(": ").map_or(*name, |(_, l)| l);
                let key = shortcuts.get(&id.to_string()).map_or(*default, String::as_str);
                m.item(*id, &if key.is_empty() { label.to_string() } else { format!("{label}\t{key}") })
            }
        };
    }
    m
}

/// The minimap's right-click menu.
pub fn minimap_menu() -> MenuSpec {
    MenuSpec::new().item(c::ID_MINIMAP, "Hide Minimap")
}

/// The menu of the status bar's "AI connected" part.
pub fn agents_menu() -> MenuSpec {
    MenuSpec::new().item(c::ID_MCP_LOG, "AI Activity Log").item(c::ID_MCP_STOP, "Stop AI Agents")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::Entry;

    fn facts() -> TabFacts {
        TabFacts { has_path: false, is_text: true, pinned: false, favourite: false, color: None, can_dock: false, can_float: false, out_of_strip: false }
    }

    fn enabled(m: &MenuSpec, id: usize) -> Option<bool> {
        m.entries().iter().find_map(|e| match e {
            Entry::Item { id: i, enabled, .. } if *i == id => Some(*enabled),
            Entry::Sub { menu, .. } => enabled(menu, id),
            _ => None,
        })
    }

    #[test]
    fn path_commands_are_greyed_out_for_an_unsaved_tab() {
        let m = tab_menu(&facts());
        assert_eq!(enabled(&m, c::ID_CP_FULLPATH), Some(false));
        assert_eq!(enabled(&m, c::ID_TAB_REVEAL), Some(false));
        assert_eq!(enabled(&tab_menu(&TabFacts { has_path: true, ..facts() }), c::ID_CP_FULLPATH), Some(true));
    }

    #[test]
    fn the_pin_item_says_what_it_will_do() {
        let label = |m: &MenuSpec| match &m.entries()[0] {
            Entry::Item { label, .. } => label.clone(),
            _ => String::new(),
        };
        assert_eq!(label(&tab_menu(&facts())), "Pin Tab");
        assert_eq!(label(&tab_menu(&TabFacts { pinned: true, ..facts() })), "Unpin Tab");
    }

    #[test]
    fn only_the_tabs_colour_is_ticked_and_none_is_offered_when_one_is_set() {
        let m = tab_menu(&TabFacts { color: Some(TAB_COLORS[2].1), ..facts() });
        let ticked: Vec<usize> = m
            .entries()
            .iter()
            .filter_map(|e| if let Entry::Sub { menu, .. } = e { Some(menu) } else { None })
            .flat_map(|menu| menu.entries().iter())
            .filter_map(|e| if let Entry::Item { id, check: Some(true), .. } = e { Some(*id) } else { None })
            .collect();
        assert_eq!(ticked, vec![c::ID_TAB_COLOR_BASE + 2]);
        assert_eq!(enabled(&m, c::ID_TAB_COLOR_NONE), Some(true));
        assert_eq!(enabled(&tab_menu(&facts()), c::ID_TAB_COLOR_NONE), Some(false));
    }

    #[test]
    fn a_custom_colour_is_ticked_under_its_own_entry() {
        let m = tab_menu(&TabFacts { color: Some((1, 2, 3)), ..facts() });
        let custom = m.entries().iter().find_map(|e| {
            if let Entry::Sub { menu, .. } = e {
                menu.entries().iter().find(|i| matches!(i, Entry::Item { id, .. } if *id == c::ID_TAB_COLOR_CUSTOM)).cloned()
            } else {
                None
            }
        });
        assert!(matches!(custom, Some(Entry::Item { check: Some(true), swatch: Some((1, 2, 3)), .. })));
    }

    #[test]
    fn compare_needs_a_text_tab_and_dock_back_a_tab_out_of_the_strip() {
        let m = tab_menu(&TabFacts { is_text: false, ..facts() });
        assert_eq!(enabled(&m, c::ID_TAB_CMP_FILE), Some(false));
        assert_eq!(enabled(&m, c::ID_TAB_CMP_CLIP), Some(false));
        assert_eq!(enabled(&tab_menu(&facts()), c::ID_UNDOCK), Some(false));
        assert_eq!(enabled(&tab_menu(&TabFacts { out_of_strip: true, ..facts() }), c::ID_UNDOCK), Some(true));
    }

    #[test]
    fn the_tab_menu_has_no_stray_separators_once_tidied() {
        let m = tab_menu(&facts()).tidy();
        let e = m.entries();
        assert!(!matches!(e.first(), Some(Entry::Sep)) && !matches!(e.last(), Some(Entry::Sep)));
        assert!(e.windows(2).all(|w| !(w[0] == Entry::Sep && w[1] == Entry::Sep)));
    }

    #[test]
    fn the_editor_menu_shows_the_users_shortcut_over_the_default() {
        let mut keys = BTreeMap::new();
        keys.insert("7".to_string(), "Ctrl+Q".to_string());
        let m = editor_menu(&[Some(("Edit: Cut", 7, "Ctrl+X")), None, Some(("Edit: Copy", 8, ""))], &keys);
        let labels: Vec<String> = m.entries().iter().filter_map(|e| if let Entry::Item { label, .. } = e { Some(label.clone()) } else { None }).collect();
        assert_eq!(labels, vec!["Cut\tCtrl+Q", "Copy"]);
        assert_eq!(m.entries()[1], Entry::Sep);
    }

    #[test]
    fn a_menu_file_names_commands_by_their_palette_names() {
        let entries = editor_entries("# remark\nEdit: Undo\n-\nnot a command\n  edit: redo  \n");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].map(|e| e.1), Some(c::ID_UNDO));
        assert_eq!(entries[1], None);
        assert_eq!(entries[2].map(|e| e.1), Some(c::ID_REDO));
    }

    /// An editor that answers nothing but new documents.
    #[derive(Default)]
    struct Quiet(std::cell::Cell<isize>);

    impl Send for Quiet {
        fn send(&self, msg: u32, _: usize, _: isize) -> isize {
            if msg == crate::sci::SCI_CREATEDOCUMENT {
                self.0.set(self.0.get() + 1);
                return self.0.get();
            }
            0
        }
    }

    #[test]
    fn the_facts_come_from_the_active_tab() {
        let mut ws = crate::workspace::Workspace::new(Quiet::default());
        ws.new_tab();
        ws.tabs[0].pinned = true;
        let f = TabFacts::of(&ws);
        assert!(f.pinned && f.is_text && !f.has_path && !f.out_of_strip);
    }
}
