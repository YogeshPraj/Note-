//! Native window services the controls need: dark title bars, menus and controls, a custom menu bar, and a few
//! window styles. Everything operating-system specific that is about *windows* lives here; actions on the
//! system (open a folder, move a file to the trash) belong to the application, not the kit.
//!
//! Window handles are raw native handles (`0` = none). On platforms without an implementation every call does
//! nothing.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod winmenu;

pub trait Native: Sync {
    /// Make the OS-drawn parts that follow the application (popup menus) dark or light.
    fn set_app_dark(&self, _dark: bool) {}
    /// Dark or light title bar for a top-level window, and the matching native theme for all its controls.
    fn style_window(&self, _window: isize, _dark: bool) {}
    /// Take the native menu bar off `window` so a custom bar can draw in its place; returns the menu handle.
    fn detach_menu_bar(&self, _window: isize) -> isize {
        0
    }
    /// Pop up menu `index` of `count` from a detached bar, moving to its neighbours on Left/Right.
    fn popup_menu(&self, _bar: isize, _window: isize, _index: i32, _count: i32, _anchor: &dyn Fn(i32) -> (i32, i32)) {}
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
        n.set_app_dark(true);
        n.style_window(1, true);
    }
}
