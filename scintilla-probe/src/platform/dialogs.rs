//! Dialogs for the Settings and Tools menus, built on the modal `form`.

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::UI::Controls::Dialogs::{CC_FULLOPEN, CC_RGBINIT, CHOOSECOLORW, ChooseColorW};

use super::clip;
use super::form::{self, Form, Kind, Spec, Values, ctl};
use crate::core::hash::Algo;
use crate::core::sci::Theme;
use crate::core::settings::{self, ROLES, Settings};
use crate::core::shortcuts;
use crate::core::workspace::SortField;
pub use super::form::Tone;

// ── Message dialog ───────────────────────────────────────────────────────────

pub const BTN_YES: usize = 201;
pub const BTN_NO: usize = 202;
pub const BTN_SAVE: usize = 203;
pub const BTN_DONT_SAVE: usize = 204;
pub const BTN_CANCEL: usize = 205;
pub const BTN_OK: usize = 206;

thread_local! {
    static MSG_PICK: Cell<usize> = const { Cell::new(0) };
}

fn message_click(f: &Form, id: usize) {
    if id >= BTN_YES {
        MSG_PICK.with(|p| p.set(id));
        f.close(true);
    }
}

/// A modern message box: a coloured badge, wrapped text (scrollable when long) and
/// right-aligned buttons. Returns the id of the button pressed, or `cancel` for Esc / close.
/// Falls back to the system box if another form is already open.
pub fn message(host: HWND, title: &str, text: &str, tone: Tone, buttons: &[(&str, usize)], default: usize, cancel: usize) -> usize {
    if form::is_open() {
        use windows::Win32::UI::WindowsAndMessaging::{IDYES, MB_ICONINFORMATION, MB_OK, MB_YESNO, MessageBoxW};
        let (t, c) = (title.encode_utf16().chain([0]).collect::<Vec<u16>>(), text.encode_utf16().chain([0]).collect::<Vec<u16>>());
        let asks = buttons.len() > 1;
        let r = unsafe { MessageBoxW(Some(host), windows::core::PCWSTR(c.as_ptr()), windows::core::PCWSTR(t.as_ptr()), if asks { MB_YESNO } else { MB_OK | MB_ICONINFORMATION }) };
        return if r == IDYES || !asks { default } else { cancel };
    }
    const W: i32 = 480;
    const TEXT_X: i32 = 76;
    let text_w = W - TEXT_X - 24;
    let th = form::text_height(text, text_w);
    let scroll = th > 300;
    let body_h = if scroll { 300 } else { th.max(36) };
    let mut ctls = vec![ctl(100, Kind::Badge(tone), "", 24, 24, 36, 36)];
    if scroll {
        ctls.push(ctl(101, Kind::MultiEdit(true), text, TEXT_X, 24, text_w, body_h));
    } else {
        ctls.push(ctl(101, Kind::Label, text, TEXT_X, 24 + (36 - th).max(0) / 2, text_w, th));
    }
    let height = 24 + body_h + 24 + 36;
    let by = height - 36;
    let mut x = W - 24;
    for (label, id) in buttons.iter().rev() {
        let bw = 100;
        x -= bw;
        ctls.push(ctl(*id, Kind::Button, label, x, by, bw, 30));
        x -= 8;
    }
    let spec = Spec { title, size: (W, height), ctls, ok: "", on_change: Some(message_click), on_apply: None };
    MSG_PICK.with(|p| p.set(cancel));
    let picked = form::run(host, spec, |f| {
        form::set_default_button(default);
        f.focus(default);
    });
    if picked.is_some() { MSG_PICK.with(|p| p.get()) } else { cancel }
}

// ── Window > Windows... ──────────────────────────────────────────────────────

/// What the user chose in the Windows dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum WinAction {
    Activate(usize),
    Save(Vec<usize>),
    Close(Vec<usize>),
    Sort(SortField, bool),
}

/// Window > Sort By, in Notepad++'s order (also the dialog's "Sort tabs" popup).
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
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::*;
        let picked = unsafe {
            let Ok(menu) = CreatePopupMenu() else { return };
            for (i, (label, _, _)) in SORTS.iter().enumerate() {
                let w: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                let _ = AppendMenuW(menu, MF_STRING, i + 1, windows::core::PCWSTR(w.as_ptr()));
            }
            let mut at = POINT::default();
            let _ = GetCursorPos(&mut at);
            let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY, at.x, at.y, Some(0), f.hwnd(), None);
            let _ = DestroyMenu(menu);
            cmd.0 as usize
        };
        if picked > 0 {
            WIN_PICK.with(|p| p.set(Some(WIN_SORT + picked)));
            f.close(true);
        }
    }
}

/// Window > Windows...: every open tab with Activate / Save / Close window(s) / Sort tabs.
pub fn windows_dialog(host: HWND, rows: &[Vec<String>], selected: usize) -> Option<WinAction> {
    WIN_SEL.with(|s| s.borrow_mut().clear());
    WIN_PICK.with(|p| p.set(None));
    // OK means Activate; keep the dialog's selection as it closes.
    let apply = |_: &Values, f: &Form| {
        WIN_SEL.with(|s| *s.borrow_mut() = f.list_selected_all(WIN_LIST));
        false
    };
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
        on_change: Some(windows_click),
        on_apply: Some(&apply),
    };
    let accepted = form::run(host, spec, |f| {
        f.list_fill(WIN_LIST, &[("Name", 130), ("Path", 260), ("Type", 90), ("Size", 60)], rows);
        if selected < rows.len() {
            f.list_select(WIN_LIST, selected);
        }
    });
    // OK means Activate (`apply` kept the selection and let the form close).
    let sel = WIN_SEL.with(|s| s.borrow().clone());
    match (accepted, WIN_PICK.with(|p| p.take())) {
        (_, Some(WIN_SAVE)) => Some(WinAction::Save(sel)),
        (_, Some(WIN_CLOSE)) => Some(WinAction::Close(sel)),
        (_, Some(id)) => SORTS.get(id - WIN_SORT - 1).map(|(_, f, d)| WinAction::Sort(*f, *d)),
        (Some(_), None) => sel.first().map(|i| WinAction::Activate(*i)),
        (None, None) => None,
    }
}

// ── Plugins > Plugins Admin ──────────────────────────────────────────────────

/// Plugins > Plugins Admin...: what is built in (rows: name, version, description).
pub fn plugins_admin(host: HWND, rows: &[Vec<String>]) {
    let spec = Spec {
        title: "Plugins Admin",
        size: (640, 380),
        ctls: vec![
            ctl(100, Kind::List, "", 12, 12, 616, 280),
            ctl(101, Kind::Label, "These plugins are built into Note++. Loading external plugin DLLs is not supported.", 12, 300, 616, 36),
        ],
        ok: "Close",
        on_change: None,
        on_apply: None,
    };
    let _ = form::run(host, spec, |f| f.list_fill(100, &[("Plugin", 140), ("Version", 70), ("Description", 380)], rows));
}

fn number(s: &str, fallback: i32) -> i32 {
    s.trim().parse().unwrap_or(fallback)
}

// ── Preferences ──────────────────────────────────────────────────────────────

/// Settings > Preferences. Returns the new settings on OK.
pub fn preferences(host: HWND, cur: &Settings) -> Option<Settings> {
    const FONT: usize = 101;
    const SIZE: usize = 102;
    const TAB: usize = 103;
    const SPACES: usize = 104;
    const EDGE: usize = 105;
    const LINENO: usize = 106;
    const FOLD: usize = 107;
    const BOOKMARK: usize = 108;
    const CARET: usize = 109;
    const SMART: usize = 110;
    const TOOLBAR: usize = 111;
    const TABBAR: usize = 112;
    const STATUS: usize = 113;
    const E_UTF8: usize = 120;
    const E_BOM: usize = 121;
    const E_ANSI: usize = 122;
    const L_CRLF: usize = 124;
    const L_LF: usize = 125;
    const L_CR: usize = 126;
    const REMOTE: usize = 130;
    let spec = Spec {
        title: "Preferences",
        size: (440, 474),
        ctls: vec![
            ctl(1000, Kind::Group, "Editor", 12, 8, 416, 118),
            ctl(1001, Kind::Label, "Font:", 24, 32, 56, 18),
            ctl(FONT, Kind::Edit, &cur.font, 84, 29, 180, 22),
            ctl(1002, Kind::Label, "Size:", 282, 32, 36, 18),
            ctl(SIZE, Kind::Edit, &cur.font_size.to_string(), 322, 29, 50, 22),
            ctl(1003, Kind::Label, "Tab size:", 24, 62, 60, 18),
            ctl(TAB, Kind::Edit, &cur.tab_width.to_string(), 84, 59, 50, 22),
            ctl(SPACES, Kind::Check, "Insert spaces instead of tabs", 160, 61, 250, 20),
            ctl(1004, Kind::Label, "Vertical guide at column (0 = none):", 24, 94, 220, 18),
            ctl(EDGE, Kind::Edit, &cur.edge_column.to_string(), 250, 91, 50, 22),
            ctl(1010, Kind::Group, "Display", 12, 132, 416, 112),
            ctl(LINENO, Kind::Check, "Line numbers", 24, 152, 180, 20),
            ctl(FOLD, Kind::Check, "Fold margin", 224, 152, 180, 20),
            ctl(BOOKMARK, Kind::Check, "Bookmark margin", 24, 176, 180, 20),
            ctl(CARET, Kind::Check, "Highlight current line", 224, 176, 190, 20),
            ctl(SMART, Kind::Check, "Highlight matching words", 24, 200, 190, 20),
            ctl(TOOLBAR, Kind::Check, "Toolbar", 224, 200, 90, 20),
            ctl(TABBAR, Kind::Check, "Tab bar", 24, 220, 90, 20),
            ctl(STATUS, Kind::Check, "Status bar", 224, 220, 120, 20),
            ctl(1020, Kind::Group, "New documents", 12, 250, 416, 100),
            ctl(1021, Kind::Label, "Encoding:", 24, 274, 64, 18),
            ctl(E_UTF8, Kind::Radio(true), "UTF-8", 92, 272, 70, 20),
            ctl(E_BOM, Kind::Radio(false), "UTF-8-BOM", 170, 272, 100, 20),
            ctl(E_ANSI, Kind::Radio(false), "ANSI", 278, 272, 70, 20),
            ctl(1022, Kind::Label, "Line ending:", 24, 312, 70, 18),
            ctl(L_CRLF, Kind::Radio(true), "Windows (CR LF)", 98, 310, 130, 20),
            ctl(L_LF, Kind::Radio(false), "Unix (LF)", 236, 310, 90, 20),
            ctl(L_CR, Kind::Radio(false), "Mac (CR)", 334, 310, 90, 20),
            ctl(1030, Kind::Group, "Live preview", 12, 356, 416, 52),
            ctl(REMOTE, Kind::Check, "Load images from the web (applies after restart)", 24, 378, 380, 20),
        ],
        ok: "OK",
        on_change: None,
        on_apply: None,
    };
    let v = form::run(host, spec, |f| {
        for (id, on) in [
            (SPACES, cur.use_spaces),
            (LINENO, cur.line_numbers),
            (FOLD, cur.fold_margin),
            (BOOKMARK, cur.bookmark_margin),
            (CARET, cur.caret_line),
            (SMART, cur.smart_highlight),
            (REMOTE, cur.preview_remote_images),
            (TOOLBAR, cur.show_toolbar),
            (TABBAR, cur.show_tabbar),
            (STATUS, cur.show_statusbar),
        ] {
            f.set_checked(id, on);
        }
        f.set_checked(
            match cur.new_doc_encoding.as_str() {
                "utf8bom" => E_BOM,
                "ansi" => E_ANSI,
                _ => E_UTF8,
            },
            true,
        );
        f.set_checked(
            match cur.new_doc_eol.as_str() {
                "lf" => L_LF,
                "cr" => L_CR,
                _ => L_CRLF,
            },
            true,
        );
    })?;
    let mut s = Settings {
        font: v.text(FONT).trim().to_string(),
        font_size: number(v.text(SIZE), cur.font_size),
        tab_width: number(v.text(TAB), cur.tab_width),
        use_spaces: v.checked(SPACES),
        edge_column: number(v.text(EDGE), cur.edge_column),
        line_numbers: v.checked(LINENO),
        fold_margin: v.checked(FOLD),
        bookmark_margin: v.checked(BOOKMARK),
        caret_line: v.checked(CARET),
        smart_highlight: v.checked(SMART),
        preview_remote_images: v.checked(REMOTE),
        show_toolbar: v.checked(TOOLBAR),
        show_tabbar: v.checked(TABBAR),
        show_statusbar: v.checked(STATUS),
        new_doc_encoding: if v.checked(E_BOM) { "utf8bom" } else if v.checked(E_ANSI) { "ansi" } else { "utf8" }.into(),
        new_doc_eol: if v.checked(L_LF) { "lf" } else if v.checked(L_CR) { "cr" } else { "crlf" }.into(),
        ..cur.clone()
    };
    s.clamp();
    Some(s)
}

// ── Style Configurator ───────────────────────────────────────────────────────

pub struct StyleEdit {
    /// Throw away every override for this theme.
    pub reset: bool,
    /// (role key, "#RRGGBB" as typed) for every role.
    pub entered: Vec<(&'static str, String)>,
}

fn pick_color(owner: HWND, start: (u8, u8, u8)) -> Option<(u8, u8, u8)> {
    let mut custom = [COLORREF(0x00FF_FFFF); 16];
    let mut cc = CHOOSECOLORW {
        lStructSize: size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: owner,
        rgbResult: COLORREF(start.0 as u32 | (start.1 as u32) << 8 | (start.2 as u32) << 16),
        lpCustColors: custom.as_mut_ptr(),
        Flags: CC_RGBINIT | CC_FULLOPEN,
        ..Default::default()
    };
    if !unsafe { ChooseColorW(&mut cc) }.as_bool() {
        return None;
    }
    let c = cc.rgbResult.0;
    Some((c as u8, (c >> 8) as u8, (c >> 16) as u8))
}

/// Settings > Style Configurator: colours of the theme in use.
pub fn style_configurator(host: HWND, theme: &Theme) -> Option<StyleEdit> {
    const FIRST: usize = 100; // edit i is FIRST + i
    const PICK: usize = 200; // button i is PICK + i
    const RESET: usize = 300;
    fn on_click(f: &Form, id: usize) {
        if (PICK..PICK + ROLES.len()).contains(&id) {
            let edit = FIRST + (id - PICK);
            let start = settings::parse_hex(&f.text(edit)).unwrap_or((0, 0, 0));
            if let Some(c) = pick_color(f.hwnd(), start) {
                f.set_text(edit, &settings::to_hex(c));
            }
        }
    }
    let mut ctls = vec![ctl(10, Kind::Label, if theme.dark { "Colours of the dark theme (#RRGGBB):" } else { "Colours of the light theme (#RRGGBB):" }, 16, 12, 300, 18)];
    for (i, (label, key)) in ROLES.iter().enumerate() {
        let y = 40 + i as i32 * 26;
        let hex = settings::color_of(theme, key).map(settings::to_hex).unwrap_or_default();
        ctls.push(ctl(400 + i, Kind::Label, label, 16, y + 3, 170, 18));
        ctls.push(ctl(FIRST + i, Kind::Edit, &hex, 190, y, 84, 22));
        ctls.push(ctl(PICK + i, Kind::Button, "...", 282, y - 1, 30, 24));
    }
    ctls.push(ctl(RESET, Kind::Check, "Reset every colour to the default", 16, 40 + ROLES.len() as i32 * 26 + 6, 290, 20));
    let spec = Spec {
        title: "Style Configurator",
        size: (336, 40 + ROLES.len() as i32 * 26 + 78),
        ctls,
        ok: "OK",
        on_change: Some(on_click),
        on_apply: None,
    };
    let v = form::run(host, spec, |_| {})?;
    Some(StyleEdit { reset: v.checked(RESET), entered: ROLES.iter().enumerate().map(|(i, (_, key))| (*key, v.text(FIRST + i).to_string())).collect() })
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
            let result: Result<String, String> = if text.is_empty() {
                Ok(String::new())
            } else {
                match shortcuts::parse(&text) {
                    None => Err("That is not a valid shortcut (try Ctrl+Shift+K, F9, Alt+Left)".into()),
                    Some(chord) => {
                        let canon = shortcuts::format(&chord);
                        ROWS.with(|r| {
                            let rows = r.borrow();
                            match rows.iter().enumerate().find(|(j, row)| *j != i && shortcuts::parse(&row.current).is_some_and(|c| c == chord)) {
                                Some((_, other)) => Err(format!("Already used by \"{}\"", other.name)),
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
                Err(e) => f.set_text(STATUS, &e),
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

/// Settings > Shortcut Mapper. Returns the edited rows on OK.
pub fn shortcut_mapper(host: HWND, rows: Vec<MapRow>) -> Option<Vec<MapRow>> {
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
            ctl(105, Kind::Label, "Select a command, type a shortcut such as Ctrl+Shift+K, and press Set.", 12, 396, 576, 18),
        ],
        ok: "OK",
        on_change: Some(mapper_click),
        on_apply: None,
    };
    let accepted = form::run(host, spec, |f| {
        let rows: Vec<Vec<String>> = ROWS.with(|r| r.borrow().iter().map(|row| vec![row.name.clone(), row.current.clone()]).collect());
        f.list_fill(100, &[("Command", 380), ("Shortcut", 170)], &rows);
    });
    accepted.map(|_| ROWS.with(|r| r.borrow().clone()))
}

// ── hashes ───────────────────────────────────────────────────────────────────

/// Tools > <algorithm> > Generate...: hash some typed text.
pub fn hash_text(host: HWND, algo: Algo) {
    const INPUT: usize = 101;
    const RESULT: usize = 102;
    const COPY: usize = 103;
    let apply = move |v: &Values, f: &Form| {
        let digest = algo.hash_bytes(v.text(INPUT).as_bytes());
        f.set_text(RESULT, &digest);
        if v.checked(COPY) {
            clip::set_text(host, &digest);
        }
        true
    };
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
        on_change: None,
        on_apply: Some(&apply),
    };
    let _ = form::run(host, spec, |_| {});
}

/// A read-only block of text with a Copy button.
pub fn text_result(host: HWND, title: &str, text: &str) {
    let apply = move |v: &Values, _f: &Form| {
        clip::set_text(host, v.text(101));
        true
    };
    let spec = Spec {
        title,
        size: (640, 380),
        ctls: vec![ctl(101, Kind::MultiEdit(true), text, 12, 12, 616, 306)],
        ok: "Copy",
        on_change: None,
        on_apply: Some(&apply),
    };
    let _ = form::run(host, spec, |_| {});
}
