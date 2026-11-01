//! Incremental Search: a small modeless window; the editor jumps to the first match as you type. Enter / Next and
//! Prev step through matches, Esc closes it.

use std::cell::{Cell, RefCell};

use notepp_core::sci::{self, Send};
use notepp_core::search;
use wxdragon::prelude::*;

use crate::app::Shared;

#[derive(Clone, Copy)]
struct Win {
    dialog: Dialog,
    edit: TextCtrl,
    status: StaticText,
}

thread_local! {
    static WIN: RefCell<Option<Win>> = const { RefCell::new(None) };
    /// Where the editor's selection started when the search was opened.
    static START: Cell<usize> = const { Cell::new(0) };
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Changed,
    Next,
    Prev,
}

fn step(app: &Shared, kind: Kind) {
    let Some(w) = WIN.with(|w| *w.borrow()) else { return };
    let Ok(a) = app.try_borrow() else { return };
    let sci = a.ws.sci;
    let text = w.edit.get_value();
    let start = START.with(|c| c.get());
    if text.is_empty() {
        sci.send(sci::SCI_SETSEL, start, start as isize);
        w.status.set_label("");
        return;
    }
    let o = search::Opts { find: text, wrap: true, ..Default::default() };
    let forward = kind != Kind::Prev;
    if kind == Kind::Changed {
        // Typing re-searches from where the search began.
        sci.send(sci::SCI_SETSEL, start, start as isize);
    }
    let before = (sci.send(sci::SCI_GETSELECTIONSTART, 0, 0), sci.send(sci::SCI_GETSELECTIONEND, 0, 0));
    match search::find(&sci, &o, forward) {
        search::Found::At { wrapped: true } => w.status.set_label("Reached the end, continued from the other end"),
        search::Found::At { wrapped: false } => w.status.set_label(""),
        _ => {
            sci.send(sci::SCI_SETSEL, before.0 as usize, before.1);
            w.status.set_label("Phrase not found");
        }
    }
}

/// Show the window (creating it on first use) with an empty box.
pub fn open(app: &Shared) {
    let (frame, sel) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, a.ws.sci.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize)
    };
    START.with(|c| c.set(sel));
    let existing = WIN.with(|w| *w.borrow());
    let w = match existing {
        Some(w) => w,
        None => {
            let d = Dialog::builder(&frame, "Incremental Search").build();
            d.set_client_size(Size::new(360, 136));
            let fp = frame.get_position();
            let fs = frame.get_size();
            d.move_window(fp.x + fs.width - 400, fp.y + 110);
            let at = |x, y, w, h| (Point::new(x, y), Size::new(w, h));
            let (p, s) = at(14, 12, 332, 18);
            StaticText::builder(&d).with_label("Search for:").with_pos(p).with_size(s).build();
            let (p, s) = at(14, 34, 332, 26);
            let edit = TextCtrl::builder(&d).with_pos(p).with_size(s).with_style(TextCtrlStyle::ProcessEnter).build();
            let (p, s) = at(14, 68, 332, 18);
            let status = StaticText::builder(&d).with_label("").with_pos(p).with_size(s).build();
            let button = |label: &str, x: i32| {
                let (p, s) = at(x, 96, 80, 26);
                Button::builder(&d).with_label(label).with_pos(p).with_size(s).build()
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
            let w = Win { dialog: d, edit, status };
            WIN.with(|c| *c.borrow_mut() = Some(w));
            w
        }
    };
    w.edit.set_value("");
    w.status.set_label("");
    w.dialog.show(true);
    w.dialog.raise();
    w.edit.set_focus();
}
