//! Context menus: tabs, the editor and the status bar segments. Each is a [`MenuSpec`] realized at the click.

use notepp_core::commands as c;
use notepp_core::workspace::{TAB_COLORS, TabKind};
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::extra::{context_entries, context_menu_path};
use crate::menu_spec::MenuSpec;

/// Show `spec` at `at` (screen coordinates); its commands arrive as ordinary menu events.
fn show(frame: &Frame, spec: MenuSpec, at: Point) {
    let mut menu = spec.tidy().realize();
    frame.popup_menu(&mut menu, Some(at));
}

/// What the tab menu depends on.
struct TabFacts {
    has_path: bool,
    is_text: bool,
    pinned: bool,
    color: Option<u8>,
}

/// The right-click menu of a tab.
fn tab_spec(f: &TabFacts) -> MenuSpec {
    let colours = TAB_COLORS
        .iter()
        .enumerate()
        .fold(MenuSpec::new(), |m, (i, (name, _))| m.check(c::ID_TAB_COLOR_BASE + i, name, f.color == Some(crate::platform::wx::to_u8(i))))
        .sep()
        .item_if(c::ID_TAB_COLOR_NONE, "No colour", f.color.is_some());
    MenuSpec::new()
        .item(c::ID_TAB_PIN, if f.pinned { "Unpin Tab" } else { "Pin Tab" })
        .sep()
        .item(c::ID_CLOSE, "Close")
        .item(c::ID_CLOSE_ALL, "Close All")
        .item(c::ID_CLOSE_OTHERS, "Close All But This")
        .item(c::ID_CLOSE_LEFT, "Close All to the Left")
        .item(c::ID_CLOSE_RIGHT, "Close All to the Right")
        .sep()
        .sub("Tab Colour", colours)
        .sep()
        .item_if(c::ID_CP_FULLPATH, "Copy Full Path", f.has_path)
        .item_if(c::ID_OPEN_FOLDER_EXPLORER, "Open Containing Folder", f.has_path)
        .sep()
        .item_if(c::ID_TAB_CMP_FILE, "Compare...", f.is_text)
        .item_if(c::ID_TAB_CMP_CLIP, "Compare with Clipboard", f.is_text)
        .sep()
        .item_if(c::ID_TAB_REVEAL, "Reveal in File Tree", f.has_path)
}

pub fn tab_menu(app: &Shared, at: Point) {
    let (frame, facts) = {
        let Ok(a) = app.try_borrow() else { return };
        let t = &a.ws.tabs[a.ws.active];
        (a.frame, TabFacts { has_path: t.path.is_some(), is_text: t.kind == TabKind::Text, pinned: t.pinned, color: t.color })
    };
    show(&frame, tab_spec(&facts), at);
}

/// The editor's right-click menu from the lines of contextmenu.txt (None is a separator).
fn editor_spec(entries: Vec<Option<(&str, usize, &str)>>, shortcuts: &std::collections::BTreeMap<String, String>) -> MenuSpec {
    let mut m = MenuSpec::new();
    for e in entries {
        m = match e {
            None => m.sep(),
            Some((name, id, default)) => {
                let label = name.split_once(": ").map_or(name, |(_, l)| l);
                let key = shortcuts.get(&id.to_string()).map_or(default, String::as_str);
                m.item(id, &if key.is_empty() { label.to_string() } else { format!("{label}\t{key}") })
            }
        };
    }
    m
}

/// Right-click in the editor: the menu listed in contextmenu.txt.
pub fn editor_menu(app: &Shared, at: Point) {
    let (frame, shortcuts) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.keymap.overrides())
    };
    let text = std::fs::read_to_string(context_menu_path()).unwrap_or_default();
    let text = if text.is_empty() { crate::extra::default_context_menu().to_string() } else { text };
    let entries = context_entries(&text);
    if entries.is_empty() {
        return;
    }
    show(&frame, editor_spec(entries, &shortcuts), at);
}

/// Right-click on the minimap: hide it (View > Document Map brings it back).
pub fn minimap_menu(app: &Shared, at: Point) {
    let Ok(a) = app.try_borrow() else { return };
    let frame = a.frame;
    drop(a);
    show(&frame, MenuSpec::new().item(c::ID_MINIMAP, "Hide Minimap"), at);
}

/// A click on the encoding / line-ending / language segment of the status bar.
pub fn status_menu(app: &Shared, seg: isize, at: Point) {
    let (frame, m) = {
        let Ok(a) = app.try_borrow() else { return };
        let m = match seg {
            crate::chrome::SEG_ENC => a.menus.enc,
            crate::chrome::SEG_EOL => a.menus.eol,
            crate::chrome::SEG_LANG => a.menus.lang,
            _ => return,
        };
        (a.frame, m)
    };
    show(&frame, crate::menus::spec(m), at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu_spec::Entry;

    fn facts() -> TabFacts {
        TabFacts { has_path: false, is_text: true, pinned: false, color: None }
    }

    fn enabled(m: &MenuSpec, id: usize) -> Option<bool> {
        fn find(m: &MenuSpec, id: usize) -> Option<bool> {
            m.entries().iter().find_map(|e| match e {
                Entry::Item { id: i, enabled, .. } if *i == id => Some(*enabled),
                Entry::Sub { menu, .. } => find(menu, id),
                _ => None,
            })
        }
        find(m, id)
    }

    #[test]
    fn path_commands_are_greyed_out_for_an_unsaved_tab() {
        let m = tab_spec(&facts());
        assert_eq!(enabled(&m, c::ID_CP_FULLPATH), Some(false));
        assert_eq!(enabled(&m, c::ID_TAB_REVEAL), Some(false));
        let saved = tab_spec(&TabFacts { has_path: true, ..facts() });
        assert_eq!(enabled(&saved, c::ID_CP_FULLPATH), Some(true));
    }

    #[test]
    fn the_pin_item_says_what_it_will_do() {
        let label = |m: &MenuSpec| match &m.entries()[0] {
            Entry::Item { label, .. } => label.clone(),
            _ => String::new(),
        };
        assert_eq!(label(&tab_spec(&facts())), "Pin Tab");
        assert_eq!(label(&tab_spec(&TabFacts { pinned: true, ..facts() })), "Unpin Tab");
    }

    #[test]
    fn only_the_tabs_colour_is_ticked_and_none_is_offered_when_one_is_set() {
        let m = tab_spec(&TabFacts { color: Some(2), ..facts() });
        let ticked: Vec<usize> = m
            .entries()
            .iter()
            .filter_map(|e| if let Entry::Sub { menu, .. } = e { Some(menu) } else { None })
            .flat_map(|menu| menu.entries().iter())
            .filter_map(|e| if let Entry::Item { id, check: Some(true), .. } = e { Some(*id) } else { None })
            .collect();
        assert_eq!(ticked, vec![c::ID_TAB_COLOR_BASE + 2]);
        assert_eq!(enabled(&m, c::ID_TAB_COLOR_NONE), Some(true));
        assert_eq!(enabled(&tab_spec(&facts()), c::ID_TAB_COLOR_NONE), Some(false));
    }

    #[test]
    fn compare_needs_a_text_tab() {
        let m = tab_spec(&TabFacts { is_text: false, ..facts() });
        assert_eq!(enabled(&m, c::ID_TAB_CMP_FILE), Some(false));
        assert_eq!(enabled(&m, c::ID_TAB_CMP_CLIP), Some(false));
    }

    #[test]
    fn the_tab_menu_has_no_stray_separators_once_tidied() {
        let m = tab_spec(&facts()).tidy();
        let e = m.entries();
        assert!(!matches!(e.first(), Some(Entry::Sep)) && !matches!(e.last(), Some(Entry::Sep)));
        assert!(e.windows(2).all(|w| !(w[0] == Entry::Sep && w[1] == Entry::Sep)));
    }

    #[test]
    fn the_editor_menu_shows_the_users_shortcut_over_the_default() {
        let mut keys = std::collections::BTreeMap::new();
        keys.insert("7".to_string(), "Ctrl+Q".to_string());
        let m = editor_spec(vec![Some(("Edit: Cut", 7, "Ctrl+X")), None, Some(("Edit: Copy", 8, ""))], &keys);
        let labels: Vec<String> = m.entries().iter().filter_map(|e| if let Entry::Item { label, .. } = e { Some(label.clone()) } else { None }).collect();
        assert_eq!(labels, vec!["Cut\tCtrl+Q", "Copy"]);
        assert_eq!(m.entries()[1], Entry::Sep);
    }
}
