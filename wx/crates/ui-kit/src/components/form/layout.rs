//! Checking and building dialog layouts, with no toolkit in it.
//!
//! The dialogs are laid out at fixed pixel positions (to match the Win32 build exactly), which makes a slip easy:
//! a control off the edge, two controls sharing an id, a control under the OK button. [`check`] finds those, and
//! [`Flow`] places rows of controls for new dialogs without hand-counting pixels.

use std::collections::HashSet;

/// A control as the checker sees it.
#[derive(Clone, Debug)]
pub struct Placed {
    /// 0 means "nobody asks for this control by id" (labels), so it may repeat.
    pub id: usize,
    /// x, y, width, height
    pub rect: (i32, i32, i32, i32),
}

#[derive(Debug, PartialEq, Eq)]
pub enum Problem {
    DuplicateId(usize),
    /// The control reaches outside the dialog.
    OutOfBounds(usize),
    /// Width or height is not positive.
    EmptySize(usize),
    /// The control sits on one of the OK / Cancel buttons.
    UnderButton(usize),
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Problem::DuplicateId(id) => write!(f, "control id {id} is used twice"),
            Problem::OutOfBounds(id) => write!(f, "control {id} reaches outside the dialog"),
            Problem::EmptySize(id) => write!(f, "control {id} has no size"),
            Problem::UnderButton(id) => write!(f, "control {id} overlaps the OK/Cancel buttons"),
        }
    }
}

fn overlaps(a: (i32, i32, i32, i32), b: (i32, i32, i32, i32)) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

/// Problems with `controls` in a dialog of `size`. `buttons` are the OK and Cancel rectangles, when they exist.
pub fn check(size: (i32, i32), controls: &[Placed], buttons: &[(i32, i32, i32, i32)]) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for c in controls {
        let (x, y, w, h) = c.rect;
        if c.id != 0 && !seen.insert(c.id) {
            out.push(Problem::DuplicateId(c.id));
        }
        if w <= 0 || h <= 0 {
            out.push(Problem::EmptySize(c.id));
            continue;
        }
        if x < 0 || y < 0 || x + w > size.0 || y + h > size.1 {
            out.push(Problem::OutOfBounds(c.id));
        }
        if buttons.iter().any(|b| overlaps(c.rect, *b)) {
            out.push(Problem::UnderButton(c.id));
        }
    }
    out
}

/// Places controls in rows down a column: each call takes the next row and moves down.
#[derive(Clone, Copy, Debug)]
pub struct Flow {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    /// Space between rows.
    pub gap: i32,
}

impl Flow {
    pub fn new(x: i32, y: i32, w: i32) -> Flow {
        Flow { x, y, w, gap: 8 }
    }

    /// The next row of height `h`, as (x, y, width, height).
    pub fn row(&mut self, h: i32) -> (i32, i32, i32, i32) {
        let r = (self.x, self.y, self.w, h);
        self.y += h + self.gap;
        r
    }

    /// The next row split into a label `label_w` wide and a field filling the rest: (label, field).
    pub fn labelled(&mut self, label_w: i32, h: i32) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
        let (x, y, w, _) = self.row(h);
        // A label's text sits a little lower than a field's top edge.
        ((x, y + 4, label_w, h - 6), (x + label_w, y, (w - label_w).max(0), h))
    }

    /// Where the flow has got to: the bottom of the last row.
    pub fn bottom(&self) -> i32 {
        self.y - self.gap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(id: usize, rect: (i32, i32, i32, i32)) -> Placed {
        Placed { id, rect }
    }

    #[test]
    fn a_clean_layout_has_no_problems() {
        let c = [p(10, (10, 10, 100, 20)), p(11, (10, 40, 100, 20))];
        assert!(check((300, 200), &c, &[(200, 160, 80, 26)]).is_empty());
    }

    #[test]
    fn duplicate_ids_are_found_but_zero_may_repeat() {
        let c = [p(5, (0, 0, 10, 10)), p(5, (20, 0, 10, 10)), p(0, (40, 0, 10, 10)), p(0, (60, 0, 10, 10))];
        assert_eq!(check((300, 200), &c, &[]), vec![Problem::DuplicateId(5)]);
    }

    #[test]
    fn controls_off_the_edge_are_found() {
        let c = [p(1, (250, 10, 100, 20)), p(2, (-5, 10, 20, 20)), p(3, (10, 190, 20, 20))];
        let r = check((300, 200), &c, &[]);
        assert_eq!(r, vec![Problem::OutOfBounds(1), Problem::OutOfBounds(2), Problem::OutOfBounds(3)]);
    }

    #[test]
    fn a_control_that_exactly_fits_is_fine() {
        assert!(check((300, 200), &[p(1, (0, 0, 300, 200))], &[]).is_empty());
    }

    #[test]
    fn empty_sizes_are_found() {
        assert_eq!(check((300, 200), &[p(1, (10, 10, 0, 20))], &[]), vec![Problem::EmptySize(1)]);
    }

    #[test]
    fn a_control_under_a_button_is_found() {
        let c = [p(1, (190, 150, 60, 30))];
        assert_eq!(check((300, 200), &c, &[(200, 160, 80, 26)]), vec![Problem::UnderButton(1)]);
    }

    #[test]
    fn touching_a_button_is_not_overlapping() {
        let c = [p(1, (200, 130, 80, 30))];
        assert!(check((300, 200), &c, &[(200, 160, 80, 26)]).is_empty());
    }

    #[test]
    fn flow_rows_stack_with_the_gap() {
        let mut f = Flow::new(10, 20, 200);
        assert_eq!(f.row(24), (10, 20, 200, 24));
        assert_eq!(f.row(24), (10, 52, 200, 24));
        assert_eq!(f.bottom(), 76);
    }

    #[test]
    fn a_labelled_row_splits_label_and_field() {
        let mut f = Flow::new(10, 10, 300);
        let (label, field) = f.labelled(90, 24);
        assert_eq!(label.0, 10);
        assert_eq!(field.0, 100);
        assert_eq!(field.2, 210);
        assert_eq!(field.3, 24);
        assert!(label.1 > field.1, "the label text sits slightly lower than the field");
    }

    #[test]
    fn flow_output_passes_the_checker() {
        let mut f = Flow::new(12, 12, 276);
        let mut placed = Vec::new();
        for id in 10..14 {
            let (_, field) = f.labelled(90, 24);
            placed.push(p(id, field));
        }
        assert!(check((300, f.bottom() + 12 + 40), &placed, &[(210, f.bottom() + 20, 80, 26)]).is_empty());
    }
}
