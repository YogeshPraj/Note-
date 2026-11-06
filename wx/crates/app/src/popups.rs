//! Context menus: tabs, the editor and the status bar segments. Each is a [`MenuSpec`] realized at the click.

use std::cell::Cell;
use std::rc::Rc;

use notepp_core::commands as c;
use notepp_core::workspace::{TAB_COLORS, TabKind};
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::extra::{context_entries, context_menu_path};
use crate::menu_spec::{Entry as SpecEntry, MenuSpec, accelerator_text};
use crate::widgets::popup_menu::{self as pm, Handlers};

thread_local! {
    // (how many menus have been opened, the bar menu showing now): a stale menu closing must not clear a newer one.
    static OPEN: Cell<(u64, Option<usize>)> = const { Cell::new((0, None)) };
}

/// The kit's entries for `spec`. `live` supplies the current enabled and ticked state (the native menu bar keeps it).
/// With `own_rules`, an item must also pass the spec's own enabled flag (a context menu's conditions, such as "Copy
/// Full Path" needing a file); the menu bar's own menus take the live state alone.
fn entries(spec: &MenuSpec, live: Option<&MenuBar>, own_rules: bool) -> Vec<pm::Entry> {
    spec.entries()
        .iter()
        .map(|e| match e {
            SpecEntry::Sep => pm::Entry::Sep,
            SpecEntry::Sub { label, menu } => pm::Entry::sub(&accelerator_text(label), entries(menu, live, own_rules)),
            SpecEntry::Item { id, label, enabled, check, swatch } => {
                let item = live.filter(|_| *id != 0).and_then(|b| b.find_item(crate::platform::wx::to_i32(*id)));
                let live_on = item.as_ref().map(MenuItem::is_enabled);
                let enabled = if own_rules { *enabled && live_on.unwrap_or(true) } else { live_on.unwrap_or(*enabled) };
                let check = check.map(|c| item.as_ref().map_or(c, MenuItem::is_checked));
                pm::Entry::item(*id, &accelerator_text(label), enabled, check).with_swatch(*swatch)
            }
        })
        .collect()
}

fn pick_handler(app: &Shared) -> Rc<dyn Fn(usize)> {
    let a = app.clone();
    Rc::new(move |id| {
        crate::app::run_command(&a, id);
        crate::app::refocus_editor(&a);
    })
}

/// Show `spec` at `at` (screen coordinates); choosing an item runs its command. Items that are also in the menu bar
/// show its current state: Cut and Copy greyed without a selection, the current encoding or line ending ticked.
fn show(app: &Shared, frame: &Frame, spec: MenuSpec, at: Point) {
    let Some(list) = app.try_borrow().ok().map(|a| entries(&spec.tidy(), Some(&a.menubar), true)) else { return };
    let handlers = Handlers { pick: pick_handler(app), nav: Rc::new(|_| {}), moved: Rc::new(|_| {}), closed: Rc::new(|| {}) };
    crate::app::take_focus_for_menu(app);
    pm::show(frame, list, (at.x, at.y), true, handlers);
}

/// Open menu `i` of the menu bar below its title. Opening the one that is already showing closes it instead (the
/// click that got here has already taken the focus from it).
pub fn bar_menu(app: &Shared, i: usize, keyboard: bool) {
    if OPEN.with(Cell::get).1 == Some(i) {
        return;
    }
    let (frame, list, at, count) = {
        let Ok(a) = app.try_borrow() else { return };
        let Some(m) = a.bar_menus.get(i) else { return };
        let (locate, count) = a.chrome.menu_locator();
        (a.frame, entries(&crate::menus::spec(*m), Some(&a.menubar), false), locate(crate::platform::wx::to_i32(i)), count)
    };
    let generation = OPEN.with(|o| {
        let g = o.get().0 + 1;
        o.set((g, Some(i)));
        g
    });
    set_bar_open(app, Some(i));
    let (a, b, m) = (app.clone(), app.clone(), app.clone());
    let handlers = Handlers {
        pick: pick_handler(app),
        nav: Rc::new(move |dir| {
            let n = crate::platform::wx::to_i32(usize::try_from(count).unwrap_or(1).max(1));
            bar_menu(&a, usize::try_from((crate::platform::wx::to_i32(i) + dir).rem_euclid(n)).unwrap_or(0), true);
        }),
        moved: Rc::new(move |(x, y)| {
            if let Ok(app) = m.try_borrow() {
                app.chrome.menu_pointer(x, y);
            }
        }),
        closed: Rc::new(move || {
            if OPEN.with(Cell::get).0 == generation {
                OPEN.with(|o| o.set((generation, None)));
                set_bar_open(&b, None);
                crate::app::refocus_editor(&b);
            }
        }),
    };
    crate::app::take_focus_for_menu(app);
    if keyboard {
        pm::show_for_keyboard(&frame, list, at, false, handlers);
    } else {
        pm::show(&frame, list, at, false, handlers);
    }
}

fn set_bar_open(app: &Shared, i: Option<usize>) {
    if let Ok(a) = app.try_borrow() {
        a.chrome.set_menu_open(i);
    }
}

/// What the tab menu depends on.
struct TabFacts {
    has_path: bool,
    is_text: bool,
    pinned: bool,
    color: Option<(u8, u8, u8)>,
    can_dock: bool,
    can_float: bool,
    docked: bool,
}

/// The right-click menu of a tab.
fn dock_spec(f: &TabFacts) -> MenuSpec {
    let sides = ["Left", "Right", "Top", "Bottom"].iter().enumerate().fold(MenuSpec::new(), |m, (i, side)| m.item_if(c::ID_DOCK_BASE + i, side, f.can_dock));
    sides.sep().item_if(c::ID_FLOAT_TAB, "Undock", f.can_float)
}

fn tab_spec(f: &TabFacts) -> MenuSpec {
    let preset = TAB_COLORS.iter().any(|(_, rgb)| Some(*rgb) == f.color);
    let mut colours =
        TAB_COLORS.iter().enumerate().fold(MenuSpec::new(), |m, (i, (name, rgb))| m.swatch(c::ID_TAB_COLOR_BASE + i, name, *rgb, f.color == Some(*rgb)));
    colours = match f.color {
        Some(rgb) if !preset => colours.sep().swatch(c::ID_TAB_COLOR_CUSTOM, "Custom colour...", rgb, true),
        _ => colours.sep().item(c::ID_TAB_COLOR_CUSTOM, "Custom colour..."),
    }
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
        .sub("Dock", dock_spec(f))
        .item_if(c::ID_UNDOCK, "Dock Back", f.docked)
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
        let (can_dock, docked) = (a.ws.can_dock(a.ws.active), a.ws.docked_index() == Some(a.ws.active) || a.ws.current_float().is_some());
        (
            a.frame,
            TabFacts {
                has_path: t.path.is_some(),
                is_text: t.kind == TabKind::Text,
                pinned: t.pinned,
                color: t.color,
                can_dock,
                can_float: a.ws.can_float(a.ws.active),
                docked,
            },
        )
    };
    show(app, &frame, tab_spec(&facts), at);
}

/// Mark the active tab with a colour chosen in the colour dialog.
pub fn custom_tab_colour(app: &Shared) {
    let (frame, tab, start) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.active, a.ws.tabs[a.ws.active].color.unwrap_or(TAB_COLORS[5].1))
    };
    if let Some(c) = crate::widgets::ask::colour_of(&frame, start)
        && let Ok(mut a) = app.try_borrow_mut()
    {
        a.ws.set_tab_color(tab, Some(c));
        a.refresh();
    }
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
    show(app, &frame, editor_spec(entries, &shortcuts), at);
}

/// Right-click on the minimap: hide it (View > Document Map brings it back).
pub fn minimap_menu(app: &Shared, at: Point) {
    let Ok(a) = app.try_borrow() else { return };
    let frame = a.frame;
    drop(a);
    show(app, &frame, MenuSpec::new().item(c::ID_MINIMAP, "Hide Minimap"), at);
}

/// A click on the encoding / line-ending / language segment of the status bar.
pub fn status_menu(app: &Shared, seg: isize, at: Point) {
    if seg == crate::chrome::SEG_AGENTS {
        let Ok(frame) = app.try_borrow().map(|a| a.frame) else { return };
        return show(app, &frame, MenuSpec::new().item(c::ID_MCP_LOG, "AI Activity Log").item(c::ID_MCP_STOP, "Stop AI Agents"), at);
    }
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
    show(app, &frame, crate::menus::spec(m), at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu_spec::Entry;

    fn facts() -> TabFacts {
        TabFacts { has_path: false, is_text: true, pinned: false, color: None, can_dock: false, can_float: false, docked: false }
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
        let m = tab_spec(&TabFacts { color: Some(TAB_COLORS[2].1), ..facts() });
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
