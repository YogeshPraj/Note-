//! The keyboard inside a custom-painted bar. A bar is one window to the system, so its items cannot take the focus
//! themselves: this walks the items the bar publishes for screen readers (see [`super::a11y`]). Left and Right move in
//! screen order, Home and End go to the ends, Enter or Space presses, and a ring shows where the keyboard is. The ring
//! appears only after [`KeyNav::enter`], so using the bar with the mouse looks as it always did.

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::DeviceContext;
use wxdragon::prelude::*;

use super::a11y::{Item, Items, Role};
use crate::geom::{Rect, Rgb};
use crate::paint::outline;
use crate::theme;

/// Thickness of the ring, before scaling.
const RING: i32 = 2;

const KEY_TAB: i32 = 9;
const KEY_RETURN: i32 = 13;
const KEY_ESCAPE: i32 = 27;
const KEY_SPACE: i32 = 32;
const KEY_END: i32 = 312;
const KEY_HOME: i32 = 313;
const KEY_LEFT: i32 = 314;
const KEY_RIGHT: i32 = 316;
const KEY_NUMPAD_ENTER: i32 = 370;

/// Where the keyboard should go when it leaves a bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Next,
    Previous,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Back,
    Forward,
    First,
    Last,
}

/// The items the keyboard can land on, in screen order.
fn targets(items: &[Item]) -> Vec<usize> {
    let mut found: Vec<usize> = (0..items.len()).filter(|&i| items[i].role != Role::Text && !items[i].disabled).collect();
    found.sort_by_key(|&i| items[i].rect.l);
    found
}

/// Where the keyboard lands on entering: the active tab, else the first item. (A selected button is one that is switched
/// on, which says nothing about where to start.)
fn start(items: &[Item], found: &[usize]) -> Option<usize> {
    found.iter().copied().find(|&i| items[i].selected && items[i].role == Role::Tab).or_else(|| found.first().copied())
}

/// Where `step` goes from `at`; it stops at the ends. An item that is gone or disabled counts as the first.
fn moved(found: &[usize], at: usize, step: Step) -> Option<usize> {
    let here = found.iter().position(|&i| i == at).unwrap_or(0);
    let to = match step {
        Step::Back => here.saturating_sub(1),
        Step::Forward => (here + 1).min(found.len().checked_sub(1)?),
        Step::First => 0,
        Step::Last => found.len().checked_sub(1)?,
    };
    found.get(to).copied()
}

type Press = Rc<dyn Fn(usize)>;

#[derive(Default)]
struct Hooks {
    press: Option<Press>,
    moved: Option<Press>,
    exit: Option<Rc<dyn Fn(Exit)>>,
}

/// The keyboard in one bar. Cheap to clone; clones share it.
#[derive(Clone)]
pub struct KeyNav {
    panel: Panel,
    items: Items,
    hooks: Rc<RefCell<Hooks>>,
}

impl std::fmt::Debug for KeyNav {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyNav").finish_non_exhaustive()
    }
}

impl KeyNav {
    pub fn new(panel: Panel, items: Items) -> KeyNav {
        const WANTS_CHARS: i64 = 0x0004_0000; // wxWANTS_CHARS: arrows and Tab are the bar's, not window navigation
        let nav = KeyNav { panel, items, hooks: Rc::default() };
        panel.set_can_focus(true);
        panel.set_style_raw(panel.get_style_raw() | WANTS_CHARS);
        let hooked = nav.clone();
        panel.on_char_hook(move |e| hooked.on_key(&e));
        let down = nav.clone();
        panel.on_key_down(move |e| down.on_key(&e));
        let left = nav.clone();
        panel.on_kill_focus(move |e| {
            left.place(None);
            e.skip(true);
        });
        nav
    }

    /// What pressing item `i` does (what a screen reader's press does: see [`super::a11y::attach`]).
    pub fn set_press(&self, press: Press) {
        self.hooks.borrow_mut().press = Some(press);
    }

    /// Told each item the keyboard lands on, so a bar can scroll it into view.
    pub fn on_moved(&self, f: impl Fn(usize) + 'static) {
        self.hooks.borrow_mut().moved = Some(Rc::new(f));
    }

    /// Told when the keyboard should leave the bar.
    pub fn on_exit(&self, f: Rc<dyn Fn(Exit)>) {
        self.hooks.borrow_mut().exit = Some(f);
    }

    /// Is the keyboard in this bar?
    pub fn in_use(&self) -> bool {
        self.items.at().is_some()
    }

    /// Bring the keyboard here. False if the bar is hidden or has nothing to land on.
    pub fn enter(&self) -> bool {
        let items = self.items.snapshot();
        let found = targets(&items);
        let Some(at) = start(&items, &found).filter(|_| self.panel.is_shown()) else { return false };
        self.panel.set_focus();
        self.place(Some(at));
        true
    }

    fn place(&self, at: Option<usize>) {
        if self.items.at() == at {
            return;
        }
        self.items.set_at(at);
        self.panel.refresh(false, None);
        if let Some(i) = at {
            let moved = self.hooks.borrow().moved.clone();
            if let Some(f) = moved {
                f(i);
            }
            super::a11y::focus_moved(self.panel, i);
        }
    }

    fn on_key(&self, e: &WindowEventData) {
        let (WindowEventData::Keyboard(k), Some(at)) = (e, self.items.at()) else { return e.skip(true) };
        let code = k.get_key_code().unwrap_or(0);
        let step = match code {
            KEY_LEFT => Some(Step::Back),
            KEY_RIGHT => Some(Step::Forward),
            KEY_HOME => Some(Step::First),
            KEY_END => Some(Step::Last),
            _ => None,
        };
        if let Some(step) = step {
            return self.place(moved(&targets(&self.items.snapshot()), at, step).or(Some(at)));
        }
        let hooks = self.hooks.borrow();
        let (press, exit) = (hooks.press.clone(), hooks.exit.clone());
        drop(hooks);
        match (code, press, exit) {
            (KEY_RETURN | KEY_NUMPAD_ENTER | KEY_SPACE, Some(press), _) => press(at),
            (KEY_TAB, _, Some(exit)) => exit(if k.shift_down() { Exit::Previous } else { Exit::Next }),
            (KEY_ESCAPE, _, Some(exit)) => exit(Exit::Editor),
            _ => e.skip(true),
        }
    }
}

/// Draw the ring round the item the keyboard is on, if any; call it last while painting the bar.
pub fn ring(dc: &impl DeviceContext, items: &Items, colour: Rgb) {
    let Some(r) = items.at().and_then(|i| items.snapshot().get(i).map(|item| item.rect)) else { return };
    for i in 0..theme::current().px(RING) {
        outline(dc, Rect::new(r.l + i, r.t + i, r.r - i, r.b - i), colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(role: Role, left: i32) -> Item {
        Item::new("x", role, Rect::new(left, 0, left + 10, 10))
    }

    #[test]
    fn the_keyboard_skips_text_and_disabled_items() {
        let mut off = item(Role::Button, 10);
        off.disabled = true;
        assert_eq!(targets(&[item(Role::Text, 0), off, item(Role::Button, 20)]), vec![2]);
    }

    #[test]
    fn items_go_in_screen_order_whatever_order_the_bar_lists_them() {
        // The status bar lists its segments from the right.
        assert_eq!(targets(&[item(Role::Button, 90), item(Role::Button, 50), item(Role::Button, 10)]), vec![2, 1, 0]);
    }

    #[test]
    fn entering_starts_on_the_selected_item_else_the_first() {
        let mut items = vec![item(Role::Tab, 0), item(Role::Tab, 20)];
        assert_eq!(start(&items, &targets(&items)), Some(0));
        items[1].selected = true;
        assert_eq!(start(&items, &targets(&items)), Some(1));
        assert_eq!(start(&[], &[]), None);
    }

    #[test]
    fn a_button_that_is_switched_on_is_not_where_the_keyboard_starts() {
        let mut items = vec![item(Role::Button, 0), item(Role::Button, 20)];
        items[1].selected = true;
        assert_eq!(start(&items, &targets(&items)), Some(0));
    }

    #[test]
    fn steps_stop_at_the_ends() {
        let found = [4, 7, 9];
        assert_eq!(moved(&found, 7, Step::Forward), Some(9));
        assert_eq!(moved(&found, 9, Step::Forward), Some(9));
        assert_eq!(moved(&found, 7, Step::Back), Some(4));
        assert_eq!(moved(&found, 4, Step::Back), Some(4));
        assert_eq!(moved(&found, 7, Step::First), Some(4));
        assert_eq!(moved(&found, 7, Step::Last), Some(9));
    }

    #[test]
    fn an_item_that_went_away_counts_as_the_first() {
        assert_eq!(moved(&[4, 7], 5, Step::Forward), Some(7));
        assert_eq!(moved(&[], 5, Step::Forward), None);
    }
}
