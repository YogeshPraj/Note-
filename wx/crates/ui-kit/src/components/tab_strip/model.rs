//! The tab strip's layout and scrolling, with no toolkit in it.
//!
//! Scrolling is by tab, not by pixel: the model remembers the index of the first visible tab and every
//! arrow click moves it by one. This is how wxWidgets' own `wxAuiTabContainer` works (its tab offset), and it
//! removes the pixel-clamping edge cases of an offset in pixels. An arrow exists only while there is
//! something to scroll to on that side, and takes no room otherwise.

use crate::geom::Rect;

/// Width of a scroll button.
pub const ARROW_W: i32 = 24;
/// Room an arrow takes, including the gap after it.
pub const SLOT: i32 = ARROW_W + 2;
/// Width of the "+" button.
pub const PLUS_W: i32 = 26;
/// Space kept free at the right for the "+" button.
const RESERVE: i32 = 34;
/// Gap before the first tab when no left arrow is shown.
pub const INSET: i32 = 4;
/// Gap between tabs.
const GAP: i32 = 2;

/// The sizes above on this display (they are given at 100%).
#[derive(Clone, Copy, Debug)]
struct Metrics {
    arrow_w: i32,
    slot: i32,
    plus_w: i32,
    reserve: i32,
    inset: i32,
    gap: i32,
    /// The small gaps around the buttons.
    edge: i32,
}

impl Metrics {
    fn current() -> Metrics {
        let t = crate::theme::current();
        let arrow_w = t.px(ARROW_W);
        Metrics {
            arrow_w,
            slot: arrow_w + t.px(SLOT - ARROW_W),
            plus_w: t.px(PLUS_W),
            reserve: t.px(RESERVE),
            inset: t.px(INSET),
            gap: t.px(GAP),
            edge: t.px(2),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Btn {
    Left,
    Right,
    Plus,
}

/// Vertical placement, so the model needs no knowledge of the strip's height.
#[derive(Clone, Copy, Debug)]
pub struct Rows {
    pub tab_top: i32,
    pub tab_bottom: i32,
    pub btn_top: i32,
    pub btn_bottom: i32,
}

/// Where everything goes for one paint.
#[derive(Clone, Debug, Default)]
pub struct Layout {
    /// One rectangle per tab; tabs scrolled out of view lie outside `clip`.
    pub tabs: Vec<Rect>,
    pub left: Option<Rect>,
    pub right: Option<Rect>,
    pub plus: Rect,
    /// Horizontal span in which tabs are visible (clip region and hit-test area).
    pub clip: (i32, i32),
    pub first: usize,
}

impl Layout {
    pub fn tab_at(&self, x: i32, y: i32) -> Option<usize> {
        if x < self.clip.0 || x >= self.clip.1 {
            return None;
        }
        self.tabs.iter().position(|r| r.has(x, y))
    }

    pub fn button_at(&self, x: i32, y: i32) -> Option<Btn> {
        match (self.left, self.right) {
            (Some(l), _) if l.has(x, y) => Some(Btn::Left),
            (_, Some(r)) if r.has(x, y) => Some(Btn::Right),
            _ if self.plus.has(x, y) => Some(Btn::Plus),
            _ => None,
        }
    }

    pub fn can_scroll(&self, b: Btn) -> bool {
        match b {
            Btn::Left => self.left.is_some(),
            Btn::Right => self.right.is_some(),
            Btn::Plus => true,
        }
    }
}

/// The rows of a vertical strip: one full-width rectangle per tab, stacked from `inset`.
pub fn column(count: usize, width: i32, row: i32, inset: i32) -> Layout {
    let tabs = (0..count)
        .map(|i| {
            let top = inset + i32::try_from(i).unwrap_or(0) * row;
            Rect::new(0, top, width, top + row)
        })
        .collect();
    Layout { tabs, left: None, right: None, plus: Rect::default(), clip: (0, width), first: 0 }
}
/// Scroll state: which tab is first in view, and whether the active tab must be brought into view.
#[derive(Debug)]
pub struct Scroll {
    first: usize,
    reveal: bool,
}

impl Default for Scroll {
    fn default() -> Self {
        Self { first: 0, reveal: true }
    }
}

impl Scroll {
    /// Bring the active tab into view at the next layout.
    pub fn reveal_active(&mut self) {
        self.reveal = true;
    }

    /// One tab towards the left or right.
    pub fn step(&mut self, b: Btn) {
        match b {
            Btn::Left => self.first = self.first.saturating_sub(1),
            Btn::Right => self.first += 1,
            Btn::Plus => {}
        }
    }

    /// Lay out `widths` (tab widths without the gap) in a strip `strip_w` wide.
    pub fn layout(&mut self, widths: &[i32], active: usize, strip_w: i32, rows: Rows) -> Layout {
        let (n, m) = (widths.len(), Metrics::current());
        let full = strip_w - m.reserve;
        // Width of tabs [s, e).
        let span = |s: usize, e: usize| -> i32 { widths[s.min(n)..e.min(n)].iter().map(|w| w + m.gap).sum() };
        let start_x = |s: usize| if s > 0 { m.slot } else { m.inset };
        // Do tabs from `s` to the end fit without a right arrow?
        let fits = |s: usize| start_x(s) + span(s, n) <= full;

        if n == 0 || fits(0) {
            self.first = 0;
            self.reveal = false;
        } else {
            self.first = self.first.min(n - 1);
            if self.reveal {
                if active < self.first {
                    self.first = active;
                } else {
                    // The smallest first tab that lets the active one be seen whole.
                    while self.first < active {
                        let end = full - if fits(self.first) { 0 } else { m.slot };
                        if start_x(self.first) + span(self.first, active + 1) <= end {
                            break;
                        }
                        self.first += 1;
                    }
                }
                self.reveal = false;
            }
            // Never scroll past the point where the last tab is fully shown.
            let max_first = (0..n).find(|s| fits(*s)).unwrap_or(n - 1);
            self.first = self.first.min(max_first);
        }

        let first = self.first;
        let left = first > 0;
        let right = n > 0 && !fits(first);
        let x0 = start_x(first);
        let end = full - if right { m.slot } else { 0 } + 2 * m.edge;

        let mut tabs = Vec::with_capacity(n);
        for (i, w) in widths.iter().enumerate().take(n) {
            let l = if i >= first { x0 + span(first, i) } else { x0 - span(i, first) };
            tabs.push(Rect::new(l, rows.tab_top, l + w, rows.tab_bottom));
        }
        let plus_x = if right { end } else { (x0 + span(first, n) + 2 * m.edge).min(end) };
        Layout {
            tabs,
            left: left.then(|| Rect::new(m.edge, rows.btn_top, m.edge + m.arrow_w, rows.btn_bottom)),
            right: right.then(|| Rect::new(strip_w - m.arrow_w - m.edge, rows.btn_top, strip_w - m.edge, rows.btn_bottom)),
            plus: Rect::new(plus_x, rows.btn_top, plus_x + m.plus_w, rows.btn_bottom - m.edge),
            clip: (if left { m.slot } else { 0 }, end),
            first,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROWS: Rows = Rows { tab_top: 4, tab_bottom: 32, btn_top: 6, btn_bottom: 30 };

    fn strip(n: usize, tab_w: i32, strip_w: i32) -> (Scroll, Vec<i32>, i32) {
        (Scroll::default(), vec![tab_w; n], strip_w)
    }

    /// A tab is whole when it lies inside the clip span.
    fn whole(l: &Layout, i: usize) -> bool {
        l.tabs[i].l >= l.clip.0 && l.tabs[i].r <= l.clip.1
    }

    #[test]
    fn few_tabs_need_no_arrows_and_start_at_the_inset() {
        let (mut s, w, sw) = strip(3, 100, 800);
        let l = s.layout(&w, 0, sw, ROWS);
        assert!(l.left.is_none() && l.right.is_none());
        assert_eq!(l.tabs[0].l, INSET);
        assert_eq!(l.plus.l, l.tabs[2].r + 6);
    }

    #[test]
    fn no_tabs_still_has_a_plus() {
        let mut s = Scroll::default();
        let l = s.layout(&[], 0, 800, ROWS);
        assert!(l.tabs.is_empty() && l.left.is_none() && l.right.is_none());
        assert!(l.plus.l >= INSET);
    }

    #[test]
    fn first_tab_has_no_left_arrow_and_no_gap() {
        let (mut s, w, sw) = strip(12, 120, 800);
        let l = s.layout(&w, 0, sw, ROWS);
        assert!(l.left.is_none());
        assert!(l.right.is_some());
        assert_eq!(l.tabs[0].l, INSET);
        assert_eq!(l.clip.0, 0);
    }

    #[test]
    fn stepping_right_shows_the_left_arrow_and_moves_one_tab() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        s.step(Btn::Right);
        let l = s.layout(&w, 0, sw, ROWS);
        assert_eq!(l.first, 1);
        assert!(l.left.is_some());
        assert_eq!(l.tabs[1].l, SLOT);
        assert_eq!(l.clip.0, SLOT);
    }

    #[test]
    fn scrolling_to_the_end_hides_the_right_arrow_and_shows_the_last_tab_whole() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        for _ in 0..30 {
            s.step(Btn::Right);
        }
        let l = s.layout(&w, 0, sw, ROWS);
        assert!(l.right.is_none());
        assert!(l.left.is_some());
        assert!(whole(&l, 11));
        assert!(l.plus.r <= sw);
    }

    #[test]
    fn stepping_left_at_the_start_is_harmless() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        s.step(Btn::Left);
        let l = s.layout(&w, 0, sw, ROWS);
        assert_eq!(l.first, 0);
        assert!(l.left.is_none());
    }

    #[test]
    fn revealing_the_last_tab_scrolls_just_enough() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        s.reveal_active();
        let l = s.layout(&w, 11, sw, ROWS);
        assert!(whole(&l, 11));
        assert!(l.right.is_none());
    }

    #[test]
    fn revealing_a_tab_in_the_middle_makes_it_whole() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        s.reveal_active();
        let l = s.layout(&w, 6, sw, ROWS);
        assert!(whole(&l, 6));
    }

    #[test]
    fn revealing_the_first_tab_scrolls_all_the_way_back() {
        let (mut s, w, sw) = strip(12, 120, 800);
        for _ in 0..30 {
            s.step(Btn::Right);
        }
        s.layout(&w, 11, sw, ROWS);
        s.reveal_active();
        let l = s.layout(&w, 0, sw, ROWS);
        assert_eq!(l.first, 0);
        assert!(l.left.is_none());
        assert!(whole(&l, 0));
    }

    #[test]
    fn closing_tabs_until_they_fit_removes_the_arrows() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        for _ in 0..5 {
            s.step(Btn::Right);
        }
        let few = vec![120; 4];
        let l = s.layout(&few, 0, sw, ROWS);
        assert!(l.left.is_none() && l.right.is_none());
        assert_eq!(l.tabs[0].l, INSET);
    }

    #[test]
    fn closing_a_tab_at_the_end_clamps_the_scroll() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        for _ in 0..30 {
            s.step(Btn::Right);
        }
        s.layout(&w, 11, sw, ROWS);
        let fewer = vec![120; 11];
        let l = s.layout(&fewer, 10, sw, ROWS);
        assert!(whole(&l, 10));
        assert!(l.right.is_none());
    }

    #[test]
    fn hit_testing_ignores_tabs_scrolled_out_of_view() {
        let (mut s, w, sw) = strip(12, 120, 800);
        s.layout(&w, 0, sw, ROWS);
        s.step(Btn::Right);
        s.step(Btn::Right);
        let l = s.layout(&w, 0, sw, ROWS);
        // Tab 0 lies left of the clip, so no point selects it.
        assert!(l.tabs[0].r <= l.clip.0);
        assert!((0..800).all(|x| l.tab_at(x, 10) != Some(0)));
        assert_eq!(l.tab_at(l.tabs[3].l + 5, 10), Some(3));
    }

    #[test]
    fn buttons_are_found_only_while_present() {
        let (mut s, w, sw) = strip(12, 120, 800);
        let l = s.layout(&w, 0, sw, ROWS);
        assert_eq!(l.button_at(10, 10), None, "no left arrow at the start");
        assert_eq!(l.button_at(sw - 10, 10), Some(Btn::Right));
        assert_eq!(l.button_at(l.plus.l + 2, 10), Some(Btn::Plus));
    }

    #[test]
    fn a_column_stacks_full_width_rows_and_hits_them() {
        let l = column(3, 180, 30, 4);
        assert_eq!(l.tabs[0], Rect::new(0, 4, 180, 34));
        assert_eq!(l.tabs[2].t, 64);
        assert_eq!(l.tab_at(10, 40), Some(1));
        assert_eq!(l.tab_at(10, 200), None);
        assert_eq!(l.tab_at(190, 40), None);
        assert_eq!(l.button_at(10, 40), None);
    }
}
