//! The menu bar: titles that open the application's menus.
//!
//! The control only draws the titles and reports which one was clicked and where its popup belongs; the
//! application shows the actual menu.
//!
//! ```ignore
//! let bar = MenuBar::new(&frame);
//! bar.set_titles(vec!["File".into(), "Edit".into(), "View".into()]);
//! bar.on_event(|MenuEvent::Open(index, x, y)| { /* show menu `index` at (x, y) */ });
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, DeviceContext};
use wxdragon::prelude::*;

use super::a11y;
use crate::geom::Rect;
use crate::paint::{Look, fill, mix, pos, text_at, text_w, wash};
use crate::pointer::Pointer;

/// Sizes before scaling (see [`crate::theme::Theme::px`]).
pub const HEIGHT: i32 = 26;
const LEFT: i32 = 6;
const PAD: i32 = 20;
/// Space above and below a title's highlight.
const INSET: i32 = 2;
/// How far the mnemonic underline sits above the bottom.
const UNDERLINE: i32 = 5;

/// Where each title goes: left to right from the left margin, `PAD` wider than its text.
pub fn layout(text_widths: &[i32], height: i32) -> Vec<Rect> {
    let t = crate::theme::current();
    let (pad, inset) = (t.px(PAD), t.px(INSET));
    let mut x = t.px(LEFT);
    text_widths
        .iter()
        .map(|w| {
            let r = Rect::new(x, inset, x + w + pad, height - inset);
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
    /// The menu that is showing: moving over another title switches to it.
    open: Option<usize>,
    handler: Option<Rc<dyn Fn(MenuEvent)>>,
    reader: a11y::Items,
}

/// Show the screen reader what was just painted.
fn publish(s: &State) {
    s.reader.publish(s.titles.iter().zip(&s.rects).map(|(t, r)| a11y::Item::new(&split_mnemonic(t).0, a11y::Role::Menu, *r)).collect());
}

impl std::fmt::Debug for MenuBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MenuBar").finish_non_exhaustive()
    }
}

/// Where menu `i`'s popup goes, as a function of `i`.
pub type Locator = Box<dyn Fn(i32) -> (i32, i32)>;

#[derive(Clone)]
pub struct MenuBar {
    panel: Panel,
    st: Rc<RefCell<State>>,
}

impl MenuBar {
    pub fn new(parent: &Frame) -> MenuBar {
        let panel = Panel::builder(parent).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_min_size(Size::new(-1, crate::theme::current().px(HEIGHT)));
        let st = Rc::new(RefCell::new(State {
            look: crate::theme::current().look(),
            font: crate::theme::current().ui_font(),
            titles: Vec::new(),
            rects: Vec::new(),
            pointer: Pointer::default(),
            open: None,
            handler: None,
            reader: a11y::Items::default(),
        }));
        let bar = MenuBar { panel, st };
        bar.wire();
        let (p, st) = (bar.panel, bar.st.clone());
        crate::theme::subscribe(move |t| {
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

    /// Tell the bar which menu is showing (or none).
    pub fn set_open(&self, i: Option<usize>) {
        self.st.borrow_mut().open = i;
        self.panel.refresh(false, None);
    }

    pub fn titles(&self) -> Vec<String> {
        self.st.borrow().titles.clone()
    }

    /// Where menu `i`'s popup goes (screen coordinates) and how many menus there are; usable while the
    /// application is busy showing one.
    pub fn locator(&self) -> (Locator, i32) {
        let (st, panel) = (self.st.clone(), self.panel);
        let n = crate::num::to_i32(st.borrow().titles.len());
        let at = move |i: i32| {
            let (x, y) = st.borrow().rects.get(crate::num::to_usize(i)).map_or((0, crate::theme::current().px(HEIGHT)), |r| (r.l, r.b));
            let p = panel.client_to_screen(Point::new(x, y));
            (p.x, p.y)
        };
        (Box::new(at), n)
    }

    /// The pointer is at screen position (x, y) while something else holds the mouse (an open menu): with a menu
    /// showing, being over another title switches to it.
    pub fn pointer_moved(&self, x: i32, y: i32) {
        let c = self.panel.screen_to_client(Point::new(x, y));
        moved(&self.st, self.panel, c.x, c.y);
    }

    fn wire(&self) {
        let (p, st) = (self.panel, self.st.clone());
        p.on_paint(move |_| paint(&st, p));

        let (st, items) = (self.st.clone(), self.st.borrow().reader.clone());
        a11y::attach(self.panel, "Menu bar", a11y::Bar::Menus, items, move |i| {
            let (r, h) = {
                let s = st.borrow();
                (s.rects.get(i).copied(), s.handler.clone())
            };
            if let (Some(r), Some(h)) = (r, h) {
                h(MenuEvent::Open(i, r.l, r.b));
            }
        });

        let (p, st) = (self.panel, self.st.clone());
        p.on_mouse_motion(move |e| {
            let (x, y) = pos(&e);
            moved(&st, p, x, y);
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

/// Hover state, and the switch to another menu when one is showing.
fn moved(st: &Rc<RefCell<State>>, p: Panel, x: i32, y: i32) {
    let (changed, switch) = {
        let mut s = st.borrow_mut();
        let hit = s.rects.iter().position(|r| r.has(x, y));
        let switch = hit.filter(|h| s.open.is_some_and(|o| o != *h)).map(|h| (h, s.rects[h]));
        if let Some((h, _)) = switch {
            s.open = Some(h);
        }
        (s.pointer.hover(hit), switch)
    };
    if changed {
        p.refresh(false, None);
    }
    let handler = st.borrow().handler.clone();
    if let (Some((h, r)), Some(handler)) = (switch, handler) {
        handler(MenuEvent::Open(h, r.l, r.b));
    }
}

/// "E&ncoding" -> ("Encoding", Some(1)): the title without the marker and the index of the underlined letter.
pub fn split_mnemonic(title: &str) -> (String, Option<usize>) {
    let mut clean = String::new();
    let mut key = None;
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.peek() {
                Some('&') => {
                    chars.next();
                    clean.push('&');
                }
                Some(_) if key.is_none() => key = Some(clean.chars().count()),
                _ => {}
            }
        } else {
            clean.push(c);
        }
    }
    (clean, key)
}

/// The menu whose mnemonic is `key` (case-insensitive).
pub fn menu_for_key(titles: &[String], key: char) -> Option<usize> {
    titles.iter().position(|t| {
        let (clean, at) = split_mnemonic(t);
        at.and_then(|i| clean.chars().nth(i)).is_some_and(|c| c.eq_ignore_ascii_case(&key))
    })
}

fn paint(st: &Rc<RefCell<State>>, panel: Panel) {
    let dc = AutoBufferedPaintDC::new(&panel);
    let mut s = st.borrow_mut();
    let lk = s.look;
    if let Some(f) = &s.font {
        dc.set_font(f);
    }
    let size = panel.get_client_size();
    fill(&dc, Rect::new(0, 0, size.width, size.height), lk.base());
    let parts: Vec<(String, Option<usize>)> = s.titles.iter().map(|t| split_mnemonic(t)).collect();
    let widths: Vec<i32> = parts.iter().map(|(t, _)| text_w(&dc, t)).collect();
    s.rects = layout(&widths, size.height);
    for (i, ((label, key), r)) in parts.iter().zip(&s.rects).enumerate() {
        if s.pointer.hot() == Some(i) || s.open == Some(i) {
            wash(&dc, *r, mix(lk.base(), lk.ink(), 0x1C), 0.0);
        }
        text_at(&dc, label, *r, lk.fg(), true);
        if let Some(k) = key {
            let before: String = label.chars().take(*k).collect();
            let letter: String = label.chars().skip(*k).take(1).collect();
            let x = r.l + (r.w() - text_w(&dc, label)) / 2 + text_w(&dc, &before);
            let y = r.b - crate::theme::current().px(UNDERLINE);
            fill(&dc, Rect::new(x, y, x + text_w(&dc, &letter), y + 1), lk.fg());
        }
    }
    publish(&s);
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

    #[test]
    fn the_marker_names_the_underlined_letter() {
        assert_eq!(split_mnemonic("E&ncoding"), ("Encoding".to_string(), Some(1)));
        assert_eq!(split_mnemonic("File"), ("File".to_string(), None));
        assert_eq!(split_mnemonic("A&&B"), ("A&B".to_string(), None));
    }

    #[test]
    fn alt_plus_a_letter_finds_its_menu() {
        let t: Vec<String> = ["&File", "&Edit", "E&ncoding", "Set&tings"].iter().map(ToString::to_string).collect();
        assert_eq!(menu_for_key(&t, 'f'), Some(0));
        assert_eq!(menu_for_key(&t, 'N'), Some(2));
        assert_eq!(menu_for_key(&t, 't'), Some(3));
        assert_eq!(menu_for_key(&t, 'z'), None);
    }
}
