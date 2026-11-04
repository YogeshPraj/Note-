//! Incremental Search: a small modeless window; the editor jumps to the first match as you type. Enter / Next and
//! Prev step through matches, Esc closes it.

use notepp_core::sci::{self, Send};
use notepp_core::search;
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::widgets::controls::{Button, Dialog, Label, TextBox};

/// The window, made on first use, and where the editor's selection started when it was opened. The app keeps it.
#[derive(Debug, Clone, Copy)]
pub struct Search {
    win: Win,
    start: usize,
}

#[derive(Debug, Clone, Copy)]
struct Win {
    dialog: Dialog,
    edit: TextBox,
    status: Label,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Changed,
    Next,
    Prev,
}

fn step(app: &Shared, kind: Kind) {
    let Ok(a) = app.try_borrow() else { return };
    let Some(Search { win: w, start }) = a.incr else { return };
    let sci = a.ws.sci;
    let text = w.edit.get_value();
    if text.is_empty() {
        sci.send(sci::SCI_SETSEL, start, crate::platform::wx::to_isize(start));
        w.status.set_label("");
        return;
    }
    let o = search::Opts { find: text, wrap: true, ..Default::default() };
    let forward = kind != Kind::Prev;
    if kind == Kind::Changed {
        // Typing re-searches from where the search began.
        sci.send(sci::SCI_SETSEL, start, crate::platform::wx::to_isize(start));
    }
    let before = (sci.send(sci::SCI_GETSELECTIONSTART, 0, 0), sci.send(sci::SCI_GETSELECTIONEND, 0, 0));
    match search::find(&sci, &o, forward) {
        search::Found::At { wrapped: true } => w.status.set_label("Reached the end, continued from the other end"),
        search::Found::At { wrapped: false } => w.status.set_label(""),
        _ => {
            sci.send(sci::SCI_SETSEL, crate::platform::wx::to_usize(before.0), before.1);
            w.status.set_label("Phrase not found");
        }
    }
}

/// Show the window (creating it on first use) with an empty box.
pub fn open(app: &Shared) {
    let (frame, start, existing) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, crate::platform::wx::to_usize(a.ws.sci.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0)), a.incr.map(|s| s.win))
    };
    let w = existing.unwrap_or_else(|| make(app, &frame));
    if let Ok(mut a) = app.try_borrow_mut() {
        a.incr = Some(Search { win: w, start });
    }
    w.edit.set_value("");
    w.status.set_label("");
    w.dialog.show(true);
    w.dialog.raise();
    w.edit.set_focus();
}

fn make(app: &Shared, frame: &Frame) -> Win {
    {
        let d = Dialog::new(frame, "Incremental Search");
        // Laid out at 100%, placed scaled to the display.
        let t = crate::widgets::theme::current();
        d.set_client_size(Size::new(t.px(360), t.px(136)));
        let fp = frame.get_position();
        let fs = frame.get_size();
        d.move_window(fp.x + fs.width - t.px(400), fp.y + t.px(110));
        let at = |x, y, w, h| (Point::new(t.px(x), t.px(y)), Size::new(t.px(w), t.px(h)));
        let (p, s) = at(14, 12, 332, 18);
        Label::new(&d, "Search for:").set_size_with_pos(p.x, p.y, s.width, s.height);
        let (p, s) = at(14, 34, 332, 26);
        let edit = TextBox::new(&d, "", true);
        edit.set_size_with_pos(p.x, p.y, s.width, s.height);
        let (p, s) = at(14, 68, 332, 18);
        let status = Label::new(&d, "");
        status.set_size_with_pos(p.x, p.y, s.width, s.height);
        let button = |label: &str, x: i32| {
            let (p, s) = at(x, 96, 80, 26);
            let b = Button::new(&d, label);
            b.set_size_with_pos(p.x, p.y, s.width, s.height);
            b
        };
        let prev = button("&Prev", 14);
        let next = button("&Next", 102);
        let close = button("Close", 266);
        next.set_default();
        let a = app.clone();
        edit.on_text_updated(move |_| step(&a, Kind::Changed));
        let a = app.clone();
        edit.on_enter_pressed(move |_| step(&a, Kind::Next));
        let a = app.clone();
        next.on_click(move |_| step(&a, Kind::Next));
        let a = app.clone();
        prev.on_click(move |_| step(&a, Kind::Prev));
        close.on_click(move |_| d.show(false));
        d.set_escape_id(ID_CANCEL);
        Win { dialog: d, edit, status }
    }
}
