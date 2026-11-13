//! Context menus: tabs, the editor and the status bar segments. Each is a [`MenuSpec`] realized at the click.

use std::cell::Cell;
use std::rc::Rc;

use notepp_core::workspace::TAB_COLORS;
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

/// The tool tips of menu items: a favourite shows its full path.
fn tip_of(app: &Shared) -> Rc<dyn Fn(usize) -> String> {
    let a = app.clone();
    Rc::new(move |id| {
        let Some(i) = id.checked_sub(notepp_core::commands::ID_FAV_BASE) else { return String::new() };
        a.try_borrow().ok().and_then(|a| a.ws.favourite_list().get(i).map(|(_, tip)| tip.clone())).unwrap_or_default()
    })
}

/// Show `spec` at `at` (screen coordinates); choosing an item runs its command. Items that are also in the menu bar
/// show its current state: Cut and Copy greyed without a selection, the current encoding or line ending ticked.
fn show(app: &Shared, frame: &Frame, spec: MenuSpec, at: Point) {
    let Some(list) = app.try_borrow().ok().map(|a| entries(&spec.tidy(), Some(&a.menubar), true)) else { return };
    let handlers = Handlers { pick: pick_handler(app), nav: Rc::new(|_| {}), moved: Rc::new(|_| {}), closed: Rc::new(|| {}), tip: tip_of(app) };
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
        tip: tip_of(app),
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

pub fn tab_menu(app: &Shared, at: Point) {
    let Some((frame, facts)) = app.try_borrow().ok().map(|a| (a.frame, notepp_core::context_menus::TabFacts::of(&a.ws))) else { return };
    show(app, &frame, notepp_core::context_menus::tab_menu(&facts), at);
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
    show(app, &frame, notepp_core::context_menus::editor_menu(&entries, &shortcuts), at);
}

/// Right-click on the minimap: hide it (View > Document Map brings it back).
pub fn minimap_menu(app: &Shared, at: Point) {
    let Ok(a) = app.try_borrow() else { return };
    let frame = a.frame;
    drop(a);
    show(app, &frame, notepp_core::context_menus::minimap_menu(), at);
}

/// A click on the encoding / line-ending / language segment of the status bar.
pub fn status_menu(app: &Shared, seg: isize, at: Point) {
    use notepp_core::status_bar::StatusAction;
    let Some(action) = crate::chrome::status_action(seg) else { return };
    if action == StatusAction::Agents {
        let Ok(frame) = app.try_borrow().map(|a| a.frame) else { return };
        return show(app, &frame, notepp_core::context_menus::agents_menu(), at);
    }
    let (frame, m) = {
        let Ok(a) = app.try_borrow() else { return };
        let m = match action {
            StatusAction::Encoding => a.menus.enc,
            StatusAction::LineEnding => a.menus.eol,
            StatusAction::Language => a.menus.lang,
            StatusAction::Agents => return,
        };
        (a.frame, m)
    };
    show(app, &frame, crate::menus::spec(m), at);
}
