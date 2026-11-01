//! Dark and light colours for the parts of the window that wx draws, plus the OS-level dark styling
//! (title bar, native controls, popup menus) through [`crate::platform`].

use std::sync::atomic::{AtomicBool, Ordering};

use wxdragon::prelude::*;

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Record the theme; true when it changed.
pub fn set_dark(on: bool) -> bool {
    DARK.swap(on, Ordering::Relaxed) != on
}

/// What a widget is for, which decides its background.
#[derive(Clone, Copy, PartialEq)]
pub enum Role {
    /// Panels, labels, buttons and check boxes.
    Surface,
    /// Text boxes and lists.
    Input,
}

pub fn background(role: Role) -> Colour {
    match (is_dark(), role) {
        (true, Role::Surface) => Colour::rgb(0x2B, 0x2B, 0x2B),
        (true, Role::Input) => Colour::rgb(0x1E, 0x1E, 0x1E),
        (false, Role::Surface) => Colour::rgb(0xF0, 0xF0, 0xF0),
        (false, Role::Input) => Colour::rgb(0xFF, 0xFF, 0xFF),
    }
}

pub fn ink() -> Colour {
    if is_dark() { Colour::rgb(0xE6, 0xE6, 0xE6) } else { Colour::rgb(0x1A, 0x1A, 0x1A) }
}

/// Colour one widget for the current theme.
pub fn paint(w: &dyn WxWidget, role: Role) {
    w.set_background_color(background(role));
    w.set_foreground_color(ink());
}

/// Dark title bar and native control theme for a window and everything inside it.
pub fn style_native(w: &dyn WxWidget) {
    crate::platform::current().style_window(w.get_handle() as isize, is_dark());
}
