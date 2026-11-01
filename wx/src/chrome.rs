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
use wxdragon::prelude::*;

use crate::widgets::paint::{ACCENT, Look, Rect as R, Rgb, WHITE, fill, mix, outline, pos, text_at, text_w};
use crate::widgets::tab_strip::{TabEvent, TabStrip};

pub const MENU_H: i32 = 26;
pub const TOOL_H: i32 = 34;
pub const TAB_H: i32 = crate::widgets::tab_strip::HEIGHT;
pub const STATUS_H: i32 = 24;

const ICON: i32 = 18;
const BTN: i32 = 28;

const STATUS_BG: Rgb = (0x00, 0x7A, 0xCC);
const STATUS_SEP: Rgb = (0x4D, 0xA3, 0xDA);

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

enum Tool {
    Sep,
    Btn { cmd: usize, tip: &'static str, icon: Option<Bitmap> },
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
    status: StatusText,
    status_hits: Vec<(R, isize)>,
}

pub struct Chrome {
    pub menu: Panel,
    pub tools: Panel,
    pub tabs: Panel,
    strip: TabStrip,
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
            status: StatusText::default(),
            status_hits: Vec::new(),
        }));

        let menu_p = Panel::builder(parent).build();
        let tools_p = Panel::builder(parent).build();
        let strip = TabStrip::new(parent);
        strip.set_font(Font::builder().with_face_name("Segoe UI").with_point_size(9).build());
        let tabs_p = strip.panel();
        let status_p = Panel::builder(parent).build();
        for (p, h) in [(&menu_p, MENU_H), (&tools_p, TOOL_H), (&status_p, STATUS_H)] {
            p.set_background_style(BackgroundStyle::Paint);
            p.set_min_size(Size::new(-1, h));
        }
        wire_menu(&st, menu_p);
        wire_tools(&st, tools_p);
        wire_status(&st, status_p);
        Chrome { menu: menu_p, tools: tools_p, tabs: tabs_p, strip, status: status_p, st }
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
        self.st.borrow_mut().handler = Some(h.clone());
        self.strip.on_event(move |e| {
            h(match e {
                TabEvent::Select(i) => Act::Select(i),
                TabEvent::Close(i) => Act::Close(i),
                TabEvent::New => Act::New,
                TabEvent::Move(a, b) => Act::Move(a, b),
                TabEvent::Context(i, x, y) => Act::Context(i, x, y),
            })
        });
    }

    pub fn set_tabs(&self, tabs: Vec<TabInfo>, active: usize) {
        self.strip.set_tabs(tabs, active);
    }

    pub fn set_theme(&self, dark: bool, editor_bg: Rgb, editor_fg: Rgb) {
        let look = Look { dark, editor_bg, editor_fg };
        self.st.borrow_mut().look = look;
        self.strip.set_look(look);
        for p in [&self.menu, &self.tools] {
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
