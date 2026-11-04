//! The tab strip as a vertical list (a settings window's category navigation): one row per tab, full width.

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::{State, Tab};
use crate::geom::{Rect, mix};
use crate::paint::{fill, text_at, wash};
use crate::theme;

/// Indent per nesting level, before scaling.
const INDENT: i32 = 14;

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
