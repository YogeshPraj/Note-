//! Painting the tab strip: tabs, their close buttons and the scroll buttons.

use super::*;

/// `v` pixels at 100%, on this display.
fn px(v: i32) -> i32 {
    crate::theme::current().px(v)
}

pub(super) fn close_rect(r: &Rect) -> Rect {
    Rect::new(r.r - px(24), r.t + px(7), r.r - px(8), r.b - px(7))
}

/// A badge fits its text.
fn badge_width(dc: &impl DeviceContext, badge: &str) -> i32 {
    (text_w(dc, badge) + px(8)).max(px(24))
}

pub(super) fn tab_width(dc: &impl DeviceContext, info: &Tab) -> i32 {
    let badge = info.badge.map_or(0, |b| badge_width(dc, b) + px(6));
    let extra = badge + if info.pinned { px(16) } else { 0 };
    (text_w(dc, &info.title) + px(56) + extra).clamp(px(96), px(260))
}

pub(super) fn paint(st: &Rc<RefCell<State>>, panel: Panel) {
    if st.borrow().vertical {
        return column::paint(st, panel);
    }
    let dc = AutoBufferedPaintDC::new(&panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let size = panel.get_client_size();
    let (w, h) = (size.width, size.height);
    fill(&dc, Rect::new(0, 0, w, h), lk.base());
    fill(&dc, Rect::new(0, h - 1, w, h), mix(lk.base(), lk.ink(), 0x30));

    let widths: Vec<i32> = s.tabs.iter().map(|t| tab_width(&dc, t)).collect();
    let active = s.active;
    let layout = s.scroll.layout(&widths, active, w, rows());
    s.layout = layout;

    dc.set_clipping_region(s.layout.clip.0, 0, (s.layout.clip.1 - s.layout.clip.0).max(0), h);
    for (i, info) in s.tabs.iter().enumerate() {
        let r = s.layout.tabs[i];
        if r.r < s.layout.clip.0 || r.l > s.layout.clip.1 {
            continue;
        }
        paint_tab(&dc, &s, i, info, r, h);
    }
    dc.destroy_clipping_region();

    for (rect, b) in [(s.layout.left, Btn::Left), (s.layout.right, Btn::Right)] {
        if let Some(rect) = rect {
            paint_button(&dc, rect, lk, s.btn_hot == Some(b), s.btn_down == Some(b), b);
        }
    }
    paint_button(&dc, s.layout.plus, lk, s.btn_hot == Some(Btn::Plus), s.btn_down == Some(Btn::Plus), Btn::Plus);
    publish(&s);
}

/// Show the screen reader what was just painted.
pub(super) fn publish(s: &State) {
    let items = s.tabs.iter().zip(&s.layout.tabs).enumerate().map(|(i, (tab, r))| {
        let mut one = a11y::Item::new(&if tab.dirty { format!("{} (modified)", tab.title) } else { tab.title.clone() }, a11y::Role::Tab, *r);
        one.selected = i == s.active;
        one
    });
    s.reader.publish(items.collect());
}

pub(super) fn paint_tab(dc: &impl DeviceContext, s: &State, i: usize, info: &Tab, r: Rect, h: i32) {
    let lk = s.look;
    let active = i == s.active;
    let hot = s.hot.map(|x| x.0) == Some(i);
    let fg = if active { lk.editor_fg } else { lk.dim() };
    if active {
        fill(dc, Rect::new(r.l, r.t, r.r, h), lk.editor_bg);
        fill(dc, Rect::new(r.l, r.t, r.r, r.t + px(2)), lk.colors.accent);
    } else if hot {
        fill(dc, Rect::new(r.l, r.t, r.r, r.b - 1), mix(lk.base(), lk.ink(), 0x18));
    }
    if !active && !hot && s.active != i + 1 && s.hot.map(|x| x.0) != Some(i + 1) {
        fill(dc, Rect::new(r.r, r.t + px(8), r.r + 1, r.b - px(8)), mix(lk.base(), lk.ink(), 0x30));
    }
    if let Some(c) = info.color {
        fill(dc, Rect::new(r.l, r.t, r.r, r.t + px(3)), c);
    }
    let mut tx = r.l + px(10);
    let mid = i32::midpoint(r.t, r.b);
    if info.pinned {
        dc.set_pen(colour(fg), 1, PenStyle::Solid);
        dc.set_brush(colour(fg), BrushStyle::Solid);
        dc.draw_ellipse(tx, mid - px(7), px(8), px(8));
        dc.draw_line(tx + px(4), mid + px(1), tx + px(4), mid + px(7));
        tx += px(16);
    }
    if info.dirty {
        dc.set_pen(colour(lk.colors.danger), 1, PenStyle::Solid);
        dc.set_brush(colour(lk.colors.danger), BrushStyle::Solid);
        dc.draw_ellipse(tx, mid - px(4), px(8), px(8));
        tx += px(14);
    }
    if let Some(badge) = info.badge {
        dc.set_pen(colour(lk.colors.badge), 1, PenStyle::Solid);
        dc.set_brush(colour(lk.colors.badge), BrushStyle::Solid);
        let w = badge_width(dc, badge);
        dc.draw_rounded_rectangle(tx, mid - px(8), w, px(16), f64::from(px(6)));
        text_at(dc, badge, Rect::new(tx, mid - px(8), tx + w, mid + px(8)), lk.colors.accent_text, true);
        tx += w + px(6);
    }
    // Long names end in an ellipsis rather than running under the close button.
    let name = ellipsize(dc, &info.title, (r.r - px(26) - tx).max(0));
    s.cut.borrow_mut().insert(i, name != info.title);
    text_at(dc, &name, Rect::new(tx, r.t, r.r - px(26), r.b), fg, false);
    if info.beside {
        fill(dc, Rect::new(r.l + px(6), r.b - px(3), r.r - px(6), r.b - px(1)), mix(lk.base(), lk.colors.accent, 0x90));
    }
    if (active || hot) && !info.pinned {
        let c = close_rect(&r);
        if s.hot == Some((i, true)) {
            let wash = if active { lk.colors.bg_active } else { mix(lk.base(), lk.ink(), 0x28) };
            fill(dc, c, wash);
        }
        dc.set_pen(colour(fg), 1, PenStyle::Solid);
        let (cx, cy) = (i32::midpoint(c.l, c.r), i32::midpoint(c.t, c.b));
        let (a, b) = (px(3), px(4));
        dc.draw_line(cx - a, cy - a, cx + b, cy + b);
        dc.draw_line(cx + a, cy - a, cx - b, cy + b);
    }
}

pub(super) fn dp(x: i32, y: i32) -> wxdragon::dc::Point {
    wxdragon::dc::Point::new(x, y)
}

pub(super) fn paint_button(dc: &impl DeviceContext, r: Rect, lk: Look, hot: bool, down: bool, glyph: Btn) {
    if hot || down {
        wash(dc, r, mix(lk.base(), lk.ink(), if down { 0x40 } else { 0x24 }), f64::from(px(4)));
    }
    let c = lk.fg();
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    let (cx, cy) = (i32::midpoint(r.l, r.r), i32::midpoint(r.t, r.b));
    let fill_mode = wxdragon::dc::PolygonFillMode::OddEven;
    let (a, b, e) = (px(3), px(5), px(6));
    match glyph {
        Btn::Left => dc.draw_polygon(&[dp(cx + a, cy - b), dp(cx - a, cy), dp(cx + a, cy + b)], 0, 0, fill_mode),
        Btn::Right => dc.draw_polygon(&[dp(cx - a, cy - b), dp(cx + a, cy), dp(cx - a, cy + b)], 0, 0, fill_mode),
        Btn::Plus => {
            dc.draw_line(cx - b, cy, cx + e, cy);
            dc.draw_line(cx, cy - b, cx, cy + e);
        }
    }
}
