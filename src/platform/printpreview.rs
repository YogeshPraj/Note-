//! File > Print Preview: the pages exactly as Scintilla will lay them out on the chosen
//! printer's paper, with Page Setup and Print available from the preview window.

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

use super::modern::{self, Ctl};
use super::print::{self, Outcome};
use crate::core::sci::Send;

const ID_PREV: usize = 11;
const ID_NEXT: usize = 12;
const ID_SETUP: usize = 13;
const ID_PRINT: usize = 14;
const ID_CLOSE: usize = 15;
const BAR_H: i32 = 44;

type SciFn = dyn Fn(u32, usize, isize) -> isize;

/// The editor, reachable from the window procedure. Only valid while `show` runs.
struct DynSci(*const SciFn);

impl Send for DynSci {
    fn send(&self, msg: u32, w: usize, l: isize) -> isize {
        unsafe { (&*self.0)(msg, w, l) }
    }
}

struct Pv {
    sci: DynSci,
    title: String,
    ic: Option<HDC>,
    starts: Vec<isize>,
    page: usize,
    /// The rendered page: bitmap, its size and the page it shows.
    cache: Option<(HBITMAP, i32, i32, usize)>,
    buttons: [HWND; 5],
    outcome: Option<Outcome>,
    done: bool,
}

thread_local! {
    static PV: RefCell<Option<Pv>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn with<R>(f: impl FnOnce(&mut Pv) -> R) -> Option<R> {
    PV.with(|p| p.borrow_mut().as_mut().map(f))
}

fn drop_cache(p: &mut Pv) {
    if let Some((b, ..)) = p.cache.take() {
        unsafe {
            let _ = DeleteObject(b.into());
        }
    }
}

/// Measure the document again (after Page Setup, or on opening).
fn relayout(p: &mut Pv) {
    drop_cache(p);
    if let Some(ic) = p.ic.take() {
        unsafe {
            let _ = DeleteDC(ic);
        }
    }
    p.ic = print::printer_ic();
    p.starts = match p.ic {
        Some(ic) => print::paginate(&p.sci, ic),
        None => vec![0],
    };
    p.page = p.page.min(p.starts.len() - 1);
}

/// The sheet of paper in client coordinates, fitted into the area under the button bar.
fn sheet_rect(client: &RECT, aspect: f64) -> RECT {
    let pad = 18;
    let (aw, ah) = ((client.right - 2 * pad).max(50), (client.bottom - BAR_H - 2 * pad).max(50));
    let (mut sw, mut sh) = (aw, (aw as f64 / aspect) as i32);
    if sh > ah {
        sh = ah;
        sw = (ah as f64 * aspect) as i32;
    }
    let left = (client.right - sw) / 2;
    let top = BAR_H + (client.bottom - BAR_H - sh) / 2;
    RECT { left, top, right: left + sw, bottom: top + sh }
}

fn paper(ic: HDC) -> (i32, i32, i32, i32) {
    let caps = |c: GET_DEVICE_CAPS_INDEX| unsafe { GetDeviceCaps(Some(ic), c) };
    (caps(PHYSICALWIDTH).max(1), caps(PHYSICALHEIGHT).max(1), caps(PHYSICALOFFSETX), caps(PHYSICALOFFSETY))
}

/// Render (or reuse) the page bitmap for the current page at `bw` x `bh`.
fn page_bitmap(p: &mut Pv, screen: HDC, bw: i32, bh: i32) -> Option<HBITMAP> {
    if let Some((b, w, h, pg)) = p.cache {
        if w == bw && h == bh && pg == p.page {
            return Some(b);
        }
    }
    drop_cache(p);
    let ic = p.ic?;
    let (pw, ph, offx, offy) = paper(ic);
    unsafe {
        let bmp = CreateCompatibleBitmap(screen, bw, bh);
        let mem = CreateCompatibleDC(Some(screen));
        let old = SelectObject(mem, bmp.into());
        FillRect(mem, &RECT { left: 0, top: 0, right: bw, bottom: bh }, HBRUSH(GetStockObject(WHITE_BRUSH).0));
        // Logical units are printer pixels; the paper's top-left corner is the bitmap's.
        SetMapMode(mem, MM_ANISOTROPIC);
        let _ = SetWindowExtEx(mem, pw, ph, None);
        let _ = SetViewportExtEx(mem, bw, bh, None);
        let _ = SetWindowOrgEx(mem, -offx, -offy, None);
        let start = p.starts.get(p.page).copied().unwrap_or(0);
        print::render_page(&p.sci, mem, ic, start);
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
        p.cache = Some((bmp, bw, bh, p.page));
        Some(bmp)
    }
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let dc = unsafe { BeginPaint(hwnd, &mut ps) };
    let mut client = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut client) };
    let pal = modern::pal();
    let (cw, ch) = (client.right.max(1), client.bottom.max(1));
    // Everything is drawn off screen first, so nothing flickers while paging.
    let mem = unsafe { CreateCompatibleDC(Some(dc)) };
    let canvas = unsafe { CreateCompatibleBitmap(dc, cw, ch) };
    let old = unsafe { SelectObject(mem, canvas.into()) };
    unsafe {
        FillRect(mem, &client, modern::brush(pal.bg));
        let desk = if modern::is_dark() { (0x16, 0x16, 0x16) } else { (0xD8, 0xD8, 0xD8) };
        FillRect(mem, &RECT { left: 0, top: BAR_H, right: cw, bottom: ch }, modern::brush(desk));
    }
    let label = with(|p| {
        let total = p.starts.len();
        let label = format!("Page {} of {}", p.page + 1, total);
        match p.ic {
            None => (label, None),
            Some(ic) => {
                let (pw, ph, ..) = paper(ic);
                let sheet = sheet_rect(&client, pw as f64 / ph as f64);
                let (bw, bh) = ((sheet.right - sheet.left).max(1), (sheet.bottom - sheet.top).max(1));
                let bmp = page_bitmap(p, dc, bw, bh);
                (label, bmp.map(|b| (b, sheet)))
            }
        }
    });
    if let Some((label, page)) = label {
        unsafe {
            if let Some((bmp, sheet)) = page {
                // A soft shadow, then the sheet itself.
                let shadow = RECT { left: sheet.left + 3, top: sheet.top + 3, right: sheet.right + 3, bottom: sheet.bottom + 3 };
                FillRect(mem, &shadow, modern::brush(if modern::is_dark() { (0, 0, 0) } else { (0xB0, 0xB0, 0xB0) }));
                let src = CreateCompatibleDC(Some(dc));
                let prev = SelectObject(src, bmp.into());
                let _ = BitBlt(mem, sheet.left, sheet.top, sheet.right - sheet.left, sheet.bottom - sheet.top, Some(src), 0, 0, SRCCOPY);
                SelectObject(src, prev);
                let _ = DeleteDC(src);
            } else {
                let mut msg: Vec<u16> = "No printer is available to lay out the pages. Install a printer (for example Microsoft Print to PDF).".encode_utf16().collect();
                let mut r = RECT { left: 20, top: BAR_H + 20, right: cw - 20, bottom: ch - 20 };
                SetBkMode(mem, TRANSPARENT);
                SetTextColor(mem, modern::colorref(pal.text));
                DrawTextW(mem, &mut msg, &mut r, DT_CENTER | DT_WORDBREAK);
            }
            // The page counter between the paging buttons and Page Setup.
            let font = super::chrome::ui_font_handle();
            let oldf = SelectObject(mem, font.into());
            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, modern::colorref(pal.text));
            let mut text: Vec<u16> = label.encode_utf16().collect();
            let mut r = RECT { left: 186, top: 0, right: 330, bottom: BAR_H };
            DrawTextW(mem, &mut text, &mut r, DT_CENTER | DT_VCENTER | DT_SINGLELINE);
            SelectObject(mem, oldf);
        }
    }
    unsafe {
        let _ = BitBlt(dc, 0, 0, cw, ch, Some(mem), 0, 0, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(canvas.into());
        let _ = DeleteDC(mem);
        let _ = EndPaint(hwnd, &ps);
    }
}

fn goto(hwnd: HWND, page: usize) {
    let changed = with(|p| {
        let page = page.min(p.starts.len() - 1);
        let changed = page != p.page;
        p.page = page;
        let (n, total) = (p.page, p.starts.len());
        unsafe {
            let _ = EnableWindow(p.buttons[0], n > 0);
            let _ = EnableWindow(p.buttons[1], n + 1 < total);
        }
        changed
    });
    if changed == Some(true) {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

fn step(hwnd: HWND, delta: isize) {
    if let Some(cur) = with(|p| p.page) {
        goto(hwnd, (cur as isize + delta).max(0) as usize);
    }
}

fn do_print(hwnd: HWND) {
    let job = with(|p| (DynSci(p.sci.0), p.title.clone()));
    let Some((sci, title)) = job else { return };
    match print::print(hwnd, &sci, &title, true) {
        Ok(Outcome::Printed { pages }) => {
            with(|p| {
                p.outcome = Some(Outcome::Printed { pages });
                p.done = true;
            });
        }
        Ok(Outcome::Cancelled) => {}
        Err(e) => {
            let t = wide(&e);
            unsafe {
                MessageBoxW(Some(hwnd), PCWSTR(t.as_ptr()), w!("Print"), MB_OK | MB_ICONWARNING);
            }
        }
    }
}

fn do_setup(hwnd: HWND) {
    if print::page_setup(hwnd) {
        with(relayout);
        let cur = with(|p| p.page).unwrap_or(0);
        with(|p| p.page = usize::MAX);
        goto(hwnd, cur);
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            unsafe { paint(hwnd) };
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_SIZE => {
            with(drop_cache);
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                ID_PREV => step(hwnd, -1),
                ID_NEXT => step(hwnd, 1),
                ID_SETUP => do_setup(hwnd),
                ID_PRINT => do_print(hwnd),
                ID_CLOSE | 2 => drop(with(|p| p.done = true)), // 2 = Esc, as IsDialogMessage reports it
                _ => {}
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            let ctrl = unsafe { GetKeyState(0x11) } < 0;
            match wp.0 as u16 {
                0x25 | 0x21 | 0x26 => step(hwnd, -1), // Left, PgUp, Up
                0x27 | 0x22 | 0x28 | 0x20 => step(hwnd, 1), // Right, PgDn, Down, Space
                0x24 => goto(hwnd, 0),                       // Home
                0x23 => goto(hwnd, usize::MAX),              // End
                0x1B => drop(with(|p| p.done = true)),       // Esc
                0x50 if ctrl => do_print(hwnd),              // Ctrl+P
                _ => {}
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) as i16) as i32;
            step(hwnd, if delta > 0 { -1 } else { 1 });
            LRESULT(0)
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => modern::ctl_color(msg, wp, lp),
        WM_CLOSE => {
            with(|p| p.done = true);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn register() {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppPrintPreview"), &mut wc).is_ok() {
            return;
        }
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hinst.into(),
            lpszClassName: w!("NoteppPrintPreview"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            ..Default::default()
        });
    }
}

/// Open the preview. Returns what happened: the document was printed, or the window was closed.
pub fn show<S: Send>(owner: HWND, sci: &S, title: &str) -> Outcome {
    register();
    let f = |m: u32, w: usize, l: isize| sci.send(m, w, l);
    let fref: &(dyn Fn(u32, usize, isize) -> isize + '_) = &f;
    // The window lives and dies inside this call, so the borrow outlives every use.
    let raw: *const SciFn = unsafe { std::mem::transmute::<*const (dyn Fn(u32, usize, isize) -> isize + '_), *const SciFn>(fref as *const _) };
    unsafe {
        let mut orc = RECT::default();
        let _ = GetWindowRect(owner, &mut orc);
        let (ww, wh) = (((orc.right - orc.left) * 4 / 5).max(900), ((orc.bottom - orc.top) * 9 / 10).max(720));
        let t = wide(&format!("Print Preview - {title}"));
        let hwnd = match CreateWindowExW(
            WS_EX_CONTROLPARENT,
            w!("NoteppPrintPreview"),
            PCWSTR(t.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MAXIMIZEBOX | WS_CLIPCHILDREN,
            orc.left + ((orc.right - orc.left) - ww) / 2,
            orc.top + ((orc.bottom - orc.top) - wh) / 4,
            ww,
            wh,
            Some(owner),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        ) {
            Ok(h) => h,
            Err(_) => return Outcome::Cancelled,
        };
        modern::frame(hwnd);
        let font = super::chrome::ui_font_handle();
        let button = |id: usize, text: &str, x: i32, w: i32| {
            let t = wide(text);
            let h = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("BUTTON"),
                PCWSTR(t.as_ptr()),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(0),
                x,
                8,
                w,
                28,
                Some(hwnd),
                Some(HMENU(id as *mut core::ffi::c_void)),
                GetModuleHandleW(None).ok().map(Into::into),
                None,
            )
            .unwrap();
            SendMessageW(h, WM_SETFONT, Some(WPARAM(font.0 as usize)), Some(LPARAM(1)));
            modern::style_control(h, Ctl::Button);
            h
        };
        let buttons = [
            button(ID_PREV, "< Previous", 8, 84),
            button(ID_NEXT, "Next >", 96, 84),
            button(ID_SETUP, "Page Setup...", 340, 112),
            button(ID_PRINT, "Print...", 458, 90),
            button(ID_CLOSE, "Close", 554, 80),
        ];
        PV.with(|p| {
            *p.borrow_mut() = Some(Pv {
                sci: DynSci(raw),
                title: title.to_string(),
                ic: None,
                starts: vec![0],
                page: 0,
                cache: None,
                buttons,
                outcome: None,
                done: false,
            })
        });
        with(relayout);
        goto(hwnd, 0);
        with(|p| {
            let _ = EnableWindow(p.buttons[0], false);
            let _ = EnableWindow(p.buttons[1], p.starts.len() > 1);
        });
        let _ = EnableWindow(owner, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus_(hwnd);

        let mut msg = MSG::default();
        while with(|p| !p.done).unwrap_or(false) {
            if !GetMessageW(&mut msg, None, 0, 0).as_bool() {
                PostQuitMessage(msg.wParam.0 as i32);
                break;
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let outcome = with(|p| p.outcome.take()).flatten().unwrap_or(Outcome::Cancelled);
        let _ = EnableWindow(owner, true);
        let _ = DestroyWindow(hwnd);
        with(|p| {
            drop_cache(p);
            if let Some(ic) = p.ic.take() {
                let _ = DeleteDC(ic);
            }
        });
        PV.with(|p| *p.borrow_mut() = None);
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(owner));
        outcome
    }
}

#[allow(non_snake_case)]
unsafe fn SetFocus_(h: HWND) -> windows::core::Result<HWND> {
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(Some(h)) }
}
