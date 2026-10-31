//! Find / Replace: a modeless dialog over the core's search functions.

use std::cell::RefCell;

use notepp_core::search::{self, Found, Opts};
use wxdragon::prelude::*;

use crate::app::Shared;

#[derive(Clone, Copy)]
struct Controls {
    dialog: Dialog,
    find: TextCtrl,
    replace: TextCtrl,
    case: CheckBox,
    word: CheckBox,
    regex: CheckBox,
    extended: CheckBox,
    wrap: CheckBox,
    dot_all: CheckBox,
    in_selection: CheckBox,
}

thread_local! {
    static OPEN: RefCell<Option<Controls>> = const { RefCell::new(None) };
}

impl Controls {
    fn opts(&self) -> Opts {
        Opts {
            find: self.find.get_value(),
            replace: self.replace.get_value(),
            match_case: self.case.is_checked(),
            whole_word: self.word.is_checked(),
            regex: self.regex.is_checked(),
            wrap: self.wrap.is_checked(),
            dot_all: self.dot_all.is_checked(),
            extended: self.extended.is_checked(),
            in_selection: self.in_selection.is_checked(),
        }
    }
}

#[derive(Clone, Copy)]
enum Action {
    Next,
    Prev,
    Replace,
    ReplaceAll,
    Count,
    MarkAll,
    ClearMarks,
}

fn run(app: &Shared, c: &Controls, action: Action) {
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let o = c.opts();
    if o.find.is_empty() && !matches!(action, Action::ClearMarks) {
        a.flash = Some("Type something to find".into());
        return a.refresh();
    }
    a.ws.search = o.clone();
    let sci = a.ws.sci;
    let say = |found: Found| match found {
        Found::At { wrapped } => if wrapped { "Found (wrapped around)".to_string() } else { "Found".to_string() },
        Found::NotFound => "Not found".to_string(),
        Found::BadPattern(e) => format!("Bad pattern: {e}"),
    };
    let msg = match action {
        Action::Next => say(search::find(&sci, &o, true)),
        Action::Prev => say(search::find(&sci, &o, false)),
        Action::Replace => say(search::replace_current(&sci, &o).1),
        Action::ReplaceAll => match search::replace_all(&sci, &o) {
            Ok(n) => format!("Replaced {n} occurrence(s)"),
            Err(e) => format!("Bad pattern: {e}"),
        },
        Action::Count => match search::count(&sci, &o) {
            Ok(n) => format!("{n} match(es)"),
            Err(e) => format!("Bad pattern: {e}"),
        },
        Action::MarkAll => match search::mark_all(&sci, &o, search::INDIC_FIND) {
            Ok(n) => format!("Marked {n} match(es)"),
            Err(e) => format!("Bad pattern: {e}"),
        },
        Action::ClearMarks => {
            search::clear_marks(&sci, search::INDIC_FIND);
            "Marks cleared".to_string()
        }
    };
    a.flash = Some(msg);
    a.refresh();
}

/// Show the dialog (creating it the first time), prefilled with `initial` when that is not empty.
pub fn open(app: &Shared, replace: bool, initial: &str) {
    if let Some(c) = OPEN.with(|o| *o.borrow()) {
        if !initial.is_empty() {
            c.find.set_value(initial);
        }
        c.dialog.show(true);
        c.dialog.raise();
        c.find.set_focus();
        c.find.select_all();
        return;
    }
    let frame = app.borrow().frame;
    let dialog = Dialog::builder(&frame, if replace { "Replace" } else { "Find" }).with_size(560, 330).build();
    let label = |text: &str| StaticText::builder(&dialog).with_label(text).build();

    let find = TextCtrl::builder(&dialog).build();
    let replace_box = TextCtrl::builder(&dialog).build();
    let check = |text: &str, on: bool| {
        let c = CheckBox::builder(&dialog).with_label(text).build();
        c.set_value(on);
        c
    };
    let case = check("Match &case", false);
    let word = check("Match &whole word only", false);
    let regex = check("Regular e&xpression", false);
    let extended = check("E&xtended (\\n, \\t, \\0, ...)", false);
    let wrap = check("Wrap ar&ound", true);
    let dot_all = check("&. matches newline", false);
    let in_selection = check("In &selection", false);

    let button = |text: &str| Button::builder(&dialog).with_label(text).build();
    let next = button("Find &Next");
    let prev = button("Find &Previous");
    let repl = button("&Replace");
    let repl_all = button("Replace &All");
    let count = button("Coun&t");
    let mark = button("&Mark All");
    let clear = button("C&lear Marks");
    let close = button("Close");

    let root = BoxSizer::builder(Orientation::Horizontal).build();
    let left = BoxSizer::builder(Orientation::Vertical).build();
    left.add(&label("Find what:"), 0, SizerFlag::All, 4);
    left.add(&find, 0, SizerFlag::Expand | SizerFlag::All, 4);
    left.add(&label("Replace with:"), 0, SizerFlag::All, 4);
    left.add(&replace_box, 0, SizerFlag::Expand | SizerFlag::All, 4);
    for c in [&case, &word, &regex, &extended, &wrap, &dot_all, &in_selection] {
        left.add(c, 0, SizerFlag::All, 3);
    }
    let right = BoxSizer::builder(Orientation::Vertical).build();
    for b in [&next, &prev, &repl, &repl_all, &count, &mark, &clear, &close] {
        right.add(b, 0, SizerFlag::Expand | SizerFlag::All, 4);
    }
    root.add_sizer(&left, 1, SizerFlag::Expand | SizerFlag::All, 8);
    root.add_sizer(&right, 0, SizerFlag::All, 8);
    dialog.set_sizer(root, true);

    let c = Controls { dialog, find, replace: replace_box, case, word, regex, extended, wrap, dot_all, in_selection };
    for (b, action) in [(next, Action::Next), (prev, Action::Prev), (repl, Action::Replace), (repl_all, Action::ReplaceAll), (count, Action::Count), (mark, Action::MarkAll), (clear, Action::ClearMarks)] {
        let a = app.clone();
        b.on_click(move |_| run(&a, &c, action));
    }
    close.on_click(move |_| c.dialog.show(false));
    OPEN.with(|o| *o.borrow_mut() = Some(c));
    if !initial.is_empty() {
        c.find.set_value(initial);
    }
    c.dialog.show(true);
    c.find.set_focus();
    c.find.select_all();
}

/// Find Next / Previous with the remembered options; opens the dialog when nothing was searched yet.
pub fn step(app: &Shared, forward: bool) {
    let (empty, initial) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.ws.search.find.is_empty(), search::selected_text(&a.ws.sci))
    };
    if empty {
        return open(app, false, &initial);
    }
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let o = a.ws.search.clone();
    let sci = a.ws.sci;
    a.flash = Some(match search::find(&sci, &o, forward) {
        Found::At { wrapped } => if wrapped { "Found (wrapped around)".to_string() } else { "Found".to_string() },
        Found::NotFound => "Not found".to_string(),
        Found::BadPattern(e) => format!("Bad pattern: {e}"),
    });
    a.refresh();
}

/// Go To Line: ask for a number and jump there.
pub fn goto_line(app: &Shared) {
    let (frame, lines) = {
        let Ok(a) = app.try_borrow() else { return };
        (a.frame, notepp_core::sci::line_count(&a.ws.sci).max(1) as usize)
    };
    let d = TextEntryDialog::builder(&frame, &format!("Line number (1 - {lines}):"), "Go To Line").build();
    if d.show_modal() == ID_OK {
        if let Some(n) = d.get_value().and_then(|v| v.trim().parse::<usize>().ok()) {
            if let Ok(a) = app.try_borrow() {
                search::goto_line(&a.ws.sci, n.clamp(1, lines.max(1)));
            }
        }
    }
}
