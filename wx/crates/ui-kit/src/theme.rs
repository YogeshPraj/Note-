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

/// The syntax palette the kit's own surfaces use (previews, the Markdown bar, the gallery).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Syntax {
    pub keyword: Rgb,
    pub string: Rgb,
    pub comment: Rgb,
    pub number: Rgb,
    pub type_: Rgb,
    pub function: Rgb,
}

/// Every colour a component may use. Components read these; they never write a colour of their own.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Colors {
    /// The window chrome: menu bar, tool bar, tab strip.
    pub bg: Rgb,
    /// Popups and dialogs that sit above the chrome.
    pub bg_elevated: Rgb,
    /// Panels, labels, buttons and check boxes.
    pub surface: Rgb,
    /// Text boxes and lists.
    pub input: Rgb,
    /// The background of the active tab and of a pressed item that should lift off the chrome.
    pub bg_active: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    /// Text and icons of a control that cannot be used.
    pub disabled: Rgb,
    /// Pure black or white: hover and press washes are mixed from this.
    pub ink: Rgb,
    pub accent: Rgb,
    /// Text on the accent colour.
    pub accent_text: Rgb,
    /// A line or divider drawn on the accent colour.
    pub accent_line: Rgb,
    pub danger: Rgb,
    pub warning: Rgb,
    pub success: Rgb,
    /// Notices that inform (an update is available).
    pub info: Rgb,
    /// Small labels on tabs.
    pub badge: Rgb,
    pub selection: Rgb,
    pub syntax: Syntax,
}

/// Gaps in pixels at 100% scale; see [`Theme::px`].
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Spacing {
    pub xs: i32,
    pub sm: i32,
    pub md: i32,
    pub lg: i32,
    pub xl: i32,
    pub xxl: i32,
}

/// Corner radii in pixels at 100% scale.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Radius {
    pub sm: i32,
    pub md: i32,
    pub lg: i32,
}

/// Typefaces. An empty family means the platform's default.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Fonts {
    pub ui_family: &'static str,
    pub ui_size: i32,
    pub editor_family: &'static str,
    pub editor_size: i32,
}

/// The single source of truth for how the application looks.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Theme {
    pub dark: bool,
    /// The editor's colours, so the active tab can merge into the page.
    pub editor_bg: Rgb,
    pub editor_fg: Rgb,
    pub colors: Colors,
    pub spacing: Spacing,
    pub radius: Radius,
    pub font: Fonts,
    /// Display scale: 1.0 is 100%.
    pub scale: f64,
}

const SPACING: Spacing = Spacing { xs: 2, sm: 4, md: 8, lg: 12, xl: 16, xxl: 24 };
const RADIUS: Radius = Radius { sm: 2, md: 4, lg: 8 };
const FONTS: Fonts = Fonts {
    ui_family: if cfg!(windows) { "Segoe UI" } else { "" },
    ui_size: 9,
    editor_family: if cfg!(windows) { "Consolas" } else { "monospace" },
    editor_size: 10,
};

impl Default for Theme {
    fn default() -> Self {
        Theme::light()
    }
}

impl Theme {
    pub fn light() -> Theme {
        Theme {
            dark: false,
            editor_bg: (0xFF, 0xFF, 0xFF),
            editor_fg: (0, 0, 0),
            colors: Colors {
                bg: (0xF3, 0xF3, 0xF3),
                bg_elevated: (0xFF, 0xFF, 0xFF),
                surface: (0xF0, 0xF0, 0xF0),
                input: (0xFF, 0xFF, 0xFF),
                bg_active: (0xE0, 0xE0, 0xE0),
                border: (0xD0, 0xD0, 0xD0),
                text: (0x1A, 0x1A, 0x1A),
                text_muted: (0x50, 0x50, 0x50),
                disabled: (0x9A, 0x9A, 0x9A),
                ink: (0, 0, 0),
                accent: (0x00, 0x7A, 0xCC),
                accent_text: (0xFF, 0xFF, 0xFF),
                accent_line: (0x4D, 0xA3, 0xDA),
                danger: (0xC0, 0x00, 0x00),
                warning: (0xD0, 0x8A, 0x12),
                success: (0x2E, 0x9E, 0x4F),
                info: (0x38, 0x83, 0xFA),
                badge: (0x4A, 0x7A, 0xFF),
                selection: (0xAD, 0xD6, 0xFF),
                syntax: Syntax {
                    keyword: (0x00, 0x00, 0xFF),
                    string: (0xA3, 0x15, 0x15),
                    comment: (0x00, 0x80, 0x00),
                    number: (0x09, 0x86, 0x58),
                    type_: (0x26, 0x7F, 0x99),
                    function: (0x79, 0x5E, 0x26),
                },
            },
            spacing: SPACING,
            radius: RADIUS,
            font: FONTS,
            scale: 1.0,
        }
    }

    pub fn dark() -> Theme {
        let light = Theme::light();
        Theme {
            dark: true,
            editor_bg: (0x1E, 0x1E, 0x1E),
            editor_fg: (0xD4, 0xD4, 0xD4),
            colors: Colors {
                bg: (0x20, 0x20, 0x20),
                bg_elevated: (0x25, 0x25, 0x26),
                surface: (0x2B, 0x2B, 0x2B),
                input: (0x1E, 0x1E, 0x1E),
                bg_active: (0x3A, 0x3A, 0x3A),
                border: (0x45, 0x45, 0x45),
                text: (0xE6, 0xE6, 0xE6),
                text_muted: (0xB0, 0xB0, 0xB0),
                disabled: (0x6E, 0x6E, 0x6E),
                ink: (0xFF, 0xFF, 0xFF),
                selection: (0x26, 0x4F, 0x78),
                syntax: Syntax {
                    keyword: (0x56, 0x9C, 0xD6),
                    string: (0xCE, 0x91, 0x78),
                    comment: (0x6A, 0x99, 0x55),
                    number: (0xB5, 0xCE, 0xA8),
                    type_: (0x4E, 0xC9, 0xB0),
                    function: (0xDC, 0xDC, 0xAA),
                },
                ..light.colors
            },
            ..light
        }
    }

    /// This theme with the editor's colours (they come from the editor's own colour scheme, not from ours).
    pub fn with_editor(self, bg: Rgb, fg: Rgb) -> Theme {
        Theme { editor_bg: bg, editor_fg: fg, ..self }
    }

    /// `px` pixels at 100% scaled to this display.
    pub fn px(self, px: i32) -> i32 {
        crate::num::round_i32(f64::from(px) * self.scale)
    }

    pub fn look(self) -> Look {
        Look { dark: self.dark, editor_bg: self.editor_bg, editor_fg: self.editor_fg, colors: self.colors }
    }

    /// The user-interface font.
    pub fn ui_font(self) -> Option<Font> {
        let f = Font::builder().with_point_size(self.font.ui_size);
        if self.font.ui_family.is_empty() { f.build() } else { f.with_face_name(self.font.ui_family).build() }
    }

    /// Background of panels, labels, buttons and check boxes.
    pub fn surface(self) -> Rgb {
        self.colors.surface
    }

    /// Background of text boxes and lists.
    pub fn input(self) -> Rgb {
        self.colors.input
    }

    pub fn ink(self) -> Rgb {
        self.colors.text
    }

    /// Background for a widget with `role`.
    pub fn background(self, role: Role) -> Rgb {
        match role {
            Role::Surface => self.colors.surface,
            Role::Input => self.colors.input,
            Role::Editor => self.editor_bg,
        }
    }

    /// Text colour for a widget with `role`.
    pub fn foreground(self, role: Role) -> Rgb {
        match role {
            Role::Editor => self.editor_fg,
            _ => self.colors.text,
        }
    }
}
/// Called with each new theme; returns false once its owner is gone, which removes it.
type Listener = Rc<dyn Fn(&Theme) -> bool>;

impl std::fmt::Debug for Hub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hub").finish_non_exhaustive()
    }
}

/// The current theme and who wants to hear about changes.
#[derive(Default)]
pub struct Hub {
    current: Option<Theme>,
    listeners: Vec<(u64, Listener)>,
    next_id: u64,
}

impl Hub {
    pub fn current(&self) -> Theme {
        self.current.unwrap_or_default()
    }

    /// Add a listener; the theme it must be applied to now, if one has been set already.
    pub fn subscribe(&mut self, f: Listener) -> (u64, Option<Theme>) {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.push((id, f));
        (id, self.current)
    }

    /// Forget listeners whose owners are gone.
    pub fn remove(&mut self, ids: &[u64]) {
        self.listeners.retain(|(id, _)| !ids.contains(id));
    }

    /// Record `theme`. Returns the listeners to call: all of them when the theme changed (or was never set, so the
    /// start-up theme is applied explicitly instead of following the system), none when it is the same.
    pub fn set(&mut self, theme: Theme) -> Vec<(u64, Listener)> {
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
    let dead: Vec<u64> = listeners.iter().filter(|(_, l)| !l(&theme)).map(|(id, _)| *id).collect();
    if !dead.is_empty() {
        HUB.with(|h| h.borrow_mut().remove(&dead));
    }
}

/// Call `f` now (if a theme is set) and on every change, until it returns false.
pub fn subscribe_while(f: impl Fn(&Theme) -> bool + 'static) {
    let f: Listener = Rc::new(f);
    let (id, now) = HUB.with(|h| h.borrow_mut().subscribe(f.clone()));
    if let Some(t) = now
        && !f(&t)
    {
        HUB.with(|h| h.borrow_mut().remove(&[id]));
    }
}

/// Call `f` now (if a theme is set) and on every change, for the life of the program.
pub fn subscribe(f: impl Fn(&Theme) + 'static) {
    subscribe_while(move |t| {
        f(t);
        true
    });
}

/// Keep `w` coloured for the theme (and styled natively) from now on; stops when the widget is destroyed.
pub fn adopt<W: WxWidget + Copy + 'static>(w: W, role: Role) {
    subscribe_while(move |_| {
        if !w.is_valid() {
            return false;
        }
        // Native theme first: it can reset colours, so ours go on last.
        style_native(&w);
        paint(&w, role);
        true
    });
}

/// What a widget is for, which decides its background.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Role {
    /// Panels, labels, buttons and check boxes.
    Surface,
    /// Text boxes and lists.
    Input,
    /// Panels that look like the editor itself: the workspace tree and the side lists.
    Editor,
}

pub fn background(role: Role) -> Colour {
    colour(current().background(role))
}

pub fn foreground(role: Role) -> Colour {
    colour(current().foreground(role))
}

/// Colour one widget for the current theme.
pub fn paint(w: &dyn WxWidget, role: Role) {
    w.set_background_color(background(role));
    w.set_foreground_color(foreground(role));
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
    crate::native::current().style_window(w.get_handle() as isize, is_dark());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn dark() -> Theme {
        Theme::dark()
    }

    fn counter() -> (Listener, Rc<Cell<u32>>) {
        let n = Rc::new(Cell::new(0));
        let n2 = n.clone();
        (
            Rc::new(move |_| {
                n2.set(n2.get() + 1);
                true
            }),
            n,
        )
    }

    #[test]
    fn the_first_set_notifies_even_when_it_is_the_default() {
        let mut h = Hub::default();
        let (l, n) = counter();
        assert_eq!(h.subscribe(l).1, None);
        for (_, l) in h.set(Theme::default()) {
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
        for (_, l) in h.set(dark()) {
            l(&dark());
        }
        assert_eq!((na.get(), nb.get()), (1, 1));
    }

    #[test]
    fn a_late_subscriber_gets_the_current_theme_to_apply() {
        let mut h = Hub::default();
        h.set(dark());
        let (l, _) = counter();
        assert_eq!(h.subscribe(l).1, Some(dark()));
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
    fn removed_listeners_are_no_longer_called() {
        let mut h = Hub::default();
        let (a, _) = counter();
        let (b, _) = counter();
        let (ida, _) = h.subscribe(a);
        h.subscribe(b);
        h.remove(&[ida]);
        assert_eq!(h.set(dark()).len(), 1);
    }

    #[test]
    fn editor_widgets_take_the_editors_colours() {
        let t = Theme::dark().with_editor((10, 20, 30), (200, 210, 220));
        assert_eq!(t.background(Role::Editor), (10, 20, 30));
        assert_eq!(t.foreground(Role::Editor), (200, 210, 220));
        assert_eq!(t.background(Role::Surface), t.surface());
        assert_eq!(t.foreground(Role::Input), t.ink());
    }

    #[test]
    fn every_light_token_has_a_dark_counterpart_that_differs_where_it_should() {
        let (l, d) = (Theme::light().colors, Theme::dark().colors);
        assert_ne!(l.bg, d.bg);
        assert_ne!(l.text, d.text);
        assert_ne!(l.syntax.keyword, d.syntax.keyword);
        assert_eq!(l.accent, d.accent, "the accent is the brand colour in both themes");
    }

    #[test]
    fn text_is_readable_on_every_background_in_both_themes() {
        // WCAG-ish: a large difference in brightness between text and its background.
        let luma = |c: Rgb| (u32::from(c.0) * 299 + u32::from(c.1) * 587 + u32::from(c.2) * 114) / 1000;
        for t in [Theme::light(), Theme::dark()] {
            for bg in [t.colors.bg, t.colors.surface, t.colors.input, t.colors.bg_elevated] {
                assert!(luma(t.colors.text).abs_diff(luma(bg)) > 120, "text on {bg:?} in dark={}", t.dark);
                assert!(luma(t.colors.text_muted).abs_diff(luma(bg)) > 60);
            }
            assert!(luma(t.colors.accent_text).abs_diff(luma(t.colors.accent)) > 90);
        }
    }

    #[test]
    fn spacing_scales_with_the_display() {
        let mut t = Theme::light();
        assert_eq!(t.px(t.spacing.md), 8);
        t.scale = 1.5;
        assert_eq!(t.px(t.spacing.md), 12);
        assert_eq!(t.px(t.spacing.xs), 3);
    }

    #[test]
    fn dark_and_light_have_distinct_surfaces_and_readable_ink() {
        let (d, l) = (dark(), Theme::default());
        assert_ne!(d.surface(), l.surface());
        assert_ne!(d.input(), d.surface());
        assert!(d.ink().0 > d.surface().0 && l.ink().0 < l.surface().0);
    }
}
