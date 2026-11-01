//! The menu bar: titles that open the application's menus.
//!
//! The control only draws the titles and reports which one was clicked and where its popup belongs; the
//! application shows the actual menu.

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::geom::Rect;
use super::paint::{Look, fill, mix, pos, text_at, text_w, wash};
use super::pointer::Pointer;

pub const HEIGHT: i32 = 26;
const LEFT: i32 = 6;
const PAD: i32 = 20;

/// Where each title goes: left to right from the left margin, `PAD` wider than its text.
pub fn layout(text_widths: &[i32], height: i32) -> Vec<Rect> {
    let mut x = LEFT;
    text_widths
        .iter()
        .map(|w| {
            let r = Rect::new(x, 2, x + w + PAD, height - 2);
            x = r.r;
            r
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuEvent {
    /// Open menu `index`; its popup belongs at (x, y) in the bar's coordinates.
    Open(usize, i32, i32),
}

struct State {
    look: Look,
    font: Option<Font>,
    titles: Vec<String>,
    rects: Vec<Rect>,
    pointer: Pointer,
    handler: Option<Rc<dyn Fn(MenuEvent)>>,
}

#[derive(Clone)]
pub struct MenuBar {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl MenuBar {
    pub fn new(parent: &Frame) -> MenuBar {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, HEIGHT));
        let st = Rc::new(RefCell::new(State {
            look: super::theme::current().look(),
            font: Font::builder().with_face_name("Segoe UI").with_point_size(9).build(),
            titles: Vec::new(),
            rects: Vec::new(),
            pointer: Pointer::default(),
            handler: None,
        }));
        let bar = MenuBar { panel, st };
        bar.wire();
        let (p, st) = (bar.panel, bar.st.clone());
        super::theme::subscribe(move |t| {
            st.borrow_mut().look = t.look();
            p.refresh(false, None);
        });
        bar
    }

    pub fn panel(&self) -> Panel {
        self.panel
    }

    pub fn on_event(&self, f: impl Fn(MenuEvent) + 'static) {
        self.st.borrow_mut().handler = Some(Rc::new(f));
    }

    pub fn set_titles(&self, titles: Vec<String>) {
        self.st.borrow_mut().titles = titles;
        self.panel.refresh(false, None);
    }

    /// Where menu `i`'s popup goes (screen coordinates) and how many menus there are; usable while the
    /// application is busy showing one.
    pub fn locator(&self) -> (Box<dyn Fn(i32) -> (i32, i32)>, i32) {
        let (st, panel) = (self.st.clone(), self.panel);
        let n = st.borrow().titles.len() as i32;
        let at = move |i: i32| {
            let (x, y) = st.borrow().rects.get(i as usize).map_or((0, HEIGHT), |r| (r.l, r.b));
            let p = panel.client_to_screen(Point::new(x, y));
            (p.x, p.y)
        };
        (Box::new(at), n)
    }

    fn wire(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, &p));

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_motion(move |e| {
            let (x, y) = pos(&e);
            let changed = {
                let mut s = st.borrow_mut();
                let hit = s.rects.iter().position(|r| r.has(x, y));
                s.pointer.hover(hit)
            };
            if changed {
                p.refresh(false, None);
            }
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_leave(move |_| {
            let changed = st.borrow_mut().pointer.leave();
            if changed {
                p.refresh(false, None);
            }
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            let hit = st.borrow().rects.iter().position(|r| r.has(x, y));
            if let Some(i) = hit {
                let r = st.borrow().rects[i];
                let h = st.borrow().handler.clone();
                if let Some(h) = h {
                    h(MenuEvent::Open(i, r.l, r.b));
                }
                p.refresh(false, None);
            }
        });
    }
}

fn paint(st: &Rc<RefCell<State>>, panel: &Panel) {
    let dc = AutoBufferedPaintDC::new(panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let size = panel.get_client_size();
    fill(&dc, Rect::new(0, 0, size.width, size.height), lk.base());
    let widths: Vec<i32> = s.titles.iter().map(|t| text_w(&dc, t)).collect();
    s.rects = layout(&widths, size.height);
    for (i, (label, r)) in s.titles.iter().zip(&s.rects).enumerate() {
        if s.pointer.hot() == Some(i) {
            wash(&dc, *r, mix(lk.base(), lk.ink(), 0x1C), 0.0);
        }
        text_at(&dc, label, *r, lk.fg(), true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_laid_out_left_to_right_without_gaps() {
        let r = layout(&[30, 40, 50], HEIGHT);
        assert_eq!(r[0].l, LEFT);
        assert_eq!(r[0].w(), 30 + PAD);
        assert_eq!(r[1].l, r[0].r);
        assert_eq!(r[2].l, r[1].r);
        assert!(r.iter().all(|x| x.t == 2 && x.b == HEIGHT - 2));
    }

    #[test]
    fn no_titles_no_rectangles() {
        assert!(layout(&[], HEIGHT).is_empty());
    }
}
