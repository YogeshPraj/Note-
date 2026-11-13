//! The popup menu: a themed replacement for the operating system's menus, which cannot follow the application's theme.
//!
//! The component draws its rows from the theme tokens and reports what was chosen; the application builds the
//! [`Entry`] tree and decides what a choice does. Sub-menus open as further levels beside their parent.
//!
//! The popup never takes the focus, so the window that opened it stays active: the application forwards key presses
//! to [`key`] while [`is_open`], and the popup holds the mouse so that a click anywhere else dismisses it.
//!
//! ```ignore
//! let entries = vec![Entry::item(1, "&Open\tCtrl+O", true, None), Entry::Sep, Entry::item(2, "E&xit", true, None)];
//! popup_menu::show(&frame, entries, (x, y), false, handlers);
//! ```

mod model;
mod paint;

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::prelude::*;

use crate::defer::later;
use crate::geom::Rect;
use crate::paint::pos;
use crate::theme;
pub use model::{Entry, Item};
use model::{Layout, by_mnemonic, first, hit, last, layout, place, place_sub, step};
use paint::{metrics, paint};

/// What the menu tells its owner.
#[derive(Clone)]
pub struct Handlers {
    /// A command was chosen.
    pub pick: Rc<dyn Fn(usize)>,
    /// Left (-1) or Right (+1) was pressed on the top level: the owner opens the neighbouring menu.
    pub nav: Rc<dyn Fn(i32)>,
    /// The pointer moved outside the menu (screen coordinates): a menu bar uses this to switch menus.
    pub moved: Rc<dyn Fn((i32, i32))>,
    /// The menu went away, by whatever route.
    pub closed: Rc<dyn Fn()>,
    /// The tool tip of the item with this command id (empty for none), shown while the pointer rests on it.
    pub tip: Rc<dyn Fn(usize) -> String>,
}

impl std::fmt::Debug for Handlers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Handlers").finish_non_exhaustive()
    }
}

struct Level {
    frame: Frame,
    panel: Panel,
    entries: Vec<Entry>,
    layout: Layout,
    hot: Option<usize>,
    /// The popup's top-left corner on the screen.
    origin: (i32, i32),
}

impl Level {
    fn contains(&self, p: (i32, i32)) -> bool {
        Rect::new(self.origin.0, self.origin.1, self.origin.0 + self.layout.width, self.origin.1 + self.layout.height).has(p.0, p.1)
    }

    fn row_at(&self, p: (i32, i32)) -> Option<usize> {
        hit(&self.entries, &self.layout, p.0 - self.origin.0, p.1 - self.origin.1)
    }
}

struct Session {
    levels: RefCell<Vec<Level>>,
    handlers: Handlers,
    owner: Frame,
    /// Was the application in the foreground when the menu opened? Only then is losing that a reason to close it.
    foreground: bool,
}

type Shared = Rc<Session>;

thread_local! {
    // The menu being shown, if any.
    static CURRENT: RefCell<Option<Shared>> = const { RefCell::new(None) };
}

fn current() -> Option<Shared> {
    CURRENT.with(|c| c.borrow().clone())
}

/// Is a menu showing?
pub fn is_open() -> bool {
    current().is_some()
}

/// Close the menu, if one is showing.
pub fn close_all() {
    if let Some(s) = current() {
        close_from(&s, 0);
    }
}

/// The application's window lost the focus: close the menu if the user went to another program (clicking the menu's own
/// window also deactivates the window that owns it, and must not close it).
pub fn deactivated() {
    if let Some(s) = current()
        && s.foreground
        && !crate::native::current().is_foreground()
    {
        close_from(&s, 0);
    }
}

/// Give a key press to the showing menu. Returns whether there was one (and so the key is spent).
pub fn key(code: i32, plain: bool) -> bool {
    match current() {
        Some(s) => {
            on_key(&s, code, plain);
            true
        }
        None => false,
    }
}

/// A mouse press anywhere in the application: one outside the menu dismisses it (and is spent, as with native menus).
fn on_press(x: i32, y: i32) -> bool {
    let Some(s) = current() else { return false };
    if s.levels.borrow().iter().any(|l| l.contains((x, y))) {
        return false;
    }
    defer_close(&s);
    true
}

/// A key press as the operating system reports it (Windows virtual-key codes), for platforms that can grab keys.
fn on_virtual_key(vk: u32, plain: bool, alt: bool) -> bool {
    let code = match vk {
        0x10..=0x12 | 0x5B | 0x5C => return false, // modifiers and the Windows keys are left alone
        // Alt+letter belongs to the menu bar (it switches to the menu with that letter), not to the open menu.
        0x30..=0x39 | 0x41..=0x5A if alt => return false,
        0x21 | 0x24 => KEY_HOME,
        0x22 | 0x23 => KEY_END,
        0x25 => KEY_LEFT,
        0x26 => KEY_UP,
        0x27 => KEY_RIGHT,
        0x28 => KEY_DOWN,
        other => i32::try_from(other).unwrap_or(0),
    };
    key(code, plain)
}

/// Show `entries` with its top-left corner at `at` (screen coordinates). `flip_up` lets a menu that would run off the bottom of the
/// screen open upwards instead (context menus); menus hanging from a bar are pushed up instead.
pub fn show(owner: &Frame, entries: Vec<Entry>, at: (i32, i32), flip_up: bool, handlers: Handlers) {
    show_from(owner, entries, at, flip_up, false, handlers);
}

/// Like `show`, for a menu opened from the keyboard: its first item is already highlighted, so Enter picks it.
pub fn show_for_keyboard(owner: &Frame, entries: Vec<Entry>, at: (i32, i32), flip_up: bool, handlers: Handlers) {
    show_from(owner, entries, at, flip_up, true, handlers);
}

fn show_from(owner: &Frame, entries: Vec<Entry>, at: (i32, i32), flip_up: bool, first_row: bool, handlers: Handlers) {
    close_all();
    let s: Shared = Rc::new(Session { levels: RefCell::new(Vec::new()), handlers, owner: *owner, foreground: crate::native::current().is_foreground() });
    CURRENT.with(|c| *c.borrow_mut() = Some(s.clone()));
    open_level(&s, entries, Anchor::Top(at, flip_up), first_row);
    crate::native::current().grab_keys(Some(on_virtual_key));
    crate::native::current().grab_clicks(Some(on_press));
}

#[derive(Clone, Copy)]
enum Anchor {
    Top((i32, i32), bool),
    Beside { parent: Rect, row_top: i32 },
}

fn screen_area(p: (i32, i32)) -> Rect {
    Display::from_point(Point::new(p.0, p.1)).map_or(Rect::new(0, 0, 100_000, 100_000), |d| {
        let a = d.client_area();
        Rect::new(a.x, a.y, a.x + a.width, a.y + a.height)
    })
}

fn open_level(s: &Shared, entries: Vec<Entry>, anchor: Anchor, pick_first: bool) {
    let frame = Frame::builder().with_parent(&s.owner).with_style(FrameStyle::StayOnTop | FrameStyle::NoTaskbar).with_size(Size::new(10, 10)).build();
    let panel = Panel::builder(&frame).build();
    panel.set_background_style(BackgroundStyle::Paint);
    // A click must not move the focus into the popup: that would activate it and deactivate the window that owns the menu.
    panel.set_can_focus(false);
    let lay = measure(panel, &entries);
    let size = (lay.width, lay.height);
    let origin = match anchor {
        Anchor::Top(at, flip) => place(at, size, screen_area(at), flip),
        Anchor::Beside { parent, row_top } => place_sub(parent, row_top, size, screen_area((parent.r, row_top))),
    };
    frame.set_client_size(Size::new(size.0, size.1));
    panel.set_size(Size::new(size.0, size.1));
    frame.move_window(origin.0, origin.1);
    let hot = if pick_first { first(&entries) } else { None };
    let i = {
        let mut lv = s.levels.borrow_mut();
        lv.push(Level { frame, panel, entries, layout: lay, hot, origin });
        lv.len() - 1
    };
    wire_level(s, i, panel);
    if !crate::native::current().show_without_activating(frame.get_handle() as isize) {
        frame.show(true);
    }
}

fn measure(panel: Panel, entries: &[Entry]) -> Layout {
    use wxdragon::dc::{ClientDC, DeviceContext};
    let dc = ClientDC::new(&panel);
    if let Some(f) = theme::current().ui_font() {
        dc.set_font(&f);
    }
    layout(entries, &|text| crate::paint::text_w(&dc, text), &metrics())
}

#[derive(Clone, Copy, PartialEq)]
enum Mouse {
    Move,
    Down,
    Up,
}

fn wire_level(s: &Shared, i: usize, panel: Panel) {
    let w = s.clone();
    panel.on_paint(move |_| {
        if let Some(l) = w.levels.borrow().get(i) {
            paint(panel, &l.entries, &l.layout, l.hot);
        }
    });
    let route = |kind: Mouse| {
        let w = s.clone();
        move |e: WindowEventData| {
            let (x, y) = pos(&e);
            let p = panel.client_to_screen(Point::new(x, y));
            mouse(&w, (p.x, p.y), kind);
        }
    };
    panel.on_mouse_motion(route(Mouse::Move));
    panel.on_mouse_left_down(route(Mouse::Down));
    panel.on_mouse_right_down(route(Mouse::Down));
    panel.on_mouse_left_up(route(Mouse::Up));
}

/// A mouse event at screen position `p`, whichever window of the menu has it.
fn mouse(s: &Shared, p: (i32, i32), kind: Mouse) {
    let target = s.levels.borrow().iter().enumerate().rev().find(|(_, l)| l.contains(p)).map(|(j, l)| (j, l.row_at(p)));
    match (target, kind) {
        (Some((j, row)), Mouse::Move) => hover(s, j, row),
        (Some((j, Some(row))), Mouse::Up) => choose(s, j, row),
        (None, Mouse::Move) => (s.handlers.moved)(p),
        (None, Mouse::Down) => defer_close(s),
        _ => {}
    }
}

fn close_from(s: &Shared, depth: usize) {
    let gone: Vec<Level> = {
        let mut lv = s.levels.borrow_mut();
        let at = depth.min(lv.len());
        lv.drain(at..).collect()
    };
    for l in gone {
        // Closing is deferred to idle time, which a modal dialog started by the picked command would postpone, and
        // the toolkit's own hide does not reach the system window at once either.
        l.frame.show(false);
        crate::native::current().hide_window(l.frame.get_handle() as isize);
        l.frame.close(true);
    }
    if depth == 0 {
        let mine = CURRENT.with(|c| {
            let mut c = c.borrow_mut();
            let mine = c.as_ref().is_some_and(|x| Rc::ptr_eq(x, s));
            if mine {
                *c = None;
            }
            mine
        });
        if mine {
            crate::native::current().grab_keys(None);
            crate::native::current().grab_clicks(None);
            (s.handlers.closed)();
        }
    }
}

fn refresh(s: &Shared, i: usize) {
    if let Some(l) = s.levels.borrow().get(i) {
        l.panel.refresh(false, None);
    }
}

fn hover(s: &Shared, i: usize, row: Option<usize>) {
    let changed = {
        let mut lv = s.levels.borrow_mut();
        let Some(l) = lv.get_mut(i) else { return };
        let changed = row.is_some() && l.hot != row;
        if changed {
            l.hot = row;
        }
        changed
    };
    if changed {
        refresh(s, i);
        show_tip(s, i);
        sync_sub(s, i, false);
    }
}

/// Put the tool tip of level `i`'s hot item on its window.
fn show_tip(s: &Shared, i: usize) {
    let lv = s.levels.borrow();
    let Some(l) = lv.get(i) else { return };
    let tip = match l.hot.map(|r| &l.entries[r]) {
        Some(Entry::Item(item)) => (s.handlers.tip)(item.id),
        _ => String::new(),
    };
    l.panel.set_tooltip(&tip);
}

/// Show the sub-menu of level `i`'s hot row, and nothing deeper if that row has none.
fn sync_sub(s: &Shared, i: usize, keyboard: bool) {
    close_from(s, i + 1);
    let next = {
        let lv = s.levels.borrow();
        let Some(l) = lv.get(i) else { return };
        let Some(row) = l.hot else { return };
        match &l.entries[row] {
            Entry::Sub { items, .. } => {
                let parent = Rect::new(l.origin.0, l.origin.1, l.origin.0 + l.layout.width, l.origin.1 + l.layout.height);
                Some((items.clone(), Anchor::Beside { parent, row_top: l.origin.1 + l.layout.rects[row].t - 1 }))
            }
            _ => None,
        }
    };
    if let Some((items, anchor)) = next {
        open_level(s, items, anchor, keyboard);
    }
}

fn choose(s: &Shared, i: usize, row: usize) {
    let entry = s.levels.borrow().get(i).map(|l| l.entries[row].clone());
    match entry {
        Some(Entry::Item(item)) if item.enabled => {
            let s = s.clone();
            later(&s.owner.clone(), move || {
                close_from(&s, 0);
                (s.handlers.pick)(item.id);
            });
        }
        Some(Entry::Sub { .. }) => {
            if let Some(l) = s.levels.borrow_mut().get_mut(i) {
                l.hot = Some(row);
            }
            sync_sub(s, i, true);
        }
        _ => {}
    }
}

mod keys;
use keys::*;
