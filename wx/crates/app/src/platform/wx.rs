//! The wxWidgets adapter: wx measures pixels, ids and positions in `i32`, the application indexes with `usize`.
//! Every such conversion in the shell goes through these checked helpers (they saturate instead of wrapping), so
//! the cast lints stay on everywhere else. The allow below covers the few raw conversions this module may need.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_precision_loss)]

pub use notepp_core::num::*;

/// The usable part of every screen (without the taskbar), the main screen first.
pub fn screen_areas() -> Vec<notepp_core::placement::Area> {
    let mut screens: Vec<_> = wxdragon::display::Display::all().filter(wxdragon::display::Display::is_ok).collect();
    screens.sort_by_key(|d| !d.is_primary());
    screens
        .iter()
        .map(|d| {
            let r = d.client_area();
            notepp_core::placement::Area { x: r.x, y: r.y, w: r.width, h: r.height }
        })
        .collect()
}
