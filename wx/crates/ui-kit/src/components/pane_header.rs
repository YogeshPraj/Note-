//! A thin bar above a pane that sits beside another: what the pane holds, and whether the keyboard is in it. The current
//! pane has an accent line under its name and full-strength text; the other is dimmed. Clicking the bar asks to go to
//! the pane; a closable one has a close mark.
//!
//! ```ignore
//! let head = PaneHeader::new(&parent, Some("dock"), true);
//! head.on_event(|e| match e { HeaderEvent::Focus => { /* go there */ } HeaderEvent::Close => { /* close */ } });
//! head.set("notes.txt", false, true);
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::a11y;
use crate::geom::{Rect, mix};
use crate::paint::{colour, ellipsize, fill, pos, text_at, text_w, wash};
use crate::pointer::Pointer;
use crate::theme;

/// Sizes before scaling (see [`crate::theme::Theme::px`]).
pub const HEIGHT: i32 = 26;
const PAD: i32 = 8;
const DOT: i32 = 8;
const CLOSE: i32 = 20;
const ACCENT: i32 = 2;

/// What the user did on the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderEvent {
    Focus,
    Close,
}

struct State {
    title: String,
    dirty: bool,
    current: bool,
    badge: Option<&'static str>,
    closable: bool,
    pointer: Pointer,
    handler: Option<Rc<dyn Fn(HeaderEvent)>>,
    reader: a11y::Items,
}

/// The close mark's place in a bar `w` wide and `h` high.
fn close_rect(w: i32, h: i32) -> Rect {
    let t = theme::current();
    let side = t.px(CLOSE).min(h);
    let right = w - t.px(PAD) / 2;
    let top = (h - side) / 2;
    Rect::new(right - side, top, right, top + side)
}

/// Where a press at (x, y) lands.
fn hit(closable: bool, w: i32, h: i32, x: i32, y: i32) -> HeaderEvent {
    if closable && close_rect(w, h).has(x, y) { HeaderEvent::Close } else { HeaderEvent::Focus }
}

/// A thin bar naming a pane.
#[derive(Clone)]
pub struct PaneHeader {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl std::fmt::Debug for PaneHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaneHeader").finish_non_exhaustive()
    }
}

impl PaneHeader {
    pub fn new(parent: &impl WxWidget, badge: Option<&'static str>, closable: bool) -> PaneHeader {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, theme::current().px(HEIGHT)));
        panel.set_can_focus(false);
        let reader = a11y::Items::default();
        let st = Rc::new(RefCell::new(State {
            title: String::new(),
            dirty: false,
            current: false,
            badge,
            closable,
            pointer: Pointer::default(),
            handler: None,
            reader: reader.clone(),
        }));
        let head = PaneHeader { panel, st };
        head.wire(reader);
        theme::subscribe(move |_| panel.refresh(false, None));
        head
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn on_event(&self, f: impl Fn(HeaderEvent) + 'static) {
        self.st.borrow_mut().handler = Some(Rc::new(f));
    }

    /// What the pane holds, whether it has unsaved changes, and whether the keyboard is in it.
    pub fn set(&self, title: &str, dirty: bool, current: bool) {
        {
            let mut s = self.st.borrow_mut();
            if s.title == title && s.dirty == dirty && s.current == current {
                return;
            }
            title.clone_into(&mut s.title);
            (s.dirty, s.current) = (dirty, current);
        }
        self.panel.refresh(false, None);
    }

    fn wire(&self, reader: a11y::Items) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, p));

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_motion(move |e| {
            let (x, y) = pos(&e);
            let area = p.get_client_size();
            let mut s = st.borrow_mut();
            let over = (hit(s.closable, area.width, area.height, x, y) == HeaderEvent::Close).then_some(0);
            if s.pointer.hover(over) {
                p.refresh(false, None);
            }
        });
        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_leave(move |_| {
            if st.borrow_mut().pointer.hover(None) {
                p.refresh(false, None);
            }
        });
        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            let area = p.get_client_size();
            let (closable, handler) = {
                let s = st.borrow();
                (s.closable, s.handler.clone())
            };
            if let Some(h) = handler {
                h(hit(closable, area.width, area.height, x, y));
            }
        });
        let st = self.st.clone();
        a11y::attach(self.panel, "Pane", a11y::Bar::Tools, reader, move |_| {
            let h = st.borrow().handler.clone();
            if let Some(h) = h {
                h(HeaderEvent::Close);
            }
        });
    }
}

fn paint(st: &Rc<RefCell<State>>, panel: Panel) {
    let dc = AutoBufferedPaintDC::new(&panel);
    let t = theme::current();
    let c = t.colors;
    let s = st.borrow();
    let area = panel.get_client_size();
    let (w, h) = (area.width, area.height);
    fill(&dc, Rect::new(0, 0, w, h), if s.current { c.surface } else { mix(c.surface, c.ink, 0x0C) });
    let line = t.px(ACCENT);
    if s.current {
        fill(&dc, Rect::new(0, h - line, w, h), c.accent);
    } else {
        fill(&dc, Rect::new(0, h - 1, w, h), c.border);
    }
    if let Some(f) = t.ui_font() {
        dc.set_font(&f);
    }
    let bar_h = if s.current { h - line } else { h };
    let mut x = t.px(PAD);
    if let Some(b) = s.badge {
        let (tw, tall) = (text_w(&dc, b) + t.px(PAD), t.px(16));
        let top = (bar_h - tall) / 2;
        dc.set_pen(colour(c.badge), 1, PenStyle::Solid);
        dc.set_brush(colour(c.badge), BrushStyle::Solid);
        dc.draw_rounded_rectangle(x, top, tw, tall, f64::from(t.px(6)));
        text_at(&dc, b, Rect::new(x, top, x + tw, top + tall), c.accent_text, true);
        x += tw + t.px(PAD);
    }
    if s.dirty {
        let dot = t.px(DOT);
        dc.set_pen(colour(c.danger), 1, PenStyle::Solid);
        dc.set_brush(colour(c.danger), BrushStyle::Solid);
        dc.draw_ellipse(x, (bar_h - dot) / 2, dot, dot);
        x += dot + t.px(PAD) / 2;
    }
    let close = close_rect(w, bar_h);
    let end = if s.closable { close.l } else { w } - t.px(PAD);
    text_at(&dc, &ellipsize(&dc, &s.title, (end - x).max(0)), Rect::new(x, 0, end, bar_h), if s.current { c.text } else { c.text_muted }, false);
    if s.closable {
        if s.pointer.hot().is_some() {
            wash(&dc, close, mix(c.surface, c.ink, 0x24), f64::from(t.px(2)));
        }
        text_at(&dc, "\u{2715}", close, if s.current { c.text } else { c.text_muted }, true);
    }
    let mut items = vec![a11y::Item::new(&s.title, a11y::Role::Text, Rect::new(0, 0, close.l, h))];
    if s.closable {
        items.push(a11y::Item::new("Undock", a11y::Role::Button, close));
    }
    s.reader.publish(items);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_on_the_close_mark_closes_and_anywhere_else_goes_to_the_pane() {
        let r = close_rect(300, 26);
        assert_eq!(hit(true, 300, 26, r.l + 1, 10), HeaderEvent::Close);
        assert_eq!(hit(true, 300, 26, 40, 10), HeaderEvent::Focus);
    }

    #[test]
    fn a_header_that_cannot_close_only_goes_to_the_pane() {
        let r = close_rect(300, 26);
        assert_eq!(hit(false, 300, 26, r.l + 1, 10), HeaderEvent::Focus);
    }
}
