//! Turning a [`MenuSpec`] (plain data from `core::menu`) into a wx menu, the one place that knows wx menus.

pub use notepp_core::menu::{Entry, MenuSpec, accelerator_text};
use wxdragon::prelude::{ItemKind, Menu};

/// Build the wx menu for `spec`.
pub fn realize(spec: &MenuSpec) -> Menu {
    let menu = Menu::builder().build();
    for e in spec.entries() {
        match e {
            Entry::Item { id, label, enabled, check, .. } => {
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
                menu.append_submenu(realize(sub), &accelerator_text(label), "");
            }
        }
    }
    menu
}
