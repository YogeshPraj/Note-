//! The pure model of a popup menu: its entries, where each row goes, and how the keyboard moves through it.

use crate::components::menu_bar::split_mnemonic;
use crate::geom::Rect;

/// A command row.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: usize,
    /// The label, with an `&` before its mnemonic letter.
    pub label: String,
    /// The shortcut shown at the right edge ("Ctrl+S"), or empty.
    pub accel: String,
    pub enabled: bool,
    /// `Some` for switches: whether it is on.
    pub check: Option<bool>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Item(Item),
    Sep,
    Sub { label: String, items: Vec<Entry> },
}

impl Entry {
    /// A command row from "Label\tShortcut" text.
    pub fn item(id: usize, text: &str, enabled: bool, check: Option<bool>) -> Entry {
        let (label, accel) = text.split_once('\t').unwrap_or((text, ""));
        Entry::Item(Item { id, label: label.to_string(), accel: accel.to_string(), enabled, check })
    }

    pub fn sub(label: &str, items: Vec<Entry>) -> Entry {
        Entry::Sub { label: label.to_string(), items }
    }

    /// Can the keyboard or the pointer choose this row?
    pub fn selectable(&self) -> bool {
        match self {
            Entry::Item(i) => i.enabled,
            Entry::Sep => false,
            Entry::Sub { .. } => true,
        }
    }

    fn raw_label(&self) -> &str {
        match self {
            Entry::Item(i) => &i.label,
            Entry::Sub { label, .. } => label,
            Entry::Sep => "",
        }
    }

    /// The label as drawn (no `&` marker) and the index of the underlined letter.
    pub fn display(&self) -> (String, Option<usize>) {
        split_mnemonic(self.raw_label())
    }

    pub fn mnemonic(&self) -> Option<char> {
        let (text, at) = self.display();
        at.and_then(|i| text.chars().nth(i))
    }

    pub fn accel(&self) -> &str {
        match self {
            Entry::Item(i) => &i.accel,
            _ => "",
        }
    }
}

/// Sizes in pixels, already scaled for the display.
#[derive(Clone, Copy, Debug)]
pub struct Metrics {
    pub row: i32,
    pub sep: i32,
    /// Space before the label's column (room for the tick) and after the last column.
    pub gutter: i32,
    pub pad: i32,
    /// Space between the label and the shortcut.
    pub gap: i32,
    /// Room for the sub-menu arrow.
    pub arrow: i32,
    pub border: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub rects: Vec<Rect>,
    pub width: i32,
    pub height: i32,
    /// Where the shortcut column starts.
    pub accel_x: i32,
}

/// Where each row goes; `text_w` measures a string.
pub fn layout(entries: &[Entry], text_w: &dyn Fn(&str) -> i32, m: &Metrics) -> Layout {
    let label_w = entries.iter().map(|e| text_w(&e.display().0)).max().unwrap_or(0);
    let accel_w = entries.iter().map(|e| text_w(e.accel())).max().unwrap_or(0);
    let has_sub = entries.iter().any(|e| matches!(e, Entry::Sub { .. }));
    let inner = m.gutter + label_w + if accel_w > 0 { m.gap + accel_w } else { 0 } + if has_sub { m.arrow } else { 0 } + m.pad;
    let width = inner + 2 * m.border;
    let mut y = m.border;
    let rects = entries
        .iter()
        .map(|e| {
            let h = if matches!(e, Entry::Sep) { m.sep } else { m.row };
            let r = Rect::new(m.border, y, width - m.border, y + h);
            y += h;
            r
        })
        .collect();
    let accel_x = width - m.border - m.pad - if has_sub { m.arrow } else { 0 } - accel_w;
    Layout { rects, width, height: y + m.border, accel_x }
}

/// The selectable row at (x, y).
pub fn hit(entries: &[Entry], l: &Layout, x: i32, y: i32) -> Option<usize> {
    l.rects.iter().position(|r| r.has(x, y)).filter(|&i| entries[i].selectable())
}

/// The next selectable row in direction `dir` (1 or -1) from `from`, wrapping around.
pub fn step(entries: &[Entry], from: Option<usize>, dir: i32) -> Option<usize> {
    let n = entries.len();
    if n == 0 {
        return None;
    }
    let start = match (from, dir > 0) {
        (Some(i), _) => i,
        (None, true) => n - 1,
        (None, false) => 0,
    };
    (1..=n).map(|k| if dir > 0 { (start + k) % n } else { (start + n - k % n) % n }).find(|&i| entries[i].selectable())
}

pub fn first(entries: &[Entry]) -> Option<usize> {
    entries.iter().position(Entry::selectable)
}

pub fn last(entries: &[Entry]) -> Option<usize> {
    entries.iter().rposition(Entry::selectable)
}

/// The row whose mnemonic is `key`, searching on from `after` so repeated presses cycle through duplicates.
pub fn by_mnemonic(entries: &[Entry], key: char, after: Option<usize>) -> Option<usize> {
    let n = entries.len();
    let start = after.map_or(0, |i| i + 1);
    (0..n).map(|k| (start + k) % n.max(1)).find(|&i| entries[i].selectable() && entries[i].mnemonic().is_some_and(|c| c.eq_ignore_ascii_case(&key)))
}

/// Where a top-level popup goes: at `at`, moved to stay inside `area`; `flip_up` puts it above `at` when it does not fit below
/// (context menus), otherwise it is pushed up.
pub fn place(at: (i32, i32), size: (i32, i32), area: Rect, flip_up: bool) -> (i32, i32) {
    let x = at.0.min(area.r - size.0).max(area.l);
    let y = if at.1 + size.1 <= area.b {
        at.1
    } else if flip_up && at.1 - size.1 >= area.t {
        at.1 - size.1
    } else {
        (area.b - size.1).max(area.t)
    };
    (x, y)
}

/// Where a sub-menu goes: beside its parent (to the left when the right side is full), level with its row.
pub fn place_sub(parent: Rect, row_top: i32, size: (i32, i32), area: Rect) -> (i32, i32) {
    let x = if parent.r + size.0 <= area.r { parent.r - 2 } else { (parent.l - size.0 + 2).max(area.l) };
    let y = row_top.min(area.b - size.1).max(area.t);
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: Metrics = Metrics { row: 20, sep: 8, gutter: 24, pad: 10, gap: 20, arrow: 16, border: 1 };

    fn width(s: &str) -> i32 {
        i32::try_from(s.chars().count()).unwrap_or(0) * 7
    }

    fn sample() -> Vec<Entry> {
        vec![
            Entry::item(1, "&New\tCtrl+N", true, None),
            Entry::Sep,
            Entry::item(2, "&Open", false, None),
            Entry::sub("&Recent", vec![Entry::item(3, "a", true, None)]),
            Entry::item(4, "O&ther", true, Some(true)),
        ]
    }

    #[test]
    fn an_item_splits_its_label_from_its_shortcut() {
        let e = Entry::item(1, "Save\tCtrl+S", true, None);
        assert!(matches!(&e, Entry::Item(i) if i.label == "Save" && i.accel == "Ctrl+S"));
    }

    #[test]
    fn rows_stack_and_separators_are_thin() {
        let l = layout(&sample(), &width, &M);
        assert_eq!(l.rects[0].t, M.border);
        assert_eq!(l.rects[1].h(), M.sep);
        assert_eq!(l.rects[2].t, l.rects[1].b);
        assert_eq!(l.height, M.border * 2 + 20 * 4 + 8);
        assert!(l.rects.iter().all(|r| r.w() == l.width - 2 * M.border));
    }

    #[test]
    fn the_width_fits_the_longest_label_the_shortcut_and_the_arrow() {
        let l = layout(&sample(), &width, &M);
        assert_eq!(l.width, 2 + 24 + 7 * 6 + 20 + 7 * 6 + 16 + 10);
    }

    #[test]
    fn only_enabled_rows_are_hit() {
        let e = sample();
        let l = layout(&e, &width, &M);
        let at = |i: usize| (5, i32::midpoint(l.rects[i].t, l.rects[i].b));
        assert_eq!(hit(&e, &l, at(0).0, at(0).1), Some(0));
        assert_eq!(hit(&e, &l, at(1).0, at(1).1), None);
        assert_eq!(hit(&e, &l, at(2).0, at(2).1), None);
        assert_eq!(hit(&e, &l, at(3).0, at(3).1), Some(3));
    }

    #[test]
    fn the_arrow_keys_skip_separators_and_disabled_rows_and_wrap() {
        let e = sample();
        assert_eq!(step(&e, Some(0), 1), Some(3));
        assert_eq!(step(&e, Some(4), 1), Some(0));
        assert_eq!(step(&e, Some(0), -1), Some(4));
        assert_eq!(step(&e, None, 1), Some(0));
        assert_eq!(step(&e, None, -1), Some(4));
        assert_eq!((first(&e), last(&e)), (Some(0), Some(4)));
    }

    #[test]
    fn a_letter_chooses_the_row_it_underlines() {
        let e = sample();
        assert_eq!(by_mnemonic(&e, 'n', None), Some(0));
        assert_eq!(by_mnemonic(&e, 'R', None), Some(3));
        assert_eq!(by_mnemonic(&e, 't', None), Some(4));
        assert_eq!(by_mnemonic(&e, 'o', None), None);
        assert_eq!(by_mnemonic(&e, 'z', None), None);
    }

    #[test]
    fn a_popup_is_kept_on_screen() {
        let area = Rect::new(0, 0, 1000, 800);
        assert_eq!(place((100, 100), (200, 300), area, false), (100, 100));
        assert_eq!(place((900, 100), (200, 300), area, false), (800, 100));
        assert_eq!(place((100, 700), (200, 300), area, true), (100, 400));
        assert_eq!(place((100, 700), (200, 300), area, false), (100, 500));
    }

    #[test]
    fn a_submenu_opens_beside_its_parent_or_on_the_other_side() {
        let area = Rect::new(0, 0, 1000, 800);
        let parent = Rect::new(100, 100, 300, 400);
        assert_eq!(place_sub(parent, 150, (200, 100), area), (298, 150));
        let near_edge = Rect::new(700, 100, 950, 400);
        assert_eq!(place_sub(near_edge, 150, (200, 100), area), (502, 150));
        assert_eq!(place_sub(parent, 780, (200, 100), area).1, 700);
    }
}
