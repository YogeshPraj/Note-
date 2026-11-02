//! Native window services the controls need: dark title bars, menus and controls, a custom menu bar, and a few
//! window styles. Everything operating-system specific that is about *windows* lives here; actions on the
//! system (open a folder, move a file to the trash) belong to the application, not the kit.
//!
//! Window handles are raw native handles (`0` = none). On platforms without an implementation every call does
//! nothing.
//!
//! Raw handles and Win32 calls need integer casts, so the cast lints are allowed in this module (and its children) only.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_precision_loss)]

#[cfg(windows)]
mod windows;

pub trait Native: Sync {
    /// Dark or light title bar for a top-level window, and the matching native theme for all its controls.
    fn style_window(&self, _window: isize, _dark: bool) {}
    /// Take the native menu bar off `window` so a custom bar can draw in its place; returns the menu handle.
    fn detach_menu_bar(&self, _window: isize) -> isize {
        0
    }
    /// While a key handler is installed, every key press of this thread goes to it first, whichever window has the focus:
    /// it gets the virtual-key code, whether neither Ctrl nor Alt is held, and whether Alt alone is held, and returns true to consume the key. None
    /// removes it. Returns false where this is not supported (the application then routes keys itself).
    fn grab_keys(&self, _on_key: Option<fn(u32, bool, bool) -> bool>) -> bool {
        false
    }
    /// Give a top-level window the application icon (the first icon embedded in the executable): title bar, taskbar, Alt+Tab.
    fn set_app_icon(&self, _window: isize) {}
    /// Give the keyboard focus to the window itself (a page in a web view otherwise keeps it, and keys typed for a menu never get here).
    fn focus_window(&self, _window: isize) {}
    /// Remove the 3-D edge the system draws around a control (in the system colours, so it is white in a dark theme).
    fn remove_edge(&self, _window: isize) {}
    /// Does this program own the foreground window?
    fn is_foreground(&self) -> bool {
        true
    }
    /// While a handler is installed, every mouse-button press of this thread goes to it first with its screen position;
    /// returning true consumes the press. None removes it. False where this is not supported.
    fn grab_clicks(&self, _on_press: Option<fn(i32, i32) -> bool>) -> bool {
        false
    }
    /// Show a popup window without taking the focus from the window that opened it. False where that is not
    /// supported; the caller then shows the window normally.
    /// Hide a window that `show_without_activating` showed (the toolkit does not know it is visible).
    fn hide_window(&self, _window: isize) {}
    fn show_without_activating(&self, _window: isize) -> bool {
        false
    }
    /// Right-to-left or left-to-right reading order for a native editor window.
    fn set_rtl(&self, _window: isize, _rtl: bool) {}
    fn is_topmost(&self, _window: isize) -> bool {
        false
    }
    fn set_topmost(&self, _window: isize, _on: bool) {}
}

/// Used where there is no native implementation.
#[cfg_attr(windows, allow(dead_code))]
struct Unsupported;

impl Native for Unsupported {}

/// The implementation for the OS this build targets.
pub fn current() -> &'static dyn Native {
    #[cfg(windows)]
    {
        &windows::Windows
    }
    #[cfg(not(windows))]
    {
        &Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_implementation_is_a_harmless_no_op() {
        let n = Unsupported;
        assert!(!n.is_topmost(1));
        assert_eq!(n.detach_menu_bar(1), 0);
        n.style_window(1, true);
    }
}
