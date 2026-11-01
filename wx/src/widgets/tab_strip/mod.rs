//! A scrollable, reorderable tab strip that paints itself.
//!
//! The stock wx notebook cannot show Note++'s tabs (dirty dot, badge, pin, colour bar, close button, "+"),
//! so this is a plain panel. It owns no application state: it is told the tabs and the active one, and reports
//! what the user did as a [`TabEvent`]. Layout and scrolling live in [`model`], which has no toolkit in it and
//! is unit tested.

pub mod model;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use notepp_core::workspace::TabInfo;
use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use self::model::{Btn, Layout, Rows, Scroll};
use super::geom::Rect;
use super::paint::{ACCENT, Look, Rgb, WHITE, colour, fill, mix, pos, text_at, text_w};

pub const HEIGHT: i32 = 32;

const ROWS: Rows = Rows { tab_top: 4, tab_bottom: HEIGHT, btn_top: 6, btn_bottom: HEIGHT - 2 };
const DIRTY: Rgb = (0xC0, 0x00, 0x00);
const BADGE: Rgb = (0x4A, 0x7A, 0xFF);
/// How often dragging a tab against the edge scrolls by one tab.
const AUTO_SCROLL: Duration = Duration::from_millis(120);

/// What the user did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabEvent {
    Select(usize),
    Close(usize),
    New,
    /// A tab was dragged from one position to another.
    Move(usize, usize),
    /// Right-click on a tab at (x, y) in the strip.
    Context(usize, i32, i32),
}

struct Drag {
    from: usize,
    x0: i32,
    moving: bool,
}

struct State {
    look: Look,
    font: Option<Font>,
    tabs: Vec<TabInfo>,
    active: usize,
    scroll: Scroll,
    layout: Layout,
    /// (tab, over its close button)
    hot: Option<(usize, bool)>,
    btn_hot: Option<Btn>,
    btn_down: Option<Btn>,
    drag: Option<Drag>,
    last_auto: Instant,
    handler: Option<Rc<dyn Fn(TabEvent)>>,
}

#[derive(Clone)]
pub struct TabStrip {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl TabStrip {
    pub fn new(parent: &Frame) -> TabStrip {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, HEIGHT));
        let st = Rc::new(RefCell::new(State {
            look: Look { dark: false, editor_bg: WHITE, editor_fg: (0, 0, 0) },
            font: None,
            tabs: Vec::new(),
            active: 0,
            scroll: Scroll::default(),
            layout: Layout::default(),
            hot: None,
            btn_hot: None,
            btn_down: None,
            drag: None,
            last_auto: Instant::now(),
            handler: None,
        }));
        let strip = TabStrip { panel, st };
        strip.wire();
        let (p, st) = (strip.panel, strip.st.clone());
        super::theme::subscribe(move |t| {
            st.borrow_mut().look = t.look();
            p.refresh(false, None);
        });
        strip
    }

    /// The panel to place in a sizer.
    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn on_event(&self, f: impl Fn(TabEvent) + 'static) {
        self.st.borrow_mut().handler = Some(Rc::new(f));
    }

    pub fn set_font(&self, font: Option<Font>) {
        self.st.borrow_mut().font = font;
    }

    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        {
            let mut s = self.st.borrow_mut();
            if s.active != active {
                s.scroll.reveal_active();
            }
            s.tabs = tabs;
            s.active = active;
        }
        self.panel.refresh(false, None);
    }

    fn emit(st: &Rc<RefCell<State>>, e: TabEvent) {
        let h = st.borrow().handler.clone();
        if let Some(h) = h {
            h(e);
        }
    }

    fn wire(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, &p));

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_motion(move |e| {
            let (x, y) = pos(&e);
            let mut moved = None;
            {
                let mut s = st.borrow_mut();
                if s.drag.is_some() {
                    moved = drag_to(&mut s, x);
                } else {
                    let hot = s.layout.tab_at(x, y).map(|i| (i, close_rect(&s.layout.tabs[i]).has(x, y) && !s.tabs[i].pinned));
                    s.hot = hot;
                    s.btn_hot = s.layout.button_at(x, y);
                }
            }
            if let Some((a, b)) = moved {
                Self::emit(&st, TabEvent::Move(a, b));
            }
            p.set_tooltip(match st.borrow().btn_hot {
                Some(Btn::Left) => "Scroll tabs left",
                Some(Btn::Right) => "Scroll tabs right",
                Some(Btn::Plus) => "New (Ctrl+N)",
                None => "",
            });
            p.refresh(false, None);
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_leave(move |_| {
            {
                let mut s = st.borrow_mut();
                s.hot = None;
                s.btn_hot = None;
            }
            p.refresh(false, None);
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            let event = {
                let mut s = st.borrow_mut();
                if let Some(b) = s.layout.button_at(x, y) {
                    s.btn_down = Some(b);
                    match b {
                        Btn::Plus => Some(TabEvent::New),
                        _ => {
                            s.scroll.step(b);
                            None
                        }
                    }
                } else if let Some(i) = s.layout.tab_at(x, y) {
                    let on_close = close_rect(&s.layout.tabs[i]).has(x, y) && !s.tabs[i].pinned && (i == s.active || s.hot.map(|h| h.0) == Some(i));
                    if on_close {
                        Some(TabEvent::Close(i))
                    } else {
                        s.drag = Some(Drag { from: i, x0: x, moving: false });
                        Some(TabEvent::Select(i))
                    }
                } else {
                    None
                }
            };
            if let Some(ev) = event {
                if matches!(ev, TabEvent::Select(_)) {
                    p.capture_mouse();
                }
                Self::emit(&st, ev);
            }
            p.refresh(false, None);
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_up(move |_| {
            let was_drag = {
                let mut s = st.borrow_mut();
                s.btn_down = None;
                s.drag.take().is_some()
            };
            if was_drag && p.has_capture() {
                p.release_mouse();
            }
            p.refresh(false, None);
        });

        let st = self.st.clone();
        self.panel.on_mouse_left_double_click(move |e| {
            let (x, y) = pos(&e);
            // On empty strip a double-click makes a tab; on a tab or button it does nothing extra.
            let empty = {
                let s = st.borrow();
                s.layout.tab_at(x, y).is_none() && s.layout.button_at(x, y).is_none()
            };
            if empty {
                Self::emit(&st, TabEvent::New);
            }
        });

        let st = self.st.clone();
        self.panel.on_mouse_middle_up(move |e| {
            let (x, y) = pos(&e);
            let hit = st.borrow().layout.tab_at(x, y);
            if let Some(i) = hit {
                Self::emit(&st, TabEvent::Close(i));
            }
        });

        let st = self.st.clone();
        self.panel.on_mouse_right_up(move |e| {
            let (x, y) = pos(&e);
            let hit = st.borrow().layout.tab_at(x, y);
            if let Some(i) = hit {
                Self::emit(&st, TabEvent::Context(i, x, y));
            }
        });
    }
}

/// Reorder while dragging; returns (from, to) when the tab moved.
fn drag_to(s: &mut State, x: i32) -> Option<(usize, usize)> {
    let d = s.drag.as_ref()?;
    let (from, x0, moving) = (d.from, d.x0, d.moving);
    if !moving && (x - x0).abs() <= 6 {
        return None;
    }
    // Against an edge, scroll one tab at a time.
    if s.last_auto.elapsed() >= AUTO_SCROLL {
        if s.layout.left.is_some() && x < s.layout.clip.0 + 16 {
            s.scroll.step(Btn::Left);
            s.last_auto = Instant::now();
        } else if s.layout.right.is_some() && x > s.layout.clip.1 - 16 {
            s.scroll.step(Btn::Right);
            s.last_auto = Instant::now();
        }
    }
    let last = s.tabs.len().saturating_sub(1);
    let hit = s.layout.tabs.iter().position(|r| x >= r.l && x < r.r);
    let to = hit.unwrap_or(if x < s.layout.tabs.first().map_or(0, |r| r.l) { 0 } else { last });
    let mut moved = None;
    if to != from && to < s.tabs.len() {
        let tab = s.tabs.remove(from);
        s.tabs.insert(to, tab);
        s.active = to;
        s.hot = Some((to, false));
        s.scroll.reveal_active();
        moved = Some((from, to));
    }
    s.drag = Some(Drag { from: to.min(last), x0, moving: true });
    moved
}

fn close_rect(r: &Rect) -> Rect {
    Rect::new(r.r - 24, r.t + 7, r.r - 8, r.b - 7)
}

fn tab_width(dc: &impl DeviceContext, info: &TabInfo) -> i32 {
    let extra = if info.badge.is_some() { 30 } else { 0 } + if info.pinned { 16 } else { 0 };
    (text_w(dc, &info.title) + 56 + extra).clamp(96, 260)
}

fn paint(st: &Rc<RefCell<State>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
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

fn paint_tab(dc: &impl DeviceContext, s: &State, i: usize, info: &TabInfo, r: Rect, h: i32) {
    let lk = s.look;
    let active = i == s.active;
    let hot = s.hot.map(|x| x.0) == Some(i);
    let fg = if active { lk.editor_fg } else { lk.dim() };
    if active {
        fill(dc, Rect::new(r.l, r.t, r.r, h), lk.editor_bg);
        fill(dc, Rect::new(r.l, r.t, r.r, r.t + 2), ACCENT);
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
        dc.set_pen(colour(DIRTY), 1, PenStyle::Solid);
        dc.set_brush(colour(DIRTY), BrushStyle::Solid);
        dc.draw_ellipse(tx, r.t + 12, 8, 8);
        tx += 14;
    }
    if let Some(badge) = info.badge {
        dc.set_pen(colour(BADGE), 1, PenStyle::Solid);
        dc.set_brush(colour(BADGE), BrushStyle::Solid);
        dc.draw_rounded_rectangle(tx, r.t + 8, 24, 16, 6.0);
        text_at(dc, badge, Rect::new(tx, r.t + 8, tx + 24, r.t + 24), WHITE, true);
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
            let wash = if active { if lk.dark { (0x3A, 0x3A, 0x3A) } else { (0xE0, 0xE0, 0xE0) } } else { mix(lk.base(), lk.ink(), 0x28) };
            fill(dc, c, wash);
        }
        dc.set_pen(colour(fg), 1, PenStyle::Solid);
        let (cx, cy) = ((c.l + c.r) / 2, (c.t + c.b) / 2);
        dc.draw_line(cx - 3, cy - 3, cx + 4, cy + 4);
        dc.draw_line(cx + 3, cy - 3, cx - 4, cy + 4);
    }
}

fn dp(x: i32, y: i32) -> wxdragon::dc::Point {
    wxdragon::dc::Point::new(x, y)
}

fn paint_button(dc: &impl DeviceContext, r: Rect, lk: Look, hot: bool, down: bool, glyph: Btn) {
    if hot || down {
        let wash = mix(lk.base(), lk.ink(), if down { 0x40 } else { 0x24 });
        dc.set_pen(colour(wash), 1, PenStyle::Solid);
        dc.set_brush(colour(wash), BrushStyle::Solid);
        dc.draw_rounded_rectangle(r.l, r.t, r.w(), r.h(), 4.0);
    }
    let c = lk.fg();
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    let (cx, cy) = ((r.l + r.r) / 2, (r.t + r.b) / 2);
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
