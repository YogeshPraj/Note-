//! The application theme: one value, and a hub that components subscribe to.
//!
//! The shell sets the theme once ([`set`]); every component that has to look different in dark or light
//! subscribes with [`subscribe`] and re-styles itself when told. A component created after the theme was set
//! is told straight away, so creation order cannot leave it with the wrong colours.
//!
//! [`Theme`] and [`Hub`] have no toolkit in them and are unit tested; the functions below them apply the
//! theme to wx widgets and to the OS-drawn frame, menus and controls (through [`crate::platform`]).

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::prelude::*;

use super::geom::Rgb;
use super::paint::{Look, colour};

/// The colours that depend on the light/dark choice.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub dark: bool,
    /// The editor's colours, so the active tab can merge into the page.
    pub editor_bg: Rgb,
    pub editor_fg: Rgb,
}

impl Default for Theme {
    fn default() -> Self {
        Theme { dark: false, editor_bg: (0xFF, 0xFF, 0xFF), editor_fg: (0, 0, 0) }
    }
}

impl Theme {
    pub fn look(self) -> Look {
        Look { dark: self.dark, editor_bg: self.editor_bg, editor_fg: self.editor_fg }
    }

    /// Background of panels, labels, buttons and check boxes.
    pub fn surface(self) -> Rgb {
        if self.dark { (0x2B, 0x2B, 0x2B) } else { (0xF0, 0xF0, 0xF0) }
    }

    /// Background of text boxes and lists.
    pub fn input(self) -> Rgb {
        if self.dark { (0x1E, 0x1E, 0x1E) } else { (0xFF, 0xFF, 0xFF) }
    }

    pub fn ink(self) -> Rgb {
        if self.dark { (0xE6, 0xE6, 0xE6) } else { (0x1A, 0x1A, 0x1A) }
    }
}

type Listener = Rc<dyn Fn(&Theme)>;

/// The current theme and who wants to hear about changes.
#[derive(Default)]
pub struct Hub {
    current: Option<Theme>,
    listeners: Vec<Listener>,
}

impl Hub {
    pub fn current(&self) -> Theme {
        self.current.unwrap_or_default()
    }

    /// Add a listener; the theme it must be applied to now, if one has been set already.
    pub fn subscribe(&mut self, f: Listener) -> Option<Theme> {
        self.listeners.push(f);
        self.current
    }

    /// Record `theme`. Returns the listeners to call: all of them when the theme changed (or was never set, so the
    /// start-up theme is applied explicitly instead of following the system), none when it is the same.
    pub fn set(&mut self, theme: Theme) -> Vec<Listener> {
        if self.current == Some(theme) {
            return Vec::new();
        }
        self.current = Some(theme);
        self.listeners.clone()
    }
}

thread_local! {
    static HUB: RefCell<Hub> = RefCell::new(Hub::default());
}

pub fn current() -> Theme {
    HUB.with(|h| h.borrow().current())
}

pub fn is_dark() -> bool {
    current().dark
}

/// Set the theme and tell every subscriber if it changed.
pub fn set(theme: Theme) {
    let listeners = HUB.with(|h| h.borrow_mut().set(theme));
    for l in listeners {
        l(&theme);
    }
}

/// Call `f` now (if a theme is set) and on every change.
pub fn subscribe(f: impl Fn(&Theme) + 'static) {
    let f: Listener = Rc::new(f);
    let now = HUB.with(|h| h.borrow_mut().subscribe(f.clone()));
    if let Some(t) = now {
        f(&t);
    }
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
    let t = current();
    colour(match role {
        Role::Surface => t.surface(),
        Role::Input => t.input(),
    })
}

pub fn ink() -> Colour {
    colour(current().ink())
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
    // Repaint when the theme changes.
    subscribe(move |_| panel.refresh(true, None));
}

/// Dark title bar and native control theme for a window and everything inside it.
pub fn style_native(w: &dyn WxWidget) {
    crate::platform::current().style_window(w.get_handle() as isize, is_dark());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn dark() -> Theme {
        Theme { dark: true, ..Theme::default() }
    }

    fn counter() -> (Listener, Rc<Cell<u32>>) {
        let n = Rc::new(Cell::new(0));
        let n2 = n.clone();
        (Rc::new(move |_| n2.set(n2.get() + 1)), n)
    }

    #[test]
    fn the_first_set_notifies_even_when_it_is_the_default() {
        let mut h = Hub::default();
        let (l, n) = counter();
        assert_eq!(h.subscribe(l), None);
        for l in h.set(Theme::default()) {
            l(&Theme::default());
        }
        assert_eq!(n.get(), 1);
    }

    #[test]
    fn setting_the_same_theme_again_notifies_nobody() {
        let mut h = Hub::default();
        h.set(dark());
        assert!(h.set(dark()).is_empty());
    }

    #[test]
    fn a_change_notifies_every_listener_once() {
        let mut h = Hub::default();
        let (a, na) = counter();
        let (b, nb) = counter();
        h.subscribe(a);
        h.subscribe(b);
        h.set(Theme::default());
        for l in h.set(dark()) {
            l(&dark());
        }
        assert_eq!((na.get(), nb.get()), (1, 1));
    }

    #[test]
    fn a_late_subscriber_gets_the_current_theme_to_apply() {
        let mut h = Hub::default();
        h.set(dark());
        let (l, _) = counter();
        assert_eq!(h.subscribe(l), Some(dark()));
    }

    #[test]
    fn editor_colour_changes_count_as_a_change() {
        let mut h = Hub::default();
        let (l, _) = counter();
        h.subscribe(l);
        h.set(dark());
        let other = Theme { editor_bg: (1, 2, 3), ..dark() };
        assert_eq!(h.set(other).len(), 1);
        assert!(h.set(other).is_empty());
    }

    #[test]
    fn dark_and_light_have_distinct_surfaces_and_readable_ink() {
        let (d, l) = (dark(), Theme::default());
        assert_ne!(d.surface(), l.surface());
        assert_ne!(d.input(), d.surface());
        assert!(d.ink().0 > d.surface().0 && l.ink().0 < l.surface().0);
    }
}
