//! Drawing the popup menu from the theme tokens.

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::Panel;

use super::model::{Entry, Item, Layout, Metrics};
use crate::geom::{Rect, Rgb, mix};
use crate::paint::{fill, text_at, text_w, wash};
use crate::theme;

/// Sizes for the current display.
pub fn metrics() -> Metrics {
    let t = theme::current();
    Metrics { row: t.px(26), sep: t.px(9), gutter: t.px(32), pad: t.px(16), gap: t.px(32), arrow: t.px(20), border: 1 }
}

pub fn paint(panel: Panel, entries: &[Entry], lay: &Layout, hot: Option<usize>) {
    let dc = AutoBufferedPaintDC::new(&panel);
    let t = theme::current();
    let c = t.colors;
    if let Some(f) = t.ui_font() {
        dc.set_font(&f);
    }
    let m = metrics();
    fill(&dc, Rect::new(0, 0, lay.width, lay.height), c.border);
    fill(&dc, Rect::new(m.border, m.border, lay.width - m.border, lay.height - m.border), c.bg_elevated);
    for (k, (entry, r)) in entries.iter().zip(&lay.rects).enumerate() {
        if matches!(entry, Entry::Sep) {
            let mid = i32::midpoint(r.t, r.b);
            fill(&dc, Rect::new(r.l + m.pad / 2, mid, r.r - m.pad / 2, mid + 1), c.border);
            continue;
        }
        if hot == Some(k) {
            wash(&dc, *r, mix(c.bg_elevated, c.ink, 0x26), 0.0);
        }
        paint_row(&dc, entry, *r, lay, &m, if entry.selectable() { c.text } else { c.disabled });
    }
}

fn paint_row(dc: &impl DeviceContext, entry: &Entry, r: Rect, lay: &Layout, m: &Metrics, ink: Rgb) {
    let (label, key) = entry.display();
    text_at(dc, &label, Rect::new(r.l + m.gutter, r.t, r.r, r.b), ink, false);
    if let Some(k) = key {
        let before: String = label.chars().take(k).collect();
        let letter: String = label.chars().skip(k).take(1).collect();
        let x = r.l + m.gutter + text_w(dc, &before);
        let y = r.b - r.h() / 4 - 1;
        fill(dc, Rect::new(x, y, x + text_w(dc, &letter), y + 1), ink);
    }
    if matches!(entry, Entry::Item(Item { check: Some(true), .. })) {
        text_at(dc, "\u{2713}", Rect::new(r.l, r.t, r.l + m.gutter, r.b), ink, true);
    }
    if !entry.accel().is_empty() {
        text_at(dc, entry.accel(), Rect::new(lay.accel_x, r.t, r.r, r.b), ink, false);
    }
    if matches!(entry, Entry::Sub { .. }) {
        text_at(dc, "\u{203A}", Rect::new(r.r - m.arrow, r.t, r.r, r.b), ink, true);
    }
}
