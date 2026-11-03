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
    #[must_use]
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
        self.font_of(self.font.ui_size, FontWeight::Normal)
    }

    /// The user-interface font in bold (row titles).
    pub fn strong_font(self) -> Option<Font> {
        self.font_of(self.font.ui_size, FontWeight::Bold)
    }

    /// A large heading font (page titles).
    pub fn title_font(self) -> Option<Font> {
        self.font_of(self.font.ui_size * 2, FontWeight::Normal)
    }

    fn font_of(self, size: i32, weight: FontWeight) -> Option<Font> {
        let f = Font::builder().with_point_size(size).with_weight(weight);
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

    /// Text colour on the surface and input backgrounds.
    pub fn text(self) -> Rgb {
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

mod apply;
pub use apply::*;

#[cfg(test)]
mod tests;
