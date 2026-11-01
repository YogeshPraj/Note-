//! The status bar: a text at the left and right-aligned segments, some of them clickable.
//!
//! It always uses the accent colour (like the Win32 shell), whatever the theme.
//!
//! ```ignore
//! let bar = StatusBar::new(&frame);
//! bar.set_left("Ln 1, Col 1");
//! bar.set_segments(vec![Segment::new("UTF-8", 1), Segment::new("Windows (CR LF)", 2)]);
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use crate::geom::Rect;
use crate::paint::{fill, pos, text_at, text_w};

pub const HEIGHT: i32 = 24;
const RIGHT_MARGIN: i32 = 8;
const SEGMENT_PAD: i32 = 22;

/// One right-aligned piece of text; clickable when it has an id.
#[derive(Clone, Default, Debug)]
pub struct Segment {
    pub text: String,
    pub id: Option<isize>,
}

impl Segment {
    pub fn text(text: impl Into<String>) -> Segment {
        Segment { text: text.into(), id: None }
    }
    pub fn button(text: impl Into<String>, id: isize) -> Segment {
        Segment { text: text.into(), id: Some(id) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusEvent {
    /// Segment `id` was clicked; (x, y) is its top-left corner in the bar.
    Clicked(isize, i32, i32),
}

/// Segment rectangles, the first segment rightmost, each `SEGMENT_PAD` wider than its text.
pub fn layout_right(text_widths: &[i32], bar_w: i32, height: i32) -> Vec<Rect> {
    let mut x = bar_w - RIGHT_MARGIN;
    text_widths
        .iter()
        .map(|w| {
            let wd = w + SEGMENT_PAD;
            let r = Rect::new(x - wd, 0, x, height);
            x -= wd;
            r
        })
        .collect()
}

struct State {
    font: Option<Font>,
    left: String,
    segments: Vec<Segment>,
    rects: Vec<Rect>,
    handler: Option<Rc<dyn Fn(StatusEvent)>>,
}

impl std::fmt::Debug for StatusBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusBar").finish_non_exhaustive()
    }
}

#[derive(Clone)]
pub struct StatusBar {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl StatusBar {
    pub fn new(parent: &Frame) -> StatusBar {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, HEIGHT));
        let st = Rc::new(RefCell::new(State {
            font: crate::theme::current().ui_font(),
            left: String::new(),
            segments: Vec::new(),
            rects: Vec::new(),
            handler: None,
        }));
        let bar = StatusBar { panel, st };
        bar.wire();
        bar
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn on_event(&self, f: impl Fn(StatusEvent) + 'static) {
        self.st.borrow_mut().handler = Some(Rc::new(f));
    }

    /// Show `left` at the left and `right` from the right edge inwards (the first segment is rightmost).
    pub fn set(&self, left: String, right: Vec<Segment>) {
        {
            let mut s = self.st.borrow_mut();
            s.left = left;
            s.segments = right;
        }
        self.panel.refresh(false, None);
    }

    fn wire(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, &p));

        let st = self.st.clone();
        self.panel.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            let clicked = {
                let s = st.borrow();
                s.rects.iter().zip(&s.segments).find(|(r, seg)| seg.id.is_some() && r.has(x, y)).map(|(r, seg)| (seg.id.unwrap_or(0), r.l, r.t))
            };
            if let Some((id, l, t)) = clicked {
                let h = st.borrow().handler.clone();
                if let Some(h) = h {
                    h(StatusEvent::Clicked(id, l, t));
                }
            }
        });
    }
}

fn paint(st: &Rc<RefCell<State>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let size = panel.get_client_size();
    let (w, h) = (size.width, size.height);
    let c = crate::theme::current().colors;
    fill(&dc, Rect::new(0, 0, w, h), c.accent);
    text_at(&dc, &s.left, Rect::new(10, 0, w / 3, h), c.accent_text, false);
    let widths: Vec<i32> = s.segments.iter().map(|g| text_w(&dc, &g.text)).collect();
    s.rects = layout_right(&widths, w, h);
    for (seg, r) in s.segments.iter().zip(&s.rects) {
        text_at(&dc, &seg.text, *r, c.accent_text, true);
        fill(&dc, Rect::new(r.l, 5, r.l + 1, h - 5), c.accent_line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_stack_from_the_right_edge_inwards() {
        let r = layout_right(&[20, 30, 40], 1000, HEIGHT);
        assert_eq!(r[0].r, 1000 - RIGHT_MARGIN);
        assert_eq!(r[1].r, r[0].l);
        assert_eq!(r[2].r, r[1].l);
        assert_eq!(r[0].w(), 20 + SEGMENT_PAD);
    }

    #[test]
    fn segments_fill_the_bar_height() {
        assert!(layout_right(&[10], 500, HEIGHT).iter().all(|r| r.t == 0 && r.b == HEIGHT));
    }

    #[test]
    fn only_segments_with_an_id_are_buttons() {
        assert!(Segment::text("INS").id.is_none());
        assert_eq!(Segment::button("UTF-8", 7).id, Some(7));
    }
}
