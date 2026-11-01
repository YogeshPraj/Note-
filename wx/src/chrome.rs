//! Custom-painted toolbar, tab strip and status bar, laid out and coloured like the Win32 shell's.
//!
//! The stock wx notebook, toolbar and status bar cannot be themed to match Note++, so these are
//! plain panels that paint themselves. They hold display state only; every user action goes to
//! the application as an [`Act`].

use std::cell::RefCell;
use std::rc::Rc;

use notepp_core::commands::*;
use notepp_core::commands::{ID_CLOSE, ID_SAVE};
use notepp_core::workspace::TabInfo;
use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::event::WindowEventData;
use wxdragon::prelude::*;

pub const MENU_H: i32 = 26;
pub const TOOL_H: i32 = 34;
pub const TAB_H: i32 = 32;
pub const STATUS_H: i32 = 24;

const ICON: i32 = 18;
const BTN: i32 = 28;
const ARROW_W: i32 = 24;

type Rgb = (u8, u8, u8);
const TAB_DIRTY: Rgb = (0xC0, 0x00, 0x00);
const ACCENT: Rgb = (0x00, 0x7A, 0xCC);
const STATUS_BG: Rgb = (0x00, 0x7A, 0xCC);
const STATUS_SEP: Rgb = (0x4D, 0xA3, 0xDA);
const BADGE: Rgb = (0x4A, 0x7A, 0xFF);
const WHITE: Rgb = (0xFF, 0xFF, 0xFF);

/// What the user did in the chrome.
#[derive(Clone, Copy, Debug)]
pub enum Act {
    Cmd(usize),
    Select(usize),
    Close(usize),
    New,
    Move(usize, usize),
    /// Right-click on tab i at (x, y) in the tab strip.
    Context(usize, i32, i32),
    /// A status segment was clicked; (x, y) is its top-left corner in the status bar.
    Status(isize, i32, i32),
    /// Open menu i; (x, y) is where its popup goes, in the menu bar's coordinates.
    Menu(usize, i32, i32),
}

pub const SEG_ENC: isize = 1;
pub const SEG_EOL: isize = 2;
pub const SEG_LANG: isize = 3;

#[derive(Default, Clone)]
pub struct StatusText {
    pub position: String,
    pub lines: String,
    pub length: String,
    pub enc: String,
    pub eol: String,
    pub lang: String,
}

#[derive(Clone, Copy, Default)]
struct R {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}

impl R {
    fn new(l: i32, t: i32, r: i32, b: i32) -> R {
        R { l, t, r, b }
    }
    fn has(&self, x: i32, y: i32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }
    fn w(&self) -> i32 {
        self.r - self.l
    }
    fn h(&self) -> i32 {
        self.b - self.t
    }
}

fn mix(base: Rgb, ink: Rgb, a: u8) -> Rgb {
    let m = |b: u8, i: u8| ((b as u32 * (255 - a as u32) + i as u32 * a as u32) / 255) as u8;
    (m(base.0, ink.0), m(base.1, ink.1), m(base.2, ink.2))
}

fn colour(c: Rgb) -> Colour {
    Colour::rgb(c.0, c.1, c.2)
}

#[derive(Clone, Copy)]
struct Look {
    dark: bool,
    editor_bg: Rgb,
    editor_fg: Rgb,
}

impl Look {
    fn fg(self) -> Rgb {
        if self.dark { (0xE6, 0xE6, 0xE6) } else { (0x1A, 0x1A, 0x1A) }
    }
    fn dim(self) -> Rgb {
        if self.dark { (0xB0, 0xB0, 0xB0) } else { (0x50, 0x50, 0x50) }
    }
    fn ink(self) -> Rgb {
        if self.dark { WHITE } else { (0, 0, 0) }
    }
    fn base(self) -> Rgb {
        if self.dark { (0x20, 0x20, 0x20) } else { (0xF3, 0xF3, 0xF3) }
    }
}

enum Tool {
    Sep,
    Btn { cmd: usize, tip: &'static str, icon: Option<Bitmap> },
}

struct Drag {
    from: usize,
    x0: i32,
    moving: bool,
}

/// The tab strip's own buttons: scroll left, scroll right and new tab.
#[derive(Clone, Copy, PartialEq)]
enum Btn {
    Left,
    Right,
    Plus,
}

struct St {
    look: Look,
    font: Option<Font>,
    handler: Option<Rc<dyn Fn(Act)>>,
    /// The update notice at the far right of the toolbar: text, colour, command and where it was drawn.
    pill: Option<(String, Rgb, usize, R)>,
    menu_items: Vec<String>,
    menu_rects: Vec<R>,
    menu_hot: Option<usize>,
    tools: Vec<(Tool, R)>,
    tool_hot: Option<usize>,
    tool_down: Option<usize>,
    checked: Vec<usize>,
    tabs: Vec<TabInfo>,
    active: usize,
    /// (index, hovering its close button)
    tab_hot: Option<(usize, bool)>,
    layout: Vec<R>,
    plus: R,
    arrows: Option<(R, R)>,
    btn_hot: Option<Btn>,
    btn_down: Option<Btn>,
    area: i32,
    offset: i32,
    follow: bool,
    drag: Option<Drag>,
    status: StatusText,
    status_hits: Vec<(R, isize)>,
}

pub struct Chrome {
    pub menu: Panel,
    pub tools: Panel,
    pub tabs: Panel,
    pub status: Panel,
    st: Rc<RefCell<St>>,
}

macro_rules! btn {
    ($cmd:expr, $file:literal, $tip:literal) => {
        (Some($cmd), include_bytes!(concat!("../../assets/", $file)) as &[u8], $tip)
    };
}

fn toolbar_items() -> Vec<(Option<usize>, &'static [u8], &'static str)> {
    let sep = (None, &[][..], "");
    vec![
        btn!(ID_NEW, "new.svg", "New (Ctrl+N)"),
        btn!(ID_NEW_WB, "whiteboard.svg", "New Whiteboard"),
        btn!(ID_OPEN, "open.svg", "Open (Ctrl+O)"),
        btn!(ID_SAVE, "save.svg", "Save (Ctrl+S)"),
        btn!(ID_SAVEALL, "saveall.svg", "Save All (Ctrl+Shift+S)"),
        btn!(ID_CLOSE, "close.svg", "Close (Ctrl+W)"),
        btn!(ID_PRINT, "print.svg", "Print"),
        btn!(ID_ENC_TOGGLE, "lock.svg", "Encrypt this file (or remove its encryption)"),
        btn!(ID_PW_TOOLBAR, "key.svg", "Passwords (Bitwarden)"),
        sep,
        btn!(ID_CUT, "cut.svg", "Cut (Ctrl+X)"),
        btn!(ID_COPY, "copy.svg", "Copy (Ctrl+C)"),
        btn!(ID_PASTE, "paste.svg", "Paste (Ctrl+V)"),
        sep,
        btn!(ID_UNDO, "undo.svg", "Undo (Ctrl+Z)"),
        btn!(ID_REDO, "redo.svg", "Redo (Ctrl+Y)"),
        sep,
        btn!(ID_FIND, "find.svg", "Find (Ctrl+F)"),
        btn!(ID_REPLACE, "replace.svg", "Replace (Ctrl+H)"),
        sep,
        btn!(ID_ZOOMIN, "zoomin.svg", "Zoom In"),
        btn!(ID_ZOOMOUT, "zoomout.svg", "Zoom Out"),
        sep,
        btn!(ID_WRAP, "wordwrap.svg", "Word Wrap"),
        btn!(ID_PREVIEW, "preview.svg", "Live Preview (Ctrl+Shift+V)"),
        btn!(ID_LOG_FOLLOW, "follow.svg", "Log Follow: stream a .log / .out / .err file as it grows"),
        btn!(ID_THEME, "darkmode.svg", "Dark Mode"),
    ]
}

fn emit(st: &Rc<RefCell<St>>, a: Act) {
    let h = st.borrow().handler.clone();
    if let Some(h) = h {
        h(a);
    }
}

fn pos(e: &WindowEventData) -> (i32, i32) {
    let p = match e {
        WindowEventData::MouseButton(m) => m.get_position(),
        WindowEventData::MouseMotion(m) => m.get_position(),
        WindowEventData::MouseEnter(m) => m.get_position(),
        WindowEventData::MouseLeave(m) => m.get_position(),
        _ => None,
    };
    p.map_or((-1, -1), |p| (p.x, p.y))
}

fn close_rect(r: &R) -> R {
    R::new(r.r - 24, r.t + 7, r.r - 8, r.b - 7)
}

fn text_at(dc: &impl DeviceContext, s: &str, r: R, c: Rgb, center: bool) {
    if s.is_empty() {
        return;
    }
    dc.set_text_foreground(colour(c));
    let (w, h) = dc.get_text_extent(s);
    let x = if center { r.l + (r.w() - w) / 2 } else { r.l };
    dc.draw_text(s, x, r.t + (r.h() - h) / 2);
}

fn text_w(dc: &impl DeviceContext, s: &str) -> i32 {
    if s.is_empty() { 0 } else { dc.get_text_extent(s).0 }
}

fn fill(dc: &impl DeviceContext, r: R, c: Rgb) {
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    dc.draw_rectangle(r.l, r.t, r.w(), r.h());
}

fn outline(dc: &impl DeviceContext, r: R, c: Rgb) {
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Transparent);
    dc.draw_rectangle(r.l, r.t, r.w(), r.h());
}

/// Where `s` would be laid out: tab rectangles, scroll offset and the "+" button.
fn layout_tabs(dc: &impl DeviceContext, width: i32, s: &mut St) {
    let mut x = 4;
    s.layout.clear();
    for info in &s.tabs {
        let extra = if info.badge.is_some() { 30 } else { 0 } + if info.pinned { 16 } else { 0 };
        let wd = (text_w(dc, &info.title) + 56 + extra).clamp(96, 260);
        s.layout.push(R::new(x, 4, x + wd, TAB_H));
        x += wd + 2;
    }
    let total = x;
    let overflow = total > width - 34;
    let avail = if overflow { width - 34 - 2 * ARROW_W } else { width - 34 };
    let pad = if overflow { ARROW_W + 2 } else { 0 };
    s.area = pad + avail + 4;
    if !overflow {
        s.offset = 0;
        s.arrows = None;
    } else {
        if s.follow {
            if let Some(a) = s.layout.get(s.active) {
                if a.r - s.offset > avail {
                    s.offset = a.r - avail;
                }
                if a.l - s.offset < 0 {
                    s.offset = a.l;
                }
            }
        }
        s.offset = s.offset.clamp(0, (total - avail).max(0));
        s.follow = false;
        s.arrows = Some((R::new(2, 6, 2 + ARROW_W, TAB_H - 2), R::new(width - ARROW_W - 2, 6, width - 2, TAB_H - 2)));
    }
    let shift = pad - s.offset;
    for r in &mut s.layout {
        r.l += shift;
        r.r += shift;
    }
    let plus_x = pad + (total - s.offset).min(avail) + 4;
    s.plus = R::new(plus_x, 6, plus_x + 26, TAB_H - 4);
}

fn paint_tools(st: &Rc<RefCell<St>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    let (w, h) = { let c = panel.get_client_size(); (c.width, c.height) };
    fill(&dc, R::new(0, 0, w, h), lk.base());
    if let Some(p) = s.pill.as_mut() {
        let wd = text_w(&dc, &p.0) + 26;
        p.3 = R::new(w - wd - 10, 6, w - 10, 3 + BTN - 3);
        fill(&dc, p.3, p.1);
        text_at(&dc, &p.0, p.3, WHITE, true);
    }
    for (i, (item, r)) in s.tools.iter().enumerate() {
        match item {
            Tool::Sep => fill(&dc, R::new(r.l + 3, r.t + 6, r.l + 4, r.b - 6), mix(lk.base(), lk.ink(), 0x40)),
            Tool::Btn { cmd, icon, .. } => {
                if s.tool_down == Some(i) {
                    fill(&dc, *r, mix(lk.base(), lk.ink(), 0x30));
                } else if s.tool_hot == Some(i) {
                    fill(&dc, *r, mix(lk.base(), lk.ink(), 0x18));
                }
                if s.checked.contains(cmd) {
                    fill(&dc, *r, mix(lk.base(), ACCENT, 0x40));
                    outline(&dc, *r, mix(lk.base(), ACCENT, 0xB0));
                }
                if let Some(b) = icon {
                    dc.draw_bitmap(b, r.l + (BTN - ICON) / 2, r.t + (BTN - ICON) / 2, true);
                }
            }
        }
    }
}

impl St {
    fn can_scroll(&self, b: Btn) -> bool {
        match b {
            Btn::Left => self.offset > 0,
            Btn::Right => self.layout.last().is_some_and(|t| t.r > self.area),
            Btn::Plus => true,
        }
    }

    fn btn_at(&self, x: i32, y: i32) -> Option<Btn> {
        match self.arrows {
            Some((l, _)) if l.has(x, y) => Some(Btn::Left),
            Some((_, r)) if r.has(x, y) => Some(Btn::Right),
            _ if self.plus.has(x, y) => Some(Btn::Plus),
            _ => None,
        }
    }

    /// Scroll so the next tab that is cut off on that side comes fully into view.
    fn scroll_tabs(&mut self, b: Btn) {
        let Some(_) = self.arrows else { return };
        let pad = ARROW_W + 2;
        let avail = self.area - pad - 4;
        // Tab edges without the scroll shift.
        let edges: Vec<(i32, i32)> = self.layout.iter().map(|r| (r.l - pad + self.offset, r.r - pad + self.offset)).collect();
        match b {
            Btn::Left => {
                if let Some((l, _)) = edges.iter().rev().find(|(l, _)| *l < self.offset) {
                    self.offset = *l;
                } else {
                    self.offset = 0;
                }
            }
            Btn::Right => {
                if let Some((_, r)) = edges.iter().find(|(_, r)| *r > self.offset + avail) {
                    self.offset = *r - avail;
                }
            }
            Btn::Plus => {}
        }
    }
}

fn dp(x: i32, y: i32) -> wxdragon::dc::Point {
    wxdragon::dc::Point::new(x, y)
}

fn tab_button(dc: &impl DeviceContext, r: R, lk: Look, hot: bool, down: bool, enabled: bool, glyph: Btn) {
    if enabled && (hot || down) {
        let wash = mix(lk.base(), lk.ink(), if down { 0x40 } else { 0x24 });
        dc.set_pen(colour(wash), 1, PenStyle::Solid);
        dc.set_brush(colour(wash), BrushStyle::Solid);
        dc.draw_rounded_rectangle(r.l, r.t, r.w(), r.h(), 4.0);
    }
    let c = if enabled { lk.fg() } else { mix(lk.base(), lk.ink(), 0x50) };
    dc.set_pen(colour(c), 1, PenStyle::Solid);
    dc.set_brush(colour(c), BrushStyle::Solid);
    let (cx, cy) = ((r.l + r.r) / 2, (r.t + r.b) / 2);
    match glyph {
        Btn::Left => dc.draw_polygon(&[dp(cx + 3, cy - 5), dp(cx - 3, cy), dp(cx + 3, cy + 5)], 0, 0, wxdragon::dc::PolygonFillMode::OddEven),
        Btn::Right => dc.draw_polygon(&[dp(cx - 3, cy - 5), dp(cx + 3, cy), dp(cx - 3, cy + 5)], 0, 0, wxdragon::dc::PolygonFillMode::OddEven),
        Btn::Plus => {
            dc.draw_line(cx - 5, cy, cx + 6, cy);
            dc.draw_line(cx, cy - 5, cx, cy + 6);
        }
    }
}

fn paint_tabs(st: &Rc<RefCell<St>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let (w, h) = { let c = panel.get_client_size(); (c.width, c.height) };
    fill(&dc, R::new(0, 0, w, h), lk.base());
    layout_tabs(&dc, w, &mut s);
    fill(&dc, R::new(0, h - 1, w, h), mix(lk.base(), lk.ink(), 0x30));
    let (clip_l, clip_r) = if let Some((l, _)) = s.arrows { (l.r, s.area) } else { (0, w) };
    dc.set_clipping_region(clip_l, 0, (clip_r - clip_l).max(0), h);
    for (i, info) in s.tabs.iter().enumerate() {
        let r = s.layout[i];
        let active = i == s.active;
        let hot = s.tab_hot.map(|x| x.0) == Some(i);
        let fg = if active { lk.editor_fg } else { lk.dim() };
        if active {
            fill(&dc, R::new(r.l, r.t, r.r, h), lk.editor_bg);
            fill(&dc, R::new(r.l, r.t, r.r, r.t + 2), ACCENT);
        } else if hot {
            fill(&dc, R::new(r.l, r.t, r.r, r.b - 1), mix(lk.base(), lk.ink(), 0x18));
        }
        if !active && !hot && s.active != i + 1 && s.tab_hot.map(|x| x.0) != Some(i + 1) {
            fill(&dc, R::new(r.r, r.t + 8, r.r + 1, r.b - 8), mix(lk.base(), lk.ink(), 0x30));
        }
        if let Some(c) = info.color {
            fill(&dc, R::new(r.l, r.t, r.r, r.t + 3), c);
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
            dc.set_pen(colour(TAB_DIRTY), 1, PenStyle::Solid);
            dc.set_brush(colour(TAB_DIRTY), BrushStyle::Solid);
            dc.draw_ellipse(tx, r.t + 12, 8, 8);
            tx += 14;
        }
        if let Some(badge) = info.badge {
            dc.set_pen(colour(BADGE), 1, PenStyle::Solid);
            dc.set_brush(colour(BADGE), BrushStyle::Solid);
            dc.draw_rounded_rectangle(tx, r.t + 8, 24, 16, 6.0);
            text_at(&dc, badge, R::new(tx, r.t + 8, tx + 24, r.t + 24), WHITE, true);
            tx += 30;
        }
        // Long names end in an ellipsis rather than running under the close button.
        let room = (r.r - 26 - tx).max(0);
        let mut name = info.title.clone();
        if text_w(&dc, &name) > room {
            while name.chars().count() > 1 && text_w(&dc, &format!("{name}\u{2026}")) > room {
                name.pop();
            }
            name.push('\u{2026}');
        }
        text_at(&dc, &name, R::new(tx, r.t, r.r - 26, r.b), fg, false);
        if (active || hot) && !info.pinned {
            let c = close_rect(&r);
            if s.tab_hot == Some((i, true)) {
                let wash = if active { if lk.dark { (0x3A, 0x3A, 0x3A) } else { (0xE0, 0xE0, 0xE0) } } else { mix(lk.base(), lk.ink(), 0x28) };
                fill(&dc, c, wash);
            }
            dc.set_pen(colour(fg), 1, PenStyle::Solid);
            let (cx, cy) = ((c.l + c.r) / 2, (c.t + c.b) / 2);
            dc.draw_line(cx - 3, cy - 3, cx + 4, cy + 4);
            dc.draw_line(cx + 3, cy - 3, cx - 4, cy + 4);
        }
    }
    dc.destroy_clipping_region();
    let state = |b: Btn| (s.btn_hot == Some(b), s.btn_down == Some(b), s.can_scroll(b));
    if let Some((l, r)) = s.arrows {
        for (rect, b) in [(l, Btn::Left), (r, Btn::Right)] {
            let (hot, down, on) = state(b);
            // An arrow that has nothing to scroll to is not drawn.
            if on {
                tab_button(&dc, rect, lk, hot, down, on, b);
            }
        }
    }
    let (hot, down, on) = state(Btn::Plus);
    tab_button(&dc, s.plus, lk, hot, down, on, Btn::Plus);
}

fn paint_status(st: &Rc<RefCell<St>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let (w, h) = { let c = panel.get_client_size(); (c.width, c.height) };
    fill(&dc, R::new(0, 0, w, h), STATUS_BG);
    let t = s.status.clone();
    text_at(&dc, &t.position, R::new(10, 0, w / 3, h), WHITE, false);
    let mut hits = Vec::new();
    let mut x = w - 8;
    let segs: [(&str, isize); 6] = [("INS", 0), (&t.lang, SEG_LANG), (&t.eol, SEG_EOL), (&t.enc, SEG_ENC), (&t.length, 0), (&t.lines, 0)];
    for (label, id) in segs {
        let wd = text_w(&dc, label) + 22;
        let r = R::new(x - wd, 0, x, h);
        text_at(&dc, label, r, WHITE, true);
        fill(&dc, R::new(r.l, 5, r.l + 1, h - 5), STATUS_SEP);
        if id != 0 {
            hits.push((r, id));
        }
        x -= wd;
    }
    s.status_hits = hits;
}

impl Chrome {
    pub fn new(parent: &Frame) -> Chrome {
        let font = Font::builder().with_face_name("Segoe UI").with_point_size(9).build();
        let look = Look { dark: false, editor_bg: WHITE, editor_fg: (0, 0, 0) };

        let mut tools = Vec::new();
        let mut x = 6;
        for (cmd, svg, tip) in toolbar_items() {
            match cmd {
                None => {
                    tools.push((Tool::Sep, R::new(x, 3, x + 8, 3 + BTN)));
                    x += 8;
                }
                Some(cmd) => {
                    let icon = BitmapBundle::from_svg_data(svg, Size::new(ICON, ICON)).and_then(|b| b.get_bitmap(Size::new(ICON, ICON)));
                    tools.push((Tool::Btn { cmd, tip, icon }, R::new(x, 3, x + BTN, 3 + BTN)));
                    x += BTN + 1;
                }
            }
        }

        let st = Rc::new(RefCell::new(St {
            look,
            font,
            handler: None,
            pill: None,
            menu_items: Vec::new(),
            menu_rects: Vec::new(),
            menu_hot: None,
            tools,
            tool_hot: None,
            tool_down: None,
            checked: Vec::new(),
            tabs: Vec::new(),
            active: 0,
            tab_hot: None,
            layout: Vec::new(),
            plus: R::default(),
            btn_hot: None,
            btn_down: None,
            arrows: None,
            area: 0,
            offset: 0,
            follow: true,
            drag: None,
            status: StatusText::default(),
            status_hits: Vec::new(),
        }));

        let menu_p = Panel::builder(parent).build();
        let tools_p = Panel::builder(parent).build();
        let tabs_p = Panel::builder(parent).build();
        let status_p = Panel::builder(parent).build();
        for (p, h) in [(&menu_p, MENU_H), (&tools_p, TOOL_H), (&tabs_p, TAB_H), (&status_p, STATUS_H)] {
            p.set_background_style(BackgroundStyle::Paint);
            p.set_min_size(Size::new(-1, h));
        }
        wire_menu(&st, menu_p);
        wire_tools(&st, tools_p);
        wire_tabs(&st, tabs_p);
        wire_status(&st, status_p);
        Chrome { menu: menu_p, tools: tools_p, tabs: tabs_p, status: status_p, st }
    }

    pub fn set_pill(&self, p: Option<(String, Rgb, usize)>) {
        self.st.borrow_mut().pill = p.map(|(t, c, cmd)| (t, c, cmd, R::default()));
        self.tools.refresh(false, None);
    }

    pub fn set_menu_titles(&self, titles: Vec<String>) {
        self.st.borrow_mut().menu_items = titles;
        self.menu.refresh(false, None);
    }

    /// Where menu `i`'s popup goes (screen coordinates) and how many menus there are, usable while the
    /// application is busy showing one.
    pub fn menu_locator(&self) -> (Box<dyn Fn(i32) -> (i32, i32)>, i32) {
        let (st, panel) = (self.st.clone(), self.menu);
        let n = st.borrow().menu_items.len() as i32;
        let at = move |i: i32| {
            let (x, y) = st.borrow().menu_rects.get(i as usize).map_or((0, MENU_H), |r| (r.l, r.b));
            let p = panel.client_to_screen(Point::new(x, y));
            (p.x, p.y)
        };
        (Box::new(at), n)
    }
    pub fn set_handler(&self, h: Rc<dyn Fn(Act)>) {
        self.st.borrow_mut().handler = Some(h);
    }

    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        {
            let mut s = self.st.borrow_mut();
            if s.active != active {
                s.follow = true;
            }
            s.tabs = tabs;
            s.active = active;
        }
        self.tabs.refresh(false, None);
    }

    pub fn set_theme(&self, dark: bool, editor_bg: Rgb, editor_fg: Rgb) {
        self.st.borrow_mut().look = Look { dark, editor_bg, editor_fg };
        for p in [&self.menu, &self.tools, &self.tabs] {
            p.refresh(false, None);
        }
    }

    pub fn set_status(&self, t: StatusText) {
        self.st.borrow_mut().status = t;
        self.status.refresh(false, None);
    }

    pub fn set_checked(&self, cmd: usize, on: bool) {
        {
            let mut s = self.st.borrow_mut();
            let has = s.checked.contains(&cmd);
            if on == has {
                return;
            }
            if on {
                s.checked.push(cmd);
            } else {
                s.checked.retain(|c| *c != cmd);
            }
        }
        self.tools.refresh(false, None);
    }
}

fn paint_menu(st: &Rc<RefCell<St>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let (w, h) = { let c = panel.get_client_size(); (c.width, c.height) };
    fill(&dc, R::new(0, 0, w, h), lk.base());
    let mut x = 6;
    let mut rects = Vec::new();
    for (i, label) in s.menu_items.iter().enumerate() {
        let wd = text_w(&dc, label) + 20;
        let r = R::new(x, 2, x + wd, h - 2);
        if s.menu_hot == Some(i) {
            fill(&dc, r, mix(lk.base(), lk.ink(), 0x1C));
        }
        text_at(&dc, label, r, lk.fg(), true);
        rects.push(r);
        x += wd;
    }
    s.menu_rects = rects;
}

fn wire_menu(st: &Rc<RefCell<St>>, p: Panel) {
    let s = st.clone();
    p.on_paint(move |_| paint_menu(&s, &p));
    let s = st.clone();
    p.on_mouse_motion(move |e| {
        let (x, y) = pos(&e);
        let mut m = s.borrow_mut();
        let hot = m.menu_rects.iter().position(|r| r.has(x, y));
        if hot != m.menu_hot {
            m.menu_hot = hot;
            drop(m);
            p.refresh(false, None);
        }
    });
    let s = st.clone();
    p.on_mouse_leave(move |_| {
        s.borrow_mut().menu_hot = None;
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_left_down(move |e| {
        let (x, y) = pos(&e);
        let hit = s.borrow().menu_rects.iter().position(|r| r.has(x, y));
        if let Some(i) = hit {
            let r = s.borrow().menu_rects[i];
            emit(&s, Act::Menu(i, r.l, r.b));
            p.refresh(false, None);
        }
    });
}

fn wire_tools(st: &Rc<RefCell<St>>, p: Panel) {
    let s = st.clone();
    p.on_paint(move |_| paint_tools(&s, &p));
    let s = st.clone();
    p.on_mouse_motion(move |e| {
        let (x, y) = pos(&e);
        let mut m = s.borrow_mut();
        let hot = m.tools.iter().position(|(it, r)| matches!(it, Tool::Btn { .. }) && r.has(x, y));
        if hot != m.tool_hot {
            m.tool_hot = hot;
            let tip = hot.and_then(|i| if let Tool::Btn { tip, .. } = &m.tools[i].0 { Some(*tip) } else { None });
            drop(m);
            p.set_tooltip(tip.unwrap_or(""));
            p.refresh(false, None);
        }
    });
    let s = st.clone();
    p.on_mouse_leave(move |_| {
        let mut m = s.borrow_mut();
        m.tool_hot = None;
        m.tool_down = None;
        drop(m);
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_left_down(move |e| {
        let (x, y) = pos(&e);
        let mut m = s.borrow_mut();
        m.tool_down = m.tools.iter().position(|(it, r)| matches!(it, Tool::Btn { .. }) && r.has(x, y));
        drop(m);
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_left_up(move |e| {
        let (x, y) = pos(&e);
        let pill = s.borrow().pill.as_ref().filter(|p| p.3.has(x, y)).map(|p| p.2);
        if let Some(c) = pill {
            return emit(&s, Act::Cmd(c));
        }
        let cmd = {
            let mut m = s.borrow_mut();
            let hit = m.tool_down.filter(|d| m.tools[*d].1.has(x, y)).and_then(|d| if let Tool::Btn { cmd, .. } = &m.tools[d].0 { Some(*cmd) } else { None });
            m.tool_down = None;
            hit
        };
        p.refresh(false, None);
        if let Some(c) = cmd {
            emit(&s, Act::Cmd(c));
        }
    });
}

fn tab_at(m: &St, x: i32, y: i32) -> Option<usize> {
    m.layout.iter().position(|r| r.has(x, y) && m.arrows.is_none_or(|(l, _)| x >= l.r && x < m.area))
}

fn wire_tabs(st: &Rc<RefCell<St>>, p: Panel) {
    let s = st.clone();
    p.on_paint(move |_| paint_tabs(&s, &p));
    let s = st.clone();
    p.on_mouse_motion(move |e| {
        let (x, y) = pos(&e);
        let mut moved = None;
        {
            let mut m = s.borrow_mut();
            if let Some(d) = &m.drag {
                let (from, x0, moving) = (d.from, d.x0, d.moving);
                if moving || (x - x0).abs() > 6 {
                    if let Some((l, _)) = m.arrows {
                        if x < l.r + 16 {
                            m.offset -= 30;
                        } else if x > m.area - 16 {
                            m.offset += 30;
                        }
                    }
                    let hit = m.layout.iter().position(|r| x >= r.l && x < r.r);
                    let to = hit.unwrap_or(if x < m.layout.first().map_or(0, |r| r.l) { 0 } else { m.tabs.len().saturating_sub(1) });
                    if to != from && to < m.tabs.len() {
                        let tab = m.tabs.remove(from);
                        m.tabs.insert(to, tab);
                        m.active = to;
                        m.tab_hot = Some((to, false));
                        m.follow = true;
                        moved = Some((from, to));
                    }
                    m.drag = Some(Drag { from: to.min(m.tabs.len().saturating_sub(1)), x0, moving: true });
                }
            } else {
                let hot = tab_at(&m, x, y).map(|i| (i, close_rect(&m.layout[i]).has(x, y) && !m.tabs[i].pinned));
                if hot != m.tab_hot {
                    m.tab_hot = hot;
                }
                m.btn_hot = m.btn_at(x, y);
            }
        }
        if let Some((a, b)) = moved {
            emit(&s, Act::Move(a, b));
        }
        let tip = match s.borrow().btn_hot.filter(|b| s.borrow().can_scroll(*b)) {
            Some(Btn::Left) => "Scroll tabs left",
            Some(Btn::Right) => "Scroll tabs right",
            Some(Btn::Plus) => "New (Ctrl+N)",
            None => "",
        };
        p.set_tooltip(tip);
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_leave(move |_| {
        {
            let mut m = s.borrow_mut();
            m.tab_hot = None;
            m.btn_hot = None;
        }
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_left_down(move |e| {
        let (x, y) = pos(&e);
        let act = {
            let mut m = s.borrow_mut();
            let hit = tab_at(&m, x, y);
            let on_close = hit.is_some_and(|i| close_rect(&m.layout[i]).has(x, y) && !m.tabs[i].pinned && (i == m.active || m.tab_hot.map(|h| h.0) == Some(i)));
            if let Some(b) = m.btn_at(x, y) {
                m.btn_down = Some(b);
                match b {
                    Btn::Plus => Some(Act::New),
                    _ => {
                        if m.can_scroll(b) {
                            m.scroll_tabs(b);
                        }
                        None
                    }
                }
            } else if let Some(i) = hit {
                if on_close {
                    Some(Act::Close(i))
                } else {
                    m.drag = Some(Drag { from: i, x0: x, moving: false });
                    Some(Act::Select(i))
                }
            } else {
                None
            }
        };
        if let Some(a) = act {
            if matches!(a, Act::Select(_)) {
                p.capture_mouse();
            }
            emit(&s, a);
        }
        p.refresh(false, None);
    });
    let s = st.clone();
    p.on_mouse_left_up(move |_| {
        s.borrow_mut().btn_down = None;
        p.refresh(false, None);
        let was = s.borrow_mut().drag.take().is_some();
        if was && p.has_capture() {
            p.release_mouse();
        }
    });
    let s = st.clone();
    p.on_mouse_left_double_click(move |e| {
        let (x, y) = pos(&e);
        // A double-click on empty strip makes a tab; on a button (even a hidden arrow) it does nothing extra.
        let empty = {
            let m = s.borrow();
            tab_at(&m, x, y).is_none() && m.btn_at(x, y).is_none()
        };
        if empty {
            emit(&s, Act::New);
        }
    });
    let s = st.clone();
    p.on_mouse_middle_up(move |e| {
        let (x, y) = pos(&e);
        let hit = tab_at(&s.borrow(), x, y);
        if let Some(i) = hit {
            emit(&s, Act::Close(i));
        }
    });
    let s = st.clone();
    p.on_mouse_right_up(move |e| {
        let (x, y) = pos(&e);
        let hit = tab_at(&s.borrow(), x, y);
        if let Some(i) = hit {
            emit(&s, Act::Context(i, x, y));
        }
    });
}

fn wire_status(st: &Rc<RefCell<St>>, p: Panel) {
    let s = st.clone();
    p.on_paint(move |_| paint_status(&s, &p));
    let s = st.clone();
    p.on_mouse_left_down(move |e| {
        let (x, y) = pos(&e);
        let seg = s.borrow().status_hits.iter().find(|(r, _)| r.has(x, y)).map(|h| (h.1, h.0));
        if let Some((id, r)) = seg {
            emit(&s, Act::Status(id, r.l, r.t));
        }
    });
}
