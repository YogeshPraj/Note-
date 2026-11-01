//! The docked Find panel (Find / Replace / Find in Files / Mark), laid out like the Win32 shell's.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use notepp_core::search::Opts;
use wxdragon::prelude::*;

use crate::app::Shared;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Find = 0,
    Replace = 1,
    InFiles = 2,
    Mark = 3,
}

pub const RESULTS_H: i32 = 180;

#[derive(Clone, Copy)]
struct P {
    panel: Panel,
    tab: [ToggleButton; 4],
    closex: Button,
    status: StaticText,
    find_lbl: StaticText,
    find: TextCtrl,
    repl_lbl: StaticText,
    repl: TextCtrl,
    dir_lbl: StaticText,
    dir: TextCtrl,
    filt_lbl: StaticText,
    filt: TextCtrl,
    browse: Button,
    case: CheckBox,
    word: CheckBox,
    wrap: CheckBox,
    regex: CheckBox,
    dot: CheckBox,
    ext: CheckBox,
    insel: CheckBox,
    next: Button,
    prev: Button,
    findall: Button,
    count: Button,
    replace: Button,
    repl_all: Button,
    fif: Button,
    mark: Button,
    clear: Button,
    close: Button,
    list: ListBox,
}

thread_local! {
    static PANEL: RefCell<Option<P>> = const { RefCell::new(None) };
    static HOST: RefCell<Option<Panel>> = const { RefCell::new(None) };
    static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) };
    static MODE: Cell<Mode> = const { Cell::new(Mode::Find) };
    static SHOWN: Cell<bool> = const { Cell::new(false) };
    static RESULTS: Cell<bool> = const { Cell::new(false) };
}

/// The empty container, placed in the window's layout; the controls are added by [`wire`].
pub fn container(frame: &Frame) -> Panel {
    let p = Panel::builder(frame).build();
    p.show(false);
    HOST.with(|h| *h.borrow_mut() = Some(p));
    FRAME.with(|f| *f.borrow_mut() = Some(*frame));
    p
}

fn p() -> Option<P> {
    PANEL.with(|c| *c.borrow())
}

pub fn visible() -> bool {
    SHOWN.with(|c| c.get())
}

pub fn mode() -> Mode {
    MODE.with(|c| c.get())
}

fn results_visible() -> bool {
    RESULTS.with(|c| c.get()) && mode() != Mode::Mark
}

/// Recolour the panel for the current theme.
pub fn restyle() {
    use crate::theme::{self, Role};
    let Some(p) = p() else { return };
    theme::paint(&p.panel, Role::Surface);
    let surface: [&dyn WxWidget; 22] = [
        &p.find_lbl, &p.repl_lbl, &p.dir_lbl, &p.filt_lbl, &p.browse, &p.case, &p.word, &p.wrap, &p.regex, &p.dot, &p.ext, &p.insel, &p.next, &p.prev,
        &p.findall, &p.count, &p.replace, &p.repl_all, &p.fif, &p.mark, &p.clear, &p.close,
    ];
    for w in surface {
        theme::paint(w, Role::Surface);
    }
    for w in p.tab.iter() {
        theme::paint(w, Role::Surface);
    }
    theme::paint(&p.status, Role::Surface);
    theme::paint(&p.closex, Role::Surface);
    let input: [&dyn WxWidget; 5] = [&p.find, &p.repl, &p.dir, &p.filt, &p.list];
    for w in input {
        theme::paint(w, Role::Input);
    }
    theme::style_native(&p.panel);
}
/// Height the panel needs in the current mode.
pub fn height() -> i32 {
    if !visible() {
        return 0;
    }
    let base = 26
        + match mode() {
            Mode::Find | Mode::Mark => 108,
            Mode::Replace => 142,
            Mode::InFiles => 176,
        };
    base + if results_visible() { RESULTS_H } else { 0 }
}

fn place(w: &impl WxWidget, x: i32, y: i32, wd: i32, h: i32) {
    w.set_size_with_pos(x, y, wd, h);
}

fn show(w: &impl WxWidget, on: bool) {
    w.show(on);
}

/// Position the controls for the current mode and width.
pub fn arrange() {
    let Some(p) = p() else { return };
    // Colours set while the panel was hidden at start-up are not reliably kept, so set them whenever it is laid out.
    restyle();
    let width = p.panel.get_client_size().width;
    let mode = mode();
    let (r1, r2, r3) = (40, 74, 108);
    for (i, t) in p.tab.iter().enumerate() {
        place(t, 8 + i as i32 * 86, 6, 84, 28);
        t.set_value(i == mode as usize);
    }
    place(&p.status, 356, 11, (width - 356 - 44).max(0), 18);
    place(&p.closex, width - 30, 7, 24, 24);

    let hide: [&dyn WxWidget; 26] = [
        &p.find_lbl, &p.find, &p.repl_lbl, &p.repl, &p.dir_lbl, &p.dir, &p.filt_lbl, &p.filt, &p.browse, &p.case, &p.word, &p.wrap, &p.regex,
        &p.dot, &p.ext, &p.insel, &p.next, &p.prev, &p.findall, &p.count, &p.replace, &p.repl_all, &p.fif, &p.mark, &p.clear, &p.close,
    ];
    for w in hide {
        w.show(false);
    }
    let label = |h: &StaticText, y: i32| {
        place(h, 12, y + 4, 90, 20);
        show(h, true);
    };
    let edit = |h: &TextCtrl, y: i32| {
        place(h, 108, y, 250, 24);
        show(h, true);
    };
    let btn = |h: &Button, x: &mut i32, y: i32, w: i32| {
        place(h, *x, y, w, 26);
        show(h, true);
        *x += w + 6;
    };
    label(&p.find_lbl, r1);
    edit(&p.find, r1);
    let mut x = 368;
    let mut checks_y = r2;
    match mode {
        Mode::Find => {
            btn(&p.next, &mut x, r1 - 1, 84);
            btn(&p.prev, &mut x, r1 - 1, 96);
            btn(&p.findall, &mut x, r1 - 1, 200);
            btn(&p.count, &mut x, r1 - 1, 64);
            btn(&p.close, &mut x, r1 - 1, 64);
        }
        Mode::Replace => {
            btn(&p.next, &mut x, r1 - 1, 84);
            btn(&p.prev, &mut x, r1 - 1, 96);
            btn(&p.count, &mut x, r1 - 1, 64);
            btn(&p.findall, &mut x, r1 - 1, 200);
            label(&p.repl_lbl, r2);
            edit(&p.repl, r2);
            let mut x2 = 368;
            btn(&p.replace, &mut x2, r2 - 1, 84);
            btn(&p.repl_all, &mut x2, r2 - 1, 96);
            btn(&p.close, &mut x2, r2 - 1, 64);
            checks_y = r3;
        }
        Mode::InFiles => {
            btn(&p.fif, &mut x, r1 - 1, 84);
            btn(&p.close, &mut x, r1 - 1, 64);
            label(&p.dir_lbl, r2);
            edit(&p.dir, r2);
            let mut x2 = 368;
            btn(&p.browse, &mut x2, r2 - 1, 34);
            label(&p.filt_lbl, r3);
            edit(&p.filt, r3);
            checks_y = r3 + 34;
        }
        Mode::Mark => {
            btn(&p.mark, &mut x, r1 - 1, 84);
            btn(&p.clear, &mut x, r1 - 1, 120);
            btn(&p.close, &mut x, r1 - 1, 64);
        }
    }
    let mut cx = 108;
    for (c, w) in [(&p.case, 100), (&p.word, 172), (&p.wrap, 110), (&p.regex, 138), (&p.dot, 140)] {
        place(c, cx, checks_y, w, 22);
        show(c, true);
        cx += w + 6;
    }
    place(&p.ext, 108, checks_y + 26, 230, 22);
    show(&p.ext, true);
    if mode != Mode::InFiles {
        place(&p.insel, 344, checks_y + 26, 130, 22);
        show(&p.insel, true);
    }
    let base = height() - if results_visible() { RESULTS_H } else { 0 };
    place(&p.list, 8, base, (width - 16).max(0), RESULTS_H - 8);
    show(&p.list, results_visible());
}

/// Resize the panel to fit its mode and let the window re-flow.
pub fn relayout() {
    let (Some(host), Some(frame)) = (HOST.with(|h| *h.borrow()), FRAME.with(|f| *f.borrow())) else { return };
    let h = height();
    host.show(h > 0);
    host.set_min_size(Size::new(-1, h));
    frame.layout();
    arrange();
}

pub fn read_opts() -> Opts {
    let Some(p) = p() else { return Opts::default() };
    Opts {
        find: p.find.get_value(),
        replace: p.repl.get_value(),
        match_case: p.case.is_checked(),
        whole_word: p.word.is_checked(),
        regex: p.regex.is_checked(),
        wrap: p.wrap.is_checked(),
        dot_all: p.dot.is_checked(),
        extended: p.ext.is_checked(),
        in_selection: p.insel.is_checked(),
    }
}

pub fn read_dir_filter() -> (PathBuf, String) {
    let Some(p) = p() else { return (PathBuf::new(), String::new()) };
    (PathBuf::from(p.dir.get_value().trim()), p.filt.get_value())
}

pub fn set_status(s: &str) {
    if let Some(p) = p() {
        p.status.set_label(s);
    }
}

pub fn set_results(lines: &[String]) {
    let Some(p) = p() else { return };
    p.list.clear();
    for l in lines {
        p.list.append(l);
    }
    RESULTS.with(|c| c.set(!lines.is_empty()));
    relayout();
}

pub fn set_dir(dir: &str) {
    if let Some(p) = p() {
        p.dir.set_value(dir);
    }
}

pub fn close() {
    SHOWN.with(|c| c.set(false));
    relayout();
}

/// Show the panel in `mode`, prefilling the search text.
pub fn open(mode: Mode, prefill: &str, last_search: &str, dir: &str) {
    let Some(p) = p() else { return };
    MODE.with(|c| c.set(mode));
    SHOWN.with(|c| c.set(true));
    if !prefill.is_empty() && !prefill.contains(['\r', '\n']) {
        p.find.set_value(prefill);
    } else if p.find.get_value().is_empty() {
        p.find.set_value(last_search);
    }
    if mode == Mode::InFiles && p.dir.get_value().is_empty() {
        p.dir.set_value(dir);
    }
    relayout();
    p.find.set_focus();
    p.find.select_all();
}

#[derive(Clone, Copy)]
pub enum Cmd {
    Next,
    Prev,
    FindAll,
    Count,
    Replace,
    ReplaceAll,
    InFiles,
    Mark,
    Clear,
    Browse,
    Close,
    OpenHit(usize),
}

/// Build the controls and hook them to the application.
pub fn wire(app: &Shared) {
    let Some(host) = HOST.with(|h| *h.borrow()) else { return };
    let label = |t: &str| StaticText::builder(&host).with_label(t).build();
    let edit = || TextCtrl::builder(&host).with_style(TextCtrlStyle::ProcessEnter).build();
    let button = |t: &str| Button::builder(&host).with_label(t).build();
    let check = |t: &str| CheckBox::builder(&host).with_label(t).build();
    let tab = |t: &str| ToggleButton::builder(&host).with_label(t).build();
    let panel = P {
        panel: host,
        tab: [tab("Find"), tab("Replace"), tab("Find in Files"), tab("Mark")],
        closex: button("\u{2715}"),
        status: label(""),
        find_lbl: label("Find what :"),
        find: edit(),
        repl_lbl: label("Replace with :"),
        repl: edit(),
        dir_lbl: label("Directory :"),
        dir: edit(),
        filt_lbl: label("Filters :"),
        filt: edit(),
        browse: button("..."),
        case: check("Match case"),
        word: check("Match whole word only"),
        wrap: check("Wrap around"),
        regex: check("Regular expression"),
        dot: check(". matches newline"),
        ext: check("Extended (\\n, \\r, \\t, \\0, \\x...)"),
        insel: check("In selection"),
        next: button("Find Next"),
        prev: button("Find Previous"),
        findall: button("Find All in Current Document"),
        count: button("Count"),
        replace: button("Replace"),
        repl_all: button("Replace All"),
        fif: button("Find All"),
        mark: button("Mark All"),
        clear: button("Clear all marks"),
        close: button("Close"),
        list: ListBox::builder(&host).build(),
    };
    panel.wrap.set_value(true);
    panel.filt.set_value("*.*");
    panel.next.set_default();
    PANEL.with(|c| *c.borrow_mut() = Some(panel));

    let fire = |app: &Shared, cmd: Cmd| {
        if let Ok(mut a) = app.try_borrow_mut() {
            a.panel_command(cmd);
        }
    };
    for (b, cmd) in [
        (panel.next, Cmd::Next),
        (panel.prev, Cmd::Prev),
        (panel.findall, Cmd::FindAll),
        (panel.count, Cmd::Count),
        (panel.replace, Cmd::Replace),
        (panel.repl_all, Cmd::ReplaceAll),
        (panel.fif, Cmd::InFiles),
        (panel.mark, Cmd::Mark),
        (panel.clear, Cmd::Clear),
        (panel.browse, Cmd::Browse),
        (panel.close, Cmd::Close),
        (panel.closex, Cmd::Close),
    ] {
        let a = app.clone();
        b.on_click(move |_| fire(&a, cmd));
    }
    for (i, t) in panel.tab.iter().enumerate() {
        let a = app.clone();
        let t = *t;
        t.on_toggle(move |_| {
            let m = [Mode::Find, Mode::Replace, Mode::InFiles, Mode::Mark][i];
            let (find, last, dir) = match a.try_borrow() {
                Ok(app) => (panel.find.get_value(), app.ws.search.find.clone(), app.results.root.display().to_string()),
                Err(_) => return,
            };
            open(m, &find, &last, &dir);
        });
    }
    for e in [panel.find, panel.repl, panel.dir, panel.filt] {
        let a = app.clone();
        e.on_text_enter(move |_| {
            let cmd = match mode() {
                Mode::InFiles => Cmd::InFiles,
                Mode::Mark => Cmd::Mark,
                _ => Cmd::Next,
            };
            fire(&a, cmd);
        });
    }
    let a = app.clone();
    panel.list.on_selection_changed(move |e| {
        if let Some(i) = e.get_selection() {
            fire(&a, Cmd::OpenHit(i.max(0) as usize));
        }
    });
    host.on_size(move |_| arrange());
}
