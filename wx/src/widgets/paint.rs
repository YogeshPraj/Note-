//! Drawing helpers and the colour scheme shared by the shell's custom-painted controls.

use wxdragon::dc::DeviceContext;
use wxdragon::event::WindowEventData;
use wxdragon::prelude::*;

pub use super::geom::{Rect, Rgb, mix};

pub const WHITE: Rgb = (0xFF, 0xFF, 0xFF);
/// The blue used for selection, the active tab's top edge and the status bar.
pub const ACCENT: Rgb = (0x00, 0x7A, 0xCC);

/// Light or dark scheme, with the editor's colours so the active tab can merge into the page.
#[derive(Clone, Copy)]
pub struct Look {
    pub dark: bool,
    pub editor_bg: Rgb,
    pub editor_fg: Rgb,
}

impl Look {
    pub fn fg(self) -> Rgb {
        if self.dark { (0xE6, 0xE6, 0xE6) } else { (0x1A, 0x1A, 0x1A) }
    }
    pub fn dim(self) -> Rgb {
        if self.dark { (0xB0, 0xB0, 0xB0) } else { (0x50, 0x50, 0x50) }
    }
    pub fn ink(self) -> Rgb {
        if self.dark { WHITE } else { (0, 0, 0) }
    }
    pub fn base(self) -> Rgb {
        if self.dark { (0x20, 0x20, 0x20) } else { (0xF3, 0xF3, 0xF3) }
    }
}

pub fn colour(c: Rgb) -> Colour {
    Colour::rgb(c.0, c.1, c.2)
}

pub fn fill(dc: &impl DeviceContext, r: Rect, c: Rgb) {
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    dc.draw_rectangle(r.l, r.t, r.w(), r.h());
}

pub fn outline(dc: &impl DeviceContext, r: Rect, c: Rgb) {
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Transparent);
    dc.draw_rectangle(r.l, r.t, r.w(), r.h());
}

pub fn text_w(dc: &impl DeviceContext, s: &str) -> i32 {
    if s.is_empty() { 0 } else { dc.get_text_extent(s).0 }
}

/// `s` in `r`, vertically centred, horizontally centred when `center`.
pub fn text_at(dc: &impl DeviceContext, s: &str, r: Rect, c: Rgb, center: bool) {
    if s.is_empty() {
        return;
    }
    dc.set_text_foreground(colour(c));
    let (w, h) = dc.get_text_extent(s);
    let x = if center { r.l + (r.w() - w) / 2 } else { r.l };
    dc.draw_text(s, x, r.t + (r.h() - h) / 2);
}

/// Mouse position of a mouse event, or (-1, -1).
pub fn pos(e: &WindowEventData) -> (i32, i32) {
    let p = match e {
        WindowEventData::MouseButton(m) => m.get_position(),
        WindowEventData::MouseMotion(m) => m.get_position(),
        WindowEventData::MouseEnter(m) => m.get_position(),
        WindowEventData::MouseLeave(m) => m.get_position(),
        _ => None,
    };
    p.map_or((-1, -1), |p| (p.x, p.y))
}
