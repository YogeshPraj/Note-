//! The tab strip as a vertical list (a settings window's category navigation): one row per tab, full width.

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::{State, Tab};
use crate::geom::{Rect, mix};
use crate::paint::{fill, outline, text_at, wash};
use crate::theme;

/// Indent per nesting level, before scaling.
const INDENT: i32 = 14;
/// Thickness of the focus ring, before scaling.
const FOCUS_RING: i32 = 2;

fn row_height() -> i32 {
    theme::current().px(30)
}

pub fn paint(st: &Rc<RefCell<State>>, panel: Panel) {
    let dc = AutoBufferedPaintDC::new(&panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let size = panel.get_client_size();
    fill(&dc, Rect::new(0, 0, size.width, size.height), lk.colors.surface);
    fill(&dc, Rect::new(size.width - 1, 0, size.width, size.height), mix(lk.base(), lk.ink(), 0x30));
    s.layout = super::model::column(s.tabs.len(), size.width - 1, row_height(), theme::current().px(4));
    for (i, (info, r)) in s.tabs.iter().zip(&s.layout.tabs).enumerate() {
        paint_row(&dc, &s, i, info, *r);
    }
    // The keyboard is here: a ring round the active row shows it.
    if s.focused
        && let Some(r) = s.layout.tabs.get(s.active)
    {
        let t = theme::current();
        for i in 0..t.px(FOCUS_RING) {
            outline(&dc, Rect::new(r.l + i, r.t + i, r.r - i, r.b - i), lk.colors.accent);
        }
    }
    super::paint::publish(&s);
}

/// The row a key moves to from `active` in a list of `count`; None for other keys or at the ends.
fn next_row(key: i32, active: usize, count: usize) -> Option<usize> {
    const KEY_END: i32 = 312;
    const KEY_HOME: i32 = 313;
    const KEY_UP: i32 = 315;
    const KEY_DOWN: i32 = 317;
    let last = count.checked_sub(1)?;
    let to = match key {
        KEY_UP => active.checked_sub(1)?,
        KEY_DOWN => (active + 1).min(last),
        KEY_HOME => 0,
        KEY_END => last,
        _ => return None,
    };
    (to != active).then_some(to)
}

/// The list takes the keyboard: Up, Down, Home and End choose a row, and a ring shows it has the focus.
pub(super) fn wire_keys(st: &Rc<RefCell<State>>, panel: Panel) {
    panel.set_can_focus(true);
    for focused in [true, false] {
        let st = st.clone();
        let changed = move |e: WindowEventData| {
            st.borrow_mut().focused = focused;
            panel.refresh(false, None);
            e.skip(true);
        };
        if focused {
            panel.on_set_focus(changed);
        } else {
            panel.on_kill_focus(changed);
        }
    }
    panel.on_mouse_left_down(move |e| {
        panel.set_focus();
        e.skip(true);
    });
    // The character hook as well as key down: as a plain panel in a dialog, the list can lose arrow keys to dialog
    // navigation before they arrive as key down.
    let hooked = st.clone();
    panel.on_char_hook(move |e| on_key(&hooked, &e));
    let st = st.clone();
    panel.on_key_down(move |e| on_key(&st, &e));
}

fn on_key(st: &Rc<RefCell<State>>, e: &WindowEventData) {
    let key = if let WindowEventData::Keyboard(k) = e { k.get_key_code().unwrap_or(0) } else { 0 };
    let to = {
        let s = st.borrow();
        next_row(key, s.active, s.tabs.len())
    };
    match to {
        Some(row) => super::TabStrip::emit(st, super::TabEvent::Select(row)),
        None => e.skip(true),
    }
}

fn paint_row(dc: &impl DeviceContext, s: &State, i: usize, info: &Tab, r: Rect) {
    let lk = s.look;
    let active = i == s.active;
    let hot = s.hot.map(|h| h.0) == Some(i);
    if active {
        fill(dc, r, lk.colors.bg_active);
        let (gap, bar) = (theme::current().px(4), theme::current().px(3));
        fill(dc, Rect::new(r.l, r.t + gap, r.l + bar, r.b - gap), lk.colors.accent);
    } else if hot {
        wash(dc, r, mix(lk.colors.surface, lk.ink(), 0x18), 0.0);
    }
    let x = r.l + theme::current().px(16 + INDENT * i32::from(info.level));
    text_at(dc, &info.title, Rect::new(x, r.t, r.r, r.b), if active { lk.fg() } else { lk.dim() }, false);
}

#[cfg(test)]
mod tests {
    use super::next_row;

    #[test]
    fn arrows_move_one_row_and_stop_at_the_ends() {
        assert_eq!(next_row(317, 0, 3), Some(1));
        assert_eq!(next_row(317, 2, 3), None);
        assert_eq!(next_row(315, 1, 3), Some(0));
        assert_eq!(next_row(315, 0, 3), None);
    }

    #[test]
    fn home_and_end_go_to_the_ends_and_other_keys_do_nothing() {
        assert_eq!(next_row(313, 2, 3), Some(0));
        assert_eq!(next_row(312, 0, 3), Some(2));
        assert_eq!(next_row(65, 0, 3), None);
        assert_eq!(next_row(317, 0, 0), None);
    }
}
