//! The docked Find panel (Find / Replace / Find in Files / Mark), laid out like the Win32 shell's.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use notepp_core::search::Opts;
use wxdragon::prelude::*;

use crate::app::Shared;
use crate::widgets::controls::{Button, CheckBox, Label, ListBox, TextBox, ToggleButton};
use crate::widgets::dock::Dock;

mod layout;

use layout::Ctl;
pub use layout::Mode;

#[derive(Clone, Copy)]
struct P {
    panel: Panel,
    tab: [ToggleButton; 4],
    closex: Button,
    status: Label,
    find_lbl: Label,
    find: TextBox,
    repl_lbl: Label,
    repl: TextBox,
    dir_lbl: Label,
    dir: TextBox,
    filt_lbl: Label,
    filt: TextBox,
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
    static DOCK: RefCell<Option<Dock>> = const { RefCell::new(None) };
    static MODE: Cell<Mode> = const { Cell::new(Mode::Find) };
    static SHOWN: Cell<bool> = const { Cell::new(false) };
    static RESULTS: Cell<bool> = const { Cell::new(false) };
}

/// The empty container, placed in the window's layout; the controls are added by [`wire`].
pub fn container(frame: &Frame) -> Panel {
    let dock = Dock::new(frame);
    DOCK.with(|d| *d.borrow_mut() = Some(dock));
    dock.panel()
}

fn p() -> Option<P> {
    PANEL.with(|c| *c.borrow())
}

pub fn visible() -> bool {
    SHOWN.with(std::cell::Cell::get)
}

pub fn mode() -> Mode {
    MODE.with(std::cell::Cell::get)
}

fn results_visible() -> bool {
    RESULTS.with(std::cell::Cell::get) && mode() != Mode::Mark
}

/// Height the panel needs in the current mode (0 while it is closed).
pub fn height() -> i32 {
    if visible() { layout::height(mode(), results_visible()) } else { 0 }
}

/// The widget behind a control of the layout.
fn widget(p: &P, c: Ctl) -> &dyn WxWidget {
    match c {
        Ctl::Tab(i) => &p.tab[i],
        Ctl::Closex => &p.closex,
        Ctl::Status => &p.status,
        Ctl::FindLbl => &p.find_lbl,
        Ctl::Find => &p.find,
        Ctl::ReplLbl => &p.repl_lbl,
        Ctl::Repl => &p.repl,
        Ctl::DirLbl => &p.dir_lbl,
        Ctl::Dir => &p.dir,
        Ctl::FiltLbl => &p.filt_lbl,
        Ctl::Filt => &p.filt,
        Ctl::Browse => &p.browse,
        Ctl::Case => &p.case,
        Ctl::Word => &p.word,
        Ctl::Wrap => &p.wrap,
        Ctl::Regex => &p.regex,
        Ctl::Dot => &p.dot,
        Ctl::Ext => &p.ext,
        Ctl::InSel => &p.insel,
        Ctl::Next => &p.next,
        Ctl::Prev => &p.prev,
        Ctl::FindAll => &p.findall,
        Ctl::Count => &p.count,
        Ctl::Replace => &p.replace,
        Ctl::ReplAll => &p.repl_all,
        Ctl::Fif => &p.fif,
        Ctl::Mark => &p.mark,
        Ctl::Clear => &p.clear,
        Ctl::Close => &p.close,
        Ctl::List => &p.list,
    }
}

/// Every control, so the ones a mode does not use can be hidden.
fn all_controls(p: &P) -> Vec<&dyn WxWidget> {
    let mut v: Vec<&dyn WxWidget> = p.tab.iter().map(|t| t as &dyn WxWidget).collect();
    v.extend([
        &p.closex as &dyn WxWidget,
        &p.status,
        &p.find_lbl,
        &p.find,
        &p.repl_lbl,
        &p.repl,
        &p.dir_lbl,
        &p.dir,
        &p.filt_lbl,
        &p.filt,
        &p.browse,
        &p.case,
        &p.word,
        &p.wrap,
        &p.regex,
        &p.dot,
        &p.ext,
        &p.insel,
        &p.next,
        &p.prev,
        &p.findall,
        &p.count,
        &p.replace,
        &p.repl_all,
        &p.fif,
        &p.mark,
        &p.clear,
        &p.close,
        &p.list,
    ]);
    v
}

/// Position the controls for the current mode and width.
pub fn arrange() {
    let Some(p) = p() else { return };
    let mode = mode();
    let l = layout::layout(mode, p.panel.get_client_size().width, results_visible());
    for w in all_controls(&p) {
        w.show(false);
    }
    for (i, t) in p.tab.iter().enumerate() {
        t.set_value(i == mode as usize);
    }
    for (c, r) in &l.placed {
        let w = widget(&p, *c);
        w.set_size_with_pos(r.l, r.t, r.w(), r.h());
        w.show(true);
    }
}

/// Resize the panel to fit its mode and let the window re-flow.
pub fn relayout() {
    let Some(dock) = DOCK.with(|d| *d.borrow()) else { return };
    dock.set_height(height());
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
#[allow(clippy::too_many_lines, reason = "wires each control of the panel to its action, one block per control")]
pub fn wire(app: &Shared) {
    let Some(host) = DOCK.with(|d| d.borrow().map(|d| d.panel())) else { return };
    let label = |t: &str| Label::new(&host, t);
    let edit = || TextBox::new(&host, "", true);
    let button = |t: &str| Button::new(&host, t);
    let check = |t: &str| CheckBox::new(&host, t);
    let tab = |t: &str| ToggleButton::new(&host, t);
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
        list: ListBox::new(&host),
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
            fire(&a, Cmd::OpenHit(crate::platform::wx::to_usize(i.max(0))));
        }
    });
    host.on_size(move |_| arrange());
}
