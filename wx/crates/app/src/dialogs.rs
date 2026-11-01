//! Dialogs built on the form helper, defined like the Win32 shell's.

use std::cell::{Cell, RefCell};

use notepp_core::hash::Algo;
use notepp_core::shortcuts;
use notepp_core::workspace::SortField;
use wxdragon::prelude::*;

use crate::widgets::form::{self, Form, Kind, Spec, ctl};

// ── Windows ──────────────────────────────────────────────────────────────────

pub enum WinAction {
    Activate(usize),
    Save(Vec<usize>),
    Close(Vec<usize>),
    Sort(SortField, bool),
}

/// Window > Sort By, in Notepad++'s order (also the dialog's "Sort tabs" choice).
pub const SORTS: [(&str, SortField, bool); 10] = [
    ("Name A to Z", SortField::Name, false),
    ("Name Z to A", SortField::Name, true),
    ("Path A to Z", SortField::Path, false),
    ("Path Z to A", SortField::Path, true),
    ("Type A to Z", SortField::Type, false),
    ("Type Z to A", SortField::Type, true),
    ("Content Length Ascending", SortField::Length, false),
    ("Content Length Descending", SortField::Length, true),
    ("Modified Time Ascending", SortField::Modified, false),
    ("Modified Time Descending", SortField::Modified, true),
];

thread_local! {
    static WIN_SEL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static WIN_PICK: Cell<Option<usize>> = const { Cell::new(None) };
    static OWNER: RefCell<Option<Frame>> = const { RefCell::new(None) };
}

const WIN_LIST: usize = 100;
const WIN_SAVE: usize = 101;
const WIN_CLOSE: usize = 102;
const WIN_SORT: usize = 103;

fn windows_click(f: &Form, id: usize) {
    if id == WIN_SAVE || id == WIN_CLOSE {
        WIN_SEL.with(|s| *s.borrow_mut() = f.list_selected_all(WIN_LIST));
        WIN_PICK.with(|p| p.set(Some(id)));
        f.close(true);
    } else if id == WIN_SORT {
        let Some(owner) = OWNER.with(|o| *o.borrow()) else { return };
        let names: Vec<&str> = SORTS.iter().map(|s| s.0).collect();
        let d = SingleChoiceDialog::builder(&owner, "Sort the tabs by:", "Sort tabs", &names).build();
        if d.show_modal() == ID_OK {
            let i = d.get_selection();
            if i >= 0 {
                WIN_PICK.with(|p| p.set(Some(WIN_SORT + 1 + crate::platform::wx::to_usize(i))));
                f.close(true);
            }
        }
    }
}

/// Window > Windows...: every open tab with Activate / Save / Close window(s) / Sort tabs.
pub fn windows_dialog(frame: &Frame, rows: &[Vec<String>], selected: usize) -> Option<WinAction> {
    OWNER.with(|o| *o.borrow_mut() = Some(*frame));
    WIN_SEL.with(|s| s.borrow_mut().clear());
    WIN_PICK.with(|p| p.set(None));
    // OK means Activate; keep the dialog's selection as it closes.
    let apply = std::rc::Rc::new(|_: &form::Values, f: &Form| {
        WIN_SEL.with(|s| *s.borrow_mut() = f.list_selected_all(WIN_LIST));
        false
    });
    let spec = Spec {
        title: "Windows",
        size: (700, 420),
        ctls: vec![
            ctl(WIN_LIST, Kind::MultiList, "", 12, 12, 560, 350),
            ctl(WIN_SAVE, Kind::Button, "&Save", 582, 36, 108, 26),
            ctl(WIN_CLOSE, Kind::Button, "&Close window(s)", 582, 68, 108, 26),
            ctl(WIN_SORT, Kind::Button, "Sort &tabs", 582, 100, 108, 26),
        ],
        ok: "&Activate",
        buttons: None,
        on_change: Some(windows_click),
        on_apply: Some(apply),
    };
    let rows_owned = rows.to_vec();
    let accepted = form::run(frame, spec, move |f| {
        f.list_fill(WIN_LIST, &[("Name", 130), ("Path", 260), ("Type", 90), ("Size", 60)], &rows_owned);
        if selected < rows_owned.len() {
            f.list_select(WIN_LIST, selected);
        }
    });
    let sel = WIN_SEL.with(|s| s.borrow().clone());
    match (accepted, WIN_PICK.with(std::cell::Cell::take)) {
        (_, Some(WIN_SAVE)) => Some(WinAction::Save(sel)),
        (_, Some(WIN_CLOSE)) => Some(WinAction::Close(sel)),
        (_, Some(id)) => SORTS.get(id - WIN_SORT - 1).map(|(_, f, d)| WinAction::Sort(*f, *d)),
        (Some(_), None) => sel.first().map(|i| WinAction::Activate(*i)),
        (None, None) => None,
    }
}

// ── Shortcut Mapper ──────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct MapRow {
    pub id: usize,
    pub name: String,
    pub default: String,
    pub current: String,
}

thread_local! {
    static ROWS: RefCell<Vec<MapRow>> = const { RefCell::new(Vec::new()) };
}

fn refresh_row(f: &Form, i: usize) {
    ROWS.with(|r| {
        if let Some(row) = r.borrow().get(i) {
            f.list_set_cell(100, i, 1, &row.current);
        }
    });
}

fn mapper_click(f: &Form, id: usize) {
    const LIST: usize = 100;
    const EDIT: usize = 101;
    const SET: usize = 102;
    const CLEAR: usize = 103;
    const RESET: usize = 104;
    const STATUS: usize = 105;
    let sel = f.list_selected(LIST);
    match id {
        LIST => {
            if let Some(i) = sel {
                ROWS.with(|r| f.set_text(EDIT, &r.borrow()[i].current));
                f.set_text(STATUS, "");
            }
        }
        SET | CLEAR => {
            let Some(i) = sel else { return f.set_text(STATUS, "Select a command first") };
            let text = if id == CLEAR { String::new() } else { f.text(EDIT).trim().to_string() };
            let result: Result<String, notepp_core::error::Error> = if text.is_empty() {
                Ok(String::new())
            } else {
                match shortcuts::parse(&text) {
                    None => Err(notepp_core::error::Error::InvalidShortcut),
                    Some(chord) => {
                        let canon = shortcuts::format(&chord);
                        ROWS.with(|r| {
                            let rows = r.borrow();
                            match rows.iter().enumerate().find(|(j, row)| *j != i && shortcuts::parse(&row.current).is_some_and(|c| c == chord)) {
                                Some((_, other)) => Err(notepp_core::error::Error::ShortcutInUse(other.name.clone())),
                                None => Ok(canon),
                            }
                        })
                    }
                }
            };
            match result {
                Ok(canon) => {
                    ROWS.with(|r| r.borrow_mut()[i].current = canon.clone());
                    f.set_text(EDIT, &canon);
                    refresh_row(f, i);
                    f.set_text(STATUS, if canon.is_empty() { "Shortcut removed" } else { "Shortcut set" });
                }
                Err(e) => f.set_text(STATUS, &e.to_string()),
            }
        }
        RESET => {
            let n = ROWS.with(|r| {
                let mut rows = r.borrow_mut();
                rows.iter_mut().for_each(|row| row.current = row.default.clone());
                rows.len()
            });
            (0..n).for_each(|i| refresh_row(f, i));
            if let Some(i) = sel {
                ROWS.with(|r| f.set_text(EDIT, &r.borrow()[i].current));
            }
            f.set_text(STATUS, "Every shortcut is back to its default");
        }
        _ => {}
    }
}

/// wxWidgets key code -> Windows virtual-key code (what the shortcut table stores).
fn vk_of(code: i32) -> Option<u16> {
    Some(match code {
        65..=90 | 48..=57 | 32 => crate::platform::wx::to_u16(code),
        340..=363 => 0x70 + crate::platform::wx::to_u16(code - 340), // F1..F24
        312 => 0x23,                                                 // End
        313 => 0x24,                                                 // Home
        314 => 0x25,                                                 // Left
        315 => 0x26,                                                 // Up
        316 => 0x27,                                                 // Right
        317 => 0x28,                                                 // Down
        366 => 0x21,                                                 // Page Up
        367 => 0x22,                                                 // Page Down
        322 => 0x2D,                                                 // Insert
        127 => 0x2E,                                                 // Delete
        _ => return None,
    })
}

/// Settings > Shortcut Mapper. Returns the edited rows on OK.
pub fn shortcut_mapper(frame: &Frame, rows: Vec<MapRow>) -> Option<Vec<MapRow>> {
    ROWS.with(|r| *r.borrow_mut() = rows);
    let spec = Spec {
        title: "Shortcut Mapper",
        size: (600, 470),
        ctls: vec![
            ctl(100, Kind::List, "", 12, 12, 576, 340),
            ctl(106, Kind::Label, "Shortcut:", 12, 368, 64, 18),
            ctl(101, Kind::Edit, "", 80, 364, 190, 22),
            ctl(102, Kind::Button, "Set", 280, 362, 64, 26),
            ctl(103, Kind::Button, "Clear", 350, 362, 64, 26),
            ctl(104, Kind::Button, "Reset all", 420, 362, 80, 26),
            ctl(105, Kind::Label, "Select a command, click the Shortcut box, press the keys you want, then press Set.", 12, 396, 576, 18),
        ],
        ok: "OK",
        buttons: None,
        on_change: Some(mapper_click),
        on_apply: None,
    };
    let accepted = form::run(frame, spec, |f| {
        let rows: Vec<Vec<String>> = ROWS.with(|r| r.borrow().iter().map(|row| vec![row.name.clone(), row.current.clone()]).collect());
        f.list_fill(100, &[("Command", 380), ("Shortcut", 170)], &rows);
        // The shortcut box records the key combination pressed in it instead of taking typed text.
        f.on_key(101, |f, code, ctrl, alt, shift| {
            let bare = !ctrl && !alt && !shift;
            match code {
                9 | 13 | 27 if bare => return false, // keep Tab / Enter / Esc working
                8 | 127 if bare => {
                    f.set_text(101, "");
                    return true;
                }
                _ => {}
            }
            if let Some(vk) = vk_of(code) {
                let chord = shortcuts::Chord { ctrl, alt, shift, vk };
                let text = shortcuts::format(&chord);
                if shortcuts::parse(&text).is_some_and(|c| c == chord) {
                    f.set_text(101, &text);
                }
            }
            true
        });
    });
    accepted.map(|_| ROWS.with(|r| r.borrow().clone()))
}

// ── Edit Popup ContextMenu ───────────────────────────────────────────────────

#[derive(Default)]
struct CtxState {
    all: Vec<String>,
    menu: Vec<String>,
    default: Vec<String>,
    /// Indices into `all` of the rows the left list shows.
    shown: Vec<usize>,
    as_text: bool,
}

thread_local! {
    static CTX: RefCell<CtxState> = RefCell::new(CtxState::default());
}

const CTX_SEP: &str = "-";
const CTX_AVAIL: usize = 100;
const CTX_MENU: usize = 101;
const CTX_FILTER: usize = 102;

fn ctx_fill_avail(f: &Form) {
    let q = f.text(CTX_FILTER).trim().to_lowercase();
    let rows = CTX.with(|c| {
        let mut c = c.borrow_mut();
        let shown: Vec<usize> = (0..c.all.len()).filter(|&i| !c.menu.contains(&c.all[i]) && (q.is_empty() || c.all[i].to_lowercase().contains(&q))).collect();
        let rows = shown.iter().map(|&i| vec![c.all[i].clone()]).collect::<Vec<_>>();
        c.shown = shown;
        rows
    });
    f.list_fill(CTX_AVAIL, &[("Available commands", 300)], &rows);
}

fn ctx_fill_menu(f: &Form, select: Option<usize>) {
    let rows = CTX.with(|c| {
        c.borrow()
            .menu
            .iter()
            .map(|n| {
                vec![if n == CTX_SEP {
                    "\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}  separator  \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}".to_string()
                } else {
                    n.clone()
                }]
            })
            .collect::<Vec<_>>()
    });
    f.list_fill(CTX_MENU, &[("In the right-click menu", 300)], &rows);
    if let Some(i) = select.filter(|i| *i < rows.len()) {
        f.list_select(CTX_MENU, i);
    }
}

fn ctx_click(f: &Form, id: usize) {
    const ADD: usize = 103;
    const REMOVE: usize = 104;
    const UP: usize = 105;
    const DOWN: usize = 106;
    const SEP: usize = 107;
    const RESET: usize = 108;
    const TEXT: usize = 109;
    let sel = f.list_selected(CTX_MENU);
    match id {
        ADD => {
            let picked = f.list_selected_all(CTX_AVAIL);
            if picked.is_empty() {
                return;
            }
            let at = sel.map_or(usize::MAX, |s| s + 1);
            let first = CTX.with(|c| {
                let mut c = c.borrow_mut();
                let names: Vec<String> = picked.iter().filter_map(|&r| c.shown.get(r).map(|&i| c.all[i].clone())).collect();
                let at = at.min(c.menu.len());
                for (k, n) in names.iter().enumerate() {
                    c.menu.insert(at + k, n.clone());
                }
                at
            });
            ctx_fill_avail(f);
            ctx_fill_menu(f, Some(first));
        }
        SEP => {
            let at = CTX.with(|c| {
                let mut c = c.borrow_mut();
                let at = sel.map_or(c.menu.len(), |s| s + 1).min(c.menu.len());
                c.menu.insert(at, CTX_SEP.into());
                at
            });
            ctx_fill_menu(f, Some(at));
        }
        REMOVE => {
            let Some(s) = sel else { return };
            CTX.with(|c| {
                c.borrow_mut().menu.remove(s);
            });
            ctx_fill_avail(f);
            ctx_fill_menu(f, Some(s.min(CTX.with(|c| c.borrow().menu.len()).saturating_sub(1))));
        }
        UP | DOWN => {
            let Some(s) = sel else { return };
            let to = CTX.with(|c| {
                let mut c = c.borrow_mut();
                let to = if id == UP { s.checked_sub(1) } else { (s + 1 < c.menu.len()).then_some(s + 1) };
                if let Some(t) = to {
                    c.menu.swap(s, t);
                }
                to
            });
            if let Some(t) = to {
                ctx_fill_menu(f, Some(t));
            }
        }
        RESET => {
            CTX.with(|c| {
                let mut c = c.borrow_mut();
                c.menu = c.default.clone();
            });
            ctx_fill_avail(f);
            ctx_fill_menu(f, Some(0));
        }
        TEXT => {
            CTX.with(|c| c.borrow_mut().as_text = true);
            f.close(true);
        }
        _ => {}
    }
}

/// Settings > Edit Popup `ContextMenu`. `all`: every command that can be offered; `current` / `default`: the menu
/// as a list of command names with "-" for a separator. Returns the new menu and whether the user asked to edit
/// the file as text instead.
pub fn context_menu_editor(frame: &Frame, all: Vec<String>, current: Vec<String>, default: Vec<String>) -> Option<(Vec<String>, bool)> {
    CTX.with(|c| *c.borrow_mut() = CtxState { all, menu: current, default, ..Default::default() });
    form::on_edit(|f, id| {
        if id == CTX_FILTER {
            ctx_fill_avail(f);
        }
    });
    let spec = Spec {
        title: "Edit Popup ContextMenu",
        size: (770, 520),
        ctls: vec![
            ctl(110, Kind::Label, "Find command:", 12, 15, 90, 18),
            ctl(CTX_FILTER, Kind::Edit, "", 104, 12, 238, 22),
            ctl(CTX_AVAIL, Kind::MultiList, "", 12, 42, 330, 370),
            ctl(103, Kind::Button, "Add  >", 350, 150, 70, 28),
            ctl(104, Kind::Button, "<  Remove", 350, 186, 70, 28),
            ctl(CTX_MENU, Kind::List, "", 428, 42, 330, 370),
            ctl(105, Kind::Button, "Move Up", 428, 420, 76, 26),
            ctl(106, Kind::Button, "Move Down", 510, 420, 84, 26),
            ctl(107, Kind::Button, "Add Separator", 600, 420, 100, 26),
            ctl(108, Kind::Button, "Reset to default", 12, 420, 120, 26),
            ctl(109, Kind::Button, "Edit as text...", 138, 420, 110, 26),
            ctl(111, Kind::Label, "Select commands on the left and press Add; they go below the selected item on the right.", 12, 452, 600, 18),
        ],
        ok: "OK",
        buttons: None,
        on_change: Some(ctx_click),
        on_apply: None,
    };
    form::run(frame, spec, |f| {
        ctx_fill_avail(f);
        ctx_fill_menu(f, Some(0));
    })?;
    CTX.with(|c| {
        let c = c.borrow();
        Some((c.menu.clone(), c.as_text))
    })
}

// ── hashes ───────────────────────────────────────────────────────────────────

/// Tools > <algorithm> > Generate...: hash some typed text.
pub fn hash_text(frame: &Frame, algo: Algo) {
    const INPUT: usize = 101;
    const RESULT: usize = 102;
    const COPY: usize = 103;
    let apply = std::rc::Rc::new(move |v: &form::Values, f: &Form| {
        let digest = algo.hash_bytes(v.text(INPUT).as_bytes());
        f.set_text(RESULT, &digest);
        if v.checked(COPY) {
            crate::services::clipboard::copy(&digest);
        }
        true
    });
    let spec = Spec {
        title: &format!("{} digest", algo.name()),
        size: (460, 300),
        ctls: vec![
            ctl(110, Kind::Label, "Text to hash:", 12, 10, 300, 18),
            ctl(INPUT, Kind::MultiEdit(false), "", 12, 30, 436, 100),
            ctl(111, Kind::Label, &format!("{} (hex):", algo.name()), 12, 142, 300, 18),
            ctl(RESULT, Kind::MultiEdit(true), "", 12, 162, 436, 52),
            ctl(COPY, Kind::Check, "Copy the result to the clipboard", 12, 224, 300, 20),
        ],
        ok: "Generate",
        buttons: None,
        on_change: None,
        on_apply: Some(apply),
    };
    let _ = form::run(frame, spec, |_| {});
}

/// A read-only block of text with a Copy button.
pub fn text_result(frame: &Frame, title: &str, text: &str) {
    let apply = std::rc::Rc::new(|v: &form::Values, _f: &Form| {
        crate::services::clipboard::copy(v.text(101));
        true
    });
    let spec = Spec {
        title,
        size: (640, 380),
        ctls: vec![ctl(101, Kind::MultiEdit(true), text, 12, 12, 616, 306)],
        ok: "Copy",
        buttons: None,
        on_change: None,
        on_apply: Some(apply),
    };
    let _ = form::run(frame, spec, |_| {});
}
