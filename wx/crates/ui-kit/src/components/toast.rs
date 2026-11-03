//! Toasts: short notices in the bottom-right corner of a window that go away by themselves (or when clicked). For
//! things the user should know but need not decide, such as a file that could not be saved. They never take the
//! keyboard. Newer toasts stack above older ones; at most three are shown.
//!
//! ```ignore
//! let toasts = Toaster::new(&frame);
//! toasts.show("Cannot save notes.txt\n\nAccess is denied.", Tone::Error);
//! ```

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use wxdragon::dc::{AutoBufferedPaintDC, ClientDC, DeviceContext};
use wxdragon::prelude::*;

use crate::geom::{Rect, Rgb, mix};
use crate::paint::{fill, outline, text_w};
use crate::theme::{self, Theme};

/// What a toast is about, which picks its accent colour and how long it stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Error,
}

/// Sizes before scaling: the toast's width, inner padding, the accent bar, the gap between toasts and to the window edge.
const WIDTH: i32 = 360;
const PAD: i32 = 12;
const BAR: i32 = 4;
const GAP: i32 = 8;
const MARGIN: i32 = 16;
/// Room on the right for the close mark.
const CLOSE_W: i32 = 20;
const MAX_SHOWN: usize = 3;
const INFO_MS: i32 = 5000;
const ERROR_MS: i32 = 10_000;

struct Shown {
    frame: Frame,
    height: i32,
    _timer: Timer<Frame>,
}

/// The toasts of one window. Cheap to clone; clones share the toasts.
#[derive(Clone)]
pub struct Toaster {
    owner: Frame,
    bottom: Rc<Cell<i32>>,
    shown: Rc<RefCell<Vec<Shown>>>,
}

impl std::fmt::Debug for Toaster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Toaster").field("shown", &self.shown.borrow().len()).finish_non_exhaustive()
    }
}

impl Toaster {
    pub fn new(owner: &Frame) -> Toaster {
        Toaster { owner: *owner, bottom: Rc::new(Cell::new(0)), shown: Rc::default() }
    }

    /// Keep toasts clear of the bottom `px` pixels of the window's client area (a status bar).
    pub fn set_bottom_inset(&self, px: i32) {
        self.bottom.set(px);
    }

    /// Show `text`; its first paragraph is the headline. Paragraphs are separated by a blank line.
    pub fn show(&self, text: &str, tone: Tone) {
        let t = theme::current();
        let frame = Frame::builder().with_parent(&self.owner).with_title(text).with_style(FrameStyle::FloatOnParent | FrameStyle::NoTaskbar).build();
        let panel = Panel::builder(&frame).build();
        panel.set_background_style(BackgroundStyle::Paint);
        panel.set_can_focus(false);
        let lines = wrap(panel, text, t.px(WIDTH) - 2 * t.px(PAD) - t.px(BAR) - t.px(CLOSE_W));
        let height = 2 * t.px(PAD) + lines.iter().map(|l| l.height).sum::<i32>();
        frame.set_client_size(Size::new(t.px(WIDTH), height));
        panel.set_size(Size::new(t.px(WIDTH), height));
        panel.on_paint(move |_| paint(panel, &lines, tone));
        let me = self.clone();
        panel.on_mouse_left_down(move |_| me.dismiss_later(frame));
        let timer = Timer::new(&frame);
        let me = self.clone();
        timer.on_tick(move |_| me.dismiss_later(frame));
        timer.start(if tone == Tone::Error { ERROR_MS } else { INFO_MS }, true);
        let old = {
            let mut shown = self.shown.borrow_mut();
            shown.push(Shown { frame, height, _timer: timer });
            if shown.len() > MAX_SHOWN { Some(shown.remove(0)) } else { None }
        };
        if let Some(old) = old {
            old.frame.destroy();
        }
        self.arrange();
        if !crate::native::current().show_without_activating(frame.get_handle() as isize) {
            frame.show(true);
        }
    }

    /// Close every toast (the window is closing or the notices are stale).
    pub fn clear(&self) {
        for s in self.shown.borrow_mut().drain(..) {
            s.frame.destroy();
        }
    }

    /// Close `frame` after the current event: it is asked from the toast's own handlers.
    fn dismiss_later(&self, frame: Frame) {
        let me = self.clone();
        crate::defer::later(&self.owner, move || {
            let gone = {
                let mut shown = me.shown.borrow_mut();
                shown.iter().position(|s| s.frame.get_handle() == frame.get_handle()).map(|i| shown.remove(i))
            };
            if let Some(s) = gone {
                s.frame.destroy();
                me.arrange();
            }
        });
    }

    /// Stack the toasts up from the window's bottom-right corner, newest lowest.
    fn arrange(&self) {
        let t = theme::current();
        let area = self.owner.get_client_size();
        let corner = self.owner.client_to_screen(Point::new(area.width - t.px(MARGIN), area.height - self.bottom.get() - t.px(MARGIN)));
        let mut y = corner.y;
        for s in self.shown.borrow().iter().rev() {
            y -= s.height;
            s.frame.move_window(corner.x - t.px(WIDTH), y);
            y -= t.px(GAP);
        }
    }
}

/// One line of a toast: its text, whether it is the headline, and its height.
struct Line {
    text: String,
    strong: bool,
    height: i32,
}

/// Break `text` into lines no wider than `width`: paragraphs on blank lines, words where they fit, and long words
/// (paths) by characters.
fn wrap(panel: Panel, text: &str, width: i32) -> Vec<Line> {
    let t = theme::current();
    let dc = ClientDC::new(&panel);
    let mut lines = Vec::new();
    for (i, para) in text.split("\n\n").enumerate() {
        let strong = i == 0;
        if let Some(f) = if strong { t.strong_font() } else { t.ui_font() } {
            dc.set_font(&f);
        }
        let height = dc.get_text_extent("Ag").1;
        if i > 0 {
            lines.push(Line { text: String::new(), strong, height: height / 2 });
        }
        for raw in para.lines() {
            for l in fit(raw, width, &|s| text_w(&dc, s)) {
                lines.push(Line { text: l, strong, height });
            }
        }
    }
    lines
}

/// Greedy line filling of one source line, measuring with `w`.
fn fit(raw: &str, width: i32, w: &dyn Fn(&str) -> i32) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in raw.split(' ') {
        let candidate = if cur.is_empty() { word.to_string() } else { format!("{cur} {word}") };
        if w(&candidate) <= width {
            cur = candidate;
            continue;
        }
        if !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        for c in word.chars() {
            if w(&format!("{cur}{c}")) > width && !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.push(c);
        }
    }
    out.push(cur);
    out
}

fn paint(panel: Panel, lines: &[Line], tone: Tone) {
    let t = theme::current();
    let dc = AutoBufferedPaintDC::new(&panel);
    let size = panel.get_client_size();
    let all = Rect::new(0, 0, size.width, size.height);
    fill(&dc, all, raised(&t));
    outline(&dc, all, t.colors.border);
    fill(&dc, Rect::new(0, 0, t.px(BAR), size.height), accent(&t, tone));
    let x = t.px(BAR) + t.px(PAD);
    let mut y = t.px(PAD);
    for l in lines {
        if let Some(f) = if l.strong { t.strong_font() } else { t.ui_font() } {
            dc.set_font(&f);
        }
        dc.set_text_foreground(crate::paint::colour(if l.strong { t.colors.text } else { t.colors.text_muted }));
        dc.draw_text(&l.text, x, y);
        y += l.height;
    }
    dc.set_text_foreground(crate::paint::colour(t.colors.text_muted));
    dc.draw_text("\u{00D7}", size.width - t.px(PAD) - t.px(CLOSE_W) / 2, t.px(PAD));
}

/// A toast sits slightly lifted off the window.
fn raised(t: &Theme) -> Rgb {
    mix(t.colors.surface, t.colors.ink, 0x0A)
}

fn accent(t: &Theme, tone: Tone) -> Rgb {
    match tone {
        Tone::Info => t.colors.info,
        Tone::Error => t.colors.danger,
    }
}

#[cfg(test)]
mod tests {
    use super::fit;

    /// Each character is 10 pixels wide.
    fn w(s: &str) -> i32 {
        i32::try_from(s.chars().count()).unwrap_or(i32::MAX) * 10
    }

    #[test]
    fn words_fill_lines_up_to_the_width() {
        assert_eq!(fit("one two three four", 90, &w), vec!["one two", "three", "four"]);
    }

    #[test]
    fn a_word_wider_than_the_line_breaks_by_characters() {
        assert_eq!(fit("C:\\very\\long\\path", 60, &w), vec!["C:\\ver", "y\\long", "\\path"]);
    }

    #[test]
    fn short_text_is_one_line() {
        assert_eq!(fit("Saved", 200, &w), vec!["Saved"]);
    }
}
