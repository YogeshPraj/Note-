//! A menu as plain data, turned into a wx menu in one place.
//!
//! Every menu in the application (the menu bar, context menus, the status bar popups) is described as a
//! [`MenuSpec`] and then realized. Building the description and tidying it (no leading, trailing or doubled
//! separators, no empty submenus) has no toolkit in it and is unit tested.

use notepp_core::shortcuts;
use wxdragon::prelude::{ItemKind, Menu};

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Item {
        id: usize,
        label: String,
        enabled: bool,
        /// None: a plain item; Some(state): a tick box that starts ticked or not.
        check: Option<bool>,
    },
    Sep,
    Sub {
        label: String,
        menu: MenuSpec,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MenuSpec {
    entries: Vec<Entry>,
}

impl MenuSpec {
    pub fn new() -> MenuSpec {
        MenuSpec::default()
    }

    #[cfg(test)]
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn item(mut self, id: usize, label: &str) -> Self {
        self.entries.push(Entry::Item { id, label: label.to_string(), enabled: true, check: None });
        self
    }

    /// An item that can be greyed out.
    pub fn item_if(mut self, id: usize, label: &str, enabled: bool) -> Self {
        self.entries.push(Entry::Item { id, label: label.to_string(), enabled, check: None });
        self
    }

    /// An item with a tick box.
    pub fn check(mut self, id: usize, label: &str, checked: bool) -> Self {
        self.entries.push(Entry::Item { id, label: label.to_string(), enabled: true, check: Some(checked) });
        self
    }

    pub fn sep(mut self) -> Self {
        self.entries.push(Entry::Sep);
        self
    }

    pub fn sub(mut self, label: &str, menu: MenuSpec) -> Self {
        self.entries.push(Entry::Sub { label: label.to_string(), menu });
        self
    }

    /// Add an entry built elsewhere (used when converting from another representation).
    pub fn push(&mut self, e: Entry) {
        self.entries.push(e);
    }

    /// Remove separators that would show as clutter (leading, trailing, doubled) and submenus with nothing in them.
    pub fn tidy(mut self) -> Self {
        let mut out: Vec<Entry> = Vec::new();
        for e in self.entries.drain(..) {
            match e {
                Entry::Sep => {
                    if !matches!(out.last(), None | Some(Entry::Sep)) {
                        out.push(Entry::Sep);
                    }
                }
                Entry::Sub { label, menu } => {
                    let menu = menu.tidy();
                    if !menu.is_empty() {
                        out.push(Entry::Sub { label, menu });
                    }
                }
                other @ Entry::Item { .. } => out.push(other),
            }
        }
        while matches!(out.last(), Some(Entry::Sep)) {
            out.pop();
        }
        MenuSpec { entries: out }
    }

    /// The ids of every item, submenus included.
    #[cfg(test)]
    pub fn ids(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for e in &self.entries {
            match e {
                Entry::Item { id, .. } => v.push(*id),
                Entry::Sub { menu, .. } => v.extend(menu.ids()),
                Entry::Sep => {}
            }
        }
        v
    }

    /// Build the wx menu.
    pub fn realize(&self) -> Menu {
        let menu = Menu::builder().build();
        for e in &self.entries {
            match e {
                Entry::Item { id, label, enabled, check } => {
                    let kind = if check.is_some() { ItemKind::Check } else { ItemKind::Normal };
                    menu.append(crate::platform::wx::to_i32(*id), &accelerator_text(label), "", kind);
                    if let Some(on) = check {
                        menu.check_item(crate::platform::wx::to_i32(*id), *on);
                    }
                    if !enabled {
                        menu.enable_item(crate::platform::wx::to_i32(*id), false);
                    }
                }
                Entry::Sep => menu.append_separator(),
                Entry::Sub { label, menu: sub } => {
                    menu.append_submenu(sub.realize(), &accelerator_text(label), "");
                }
            }
        }
        menu
    }
}

/// "Name\tCtrl+Z or Alt+Backspace" -> "Name\tCtrl+Z": wxWidgets takes one accelerator per item, and only one it
/// can parse; otherwise the label is shown without any.
pub fn accelerator_text(label: &str) -> String {
    match label.split_once('\t') {
        None => label.to_string(),
        Some((name, keys)) => {
            let first = keys.split(" or ").next().unwrap_or(keys).trim();
            if shortcuts::parse(first).is_some() { format!("{name}\t{first}") } else { name.to_string() }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tidy_removes_leading_trailing_and_doubled_separators() {
        let m = MenuSpec::new().sep().item(1, "A").sep().sep().item(2, "B").sep().tidy();
        assert_eq!(m.entries().len(), 3);
        assert!(matches!(m.entries()[0], Entry::Item { id: 1, .. }));
        assert_eq!(m.entries()[1], Entry::Sep);
        assert!(matches!(m.entries()[2], Entry::Item { id: 2, .. }));
    }

    #[test]
    fn tidy_drops_empty_submenus_and_the_separator_they_leave_behind() {
        let m = MenuSpec::new().item(1, "A").sep().sub("Empty", MenuSpec::new().sep()).tidy();
        assert_eq!(m.entries().len(), 1);
    }

    #[test]
    fn tidy_keeps_a_submenu_with_content_and_tidies_inside_it() {
        let m = MenuSpec::new().sub("Colour", MenuSpec::new().item(5, "Red").sep().tidy()).tidy();
        match &m.entries()[0] {
            Entry::Sub { menu, .. } => assert_eq!(menu.entries().len(), 1),
            _ => panic!("submenu lost"),
        }
    }

    #[test]
    fn tidy_of_nothing_is_nothing() {
        assert!(MenuSpec::new().sep().sep().tidy().is_empty());
    }

    #[test]
    fn items_carry_enabled_and_check_state() {
        let m = MenuSpec::new().item_if(1, "A", false).check(2, "B", true).item(3, "C");
        assert!(matches!(m.entries()[0], Entry::Item { enabled: false, check: None, .. }));
        assert!(matches!(m.entries()[1], Entry::Item { enabled: true, check: Some(true), .. }));
        assert!(matches!(m.entries()[2], Entry::Item { enabled: true, check: None, .. }));
    }

    #[test]
    fn ids_include_submenu_items_in_order() {
        let m = MenuSpec::new().item(1, "A").sub("S", MenuSpec::new().item(2, "B").item(3, "C")).sep().item(4, "D");
        assert_eq!(m.ids(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn accelerators_keep_the_first_parsable_choice() {
        assert_eq!(accelerator_text("Undo\tCtrl+Z or Alt+Backspace"), "Undo\tCtrl+Z");
        assert_eq!(accelerator_text("Save\tCtrl+S"), "Save\tCtrl+S");
    }

    #[test]
    fn labels_without_a_shortcut_are_unchanged() {
        assert_eq!(accelerator_text("Close"), "Close");
    }

    #[test]
    fn an_accelerator_wx_cannot_parse_is_dropped() {
        assert_eq!(accelerator_text("Odd\tNotAKey+Q"), "Odd");
    }
}
