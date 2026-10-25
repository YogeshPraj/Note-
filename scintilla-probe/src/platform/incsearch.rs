//! Incremental Search: a small modeless window; the editor jumps to the first
//! match as you type. Enter / Prev step through matches, Esc closes it.

use std::cell::Cell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

/// Sent to the host: wParam is one of the `K_*` codes.
pub const WM_INCR: u32 = WM_APP + 80;
pub const K_CHANGED: usize = 0;
pub const K_NEXT: usize = 1;
pub const K_PREV: usize = 2;
pub const K_CLOSE: usize = 3;

const C_EDIT: usize = 100;
const C_STATUS: usize = 101;
const B_NEXT: usize = 1;
const B_CLOSE: usize = 2;
const B_PREV: usize = 3;

thread_local! {
    static WND: Cell<Option<(HWND, HWND, HWND)>> = const { Cell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn hwnd() -> Option<HWND> {
    WND.with(|c| c.get()).map(|w| w.0)
}

pub fn text() -> String {
    let Some((_, edit, _)) = WND.with(|c| c.get()) else { return String::new() };
    unsafe {
        let n = GetWindowTextLengthW(edit).max(0) as usize;
        let mut buf = vec![0u16; n + 1];
        let got = GetWindowTextW(edit, &mut buf).max(0) as usize;
        String::from_utf16_lossy(&buf[..got])
    }
}

pub fn set_status(s: &str) {
    if let Some((_, _, st)) = WND.with(|c| c.get()) {
        let t = wide(s);
        unsafe {
            let _ = SetWindowTextW(st, PCWSTR(t.as_ptr()));
        }
    }
}

pub fn close() {
    if let Some((h, _, _)) = WND.with(|c| c.get()) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let tell = |k: usize| unsafe {
        if let Ok(host) = GetWindow(hwnd, GW_OWNER) {
            let _ = PostMessageW(Some(host), WM_INCR, WPARAM(k), LPARAM(0));
        }
    };
    match msg {
        WM_ERASEBKGND => {
            let dc = windows::Win32::Graphics::Gdi::HDC(wp.0 as *mut core::ffi::c_void);
            let mut r = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut r);
            }
            super::modern::fill(dc, &r, super::modern::pal().bg);
            LRESULT(1)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORBTN => super::modern::ctl_color(msg, wp, lp),
        WM_DRAWITEM => {
            let dis = unsafe { &*(lp.0 as *const windows::Win32::UI::Controls::DRAWITEMSTRUCT) };
            super::modern::paint_button(dis, dis.CtlID as usize == B_NEXT);
            LRESULT(1)
        }
        0x0400 => LRESULT(((0x534B_u32 << 16) | B_NEXT as u32) as isize), // DM_GETDEFID: Enter = Next
        WM_COMMAND => {
            let (id, code) = (wp.0 & 0xFFFF, (wp.0 >> 16) & 0xFFFF);
            match (id, code) {
                (C_EDIT, 0x0300) => tell(K_CHANGED), // EN_CHANGE
                (B_NEXT, _) => tell(K_NEXT),
                (B_PREV, _) => tell(K_PREV),
                (B_CLOSE, _) => tell(K_CLOSE),
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            tell(K_CLOSE);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// Give an edit box the themed (rounded on Windows 11) client-edge border.
fn sunken(h: HWND) {
    unsafe {
        let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
        SetWindowLongPtrW(h, GWL_EXSTYLE, ex | WS_EX_CLIENTEDGE.0 as isize);
        let _ = SetWindowPos(h, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
    }
}

fn make(parent: HWND, class: PCWSTR, text: &str, style: u32, id: usize, r: (i32, i32, i32, i32)) -> HWND {
    unsafe {
        let t = wide(text);
        let h = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(t.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
            r.0,
            r.1,
            r.2,
            r.3,
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

/// Show the window (creating it on first use) with an empty box.
pub fn open(host: HWND) {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppIncr"), &mut wc).is_err() {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: hinst.into(),
                lpszClassName: w!("NoteppIncr"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                ..Default::default() // no class brush: WM_ERASEBKGND paints the themed background
            });
        }
        let (win, edit, _) = match WND.with(|c| c.get()) {
            Some(w) => w,
            None => {
                let style = WS_POPUP | WS_CAPTION | WS_SYSMENU;
                let ex = WS_EX_CONTROLPARENT | WS_EX_TOOLWINDOW;
                let mut rc = RECT { left: 0, top: 0, right: 360, bottom: 136 };
                let _ = AdjustWindowRectEx(&mut rc, style, false, ex);
                let mut hr = RECT::default();
                let _ = GetWindowRect(host, &mut hr);
                let title = wide("Incremental Search");
                let win = CreateWindowExW(
                    ex,
                    w!("NoteppIncr"),
                    PCWSTR(title.as_ptr()),
                    style,
                    hr.right - (rc.right - rc.left) - 40,
                    hr.top + 110,
                    rc.right - rc.left,
                    rc.bottom - rc.top,
                    Some(host),
                    None,
                    Some(hinst.into()),
                    None,
                )
                .unwrap();
                make(win, w!("STATIC"), "Search for:", 0, 99, (14, 12, 332, 18));
                let edit = make(win, w!("EDIT"), "", 0x0001_0000 | 0x80, C_EDIT, (14, 34, 332, 26));
                sunken(edit);
                let status = make(win, w!("STATIC"), "", 0, C_STATUS, (14, 68, 332, 18));
                for (text, id, x, w) in [("&Prev", B_PREV, 14, 84), ("&Next", B_NEXT, 106, 84), ("Close", B_CLOSE, 262, 84)] {
                    let b = make(win, w!("BUTTON"), text, 0x0001_0000 | 0xB, id, (x, 94, w, 30));
                    super::modern::track_hover(b);
                }
                WND.with(|c| c.set(Some((win, edit, status))));
                (win, edit, status)
            }
        };
        super::modern::frame(win);
        super::modern::style_control(edit, super::modern::Ctl::Edit);
        let empty = wide("");
        let _ = SetWindowTextW(edit, PCWSTR(empty.as_ptr()));
        set_status("");
        let _ = ShowWindow(win, SW_SHOW);
        let _ = SetFocus(Some(edit));
    }
}
