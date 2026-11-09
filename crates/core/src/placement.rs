//! Where the main window goes: the place it was left, if it can still be reached, or a size that fits the screen.
//!
//! The screens are given as their usable areas (without the taskbar), in pixels. Nothing here knows a toolkit.

use crate::session::FloatRect;

/// A usable part of a screen, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// How much of a screen a new window may take, in thousandths: room to see that it is a window.
const SHARE: i32 = 900;
/// The height of a title bar, roughly: the part of a window that must be on a screen to move it.
const TITLE: i32 = 32;
/// How much of a title bar must be on a screen across, to grab it.
const GRAB: i32 = 120;

/// A window of the size wanted, made smaller to fit `area` if it must, in its middle.
pub fn fit(want_w: i32, want_h: i32, area: Area) -> FloatRect {
    let w = want_w.min(area.w * SHARE / 1000).max(1);
    let h = want_h.min(area.h * SHARE / 1000).max(1);
    FloatRect { x: area.x + (area.w - w) / 2, y: area.y + (area.h - h) / 2, w, h, maximized: false }
}

/// The screen whose area holds the title bar of `r` well enough to grab it.
fn holding(r: &FloatRect, areas: &[Area]) -> Option<Area> {
    areas.iter().copied().find(|a| {
        let across = (r.x + r.w).min(a.x + a.w) - r.x.max(a.x);
        let top_inside = r.y >= a.y && r.y + TITLE <= a.y + a.h;
        top_inside && across >= GRAB.min(r.w)
    })
}

/// Where the window goes: where it was left, made to fit that screen if the screen has shrunk since, or else the size
/// wanted, fitted to the first screen (the main one). A place whose title bar is off every screen is not used: a screen may
/// have gone, or its scale changed, and a window that cannot be grabbed cannot be moved back.
pub fn place(saved: Option<FloatRect>, want_w: i32, want_h: i32, areas: &[Area]) -> FloatRect {
    let Some(&main) = areas.first() else { return FloatRect { x: 0, y: 0, w: want_w, h: want_h, maximized: false } };
    let Some(r) = saved.filter(|r| r.w > 0 && r.h > 0) else { return fit(want_w, want_h, main) };
    let Some(area) = holding(&r, areas) else { return FloatRect { maximized: r.maximized, ..fit(want_w, want_h, main) } };
    let (w, h) = (r.w.min(area.w), r.h.min(area.h));
    let x = r.x.clamp(area.x, area.x + area.w - w);
    let y = r.y.clamp(area.y, area.y + area.h - h);
    FloatRect { x, y, w, h, maximized: r.maximized }
}

/// What a window keeps for next time: where it is, or, while maximised, the place it goes back to. A minimised window says
/// nothing (`None`: keep what was kept); one that was maximised from the start keeps its size as it is.
pub fn remember(kept: Option<FloatRect>, now: FloatRect, minimized: bool) -> Option<FloatRect> {
    if minimized {
        return kept;
    }
    Some(match kept {
        Some(before) if now.maximized => FloatRect { maximized: true, ..before },
        _ => now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920 by 1200 laptop at 150%, taskbar at the bottom.
    const LAPTOP: Area = Area { x: 0, y: 0, w: 1920, h: 1152 };
    const BIG: Area = Area { x: 0, y: 0, w: 3840, h: 2112 };
    /// A second screen to the left of the main one.
    const LEFT: Area = Area { x: -1920, y: 0, w: 1920, h: 1040 };

    fn rect(x: i32, y: i32, w: i32, h: i32) -> FloatRect {
        FloatRect { x, y, w, h, maximized: false }
    }

    #[test]
    fn a_new_window_too_tall_for_a_laptop_fits_it_with_its_title_bar_showing() {
        // 1200 by 800 at 150%: the case that put the title bar above the screen.
        let r = place(None, 1800, 1200, &[LAPTOP]);
        assert!(r.y >= 0, "the title bar is on the screen");
        assert!(r.y + r.h <= LAPTOP.h && r.x + r.w <= LAPTOP.w);
        assert_eq!((r.w, r.h), (1728, 1036));
    }

    #[test]
    fn a_new_window_that_fits_keeps_its_size_and_sits_in_the_middle() {
        assert_eq!(place(None, 1200, 800, &[BIG]), rect(1320, 656, 1200, 800));
    }

    #[test]
    fn the_taskbar_is_left_alone() {
        let above_taskbar = Area { x: 0, y: 48, w: 1920, h: 1104 };
        let r = place(None, 1800, 1200, &[above_taskbar]);
        assert!(r.y >= 48);
    }

    #[test]
    fn a_window_comes_back_where_it_was_left() {
        let left = FloatRect { x: 100, y: 50, w: 900, h: 700, maximized: true };
        assert_eq!(place(Some(left), 1800, 1200, &[LAPTOP]), left);
    }

    #[test]
    fn a_window_left_on_a_screen_to_the_left_comes_back_there() {
        let left = rect(-1500, 100, 1000, 700);
        assert_eq!(place(Some(left), 1200, 800, &[LAPTOP, LEFT]), left);
    }

    #[test]
    fn a_window_on_a_screen_that_has_gone_comes_to_the_main_one_and_stays_maximised() {
        let gone = FloatRect { x: -1500, y: 100, w: 1000, h: 700, maximized: true };
        let r = place(Some(gone), 1800, 1200, &[LAPTOP]);
        assert!(r.x >= 0 && r.y >= 0);
        assert!(r.maximized);
    }

    #[test]
    fn a_title_bar_above_the_screen_is_not_used() {
        let r = place(Some(rect(60, -40, 1800, 1200)), 1800, 1200, &[LAPTOP]);
        assert!(r.y >= 0 && r.y + r.h <= LAPTOP.h);
    }

    #[test]
    fn a_window_left_bigger_than_its_screen_now_is_is_made_to_fit() {
        // Left on a big monitor at 100%, then the laptop alone: the place is on the laptop, the size is not.
        let r = place(Some(rect(10, 10, 3000, 2000)), 1200, 800, &[LAPTOP]);
        assert_eq!(r, rect(0, 0, 1920, 1152));
    }

    #[test]
    fn a_window_hanging_off_one_side_is_pulled_in() {
        let r = place(Some(rect(1500, 100, 1000, 700)), 1200, 800, &[LAPTOP]);
        assert_eq!(r, rect(920, 100, 1000, 700));
    }

    #[test]
    fn a_sliver_of_title_bar_is_not_enough() {
        let r = place(Some(rect(1880, 100, 1000, 700)), 1200, 800, &[LAPTOP]);
        assert_eq!(r, fit(1200, 800, LAPTOP));
    }

    #[test]
    fn a_maximised_window_keeps_the_place_it_goes_back_to() {
        let before = rect(100, 100, 900, 600);
        let filling = FloatRect { x: -8, y: -8, w: 1936, h: 1048, maximized: true };
        assert_eq!(remember(Some(before), filling, false), Some(FloatRect { maximized: true, ..before }));
    }

    #[test]
    fn a_window_keeps_where_it_is_and_a_minimised_one_keeps_what_it_had() {
        let before = rect(100, 100, 900, 600);
        let now = rect(300, 200, 1000, 700);
        assert_eq!(remember(Some(before), now, false), Some(now));
        assert_eq!(remember(Some(before), now, true), Some(before));
        assert_eq!(remember(None, now, true), None);
        let maximised_from_the_start = FloatRect { maximized: true, ..now };
        assert_eq!(remember(None, maximised_from_the_start, false), Some(maximised_from_the_start));
    }

    #[test]
    fn nothing_saved_or_no_screen_known_still_gives_a_window() {
        assert_eq!(place(Some(rect(0, 0, 0, 0)), 1200, 800, &[LAPTOP]), fit(1200, 800, LAPTOP));
        assert_eq!(place(None, 1200, 800, &[]), rect(0, 0, 1200, 800));
    }
}
