//! A scrollable, reorderable tab strip that paints itself.
//!
//! The stock wx notebook cannot show Note++'s tabs (dirty dot, badge, pin, colour bar, close button, "+"),
//! so this is a plain panel. It owns no application state: it is told the tabs and the active one, and reports
//! what the user did as a [`TabEvent`]. Layout and scrolling live in [`model`], which has no toolkit in it and
//! is unit tested.
//!
//! ```ignore
//! let strip = TabStrip::new(&frame);
//! strip.set_tabs(vec![Tab { title: "new 1".into(), dirty: true, badge: None, pinned: false, color: None, level: 0 }], 0);
//! strip.on_event(|e| match e { TabEvent::Select(i) => { /* activate tab i */ } _ => {} });
//! ```

mod column;
pub mod model;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use self::model::{Btn, Layout, Rows, Scroll};
use crate::geom::Rect;
use crate::paint::{Look, colour, fill, mix, pos, text_at, text_w, wash};

pub const HEIGHT: i32 = 32;

const ROWS: Rows = Rows { tab_top: 4, tab_bottom: HEIGHT, btn_top: 6, btn_bottom: HEIGHT - 2 };
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

/// One tab as the strip draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub title: String,
    pub dirty: bool,
    /// Small pill shown before the title (e.g. "wb").
    pub badge: Option<&'static str>,
    pub pinned: bool,
    /// The colour mark of the tab.
    pub color: Option<(u8, u8, u8)>,
    /// Nesting depth in a vertical strip (indents the row); 0 elsewhere.
    pub level: u8,
}

struct Drag {
    from: usize,
    x0: i32,
    moving: bool,
}

struct State {
    /// A column of full-width rows (category navigation) instead of a row of tabs.
    vertical: bool,
    look: Look,
    font: Option<Font>,
    tabs: Vec<Tab>,
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
    /// Text for the tool tip of a tab that is not the active one, asked for when the pointer comes to rest on it.
    preview: Option<Rc<dyn Fn(usize) -> String>>,
    /// The tab the tool tip was last made for (None: ask again).
    tip_for: Option<usize>,
}

impl std::fmt::Debug for TabStrip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabStrip").finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct TabStrip {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl TabStrip {
    pub fn new(parent: &impl WxWidget) -> TabStrip {
        let panel = Panel::builder(parent).build();
        panel.set_min_size(Size::new(-1, HEIGHT));
        Self::build(panel, false)
    }

    /// A vertical strip width pixels wide: one full-width row per tab, selecting only (no close, +, scrolling or dragging).
    pub fn vertical(parent: &impl WxWidget, width: i32) -> TabStrip {
        let panel = Panel::builder(parent).build();
        panel.set_min_size(Size::new(width, -1));
        Self::build(panel, true)
    }

    fn build(panel: Panel, vertical: bool) -> TabStrip {
        panel.set_background_style(BackgroundStyle::Paint);
        let st = Rc::new(RefCell::new(State {
            vertical,
            look: crate::theme::Theme::light().look(),
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
            preview: None,
            tip_for: None,
        }));
        let strip = TabStrip { panel, st };
        strip.wire();
        let (p, st) = (strip.panel, strip.st.clone());
        crate::theme::subscribe(move |t| {
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

    /// Show `f(i)` as the tool tip of tab `i` while the pointer is over it, except on the active tab.
    pub fn set_preview(&self, f: impl Fn(usize) -> String + 'static) {
        self.st.borrow_mut().preview = Some(Rc::new(f));
    }

    pub fn set_font(&self, font: Option<Font>) {
        self.st.borrow_mut().font = font;
    }

    pub fn set_tabs(&self, tabs: Vec<Tab>, active: usize) {
        {
            let mut s = self.st.borrow_mut();
            if s.active != active {
                s.scroll.reveal_active();
            }
            // A different set or order of tabs (or another active one) makes the tool tip stale; a redraw does not.
            if s.active != active || s.tabs.len() != tabs.len() || s.tabs.iter().zip(&tabs).any(|(a, b)| a.title != b.title) {
                s.tip_for = None;
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
        self.wire_motion_and_press();
        self.wire_release_and_clicks();
    }

    fn wire_motion_and_press(&self) {
        self.wire_motion_and_press_1();
        self.wire_motion_and_press_2();
    }

    fn wire_motion_and_press_1(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, p));

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_motion(move |e| {
            let (x, y) = pos(&e);
            let mut moved = None;
            {
                let mut s = st.borrow_mut();
                if s.drag.is_some() {
                    moved = drag_to(&mut s, x);
                } else if s.vertical {
                    s.hot = s.layout.tab_at(x, y).map(|i| (i, false));
                } else {
                    let hot = s.layout.tab_at(x, y).map(|i| (i, close_rect(&s.layout.tabs[i]).has(x, y) && !s.tabs[i].pinned));
                    s.hot = hot;
                    s.btn_hot = s.layout.button_at(x, y);
                }
            }
            if let Some((a, b)) = moved {
                Self::emit(&st, TabEvent::Move(a, b));
            }
            // Copied out first: the borrow must not be held while the tab's tool tip is made (that borrows again).
            let btn = st.borrow().btn_hot;
            let tip = match btn {
                Some(Btn::Left) => Some("Scroll tabs left".to_string()),
                Some(Btn::Right) => Some("Scroll tabs right".to_string()),
                Some(Btn::Plus) => Some("New (Ctrl+N)".to_string()),
                None => tab_tip(&st),
            };
            if let Some(tip) = tip {
                p.set_tooltip(&tip);
            }
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
    }

    fn wire_motion_and_press_2(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            let event = {
                let mut s = st.borrow_mut();
                if s.vertical {
                    s.layout.tab_at(x, y).map(TabEvent::Select)
                } else if let Some(b) = s.layout.button_at(x, y) {
                    s.btn_down = Some(b);
                    if b == Btn::Plus {
                        Some(TabEvent::New)
                    } else {
                        s.scroll.step(b);
                        None
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
                if matches!(ev, TabEvent::Select(_)) && !st.borrow().vertical {
                    p.capture_mouse();
                }
                Self::emit(&st, ev);
            }
            p.refresh(false, None);
        });
    }

    fn wire_release_and_clicks(&self) {
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
        self.panel.on_mouse_middle_up(move |e| {
            let (x, y) = pos(&e);
            let hit = st.borrow().layout.tab_at(x, y).filter(|_| !st.borrow().vertical);
            if let Some(i) = hit {
                Self::emit(&st, TabEvent::Close(i));
            }
        });

        let st = self.st.clone();
        self.panel.on_mouse_right_up(move |e| {
            let (x, y) = pos(&e);
            let hit = st.borrow().layout.tab_at(x, y).filter(|_| !st.borrow().vertical);
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

/// The tool tip for the tab under the pointer: a preview of its text, but not for the active tab (its text is on
/// screen), nor over a close button. `None` leaves the tool tip as it is (already made for this tab).
fn tab_tip(st: &Rc<RefCell<State>>) -> Option<String> {
    let (hot, active, vertical, preview, done) = {
        let s = st.borrow();
        (s.hot, s.active, s.vertical, s.preview.clone(), s.tip_for)
    };
    let wanted = hot.filter(|(i, on_close)| !vertical && !on_close && *i != active).map(|(i, _)| i);
    if wanted == done {
        return None;
    }
    st.borrow_mut().tip_for = wanted;
    Some(match (wanted, preview) {
        (Some(i), Some(f)) => f(i),
        _ => String::new(),
    })
}

mod paint;
use paint::*;
