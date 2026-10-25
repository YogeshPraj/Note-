//! The Markdown formatting toolbar: a row of native buttons shown above the editor while a
//! Markdown file is active. Each button's control id is the host command id, so a click is
//! simply forwarded to the host as `WM_COMMAND`; the host does the formatting.

use std::cell::Cell;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, PWSTR, w};

use super::modern;

pub const HEIGHT: i32 = 38;

const BS_PUSHLIKE_CHECK: u32 = 0x1003; // BS_AUTOCHECKBOX | BS_PUSHLIKE
const TABSTOP: u32 = 0x0001_0000;

/// How a button looks: its label, tooltip, the command id it sends, its width, and its font style.
#[derive(Clone, Copy)]
pub struct Item {
    pub id: usize,
    pub label: &'static str,
    pub tip: &'static str,
    pub width: i32,
    pub style: Style,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Style {
    Plain,
    Bold,
    Italic,
    Strike,
    /// A two-state (pushed in / out) toggle, drawn on the right side of the bar.
    Toggle,
}

/// What the bar shows, left to right. `None` is a gap between groups.
pub fn layout(ids: &Ids) -> Vec<Option<Item>> {
    let b = |id, label, tip, width, style| Some(Item { id, label, tip, width, style });
    vec![
        b(ids.bold, "B", "Bold", 30, Style::Bold),
        b(ids.italic, "I", "Italic", 30, Style::Italic),
        b(ids.strike, "S", "Strikethrough", 30, Style::Strike),
        b(ids.code, "</>", "Inline code", 38, Style::Plain),
        None,
        b(ids.h1, "H1", "Heading 1", 34, Style::Plain),
        b(ids.h2, "H2", "Heading 2", 34, Style::Plain),
        b(ids.h3, "H3", "Heading 3", 34, Style::Plain),
        None,
        b(ids.quote, "\u{275D}", "Quote", 30, Style::Plain),
        b(ids.bullet, "\u{2022}\u{2261}", "Bulleted list", 40, Style::Plain),
        b(ids.numbered, "1.", "Numbered list", 32, Style::Plain),
        b(ids.task, "[x]", "Task list", 32, Style::Plain),
        None,
        b(ids.link, "Link", "Insert link", 46, Style::Plain),
        b(ids.image, "Image", "Insert image", 54, Style::Plain),
        b(ids.table, "Table", "Insert table", 50, Style::Plain),
        b(ids.code_block, "{ }", "Code block", 38, Style::Plain),
        b(ids.rule, "\u{2015}", "Horizontal rule", 30, Style::Plain),
    ]
}

/// The command ids the host assigned to each button.
#[derive(Clone, Copy)]
pub struct Ids {
    pub bold: usize,
    pub italic: usize,
    pub strike: usize,
    pub code: usize,
    pub h1: usize,
    pub h2: usize,
    pub h3: usize,
    pub quote: usize,
    pub bullet: usize,
    pub numbered: usize,
    pub task: usize,
    pub link: usize,
    pub image: usize,
    pub table: usize,
    pub code_block: usize,
    pub rule: usize,
    pub preview: usize,
    pub sync: usize,
}

#[derive(Clone, Copy)]
struct Bar {
    hwnd: HWND,
}

thread_local! {
    static BAR: Cell<Option<Bar>> = const { Cell::new(None) };
    static BUTTONS: std::cell::RefCell<Vec<(usize, HWND, Style, i32)>> = const { std::cell::RefCell::new(Vec::new()) };
}

pub fn hwnd() -> Option<HWND> {
    BAR.with(|b| b.get()).map(|b| b.hwnd)
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn font(weight: i32, italic: bool, strike: bool) -> HFONT {
    unsafe {
        CreateFontW(-13, 0, 0, 0, weight, italic as u32, 0, strike as u32, FONT_CHARSET(1), FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), CLEARTYPE_QUALITY, 0, w!("Segoe UI"))
    }
}

pub fn create(host: HWND, ids: &Ids) {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX { dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES });
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppMdBar"), &mut wc).is_err() {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(bar_proc),
                hInstance: hinst.into(),
                lpszClassName: w!("NoteppMdBar"),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                ..Default::default()
            });
        }
        let hwnd = CreateWindowExW(WS_EX_CONTROLPARENT, w!("NoteppMdBar"), PCWSTR::null(), WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, 10, HEIGHT, Some(host), None, Some(hinst.into()), None).unwrap();
        // TTS_ALWAYSTIP: show tips even while the window is not active.
        let tip = CreateWindowExW(WS_EX_TOPMOST, w!("tooltips_class32"), PCWSTR::null(), WS_POPUP | WINDOW_STYLE(0x01), 0, 0, 0, 0, Some(hwnd), None, Some(hinst.into()), None).unwrap_or_default();
        BAR.with(|b| b.set(Some(Bar { hwnd })));

        let ui = super::chrome::ui_font_handle();
        let (bold, italic, strike) = (font(700, false, false), font(400, true, false), font(400, false, true));
        let mut buttons = Vec::new();
        let mut add = |item: Item| {
            let style = TABSTOP | if item.style == Style::Toggle { BS_PUSHLIKE_CHECK } else { 0 };
            let t = wide(item.label);
            let h = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), PCWSTR(t.as_ptr()), WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style), 0, 0, item.width, 28, Some(hwnd), Some(HMENU(item.id as *mut core::ffi::c_void)), Some(hinst.into()), None).unwrap();
            let f = match item.style {
                Style::Bold => bold,
                Style::Italic => italic,
                Style::Strike => strike,
                _ => ui,
            };
            SendMessageW(h, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
            modern::style_control(h, modern::Ctl::Button);
            if !tip.0.is_null() {
                let mut text = wide(item.tip);
                let ti = TTTOOLINFOW {
                    cbSize: size_of::<TTTOOLINFOW>() as u32,
                    uFlags: TTF_IDISHWND | TTF_SUBCLASS,
                    hwnd,
                    uId: h.0 as usize,
                    lpszText: PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(tip, TTM_ADDTOOLW, None, Some(LPARAM(&ti as *const _ as isize)));
            }
            buttons.push((item.id, h, item.style, item.width));
        };
        for item in layout(ids).into_iter().flatten() {
            add(item);
        }
        add(Item { id: ids.preview, label: "Preview", tip: "Show or hide the live preview (Ctrl+Shift+V)", width: 66, style: Style::Toggle });
        add(Item { id: ids.sync, label: "Sync scroll", tip: "Keep the editor and the preview scrolled together", width: 88, style: Style::Toggle });
        BUTTONS.with(|b| *b.borrow_mut() = buttons);
        arrange(ids, 800);
    }
}

/// Position the buttons for a bar `width` pixels wide: formatting buttons from the left,
/// the two toggles pinned to the right.
pub fn arrange(ids: &Ids, width: i32) {
    let (mut x, y) = (8, 5);
    let gap = 8;
    let mut order = layout(ids).into_iter();
    BUTTONS.with(|b| {
        let buttons = b.borrow();
        let find = |id: usize| buttons.iter().find(|(i, ..)| *i == id).copied();
        for slot in order.by_ref() {
            match slot {
                None => x += gap,
                Some(item) => {
                    if let Some((_, h, _, w)) = find(item.id) {
                        unsafe {
                            let _ = MoveWindow(h, x, y, w, 28, false);
                        }
                        x += w + 2;
                    }
                }
            }
        }
        let mut right = width - 8;
        for id in [ids.sync, ids.preview] {
            if let Some((_, h, _, w)) = find(id) {
                right -= w;
                unsafe {
                    let _ = MoveWindow(h, right, y, w, 28, false);
                }
                right -= 6;
            }
        }
    });
}

/// Set a toggle button's pushed state.
pub fn set_checked(id: usize, on: bool) {
    BUTTONS.with(|b| {
        if let Some((_, h, _, _)) = b.borrow().iter().find(|(i, ..)| *i == id) {
            unsafe {
                if (SendMessageW(*h, 0x00F0, None, None).0 == 1) != on {
                    SendMessageW(*h, 0x00F1, Some(WPARAM(on as usize)), None); // BM_SETCHECK
                }
            }
        }
    });
}

/// Re-apply the light/dark native styling after the app theme changed.
pub fn retheme() {
    BUTTONS.with(|b| {
        for (_, h, _, _) in b.borrow().iter() {
            modern::style_control(*h, modern::Ctl::Button);
        }
    });
    if let Some(bar) = hwnd() {
        unsafe {
            let _ = RedrawWindow(Some(bar), None, None, RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN);
        }
    }
}

unsafe extern "system" fn bar_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => modern::erase_background(hwnd, wp),
        WM_CTLCOLORBTN | WM_CTLCOLORSTATIC => modern::ctl_color(msg, wp, lp),
        WM_COMMAND => {
            // A click on one of the buttons: hand the command to the host.
            let (id, code) = (wp.0 & 0xFFFF, (wp.0 >> 16) & 0xFFFF);
            if code == 0 {
                if let Ok(host) = unsafe { GetParent(hwnd) } {
                    unsafe {
                        let _ = PostMessageW(Some(host), WM_COMMAND, WPARAM(id), LPARAM(0));
                    }
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
