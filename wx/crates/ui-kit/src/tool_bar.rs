//! A row of icon buttons and separators, with an optional notice (the update pill) at the right.
//!
//! The control knows nothing about commands beyond an id it hands back in a [`ToolEvent`]. Buttons can be
//! marked checked from outside.

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::geom::Rect;
use super::paint::{ACCENT, Look, Rgb, WHITE, fill, mix, outline, pos, text_at, text_w, wash};
use super::pointer::Pointer;

pub const HEIGHT: i32 = 34;
const ICON: i32 = 18;
const BTN: i32 = 28;
const LEFT: i32 = 6;
const SEP_W: i32 = 8;

/// What goes in the row.
pub enum Item {
    Separator,
    /// aded is the icon shown while the command is not available (when absent the normal icon is used).
    Button { cmd: usize, tip: String, icon: Option<Bitmap>, faded: Option<Bitmap> },
}

/// A coloured notice at the right edge that acts like a button.
#[derive(Clone)]
pub struct Pill {
    pub text: String,
    pub colour: Rgb,
    pub cmd: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolEvent {
    Command(usize),
}

/// Where each item goes: a button is `BTN` wide with a 1px gap, a separator `SEP_W` wide. `is_button` says which.
pub fn layout(is_button: &[bool]) -> Vec<Rect> {
    let mut x = LEFT;
    is_button
        .iter()
        .map(|b| {
            if *b {
                let r = Rect::new(x, 3, x + BTN, 3 + BTN);
                x += BTN + 1;
                r
            } else {
                let r = Rect::new(x, 3, x + SEP_W, 3 + BTN);
                x += SEP_W;
                r
            }
        })
        .collect()
}

/// The notice's rectangle for a text `text_w` wide in a bar `bar_w` wide.
pub fn pill_rect(text_w: i32, bar_w: i32) -> Rect {
    let wd = text_w + 26;
    Rect::new(bar_w - wd - 10, 6, bar_w - 10, 3 + BTN - 3)
}

struct State {
    look: Look,
    font: Option<Font>,
    items: Vec<Item>,
    rects: Vec<Rect>,
    checked: Vec<usize>,
    disabled: Vec<usize>,
    pill: Option<(Pill, Rect)>,
    pointer: Pointer,
    handler: Option<Rc<dyn Fn(ToolEvent)>>,
}

impl State {
    fn button_at(&self, x: i32, y: i32) -> Option<usize> {
        self.items.iter().zip(&self.rects).position(|(it, r)| matches!(it, Item::Button { cmd, .. } if !self.disabled.contains(cmd)) && r.has(x, y))
    }
}

#[derive(Clone)]
pub struct ToolBar {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl ToolBar {
    pub fn new(parent: &Frame, items: Vec<Item>) -> ToolBar {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, HEIGHT));
        let kinds: Vec<bool> = items.iter().map(|i| matches!(i, Item::Button { .. })).collect();
        let st = Rc::new(RefCell::new(State {
            look: super::theme::current().look(),
            font: Font::builder().with_face_name("Segoe UI").with_point_size(9).build(),
            rects: layout(&kinds),
            items,
            checked: Vec::new(),
            disabled: Vec::new(),
            pill: None,
            pointer: Pointer::default(),
            handler: None,
        }));
        let bar = ToolBar { panel, st };
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

    pub fn on_event(&self, f: impl Fn(ToolEvent) + 'static) {
        self.st.borrow_mut().handler = Some(Rc::new(f));
    }

    pub fn set_pill(&self, pill: Option<Pill>) {
        self.st.borrow_mut().pill = pill.map(|p| (p, Rect::default()));
        self.panel.refresh(false, None);
    }

    /// A disabled button is drawn faded and ignores the pointer.
    pub fn set_enabled(&self, cmd: usize, on: bool) {
        {
            let mut s = self.st.borrow_mut();
            let off = s.disabled.contains(&cmd);
            if on != off {
                return;
            }
            if on {
                s.disabled.retain(|c| *c != cmd);
            } else {
                s.disabled.push(cmd);
            }
        }
        self.panel.refresh(false, None);
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
        self.panel.refresh(false, None);
    }

    fn emit(st: &Rc<RefCell<State>>, e: ToolEvent) {
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
            let (changed, tip) = {
                let mut s = st.borrow_mut();
                let hit = s.button_at(x, y);
                let changed = s.pointer.hover(hit);
                let tip = hit.and_then(|i| if let Item::Button { tip, .. } = &s.items[i] { Some(tip.clone()) } else { None });
                (changed, tip)
            };
            if changed {
                p.set_tooltip(tip.as_deref().unwrap_or(""));
                p.refresh(false, None);
            }
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_leave(move |_| {
            if st.borrow_mut().pointer.leave() {
                p.refresh(false, None);
            }
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_down(move |e| {
            let (x, y) = pos(&e);
            {
                let mut s = st.borrow_mut();
                let hit = s.button_at(x, y);
                s.pointer.press(hit);
            }
            p.refresh(false, None);
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_left_up(move |e| {
            let (x, y) = pos(&e);
            let pill = st.borrow().pill.as_ref().filter(|(_, r)| r.has(x, y)).map(|(p, _)| p.cmd);
            if let Some(cmd) = pill {
                return Self::emit(&st, ToolEvent::Command(cmd));
            }
            let cmd = {
                let mut s = st.borrow_mut();
                let hit = s.button_at(x, y);
                s.pointer.release(hit).and_then(|i| if let Item::Button { cmd, .. } = &s.items[i] { Some(*cmd) } else { None })
            };
            p.refresh(false, None);
            if let Some(cmd) = cmd {
                Self::emit(&st, ToolEvent::Command(cmd));
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
    if let Some((pill, rect)) = s.pill.as_mut() {
        *rect = pill_rect(text_w(&dc, &pill.text), size.width);
        fill(&dc, *rect, pill.colour);
        text_at(&dc, &pill.text, *rect, WHITE, true);
    }
    for (i, (item, r)) in s.items.iter().zip(&s.rects).enumerate() {
        match item {
            Item::Separator => fill(&dc, Rect::new(r.l + 3, r.t + 6, r.l + 4, r.b - 6), mix(lk.base(), lk.ink(), 0x40)),
            Item::Button { cmd, icon, faded, .. } => {
                let off = s.disabled.contains(cmd);
                if off {
                    // no hover or press effect on a command that cannot run
                } else if s.pointer.down() == Some(i) {
                    wash(&dc, *r, mix(lk.base(), lk.ink(), 0x30), 0.0);
                } else if s.pointer.hot() == Some(i) {
                    wash(&dc, *r, mix(lk.base(), lk.ink(), 0x18), 0.0);
                }
                if s.checked.contains(cmd) {
                    fill(&dc, *r, mix(lk.base(), ACCENT, 0x40));
                    outline(&dc, *r, mix(lk.base(), ACCENT, 0xB0));
                }
                if let Some(b) = if off { faded.as_ref().or(icon.as_ref()) } else { icon.as_ref() } {
                    dc.draw_bitmap(b, r.l + (BTN - ICON) / 2, r.t + (BTN - ICON) / 2, true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_and_separators_advance_by_their_own_width() {
        let r = layout(&[true, true, false, true]);
        assert_eq!(r[0], Rect::new(LEFT, 3, LEFT + BTN, 3 + BTN));
        assert_eq!(r[1].l, r[0].r + 1);
        assert_eq!(r[2].l, r[1].r + 1);
        assert_eq!(r[2].w(), SEP_W);
        assert_eq!(r[3].l, r[2].r);
    }

    #[test]
    fn no_items_no_rectangles() {
        assert!(layout(&[]).is_empty());
    }

    #[test]
    fn the_pill_hugs_the_right_edge_and_grows_with_its_text() {
        let a = pill_rect(40, 1000);
        let b = pill_rect(80, 1000);
        assert_eq!(a.r, 990);
        assert_eq!(b.r, 990);
        assert_eq!(b.w() - a.w(), 40);
        assert!(a.l > 0 && a.h() > 0);
    }
}
