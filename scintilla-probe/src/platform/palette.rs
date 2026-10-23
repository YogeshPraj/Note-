//! Command Palette (Ctrl+Shift+P) and Quick Open (Ctrl+P): one popup with a
//! filter box and a result list. The popup only filters and displays; the
//! choice is posted to the host as `WM_PALETTE` (wParam = kind, lParam = the
//! item's id) and executed in `win32.rs`.

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::chrome::ui_font_handle;

pub const WM_PALETTE: u32 = WM_APP + 40;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Commands = 0,
    Files = 1,
}

#[derive(Clone)]
pub struct Item {
    pub label: String,
    /// Right-hand detail: a shortcut, or the folder of a file.
    pub hint: String,
    /// Command id, or index into the host's file list.
    pub id: usize,
}

const WIDTH: i32 = 640;
const ROW_H: i32 = 26;
const MAX_ROWS: usize = 11;
const EDIT_H: i32 = 30;
const C_EDIT: usize = 1;
const C_LIST: usize = 2;

thread_local! {
    static WIN: Cell<Option<(HWND, HWND, HWND)>> = const { Cell::new(None) };
    static KIND: Cell<Kind> = const { Cell::new(Kind::Commands) };
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    static SHOWN: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

static EDIT_PREV: AtomicIsize = AtomicIsize::new(0);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn create(host: HWND) -> (HWND, HWND, HWND) {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let class = w!("NoteppPalette");
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), class, &mut wc).is_err() {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(palette_proc),
                hInstance: hinst.into(),
                lpszClassName: class,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hbrBackground: HBRUSH(6 as *mut core::ffi::c_void), // COLOR_WINDOW + 1
                ..Default::default()
            });
        }
        let win = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_CONTROLPARENT,
            class,
            PCWSTR::null(),
            WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
            0,
            0,
            WIDTH,
            100,
            Some(host),
            None,
            Some(hinst.into()),
            None,
        )
        .unwrap();
        let font = ui_font_handle();
        let child = |class: PCWSTR, style: u32, id: usize| {
            let h = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                0,
                0,
                10,
                10,
                Some(win),
                Some(HMENU(id as *mut core::ffi::c_void)),
                Some(hinst.into()),
                None,
            )
            .unwrap();
            SendMessageW(h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            h
        };
        let edit = child(w!("EDIT"), 0x80 | 0x0001_0000, C_EDIT); // ES_AUTOHSCROLL | WS_TABSTOP
        // LBS_NOTIFY | LBS_NOINTEGRALHEIGHT | WS_VSCROLL
        let list = child(w!("LISTBOX"), 0x0001 | 0x0100 | 0x0020_0000, C_LIST);
        SendMessageW(list, 0x01A0, Some(WPARAM(0)), Some(LPARAM(ROW_H as isize))); // LB_SETITEMHEIGHT
        EDIT_PREV.store(SetWindowLongPtrW(edit, GWLP_WNDPROC, edit_proc as *const () as isize), Ordering::Relaxed);
        (win, edit, list)
    }
}

/// Show the palette with `items`, centred near the top of `host`.
pub fn open(host: HWND, kind: Kind, items: Vec<Item>) {
    let (win, edit, _) = match WIN.with(|c| c.get()) {
        Some(w) => w,
        None => {
            let w = create(host);
            WIN.with(|c| c.set(Some(w)));
            w
        }
    };
    KIND.with(|c| c.set(kind));
    ITEMS.with(|c| *c.borrow_mut() = items);
    unsafe {
        let _ = SetWindowTextW(edit, w!(""));
        let mut hr = RECT::default();
        let _ = GetWindowRect(host, &mut hr);
        let x = hr.left + ((hr.right - hr.left) - WIDTH) / 2;
        let _ = SetWindowPos(win, None, x, hr.top + 80, WIDTH, 100, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    refilter();
    unsafe {
        let _ = ShowWindow(win, SW_SHOW);
        let _ = SetFocus(Some(edit));
    }
}

pub fn hide() {
    if let Some((win, _, _)) = WIN.with(|c| c.get()) {
        unsafe {
            let _ = ShowWindow(win, SW_HIDE);
        }
    }
}

/// Every whitespace-separated word of `query` must occur in `text`
/// (case-insensitive). Earlier matches score better.
pub fn matches(query: &str, text: &str) -> Option<usize> {
    let t = text.to_lowercase();
    let mut score = 0;
    for word in query.to_lowercase().split_whitespace() {
        score += t.find(word)?;
    }
    Some(score)
}

fn refilter() {
    let Some((win, edit, list)) = WIN.with(|c| c.get()) else { return };
    let q = unsafe {
        let n = GetWindowTextLengthW(edit).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(edit, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    };
    let shown: Vec<usize> = ITEMS.with(|items| {
        let items = items.borrow();
        let mut scored: Vec<(usize, usize)> =
            items.iter().enumerate().filter_map(|(i, it)| matches(&q, &format!("{} {}", it.label, it.hint)).map(|s| (s, i))).collect();
        scored.sort_by_key(|(s, i)| (*s, *i));
        scored.into_iter().take(300).map(|(_, i)| i).collect()
    });
    unsafe {
        SendMessageW(list, 0x0184, None, None); // LB_RESETCONTENT
        ITEMS.with(|items| {
            let items = items.borrow();
            for &i in &shown {
                let it = &items[i];
                let text = if it.hint.is_empty() { it.label.clone() } else { format!("{}    \u{2014}  {}", it.label, it.hint) };
                let t = wide(&text);
                SendMessageW(list, 0x0180, None, Some(LPARAM(t.as_ptr() as isize))); // LB_ADDSTRING
            }
        });
        if !shown.is_empty() {
            SendMessageW(list, 0x0186, Some(WPARAM(0)), None); // LB_SETCURSEL
        }
        let rows = shown.len().clamp(1, MAX_ROWS) as i32;
        let list_h = rows * ROW_H + 4;
        let _ = MoveWindow(edit, 8, 6, WIDTH - 18, EDIT_H - 6, true);
        let _ = MoveWindow(list, 0, EDIT_H + 6, WIDTH - 2, list_h, true);
        let mut wr = RECT::default();
        let _ = GetWindowRect(win, &mut wr);
        let _ = SetWindowPos(win, None, wr.left, wr.top, WIDTH, EDIT_H + 6 + list_h + 2, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    SHOWN.with(|c| *c.borrow_mut() = shown);
}

fn move_selection(delta: isize) {
    let Some((_, _, list)) = WIN.with(|c| c.get()) else { return };
    unsafe {
        let count = SendMessageW(list, 0x018B, None, None).0; // LB_GETCOUNT
        if count <= 0 {
            return;
        }
        let cur = SendMessageW(list, 0x0188, None, None).0; // LB_GETCURSEL
        let next = (cur + delta).clamp(0, count - 1);
        SendMessageW(list, 0x0186, Some(WPARAM(next as usize)), None);
    }
}

fn accept() {
    let Some((win, _, list)) = WIN.with(|c| c.get()) else { return };
    let sel = unsafe { SendMessageW(list, 0x0188, None, None).0 };
    let id = SHOWN.with(|s| s.borrow().get(sel.max(0) as usize).copied()).and_then(|i| ITEMS.with(|it| it.borrow().get(i).map(|x| x.id)));
    hide();
    if let (Some(id), Ok(host)) = (id, unsafe { GetWindow(win, GW_OWNER) }) {
        let kind = KIND.with(|c| c.get()) as usize;
        unsafe {
            let _ = PostMessageW(Some(host), WM_PALETTE, WPARAM(kind), LPARAM(id as isize));
        }
    }
}

unsafe extern "system" fn edit_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let prev: WNDPROC = unsafe { std::mem::transmute(EDIT_PREV.load(Ordering::Relaxed)) };
    match msg {
        WM_KEYDOWN => match wp.0 as u32 {
            0x26 => {
                move_selection(-1);
                return LRESULT(0);
            }
            0x28 => {
                move_selection(1);
                return LRESULT(0);
            }
            0x21 => {
                move_selection(-(MAX_ROWS as isize));
                return LRESULT(0);
            }
            0x22 => {
                move_selection(MAX_ROWS as isize);
                return LRESULT(0);
            }
            0x0D => {
                accept();
                return LRESULT(0);
            }
            0x1B => {
                hide();
                return LRESULT(0);
            }
            _ => {}
        },
        WM_CHAR if wp.0 == 0x0D || wp.0 == 0x1B => return LRESULT(0), // no beep
        _ => {}
    }
    unsafe { CallWindowProcW(prev, hwnd, msg, wp, lp) }
}

unsafe extern "system" fn palette_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COMMAND => {
            let id = wp.0 & 0xFFFF;
            let code = (wp.0 >> 16) & 0xFFFF;
            if id == C_EDIT && code == 0x0300 {
                refilter(); // EN_CHANGE
            } else if id == C_LIST && code == 2 {
                accept(); // LBN_DBLCLK
            }
            LRESULT(0)
        }
        WM_ACTIVATE if (wp.0 & 0xFFFF) == 0 => {
            hide();
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_words_must_match() {
        assert!(matches("sort asc", "Sort Lines Ascending").is_some());
        assert!(matches("sort zzz", "Sort Lines Ascending").is_none());
        assert!(matches("", "anything").is_some());
    }

    #[test]
    fn earlier_matches_rank_higher() {
        assert!(matches("save", "Save As").unwrap() < matches("save", "Close and Save").unwrap());
    }
}
