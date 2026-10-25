//! The docked Find panel (Find / Replace / Find in Files / Mark) and the Go To
//! dialog. Both only own their widgets; actions are posted to the host as
//! `WM_PANEL` and executed in `win32.rs`, which reads the options back with
//! `read_opts()` and reports results with `set_status()` / `set_results()`.

use std::cell::Cell;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{HBRUSH, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE, RedrawWindow};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::Shell::{BIF_NEWDIALOGSTYLE, BROWSEINFOW, SHBrowseForFolderW, SHGetPathFromIDListW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use crate::core::search::Opts;

pub const WM_PANEL: u32 = WM_APP + 30;

// Messages posted to the host (WM_PANEL wParam).
pub const P_NEXT: usize = 1; // IDOK: Enter in the panel
pub const P_CLOSE: usize = 2; // IDCANCEL: Esc / Close button
pub const P_PREV: usize = 6020;
pub const P_FINDALL: usize = 6021;
pub const P_COUNT: usize = 6022;
pub const P_REPLACE: usize = 6023;
pub const P_REPLACE_ALL: usize = 6024;
pub const P_FIF: usize = 6025;
pub const P_MARK: usize = 6026;
pub const P_CLEAR: usize = 6027;
pub const P_BROWSE: usize = 6028;
pub const P_LIST_OPEN: usize = 6040; // lParam = result index
pub const P_RELAYOUT: usize = 6041;
pub const P_GOTO: usize = 6042; // lParam = line number

const C_FIND: usize = 6001;
const C_REPL: usize = 6002;
const C_DIR: usize = 6003;
const C_FILTER: usize = 6004;
const C_CASE: usize = 6010;
const C_WORD: usize = 6011;
const C_WRAP: usize = 6012;
const C_REGEX: usize = 6013;
const C_DOT: usize = 6014;
const C_STATUS: usize = 6030;
const C_LIST: usize = 6031;
const C_TAB: usize = 6050; // + 0..3: Find, Replace, Find in Files, Mark
const G_EDIT: usize = 6101;
const G_LABEL: usize = 6102;

const RESULTS_H: i32 = 180;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Find = 0,
    Replace = 1,
    InFiles = 2,
    Mark = 3,
}

#[derive(Clone, Copy)]
struct Panel {
    hwnd: HWND,
    tab: [HWND; 4],
    closex: HWND,
    status: HWND,
    find_lbl: HWND,
    find: HWND,
    repl_lbl: HWND,
    repl: HWND,
    dir_lbl: HWND,
    dir: HWND,
    filt_lbl: HWND,
    filt: HWND,
    browse: HWND,
    case: HWND,
    word: HWND,
    wrap: HWND,
    regex: HWND,
    dot: HWND,
    next: HWND,
    prev: HWND,
    findall: HWND,
    count: HWND,
    replace: HWND,
    repl_all: HWND,
    fif: HWND,
    mark: HWND,
    clear: HWND,
    close: HWND,
    list: HWND,
}

thread_local! {
    static PANEL: Cell<Option<Panel>> = const { Cell::new(None) };
    static MODE: Cell<Mode> = const { Cell::new(Mode::Find) };
    static SHOWN: Cell<bool> = const { Cell::new(false) };
    static RESULTS: Cell<bool> = const { Cell::new(false) };
    static GOTO: Cell<Option<(HWND, HWND, HWND)>> = const { Cell::new(None) };
}

// ---- small widget helpers ------------------------------------------------

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn text_of(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(h, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

fn set_text(h: HWND, s: &str) {
    let t = wide(s);
    unsafe {
        let _ = SetWindowTextW(h, PCWSTR(t.as_ptr()));
    }
}

/// Native check boxes keep their own state.
fn is_checked(h: HWND) -> bool {
    unsafe { SendMessageW(h, 0x00F0, None, None).0 == 1 } // BM_GETCHECK
}

fn set_checked(h: HWND, on: bool) {
    unsafe { SendMessageW(h, 0x00F1, Some(WPARAM(on as usize)), None) }; // BM_SETCHECK
}

fn show(h: HWND, on: bool) {
    unsafe {
        let _ = ShowWindow(h, if on { SW_SHOW } else { SW_HIDE });
    }
}

fn place(h: HWND, x: i32, y: i32, w: i32, ht: i32) {
    unsafe {
        let _ = MoveWindow(h, x, y, w, ht, true);
    }
}

const TABSTOP: u32 = 0x0001_0000;
const BS_PUSH: u32 = 0x0;
const BS_DEFPUSH: u32 = 0x1;
const BS_CHECK: u32 = 0x3;
const ES_AUTOHSCROLL: u32 = 0x80;

/// Give an edit box the themed (rounded on Windows 11) client-edge border.
fn sunken(h: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_CLIENTEDGE.0 as isize);
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
    }
}

fn control(parent: HWND, class: PCWSTR, text: &str, style: u32, id: usize) -> HWND {
    unsafe {
        let t = wide(text);
        let h = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            0,
            0,
            10,
            10,
            Some(parent),
            Some(HMENU(id as *mut core::ffi::c_void)),
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap();
        SendMessageW(h, WM_SETFONT, Some(WPARAM(super::chrome::ui_font_handle().0 as usize)), Some(LPARAM(1)));
        h
    }
}

fn register_class(name: PCWSTR, proc: WNDPROC, brush: bool) {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), name, &mut wc).is_ok() {
            return;
        }
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: proc,
            hInstance: hinst.into(),
            lpszClassName: name,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            hbrBackground: if brush { HBRUSH(16 as *mut core::ffi::c_void) } else { HBRUSH::default() }, // COLOR_BTNFACE + 1
            ..Default::default()
        });
    }
}

fn post_host(panel_or_dialog: HWND, id: usize, arg: isize) {
    unsafe {
        if let Ok(host) = GetParent(panel_or_dialog).or_else(|_| GetWindow(panel_or_dialog, GW_OWNER)) {
            let _ = PostMessageW(Some(host), WM_PANEL, WPARAM(id), LPARAM(arg));
        }
    }
}

// ---- the panel ----------------------------------------------------------

pub fn create(host: HWND) {
    register_class(w!("NoteppFindPanel"), Some(panel_proc), false);
    unsafe {
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            w!("NoteppFindPanel"),
            PCWSTR::null(),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            10,
            10,
            Some(host),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap();

        // The four modes are native toggle buttons (push-like radio buttons): they theme
        // properly in dark mode, unlike the tab control.
        let tab: [HWND; 4] = std::array::from_fn(|i| {
            // BS_AUTORADIOBUTTON | BS_PUSHLIKE (first one starts the group)
            let style = 0x9 | 0x1000 | if i == 0 { 0x0002_0000 | TABSTOP } else { 0 };
            let h = control(hwnd, w!("BUTTON"), ["Find", "Replace", "Find in Files", "Mark"][i], style, C_TAB + i);
            super::modern::style_control(h, super::modern::Ctl::Button);
            h
        });
        let edit = |id| {
            let h = control(hwnd, w!("EDIT"), "", TABSTOP | ES_AUTOHSCROLL, id);
            sunken(h);
            super::modern::style_control(h, super::modern::Ctl::Edit);
            h
        };
        let label = |t: &str| control(hwnd, w!("STATIC"), t, 0, 0);
        let button = |t: &str, id, def: bool| {
            let h = control(hwnd, w!("BUTTON"), t, TABSTOP | if def { BS_DEFPUSH } else { BS_PUSH }, id);
            super::modern::style_control(h, super::modern::Ctl::Button);
            h
        };
        let check = |t: &str, id| {
            let h = control(hwnd, w!("BUTTON"), t, TABSTOP | BS_CHECK, id);
            super::modern::style_control(h, super::modern::Ctl::Button);
            h
        };

        let closex = control(hwnd, w!("BUTTON"), "✕", BS_PUSH | 0x8000, P_CLOSE); // BS_FLAT
        super::modern::style_control(closex, super::modern::Ctl::Button);
        let p = Panel {
            hwnd,
            tab,
            closex,
            status: control(hwnd, w!("STATIC"), "", 0, C_STATUS),
            find_lbl: label("Find what :"),
            find: edit(C_FIND),
            repl_lbl: label("Replace with :"),
            repl: edit(C_REPL),
            dir_lbl: label("Directory :"),
            dir: edit(C_DIR),
            filt_lbl: label("Filters :"),
            filt: edit(C_FILTER),
            browse: button("...", P_BROWSE, false),
            case: check("Match case", C_CASE),
            word: check("Match whole word only", C_WORD),
            wrap: check("Wrap around", C_WRAP),
            regex: check("Regular expression", C_REGEX),
            dot: check(". matches newline", C_DOT),
            next: button("Find Next", P_NEXT, true),
            prev: button("Find Previous", P_PREV, false),
            findall: button("Find All in Current Document", P_FINDALL, false),
            count: button("Count", P_COUNT, false),
            replace: button("Replace", P_REPLACE, false),
            repl_all: button("Replace All", P_REPLACE_ALL, false),
            fif: button("Find All", P_FIF, false),
            mark: button("Mark All", P_MARK, false),
            clear: button("Clear all marks", P_CLEAR, false),
            close: button("Close", P_CLOSE, false),
            list: {
                // WS_VSCROLL | LBS_NOTIFY | LBS_NOINTEGRALHEIGHT
                let h = control(hwnd, w!("LISTBOX"), "", 0x0020_0000 | 0x0000_0001 | 0x0000_0100 | TABSTOP, C_LIST);
                sunken(h);
                super::modern::style_control(h, super::modern::Ctl::Button);
                h
            },
        };
        set_checked(p.wrap, true);
        set_text(p.filt, "*.*");
        PANEL.with(|c| c.set(Some(p)));
        show(hwnd, false);
    }
}

pub fn hwnd() -> Option<HWND> {
    PANEL.with(|c| c.get()).map(|p| p.hwnd)
}

pub fn mode() -> Mode {
    MODE.with(|c| c.get())
}

pub fn visible() -> bool {
    SHOWN.with(|c| c.get())
}

/// Height the host should give the panel (0 when hidden).
pub fn height() -> i32 {
    if !visible() {
        return 0;
    }
    let base = match MODE.with(|c| c.get()) {
        Mode::Find | Mode::Mark => 108,
        Mode::Replace => 142,
        Mode::InFiles => 176,
    };
    base + if results_visible() { RESULTS_H } else { 0 }
}

fn results_visible() -> bool {
    RESULTS.with(|c| c.get()) && MODE.with(|c| c.get()) != Mode::Mark
}

/// Position the child controls for the current mode and width.
pub fn arrange(width: i32) {
    let Some(p) = PANEL.with(|c| c.get()) else { return };
    let mode = MODE.with(|c| c.get());
    let btn = |h: HWND, x: &mut i32, y: i32, w: i32| {
        place(h, *x, y, w, 26);
        *x += w + 6;
    };
    let (r1, r2, r3) = (40, 74, 108);
    for (i, t) in p.tab.iter().enumerate() {
        place(*t, 8 + i as i32 * 86, 6, 84, 28);
    }
    place(p.status, 356, 11, (width - 356 - 44).max(0), 18);
    place(p.closex, width - 30, 7, 24, 24);

    // Everything starts hidden; each mode reveals what it uses.
    let all = [
        p.find_lbl, p.find, p.repl_lbl, p.repl, p.dir_lbl, p.dir, p.filt_lbl, p.filt, p.browse, p.case, p.word, p.wrap, p.regex,
        p.dot, p.next, p.prev, p.findall, p.count, p.replace, p.repl_all, p.fif, p.mark, p.clear, p.close,
    ];
    for h in all {
        show(h, false);
    }
    let label = |h: HWND, y: i32| {
        place(h, 12, y + 4, 90, 20);
        show(h, true);
    };
    let edit = |h: HWND, y: i32| {
        place(h, 108, y, 250, 24);
        show(h, true);
    };
    let place_btn = |h: HWND, x: &mut i32, y: i32, w: i32| {
        btn(h, x, y, w);
        show(h, true);
    };
    let checks = |y: i32| {
        let mut x = 108;
        for (h, w) in [(p.case, 100), (p.word, 172), (p.wrap, 110), (p.regex, 138), (p.dot, 140)] {
            place(h, x, y, w, 22);
            show(h, true);
            x += w + 6;
        }
    };

    label(p.find_lbl, r1);
    edit(p.find, r1);
    let mut x = 368;
    let mut checks_y = r2;
    match mode {
        Mode::Find => {
            place_btn(p.next, &mut x, r1 - 1, 84);
            place_btn(p.prev, &mut x, r1 - 1, 96);
            place_btn(p.findall, &mut x, r1 - 1, 200);
            place_btn(p.count, &mut x, r1 - 1, 64);
            place_btn(p.close, &mut x, r1 - 1, 64);
        }
        Mode::Replace => {
            place_btn(p.next, &mut x, r1 - 1, 84);
            place_btn(p.prev, &mut x, r1 - 1, 96);
            place_btn(p.count, &mut x, r1 - 1, 64);
            place_btn(p.findall, &mut x, r1 - 1, 200);
            label(p.repl_lbl, r2);
            edit(p.repl, r2);
            let mut x2 = 368;
            place_btn(p.replace, &mut x2, r2 - 1, 84);
            place_btn(p.repl_all, &mut x2, r2 - 1, 96);
            place_btn(p.close, &mut x2, r2 - 1, 64);
            checks_y = r3;
        }
        Mode::InFiles => {
            place_btn(p.fif, &mut x, r1 - 1, 84);
            place_btn(p.close, &mut x, r1 - 1, 64);
            label(p.dir_lbl, r2);
            edit(p.dir, r2);
            let mut x2 = 368;
            place_btn(p.browse, &mut x2, r2 - 1, 34);
            label(p.filt_lbl, r3);
            edit(p.filt, r3);
            checks_y = r3 + 34;
        }
        Mode::Mark => {
            place_btn(p.mark, &mut x, r1 - 1, 84);
            place_btn(p.clear, &mut x, r1 - 1, 120);
            place_btn(p.close, &mut x, r1 - 1, 64);
        }
    }
    checks(checks_y);
    if mode == Mode::InFiles {
        show(p.findall, false);
    }

    let base = height() - if results_visible() { RESULTS_H } else { 0 };
    place(p.list, 8, base, (width - 16).max(0), RESULTS_H - 8);
    show(p.list, results_visible());
}

/// Show the panel in `mode`, prefilling the search text.
pub fn open(mode: Mode, prefill: &str, last_search: &str, dir: &str) {
    let Some(p) = PANEL.with(|c| c.get()) else { return };
    MODE.with(|c| c.set(mode));
    SHOWN.with(|c| c.set(true));
    unsafe {
        for (i, t) in p.tab.iter().enumerate() {
            set_checked(*t, i == mode as usize);
        }
        let _ = ShowWindow(p.hwnd, SW_SHOW);
    }
    if !prefill.is_empty() && !prefill.contains(['\r', '\n']) {
        set_text(p.find, prefill);
    } else if text_of(p.find).is_empty() {
        set_text(p.find, last_search);
    }
    if mode == Mode::InFiles && text_of(p.dir).is_empty() {
        set_text(p.dir, dir);
    }
    post_host(p.hwnd, P_RELAYOUT, 0);
    unsafe {
        let _ = SetFocus(Some(p.find));
        SendMessageW(p.find, 0x00B1, Some(WPARAM(0)), Some(LPARAM(-1))); // EM_SETSEL all
    }
}

pub fn close() {
    let Some(p) = PANEL.with(|c| c.get()) else { return };
    SHOWN.with(|c| c.set(false));
    show(p.hwnd, false);
}

pub fn read_opts() -> Opts {
    let Some(p) = PANEL.with(|c| c.get()) else { return Opts::default() };
    Opts {
        find: text_of(p.find),
        replace: text_of(p.repl),
        match_case: is_checked(p.case),
        whole_word: is_checked(p.word),
        regex: is_checked(p.regex),
        wrap: is_checked(p.wrap),
        dot_all: is_checked(p.dot),
    }
}

pub fn read_dir_filter() -> (PathBuf, String) {
    let Some(p) = PANEL.with(|c| c.get()) else { return (PathBuf::new(), String::new()) };
    (PathBuf::from(text_of(p.dir).trim()), text_of(p.filt))
}

pub fn set_status(s: &str) {
    if let Some(p) = PANEL.with(|c| c.get()) {
        set_text(p.status, s);
    }
}

/// Replace the results list; the host relayouts to make room for it.
pub fn set_results(lines: &[String]) {
    let Some(p) = PANEL.with(|c| c.get()) else { return };
    unsafe {
        SendMessageW(p.list, 0x0184, None, None); // LB_RESETCONTENT
        for l in lines {
            let t = wide(l);
            SendMessageW(p.list, 0x0180, None, Some(LPARAM(t.as_ptr() as isize))); // LB_ADDSTRING
        }
    }
    RESULTS.with(|c| c.set(!lines.is_empty()));
    post_host(p.hwnd, P_RELAYOUT, 0);
}

pub fn set_dir(dir: &str) {
    if let Some(p) = PANEL.with(|c| c.get()) {
        set_text(p.dir, dir);
    }
}

pub fn browse_folder(owner: HWND) -> Option<PathBuf> {
    unsafe {
        let title = wide("Select a folder to search");
        let bi = BROWSEINFOW { hwndOwner: owner, lpszTitle: PCWSTR(title.as_ptr()), ulFlags: BIF_NEWDIALOGSTYLE, ..Default::default() };
        let pidl = SHBrowseForFolderW(&bi);
        if pidl.is_null() {
            return None;
        }
        let mut buf = [0u16; 260];
        if SHGetPathFromIDListW(pidl, &mut buf).as_bool() {
            let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
        } else {
            None
        }
    }
}

/// Re-apply the light/dark native styling after the app theme changed (cheap no-op otherwise).
pub fn retheme() {
    use super::modern::{Ctl, is_dark, style_control};
    static LAST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    let Some(p) = PANEL.with(|c| c.get()) else { return };
    if LAST.swap(is_dark(), std::sync::atomic::Ordering::Relaxed) == is_dark() {
        return;
    }
    for h in [p.find, p.repl, p.dir, p.filt] {
        style_control(h, Ctl::Edit);
    }
    for h in p.tab.into_iter().chain([
        p.closex, p.browse, p.case, p.word, p.wrap, p.regex, p.dot, p.next, p.prev, p.findall, p.count, p.replace, p.repl_all, p.fif, p.mark,
        p.clear, p.close, p.list,
    ]) {
        style_control(h, Ctl::Button);
    }
    unsafe {
        let _ = RedrawWindow(Some(p.hwnd), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME);
    }
}

unsafe extern "system" fn panel_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => super::modern::erase_background(hwnd, wp),
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX | WM_CTLCOLORBTN => super::modern::ctl_color(msg, wp, lp),
        WM_COMMAND => {
            let id = wp.0 & 0xFFFF;
            let code = (wp.0 >> 16) & 0xFFFF;
            match id {
                _ if (C_TAB..C_TAB + 4).contains(&id) && code == 0 => {
                    MODE.with(|c| {
                        c.set(match id - C_TAB {
                            1 => Mode::Replace,
                            2 => Mode::InFiles,
                            3 => Mode::Mark,
                            _ => Mode::Find,
                        })
                    });
                    post_host(hwnd, P_RELAYOUT, 0);
                }
                C_LIST if code == 2 => {
                    // LBN_DBLCLK
                    if let Some(p) = PANEL.with(|c| c.get()) {
                        let sel = unsafe { SendMessageW(p.list, 0x0188, None, None).0 }; // LB_GETCURSEL
                        if sel >= 0 {
                            post_host(hwnd, P_LIST_OPEN, sel);
                        }
                    }
                }
                P_NEXT | P_CLOSE | P_PREV | P_FINDALL | P_COUNT | P_REPLACE | P_REPLACE_ALL | P_FIF | P_MARK | P_CLEAR | P_BROWSE if code == 0 || code == 1 => {
                    post_host(hwnd, id, 0)
                }
                _ => {}
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

// ---- Go To dialog ---------------------------------------------------------------

pub fn goto_hwnd() -> Option<HWND> {
    GOTO.with(|c| c.get()).map(|g| g.0)
}

pub fn open_goto(host: HWND, total_lines: isize) {
    register_class(w!("NoteppGoto"), Some(goto_proc), false);
    let (hwnd, label, edit) = match GOTO.with(|c| c.get()) {
        Some(g) => g,
        None => unsafe {
            let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
            let ex = WS_EX_CONTROLPARENT | WS_EX_TOOLWINDOW;
            let mut rc = RECT { left: 0, top: 0, right: 300, bottom: 118 };
            let _ = AdjustWindowRectEx(&mut rc, style, false, ex);
            let mut host_rc = RECT::default();
            let _ = GetWindowRect(host, &mut host_rc);
            let t = wide("Go To Line");
            let hwnd = CreateWindowExW(
                ex,
                w!("NoteppGoto"),
                PCWSTR(t.as_ptr()),
                style,
                host_rc.right - (rc.right - rc.left) - 40,
                host_rc.top + 110,
                rc.right - rc.left,
                rc.bottom - rc.top,
                Some(host),
                None,
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
            .unwrap();
            let label = control(hwnd, w!("STATIC"), "Line number:", 0, G_LABEL);
            let edit = control(hwnd, w!("EDIT"), "", TABSTOP | ES_AUTOHSCROLL | 0x2000, G_EDIT); // ES_NUMBER
            sunken(edit);
            super::modern::style_control(edit, super::modern::Ctl::Edit);
            let go = control(hwnd, w!("BUTTON"), "&Go", TABSTOP | BS_DEFPUSH, 1);
            let cancel = control(hwnd, w!("BUTTON"), "Cancel", TABSTOP | BS_PUSH, 2);
            place(label, 14, 14, 272, 20);
            place(edit, 14, 38, 272, 26);
            place(go, 118, 78, 80, 26);
            place(cancel, 206, 78, 80, 26);
            GOTO.with(|c| c.set(Some((hwnd, label, edit))));
            (hwnd, label, edit)
        },
    };
    set_text(label, &format!("Line number (1 - {total_lines}):"));
    set_text(edit, "");
    super::modern::frame(hwnd);
    super::modern::style_control(edit, super::modern::Ctl::Edit);
    for id in [1, 2] {
        if let Ok(b) = unsafe { GetDlgItem(Some(hwnd), id) } {
            super::modern::style_control(b, super::modern::Ctl::Button);
        }
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(edit));
    }
}

unsafe extern "system" fn goto_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => super::modern::erase_background(hwnd, wp),
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => super::modern::ctl_color(msg, wp, lp),
        WM_COMMAND => {
            let id = wp.0 & 0xFFFF;
            if let Some((_, _, edit)) = GOTO.with(|c| c.get()) {
                if id == 1 {
                    if let Ok(n) = text_of(edit).trim().parse::<isize>() {
                        post_host(hwnd, P_GOTO, n);
                    }
                    show(hwnd, false);
                } else if id == 2 {
                    show(hwnd, false);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            show(hwnd, false);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
