//! Painting the tab strip: tabs, their close buttons and the scroll buttons.

use super::*;

pub(super) fn close_rect(r: &Rect) -> Rect {
    Rect::new(r.r - 24, r.t + 7, r.r - 8, r.b - 7)
}

pub(super) fn tab_width(dc: &impl DeviceContext, info: &Tab) -> i32 {
    let extra = if info.badge.is_some() { 30 } else { 0 } + if info.pinned { 16 } else { 0 };
    (text_w(dc, &info.title) + 56 + extra).clamp(96, 260)
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
    let layout = s.scroll.layout(&widths, active, w, ROWS);
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
}

pub(super) fn paint_tab(dc: &impl DeviceContext, s: &State, i: usize, info: &Tab, r: Rect, h: i32) {
    let lk = s.look;
    let active = i == s.active;
    let hot = s.hot.map(|x| x.0) == Some(i);
    let fg = if active { lk.editor_fg } else { lk.dim() };
    if active {
        fill(dc, Rect::new(r.l, r.t, r.r, h), lk.editor_bg);
        fill(dc, Rect::new(r.l, r.t, r.r, r.t + 2), lk.colors.accent);
    } else if hot {
        fill(dc, Rect::new(r.l, r.t, r.r, r.b - 1), mix(lk.base(), lk.ink(), 0x18));
    }
    if !active && !hot && s.active != i + 1 && s.hot.map(|x| x.0) != Some(i + 1) {
        fill(dc, Rect::new(r.r, r.t + 8, r.r + 1, r.b - 8), mix(lk.base(), lk.ink(), 0x30));
    }
    if let Some(c) = info.color {
        fill(dc, Rect::new(r.l, r.t, r.r, r.t + 3), c);
    }
    let mut tx = r.l + 10;
    if info.pinned {
        dc.set_pen(colour(fg), 1, PenStyle::Solid);
        dc.set_brush(colour(fg), BrushStyle::Solid);
        dc.draw_ellipse(tx, r.t + 9, 8, 8);
        dc.draw_line(tx + 4, r.t + 17, tx + 4, r.t + 23);
        tx += 16;
    }
    if info.dirty {
        dc.set_pen(colour(lk.colors.danger), 1, PenStyle::Solid);
        dc.set_brush(colour(lk.colors.danger), BrushStyle::Solid);
        dc.draw_ellipse(tx, r.t + 12, 8, 8);
        tx += 14;
    }
    if let Some(badge) = info.badge {
        dc.set_pen(colour(lk.colors.badge), 1, PenStyle::Solid);
        dc.set_brush(colour(lk.colors.badge), BrushStyle::Solid);
        dc.draw_rounded_rectangle(tx, r.t + 8, 24, 16, 6.0);
        text_at(dc, badge, Rect::new(tx, r.t + 8, tx + 24, r.t + 24), lk.colors.accent_text, true);
        tx += 30;
    }
    // Long names end in an ellipsis rather than running under the close button.
    let room = (r.r - 26 - tx).max(0);
    let mut name = info.title.clone();
    if text_w(dc, &name) > room {
        while name.chars().count() > 1 && text_w(dc, &format!("{name}\u{2026}")) > room {
            name.pop();
        }
        name.push('\u{2026}');
    }
    text_at(dc, &name, Rect::new(tx, r.t, r.r - 26, r.b), fg, false);
    if (active || hot) && !info.pinned {
        let c = close_rect(&r);
        if s.hot == Some((i, true)) {
            let wash = if active { lk.colors.bg_active } else { mix(lk.base(), lk.ink(), 0x28) };
            fill(dc, c, wash);
        }
        dc.set_pen(colour(fg), 1, PenStyle::Solid);
        let (cx, cy) = (i32::midpoint(c.l, c.r), i32::midpoint(c.t, c.b));
        dc.draw_line(cx - 3, cy - 3, cx + 4, cy + 4);
        dc.draw_line(cx + 3, cy - 3, cx - 4, cy + 4);
    }
}

pub(super) fn dp(x: i32, y: i32) -> wxdragon::dc::Point {
    wxdragon::dc::Point::new(x, y)
}

pub(super) fn paint_button(dc: &impl DeviceContext, r: Rect, lk: Look, hot: bool, down: bool, glyph: Btn) {
    if hot || down {
        wash(dc, r, mix(lk.base(), lk.ink(), if down { 0x40 } else { 0x24 }), 4.0);
    }
    let c = lk.fg();
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    let (cx, cy) = (i32::midpoint(r.l, r.r), i32::midpoint(r.t, r.b));
    let fill_mode = wxdragon::dc::PolygonFillMode::OddEven;
    match glyph {
        Btn::Left => dc.draw_polygon(&[dp(cx + 3, cy - 5), dp(cx - 3, cy), dp(cx + 3, cy + 5)], 0, 0, fill_mode),
        Btn::Right => dc.draw_polygon(&[dp(cx - 3, cy - 5), dp(cx + 3, cy), dp(cx - 3, cy + 5)], 0, 0, fill_mode),
        Btn::Plus => {
            dc.draw_line(cx - 5, cy, cx + 6, cy);
            dc.draw_line(cx, cy - 5, cx, cy + 6);
        }
    }
}
