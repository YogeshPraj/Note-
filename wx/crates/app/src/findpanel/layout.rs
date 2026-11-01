//! Where every control of the Find panel goes, for each mode and width. No toolkit in it, so it is unit tested.
//!
//! The panel is a small form: a row of mode tabs, then up to three rows of label, text box and buttons, a
//! block of option check boxes, and an optional results list underneath.

use crate::widgets::geom::Rect;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Find = 0,
    Replace = 1,
    InFiles = 2,
    Mark = 3,
}

/// Height of the results list including its margin.
pub const RESULTS_H: i32 = 180;

/// A control of the panel.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Ctl {
    /// Mode tab 0..=3.
    Tab(usize),
    Closex,
    Status,
    FindLbl,
    Find,
    ReplLbl,
    Repl,
    DirLbl,
    Dir,
    FiltLbl,
    Filt,
    Browse,
    Case,
    Word,
    Wrap,
    Regex,
    Dot,
    Ext,
    InSel,
    Next,
    Prev,
    FindAll,
    Count,
    Replace,
    ReplAll,
    Fif,
    Mark,
    Clear,
    Close,
    List,
}

/// The controls to show and where; everything else is hidden.
#[derive(Debug)]
pub struct Layout {
    pub placed: Vec<(Ctl, Rect)>,
    /// Height of the whole panel.
    #[cfg(test)]
    pub height: i32,
}

impl Layout {
    #[cfg(test)]
    pub fn rect(&self, c: Ctl) -> Option<Rect> {
        self.placed.iter().find(|(k, _)| *k == c).map(|(_, r)| *r)
    }
}

const TABS_H: i32 = 26;
/// Rows of the form: first, second and third line.
const R1: i32 = 40;
const R2: i32 = 74;
const R3: i32 = 108;
const BTN_H: i32 = 26;
/// Left edge of the first button after the text box.
const BTN_X: i32 = 368;

/// Height of the panel in `mode`, with the results list when `results` is shown.
pub fn height(mode: Mode, results: bool) -> i32 {
    let form = match mode {
        Mode::Find | Mode::Mark => 108,
        Mode::Replace => 142,
        Mode::InFiles => 176,
    };
    TABS_H + form + if results { RESULTS_H } else { 0 }
}

pub fn layout(mode: Mode, width: i32, results: bool) -> Layout {
    let mut v: Vec<(Ctl, Rect)> = Vec::new();
    let at = |x: i32, y: i32, w: i32, h: i32| Rect::new(x, y, x + w, y + h);

    for i in 0..4 {
        v.push((Ctl::Tab(i), at(8 + crate::platform::wx::to_i32(i) * 86, 6, 84, 28)));
    }
    v.push((Ctl::Status, at(356, 11, (width - 356 - 44).max(0), 18)));
    v.push((Ctl::Closex, at(width - 30, 7, 24, 24)));

    let label = |v: &mut Vec<(Ctl, Rect)>, c: Ctl, y: i32| v.push((c, at(12, y + 4, 90, 20)));
    let edit = |v: &mut Vec<(Ctl, Rect)>, c: Ctl, y: i32| v.push((c, at(108, y, 250, 24)));
    // A row of buttons from `BTN_X`, 6px apart.
    let buttons = |v: &mut Vec<(Ctl, Rect)>, y: i32, row: &[(Ctl, i32)]| {
        let mut x = BTN_X;
        for (c, w) in row {
            v.push((*c, at(x, y, *w, BTN_H)));
            x += w + 6;
        }
    };

    label(&mut v, Ctl::FindLbl, R1);
    edit(&mut v, Ctl::Find, R1);
    let mut checks_y = R2;
    match mode {
        Mode::Find => buttons(&mut v, R1 - 1, &[(Ctl::Next, 84), (Ctl::Prev, 96), (Ctl::FindAll, 200), (Ctl::Count, 64), (Ctl::Close, 64)]),
        Mode::Replace => {
            buttons(&mut v, R1 - 1, &[(Ctl::Next, 84), (Ctl::Prev, 96), (Ctl::Count, 64), (Ctl::FindAll, 200)]);
            label(&mut v, Ctl::ReplLbl, R2);
            edit(&mut v, Ctl::Repl, R2);
            buttons(&mut v, R2 - 1, &[(Ctl::Replace, 84), (Ctl::ReplAll, 96), (Ctl::Close, 64)]);
            checks_y = R3;
        }
        Mode::InFiles => {
            buttons(&mut v, R1 - 1, &[(Ctl::Fif, 84), (Ctl::Close, 64)]);
            label(&mut v, Ctl::DirLbl, R2);
            edit(&mut v, Ctl::Dir, R2);
            buttons(&mut v, R2 - 1, &[(Ctl::Browse, 34)]);
            label(&mut v, Ctl::FiltLbl, R3);
            edit(&mut v, Ctl::Filt, R3);
            checks_y = R3 + 34;
        }
        Mode::Mark => buttons(&mut v, R1 - 1, &[(Ctl::Mark, 84), (Ctl::Clear, 120), (Ctl::Close, 64)]),
    }

    let mut x = 108;
    for (c, w) in [(Ctl::Case, 100), (Ctl::Word, 172), (Ctl::Wrap, 110), (Ctl::Regex, 138), (Ctl::Dot, 140)] {
        v.push((c, at(x, checks_y, w, 22)));
        x += w + 6;
    }
    v.push((Ctl::Ext, at(108, checks_y + 26, 230, 22)));
    if mode != Mode::InFiles {
        v.push((Ctl::InSel, at(344, checks_y + 26, 130, 22)));
    }

    let height = height(mode, results);
    if results {
        v.push((Ctl::List, at(8, height - RESULTS_H, (width - 16).max(0), RESULTS_H - 8)));
    }
    Layout {
        placed: v,
        #[cfg(test)]
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Mode; 4] = [Mode::Find, Mode::Replace, Mode::InFiles, Mode::Mark];

    #[test]
    fn heights_match_the_win32_panel() {
        assert_eq!(height(Mode::Find, false), 134);
        assert_eq!(height(Mode::Mark, false), 134);
        assert_eq!(height(Mode::Replace, false), 168);
        assert_eq!(height(Mode::InFiles, false), 202);
        assert_eq!(height(Mode::Find, true), 134 + RESULTS_H);
    }

    #[test]
    fn no_control_is_placed_twice() {
        for m in ALL {
            let l = layout(m, 900, true);
            let mut seen = std::collections::HashSet::new();
            assert!(l.placed.iter().all(|(c, _)| seen.insert(*c)), "{m:?}");
        }
    }

    #[test]
    fn every_mode_has_the_tabs_a_find_box_and_a_close_button() {
        for m in ALL {
            let l = layout(m, 900, false);
            for i in 0..4 {
                assert!(l.rect(Ctl::Tab(i)).is_some());
            }
            assert!(l.rect(Ctl::Find).is_some());
            assert!(l.rect(Ctl::Close).is_some());
            assert!(l.rect(Ctl::Closex).is_some());
        }
    }

    #[test]
    fn each_mode_shows_only_its_own_controls() {
        let find = layout(Mode::Find, 900, false);
        assert!(find.rect(Ctl::Repl).is_none() && find.rect(Ctl::Dir).is_none() && find.rect(Ctl::Mark).is_none());
        let rep = layout(Mode::Replace, 900, false);
        assert!(rep.rect(Ctl::Repl).is_some() && rep.rect(Ctl::ReplAll).is_some() && rep.rect(Ctl::Fif).is_none());
        let files = layout(Mode::InFiles, 900, false);
        assert!(files.rect(Ctl::Dir).is_some() && files.rect(Ctl::Filt).is_some() && files.rect(Ctl::Browse).is_some());
        assert!(files.rect(Ctl::InSel).is_none(), "in-selection makes no sense across files");
        let mark = layout(Mode::Mark, 900, false);
        assert!(mark.rect(Ctl::Mark).is_some() && mark.rect(Ctl::Clear).is_some() && mark.rect(Ctl::Next).is_none());
    }

    #[test]
    fn buttons_follow_the_text_box_left_to_right_with_gaps() {
        let l = layout(Mode::Find, 900, false);
        let (next, prev) = (l.rect(Ctl::Next).unwrap(), l.rect(Ctl::Prev).unwrap());
        assert_eq!(next.l, BTN_X);
        assert_eq!(prev.l, next.r + 6);
        assert!(l.rect(Ctl::Find).unwrap().r < next.l);
    }

    #[test]
    fn the_option_checks_move_down_as_rows_are_added() {
        let y = |m| layout(m, 900, false).rect(Ctl::Case).unwrap().t;
        assert!(y(Mode::Find) < y(Mode::Replace));
        assert!(y(Mode::Replace) < y(Mode::InFiles));
    }

    #[test]
    fn the_results_list_sits_below_the_form_and_fills_the_width() {
        let l = layout(Mode::Find, 900, true);
        let list = l.rect(Ctl::List).unwrap();
        assert_eq!(list.t, height(Mode::Find, false));
        assert_eq!(list.w(), 900 - 16);
        assert!(list.b <= l.height);
        assert!(layout(Mode::Find, 900, false).rect(Ctl::List).is_none());
    }

    #[test]
    fn the_close_cross_and_status_track_the_width() {
        let (a, b) = (layout(Mode::Find, 800, false), layout(Mode::Find, 1000, false));
        assert_eq!(b.rect(Ctl::Closex).unwrap().l - a.rect(Ctl::Closex).unwrap().l, 200);
        assert_eq!(b.rect(Ctl::Status).unwrap().w() - a.rect(Ctl::Status).unwrap().w(), 200);
    }

    #[test]
    fn a_very_narrow_panel_never_gives_negative_sizes() {
        for m in ALL {
            assert!(layout(m, 100, true).placed.iter().all(|(_, r)| r.w() >= 0 && r.h() >= 0));
        }
    }
}
