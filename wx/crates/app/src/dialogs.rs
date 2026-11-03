//! Dialogs built on the form helper, defined like the Win32 shell's.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

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

/// Window > Sort By, in the customary order (also the dialog's "Sort tabs" choice).
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

/// Told the shortcuts as they are now.
type MapApply = Rc<dyn Fn(Vec<MapRow>)>;
/// Told the right-click menu as it is now.
type MenuApply = Rc<dyn Fn(Vec<String>)>;

#[derive(Clone)]
pub struct MapRow {
    pub id: usize,
    pub name: String,
    pub default: String,
    pub current: String,
}

thread_local! {
    static ROWS: RefCell<Vec<MapRow>> = const { RefCell::new(Vec::new()) };
    static MAP_APPLY: RefCell<Option<MapApply>> = const { RefCell::new(None) };
}

/// Tell the owner the shortcuts as they are now.
fn commit_shortcuts() {
    let rows = ROWS.with(|r| r.borrow().clone());
    if let Some(apply) = MAP_APPLY.with(|a| a.borrow().clone()) {
        apply(rows);
    }
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
                    ROWS.with(|r| r.borrow_mut()[i].current.clone_from(&canon));
                    f.set_text(EDIT, &canon);
                    refresh_row(f, i);
                    f.set_text(STATUS, if canon.is_empty() { "Shortcut removed" } else { "Shortcut set" });
                    commit_shortcuts();
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
            commit_shortcuts();
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

/// The size of the shortcut mapper's area in the settings window.
pub const MAPPER_SIZE: (i32, i32) = (600, 430);

/// The shortcut mapper, built into a settings page. `apply` gets every row whenever a shortcut changes.
pub fn embed_shortcut_mapper(parent: &impl WxWidget, rows: Vec<MapRow>, apply: MapApply) {
    ROWS.with(|r| *r.borrow_mut() = rows);
    MAP_APPLY.with(|a| *a.borrow_mut() = Some(apply));
    let spec = Spec {
        title: "Shortcut Mapper",
        size: MAPPER_SIZE,
        ctls: vec![
            ctl(100, Kind::List, "", 0, 0, 580, 340),
            ctl(106, Kind::Label, "Shortcut:", 0, 356, 64, 18),
            ctl(101, Kind::Edit, "", 68, 352, 190, 22),
            ctl(102, Kind::Button, "Set", 268, 350, 64, 26),
            ctl(103, Kind::Button, "Clear", 338, 350, 64, 26),
            ctl(104, Kind::Button, "Reset all", 408, 350, 80, 26),
            ctl(105, Kind::Label, "Select a command, click the Shortcut box, press the keys you want, then press Set.", 0, 384, 580, 18),
        ],
        ok: "",
        buttons: None,
        on_change: Some(mapper_click),
        on_apply: None,
    };
    form::embed(parent, spec, |f| {
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
}
mod context_menu;
pub use context_menu::*;
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
