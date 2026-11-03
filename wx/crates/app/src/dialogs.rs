//! Dialogs built on the form helper, defined like the Win32 shell's.

use std::cell::RefCell;
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

const WIN_LIST: usize = 100;
const WIN_SAVE: usize = 101;
const WIN_CLOSE: usize = 102;
const WIN_SORT: usize = 103;

/// What the Windows dialog keeps while it is open: its owner, the selected rows and the button that closed it.
struct WinState {
    owner: Frame,
    sel: Vec<usize>,
    pick: Option<usize>,
}

fn windows_click(f: &Form, id: usize) {
    if id == WIN_SAVE || id == WIN_CLOSE {
        let sel = f.list_selected_all(WIN_LIST);
        f.with_state(|s: &mut WinState| (s.sel, s.pick) = (sel, Some(id)));
        f.close(true);
    } else if id == WIN_SORT {
        let Some(owner) = f.with_state(|s: &mut WinState| s.owner) else { return };
        let names: Vec<&str> = SORTS.iter().map(|s| s.0).collect();
        if let Some(i) = crate::widgets::ask::choice(&owner, "Sort tabs", "Sort the tabs by:", &names) {
            f.with_state(|s: &mut WinState| s.pick = Some(WIN_SORT + 1 + i));
            f.close(true);
        }
    }
}

/// Window > Windows...: every open tab with Activate / Save / Close window(s) / Sort tabs.
pub fn windows_dialog(frame: &Frame, rows: &[Vec<String>], selected: usize) -> Option<WinAction> {
    // OK means Activate; keep the dialog's selection as it closes.
    let apply = std::rc::Rc::new(|_: &form::Values, f: &Form| {
        let sel = f.list_selected_all(WIN_LIST);
        f.with_state(|s: &mut WinState| s.sel = sel);
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
    let (rows_owned, owner) = (rows.to_vec(), *frame);
    let opened: Rc<RefCell<Option<Form>>> = Rc::default();
    let keep = opened.clone();
    let accepted = form::run(frame, spec, move |f| {
        f.set_state(WinState { owner, sel: Vec::new(), pick: None });
        f.list_fill(WIN_LIST, &[("Name", 130), ("Path", 260), ("Type", 90), ("Size", 60)], &rows_owned);
        if selected < rows_owned.len() {
            f.list_select(WIN_LIST, selected);
        }
        *keep.borrow_mut() = Some(f.clone());
    });
    let WinState { sel, pick, .. } = opened.borrow_mut().take()?.take_state::<WinState>()?;
    match (accepted, pick) {
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

/// What the mapper keeps while it is shown: every row, and whom to tell when one changes.
struct MapperState {
    rows: Vec<MapRow>,
    apply: MapApply,
}

/// Tell the owner the shortcuts as they are now (outside the state's borrow: applying rebuilds the settings pages).
fn commit_shortcuts(f: &Form) {
    if let Some((rows, apply)) = f.with_state(|s: &mut MapperState| (s.rows.clone(), s.apply.clone())) {
        apply(rows);
    }
}

/// The shortcut of row `i` as it is now.
fn current_of(f: &Form, i: usize) -> Option<String> {
    f.with_state(|s: &mut MapperState| s.rows.get(i).map(|r| r.current.clone())).flatten()
}

fn refresh_row(f: &Form, i: usize) {
    if let Some(current) = current_of(f, i) {
        f.list_set_cell(100, i, 1, &current);
    }
}

/// `text` as the canonical shortcut for row `i` (empty clears it), unless it is not a shortcut or another row has it.
fn checked_shortcut(f: &Form, i: usize, text: &str) -> Result<String, notepp_core::error::Error> {
    if text.is_empty() {
        return Ok(String::new());
    }
    let chord = shortcuts::parse(text).ok_or(notepp_core::error::Error::InvalidShortcut)?;
    let taken_by = f.with_state(|s: &mut MapperState| {
        s.rows.iter().enumerate().find(|(j, row)| *j != i && shortcuts::parse(&row.current).is_some_and(|c| c == chord)).map(|(_, o)| o.name.clone())
    });
    match taken_by.flatten() {
        Some(other) => Err(notepp_core::error::Error::ShortcutInUse(other)),
        None => Ok(shortcuts::format(&chord)),
    }
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
            if let Some(current) = sel.and_then(|i| current_of(f, i)) {
                f.set_text(EDIT, &current);
                f.set_text(STATUS, "");
            }
        }
        SET | CLEAR => {
            let Some(i) = sel else { return f.set_text(STATUS, "Select a command first") };
            let text = if id == CLEAR { String::new() } else { f.text(EDIT).trim().to_string() };
            match checked_shortcut(f, i, &text) {
                Ok(canon) => {
                    f.with_state(|s: &mut MapperState| s.rows.get_mut(i).map(|r| r.current.clone_from(&canon)));
                    f.set_text(EDIT, &canon);
                    refresh_row(f, i);
                    f.set_text(STATUS, if canon.is_empty() { "Shortcut removed" } else { "Shortcut set" });
                    commit_shortcuts(f);
                }
                Err(e) => f.set_text(STATUS, &e.to_string()),
            }
        }
        RESET => {
            let n = f
                .with_state(|s: &mut MapperState| {
                    s.rows.iter_mut().for_each(|row| row.current = row.default.clone());
                    s.rows.len()
                })
                .unwrap_or(0);
            (0..n).for_each(|i| refresh_row(f, i));
            if let Some(current) = sel.and_then(|i| current_of(f, i)) {
                f.set_text(EDIT, &current);
            }
            f.set_text(STATUS, "Every shortcut is back to its default");
            commit_shortcuts(f);
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
    let shown: Vec<Vec<String>> = rows.iter().map(|row| vec![row.name.clone(), row.current.clone()]).collect();
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
    form::embed(parent, spec, move |f| {
        f.set_state(MapperState { rows, apply });
        f.list_fill(100, &[("Command", 380), ("Shortcut", 170)], &shown);
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
