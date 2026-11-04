//! What a screen reader sees of a custom-painted bar. Such a bar is one window as far as the system knows, so each one
//! publishes its items here while painting: their names, roles and places. A reader can then list them, say which is
//! selected, and press them. Windows only (MSAA); elsewhere the bar just gets its name.
//!
//! ```ignore
//! let items = Items::default();
//! a11y::attach(panel, "Tabs", Bar::Tabs, items.clone(), |i| select(i));
//! items.publish(vec![Item::new("notes.txt", Role::Tab, rect)]);
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wxdragon::prelude::*;

use crate::geom::Rect;

/// What kind of bar this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    Tabs,
    Tools,
    Menus,
    Status,
}

/// What kind of thing an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Tab,
    Button,
    Menu,
    Text,
}

/// One thing in a bar. `rect` is in the bar's client coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub role: Role,
    pub rect: Rect,
    pub selected: bool,
    pub disabled: bool,
}

impl Item {
    pub fn new(name: &str, role: Role, rect: Rect) -> Item {
        Item { name: name.to_string(), role, rect, selected: false, disabled: false }
    }
}

/// The items a bar shows now. Cheap to clone; clones share them.
#[derive(Debug, Clone, Default)]
pub struct Items(Rc<RefCell<Vec<Item>>>);

impl Items {
    /// Replace the items, as they were just painted.
    pub fn publish(&self, items: Vec<Item>) {
        if let Ok(mut now) = self.0.try_borrow_mut() {
            *now = items;
        }
    }

    /// Item `child` (children count from 1; 0 is the bar itself).
    fn child(&self, child: i32) -> Option<Item> {
        let index = usize::try_from(child).ok()?.checked_sub(1)?;
        self.0.try_borrow().ok()?.get(index).cloned()
    }

    fn count(&self) -> usize {
        self.0.try_borrow().map_or(0, |v| v.len())
    }

    /// The child at (x, y) in the bar's client coordinates; 0 when there is none.
    fn child_at(&self, x: i32, y: i32) -> i32 {
        self.0.try_borrow().ok().and_then(|v| v.iter().position(|i| i.rect.has(x, y))).map_or(0, |i| crate::num::to_i32(i + 1))
    }
}

#[cfg(windows)]
mod msaa {
    use std::cell::Cell;
    use std::rc::Rc;

    use wxdragon::accessible::{AccRole, AccState, AccStatus, Accessible, AccessibleImpl};
    use wxdragon::prelude::*;

    use super::{Bar, Item, Items, Role};

    pub(super) struct Reader {
        pub(super) panel: Panel,
        pub(super) name: String,
        pub(super) bar: Bar,
        pub(super) items: Items,
        /// The item to press once this call has returned, and the timer that does it.
        pub(super) pending: Rc<Cell<Option<usize>>>,
        pub(super) timer: Timer<Panel>,
    }

    fn state(item: &Item) -> AccState {
        let mut s = AccState::empty();
        if item.selected {
            s |= AccState::SELECTED;
        }
        if item.disabled {
            s |= AccState::UNAVAILABLE;
        }
        s
    }

    impl AccessibleImpl for Reader {
        fn get_child_count(&self) -> (AccStatus, i32) {
            (AccStatus::Ok, crate::num::to_i32(self.items.count()))
        }

        fn get_role(&self, child: i32) -> (AccStatus, AccRole) {
            let role = match (child, self.items.child(child).map(|i| i.role), self.bar) {
                (0, _, Bar::Tabs) => AccRole::PageTabList,
                (0, _, Bar::Tools) => AccRole::ToolBar,
                (0, _, Bar::Menus) => AccRole::MenuBar,
                (0, _, Bar::Status) => AccRole::StatusBar,
                (_, Some(Role::Tab), _) => AccRole::PageTab,
                (_, Some(Role::Button), _) => AccRole::PushButton,
                (_, Some(Role::Menu), _) => AccRole::MenuItem,
                (_, Some(Role::Text), _) => AccRole::StaticText,
                (_, None, _) => return (AccStatus::InvalidArg, AccRole::None),
            };
            (AccStatus::Ok, role)
        }

        fn get_name(&self, child: i32) -> (AccStatus, Option<String>) {
            if child == 0 {
                return (AccStatus::Ok, Some(self.name.clone()));
            }
            self.items.child(child).map_or((AccStatus::InvalidArg, None), |i| (AccStatus::Ok, Some(i.name)))
        }

        fn get_state(&self, child: i32) -> (AccStatus, AccState) {
            self.items.child(child).map_or((AccStatus::NotImplemented, AccState::empty()), |i| (AccStatus::Ok, state(&i)))
        }

        fn get_default_action(&self, child: i32) -> (AccStatus, Option<String>) {
            match self.items.child(child) {
                Some(i) if i.role != Role::Text && !i.disabled => (AccStatus::Ok, Some("Press".to_string())),
                _ => (AccStatus::NotImplemented, None),
            }
        }

        fn do_default_action(&self, child: i32) -> AccStatus {
            match self.items.child(child) {
                Some(i) if i.role != Role::Text && !i.disabled => {
                    self.pending.set(Some(crate::num::to_usize(child - 1)));
                    self.timer.start(1, true);
                    AccStatus::Ok
                }
                _ => AccStatus::NotImplemented,
            }
        }

        fn get_location(&self, child: i32) -> (AccStatus, wxdragon::geometry::Rect) {
            let Some(i) = self.items.child(child) else { return (AccStatus::NotImplemented, wxdragon::geometry::Rect::default()) };
            let at = self.panel.client_to_screen(Point::new(i.rect.l, i.rect.t));
            (AccStatus::Ok, wxdragon::geometry::Rect::new(at.x, at.y, i.rect.w(), i.rect.h()))
        }

        fn hit_test(&self, pt: wxdragon::geometry::Point) -> (AccStatus, i32, Option<Accessible>) {
            let at = self.panel.screen_to_client(Point::new(pt.x, pt.y));
            (AccStatus::Ok, self.items.child_at(at.x, at.y), None)
        }
    }
}

/// Name the bar and, on Windows, show its items to screen readers. `press` is told the index of the item pressed.
pub fn attach(panel: Panel, name: &str, bar: Bar, items: Items, press: impl Fn(usize) + 'static) {
    panel.set_accessibility_label(name);
    // A press can open a dialog, which must not run inside the reader's call.
    let pending: Rc<Cell<Option<usize>>> = Rc::default();
    let timer = Timer::new(&panel);
    let due = pending.clone();
    timer.on_tick(move |_| {
        if let Some(i) = due.take() {
            press(i);
        }
    });
    #[cfg(windows)]
    panel.set_accessible(wxdragon::accessible::Accessible::new(&panel, msaa::Reader { panel, name: name.to_string(), bar, items, pending, timer }));
    #[cfg(not(windows))]
    let _ = (bar, items, pending, timer);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Items {
        let items = Items::default();
        items.publish(vec![Item::new("a", Role::Tab, Rect::new(0, 0, 10, 10)), Item::new("b", Role::Tab, Rect::new(10, 0, 20, 10))]);
        items
    }

    #[test]
    fn children_count_from_one() {
        let items = items();
        assert_eq!(items.count(), 2);
        assert!(items.child(0).is_none());
        assert_eq!(items.child(2).map(|i| i.name), Some("b".to_string()));
        assert!(items.child(3).is_none());
    }

    #[test]
    fn a_point_finds_its_child() {
        assert_eq!(items().child_at(15, 5), 2);
        assert_eq!(items().child_at(25, 5), 0);
    }
}
