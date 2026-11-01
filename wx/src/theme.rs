//! Dark and light colours for the parts of the window that wx draws, plus the OS-level dark styling
//! (title bar, native controls, popup menus) through [`crate::platform`].

use std::sync::atomic::{AtomicBool, Ordering};

use wxdragon::prelude::*;

static DARK: AtomicBool = AtomicBool::new(false);
static APPLIED: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Record the theme; true when it changed, and on the first call so the start-up theme is applied explicitly
/// (the toolkit otherwise follows the system's dark setting).
pub fn set_dark(on: bool) -> bool {
    let first = !APPLIED.swap(true, Ordering::Relaxed);
    DARK.swap(on, Ordering::Relaxed) != on || first
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
    w.refresh(true, None);
}

/// Make a container fill its own background with the surface colour on every repaint, instead of relying on the
/// toolkit's erase (which follows the system's dark setting rather than ours).
pub fn own_background(panel: Panel) {
    panel.set_background_style(BackgroundStyle::Paint);
    panel.on_paint(move |_| {
        let dc = AutoBufferedPaintDC::new(&panel);
        let size = panel.get_client_size();
        let c = background(Role::Surface);
        dc.set_pen(c, 1, PenStyle::Solid);
        dc.set_brush(c, BrushStyle::Solid);
        dc.draw_rectangle(0, 0, size.width, size.height);
    });
}

/// Dark title bar and native control theme for a window and everything inside it.
pub fn style_native(w: &dyn WxWidget) {
    crate::platform::current().style_window(w.get_handle() as isize, is_dark());
}
