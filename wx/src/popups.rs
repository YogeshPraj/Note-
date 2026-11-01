//! Context menus: tabs, the editor and the status bar segments.

use notepp_core::commands as c;
use notepp_core::workspace::{TabKind, TAB_COLORS};
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::extra::{context_entries, context_menu_path};

fn add(menu: &Menu, id: usize, label: &str, enabled: bool) {
    menu.append(id as i32, label, "", ItemKind::Normal);
    if !enabled {
        menu.enable_item(id as i32, false);
    }
}

/// Show `menu` at `at` (screen coordinates); its commands arrive as ordinary menu events.
fn show(frame: &Frame, menu: &mut Menu, at: Point) {
    frame.popup_menu(menu, Some(at));
}

/// The right-click menu of the active tab.
pub fn tab_menu(app: &Shared, at: Point) {
    let (frame, has_path, is_text, pinned, color) = {
        let Ok(a) = app.try_borrow() else { return };
        let t = &a.ws.tabs[a.ws.active];
        (a.frame, t.path.is_some(), t.kind == TabKind::Text, t.pinned, t.color)
    };
    let mut menu = Menu::builder().build();
    add(&menu, c::ID_TAB_PIN, if pinned { "Unpin Tab" } else { "Pin Tab" }, true);
    menu.append_separator();
    add(&menu, c::ID_CLOSE, "Close", true);
    add(&menu, c::ID_CLOSE_ALL, "Close All", true);
    add(&menu, c::ID_CLOSE_OTHERS, "Close All But This", true);
    add(&menu, c::ID_CLOSE_LEFT, "Close All to the Left", true);
    add(&menu, c::ID_CLOSE_RIGHT, "Close All to the Right", true);
    menu.append_separator();
    let colors = Menu::builder().build();
    for (i, (name, _)) in TAB_COLORS.iter().enumerate() {
        let id = (c::ID_TAB_COLOR_BASE + i) as i32;
        colors.append(id, name, "", ItemKind::Check);
        colors.check_item(id, color == Some(i as u8));
    }
    colors.append_separator();
    add(&colors, c::ID_TAB_COLOR_NONE, "No colour", color.is_some());
    menu.append_submenu(colors, "Tab Colour", "");
    menu.append_separator();
    add(&menu, c::ID_CP_FULLPATH, "Copy Full Path", has_path);
    add(&menu, c::ID_OPEN_FOLDER_EXPLORER, "Open Containing Folder", has_path);
    menu.append_separator();
    add(&menu, c::ID_TAB_CMP_FILE, "Compare...", is_text);
    add(&menu, c::ID_TAB_CMP_CLIP, "Compare with Clipboard", is_text);
    menu.append_separator();
    add(&menu, c::ID_TAB_REVEAL, "Reveal in File Tree", has_path);
    show(&frame, &mut menu, at);
}

/// Right-click in the editor: the menu listed in contextmenu.txt.
pub fn editor_menu(app: &Shared, at: Point) {
    let (frame, shortcuts) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.settings.shortcuts.clone())
    };
    let text = std::fs::read_to_string(context_menu_path()).unwrap_or_default();
    let text = if text.is_empty() { crate::extra::default_context_menu().to_string() } else { text };
    let entries = context_entries(&text);
    if entries.is_empty() {
        return;
    }
    let mut menu = Menu::builder().build();
    for e in entries {
        match e {
            None => menu.append_separator(),
            Some((name, id, default)) => {
                let label = name.split_once(": ").map_or(name, |(_, l)| l);
                let key = shortcuts.get(&id.to_string()).map(String::as_str).unwrap_or(default);
                menu.append(id as i32, &if key.is_empty() { label.to_string() } else { format!("{label}\t{key}") }, "", ItemKind::Normal);
            }
        }
    }
    show(&frame, &mut menu, at);
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
    let mut menu = crate::menus::realize(m);
    show(&frame, &mut menu, at);
}
