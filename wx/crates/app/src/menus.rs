//! The main menu bar: its structure is `notepp_core::main_menu` (plain data); this turns a menu of it into a wx menu.

use wxdragon::prelude::Menu;

pub use notepp_core::main_menu::*;

/// Build the wxWidgets menu for a menu under construction.
pub fn realize(m: M) -> Menu {
    crate::menu_spec::realize(&spec(m))
}
