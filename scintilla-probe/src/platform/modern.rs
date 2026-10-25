//! The modern (Windows 11 / Fluent-style) look shared by dialogs, the find panel
//! and message boxes: one palette for light and dark, rounded owner-drawn buttons
//! with hover and pressed states, dark scroll bars / edits / lists, and the frame
//! (rounded corners, Mica title bar). Controls that are not owner-drawn keep their
//! native themed look, switched to the dark variants through `SetWindowTheme`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, SetWindowTheme, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent};
use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCSTR, PCWSTR, w};

pub type Rgb = (u8, u8, u8);

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Follow the editor theme. Also flips the native popup menus between light and dark.
pub fn set_dark(on: bool) {
    if DARK.swap(on, Ordering::Relaxed) != on {
        native_menus(on);
    }
}

#[derive(Clone, Copy)]
pub struct Pal {
    pub bg: Rgb,
    pub card: Rgb,
    pub card_border: Rgb,
    pub text: Rgb,
    pub disabled: Rgb,
    pub field: Rgb,
    /// Selected row in lists and the palette.
    pub sel_bg: Rgb,
    pub button: Rgb,
    pub button_border: Rgb,
    pub button_hover: Rgb,
    pub button_down: Rgb,
    pub accent: Rgb,
    pub accent_hover: Rgb,
    pub accent_down: Rgb,
    pub accent_text: Rgb,
}

pub fn pal() -> Pal {
    if is_dark() {
        Pal {
            bg: (0x20, 0x20, 0x20),
            card: (0x2B, 0x2B, 0x2B),
            card_border: (0x3A, 0x3A, 0x3A),
            text: (0xFF, 0xFF, 0xFF),
            disabled: (0x7A, 0x7A, 0x7A),
            field: (0x2D, 0x2D, 0x2D),
            sel_bg: (0x2D, 0x3F, 0x50),
            button: (0x37, 0x37, 0x37),
            button_border: (0x4A, 0x4A, 0x4A),
            button_hover: (0x3F, 0x3F, 0x3F),
            button_down: (0x2F, 0x2F, 0x2F),
            accent: (0x4C, 0xC2, 0xFF),
            accent_hover: (0x47, 0xB1, 0xE8),
            accent_down: (0x42, 0xA1, 0xD2),
            accent_text: (0x00, 0x00, 0x00),
        }
    } else {
        Pal {
            bg: (0xF3, 0xF3, 0xF3),
            card: (0xFF, 0xFF, 0xFF),
            card_border: (0xE5, 0xE5, 0xE5),
            text: (0x1B, 0x1B, 0x1B),
            disabled: (0x9A, 0x9A, 0x9A),
            field: (0xFF, 0xFF, 0xFF),
            sel_bg: (0xE5, 0xF1, 0xFB),
            button: (0xFD, 0xFD, 0xFD),
            button_border: (0xD1, 0xD1, 0xD1),
            button_hover: (0xF5, 0xF5, 0xF5),
            button_down: (0xEB, 0xEB, 0xEB),
            accent: (0x00, 0x67, 0xC0),
            accent_hover: (0x19, 0x75, 0xC5),
            accent_down: (0x31, 0x83, 0xCA),
            accent_text: (0xFF, 0xFF, 0xFF),
        }
    }
}

pub fn colorref((r, g, b): Rgb) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

thread_local! {
    static BRUSHES: RefCell<HashMap<u32, HBRUSH>> = RefCell::new(HashMap::new());
    static SEMIBOLD: Cell<Option<HFONT>> = const { Cell::new(None) };
    static HOT: Cell<isize> = const { Cell::new(0) };
}

/// A shared solid brush (kept for the life of the process; there are only a handful of colours).
pub fn brush(c: Rgb) -> HBRUSH {
    BRUSHES.with(|m| *m.borrow_mut().entry(colorref(c).0).or_insert_with(|| unsafe { CreateSolidBrush(colorref(c)) }))
}

pub fn semibold_font() -> HFONT {
    SEMIBOLD.with(|c| {
        if let Some(f) = c.get() {
            return f;
        }
        let f = unsafe {
            CreateFontW(-13, 0, 0, 0, 600, 0, 0, 0, FONT_CHARSET(1), FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), CLEARTYPE_QUALITY, 0, w!("Segoe UI Semibold"))
        };
        c.set(Some(f));
        f
    })
}


/// Draw `text` in `rect` with the font currently selected into `dc`.
pub fn draw_text(dc: HDC, text: &str, rect: &RECT, color: Rgb, flags: DRAW_TEXT_FORMAT) {
    if text.is_empty() {
        return;
    }
    let mut buf: Vec<u16> = text.encode_utf16().collect();
    let mut rr = *rect;
    unsafe {
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, colorref(color));
        DrawTextW(dc, &mut buf, &mut rr, flags);
    }
}

/// A rounded rectangle with a 1px border (`radius` is the corner diameter), anti-aliased
/// through GDI+ (plain GDI where GDI+ cannot start).
pub fn round_rect(dc: HDC, r: &RECT, radius: i32, fill: Rgb, border: Rgb) {
    if aa_round_rect(dc, r, radius, fill, border) {
        return;
    }
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, colorref(border));
        let (ob, op) = (SelectObject(dc, brush(fill).into()), SelectObject(dc, pen.into()));
        let _ = RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        let _ = DeleteObject(pen.into());
    }
}

fn argb((r, g, b): Rgb) -> u32 {
    0xFF00_0000 | (r as u32) << 16 | (g as u32) << 8 | b as u32
}

/// GDI+ is started once per process; `false` means it is unavailable.
fn gdiplus_ready() -> bool {
    use windows::Win32::Graphics::GdiPlus::{GdiplusStartup, GdiplusStartupInput, GdiplusStartupOutput};
    static STATE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STATE.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput { GdiplusVersion: 1, ..Default::default() };
        let mut out = GdiplusStartupOutput::default();
        GdiplusStartup(&mut token, &input, &mut out).0 == 0
    })
}

fn aa_round_rect(dc: HDC, r: &RECT, radius: i32, fill: Rgb, border: Rgb) -> bool {
    use windows::Win32::Graphics::GdiPlus::*;
    if !gdiplus_ready() {
        return false;
    }
    let (x, y) = (r.left as f32 + 0.5, r.top as f32 + 0.5);
    let (w, h) = ((r.right - r.left) as f32 - 1.0, (r.bottom - r.top) as f32 - 1.0);
    if w <= 0.0 || h <= 0.0 {
        return true;
    }
    let d = (radius as f32).min(w).min(h);
    unsafe {
        let mut g = std::ptr::null_mut();
        if GdipCreateFromHDC(dc, &mut g).0 != 0 {
            return false;
        }
        GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
        let mut path = std::ptr::null_mut();
        GdipCreatePath(FillModeAlternate, &mut path);
        GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
        GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
        GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
        GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
        GdipClosePathFigure(path);
        let mut brush = std::ptr::null_mut();
        GdipCreateSolidFill(argb(fill), &mut brush);
        GdipFillPath(g, brush as *mut GpBrush, path);
        let mut pen = std::ptr::null_mut();
        GdipCreatePen1(argb(border), 1.0, UnitPixel, &mut pen);
        GdipDrawPath(g, pen, path);
        GdipDeletePen(pen);
        GdipDeleteBrush(brush as *mut GpBrush);
        GdipDeletePath(path);
        GdipDeleteGraphics(g);
    }
    true
}

pub fn fill(dc: HDC, r: &RECT, c: Rgb) {
    unsafe {
        FillRect(dc, r, brush(c));
    }
}

/// Paint one owner-drawn push button. `primary` is the accent-filled default button.
pub fn paint_button(dis: &DRAWITEMSTRUCT, primary: bool) {
    let p = pal();
    let (dc, r) = (dis.hDC, dis.rcItem);
    let disabled = dis.itemState.0 & 0x4 != 0;
    let down = dis.itemState.0 & 0x1 != 0;
    let hot = HOT.with(|h| h.get()) == dis.hwndItem.0 as isize;
    let focused = dis.itemState.0 & 0x10 != 0;
    // Paint the surroundings first so the rounded corners blend into the parent.
    fill(dc, &r, p.bg);
    let (bg, border, fg) = if disabled {
        (p.button, p.card_border, p.disabled)
    } else if primary {
        (if down { p.accent_down } else if hot { p.accent_hover } else { p.accent }, p.accent, p.accent_text)
    } else {
        (if down { p.button_down } else if hot { p.button_hover } else { p.button }, p.button_border, p.text)
    };
    let body = RECT { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
    round_rect(dc, &body, 8, bg, border);
    if focused && !disabled {
        let ring = RECT { left: r.left + 2, top: r.top + 2, right: r.right - 2, bottom: r.bottom - 2 };
        unsafe {
            let pen = CreatePen(PS_SOLID, 1, colorref(if primary { p.accent_text } else { p.accent }));
            let (ob, op) = (SelectObject(dc, GetStockObject(NULL_BRUSH)), SelectObject(dc, pen.into()));
            let _ = RoundRect(dc, ring.left, ring.top, ring.right, ring.bottom, 6, 6);
            SelectObject(dc, ob);
            SelectObject(dc, op);
            let _ = DeleteObject(pen.into());
        }
    }
    let mut buf = [0u16; 128];
    let n = unsafe { GetWindowTextW(dis.hwndItem, &mut buf) }.max(0) as usize;
    let text = String::from_utf16_lossy(&buf[..n]);
    unsafe {
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        draw_text(dc, &text, &r, fg, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX);
        SelectObject(dc, old);
    }
}

/// Paint one owner-drawn check box or radio button (`on` is its state, `bg` what lies behind it).
pub fn paint_tick(dis: &DRAWITEMSTRUCT, radio: bool, on: bool, bg: Rgb) {
    let p = pal();
    let (dc, r) = (dis.hDC, dis.rcItem);
    let disabled = dis.itemState.0 & 0x4 != 0;
    let down = dis.itemState.0 & 0x1 != 0;
    let hot = HOT.with(|h| h.get()) == dis.hwndItem.0 as isize;
    let focused = dis.itemState.0 & 0x10 != 0;
    fill(dc, &r, bg);
    let size = 18;
    let top = r.top + (r.bottom - r.top - size) / 2;
    let b = RECT { left: r.left + 2, top, right: r.left + 2 + size, bottom: top + size };
    let edge = if is_dark() { (0x9D, 0x9D, 0x9D) } else { (0x6B, 0x6B, 0x6B) };
    let (fill_c, border) = if disabled {
        (if on { p.disabled } else { p.bg }, p.disabled)
    } else if on {
        (if down { p.accent_down } else if hot { p.accent_hover } else { p.accent }, p.accent)
    } else {
        (if down { p.button_down } else if hot { p.button_hover } else { p.field }, if focused { p.accent } else { edge })
    };
    round_rect(dc, &b, if radio { size } else { 8 }, fill_c, border);
    if on {
        unsafe {
            if radio {
                let dot = RECT { left: b.left + 5, top: b.top + 5, right: b.right - 5, bottom: b.bottom - 5 };
                round_rect(dc, &dot, 8, p.accent_text, p.accent_text);
            } else {
                let pen = CreatePen(PS_SOLID, 2, colorref(p.accent_text));
                let old = SelectObject(dc, pen.into());
                let _ = MoveToEx(dc, b.left + 4, b.top + 9, None);
                let _ = LineTo(dc, b.left + 7, b.top + 12);
                let _ = LineTo(dc, b.left + 13, b.top + 5);
                SelectObject(dc, old);
                let _ = DeleteObject(pen.into());
            }
        }
    }
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(dis.hwndItem, &mut buf) }.max(0) as usize;
    let text = String::from_utf16_lossy(&buf[..n]);
    let label = RECT { left: b.right + 8, top: r.top, right: r.right, bottom: r.bottom };
    unsafe {
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        draw_text(dc, &text, &label, if disabled { p.disabled } else { p.text }, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_HIDEPREFIX);
        SelectObject(dc, old);
    }
}

unsafe extern "system" fn hover_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_MOUSEMOVE => {
            if HOT.with(|h| h.replace(hwnd.0 as isize)) != hwnd.0 as isize {
                let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = unsafe { TrackMouseEvent(&mut t) };
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
        }
        WM_MOUSELEAVE => {
            HOT.with(|h| {
                if h.get() == hwnd.0 as isize {
                    h.set(0);
                }
            });
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Track the mouse over an owner-drawn button so it can paint its hover state.
pub fn track_hover(button: HWND) {
    unsafe {
        let _ = SetWindowSubclass(button, Some(hover_proc), 1, 0);
    }
}

/// Which kind of native control `style_control` is preparing.
#[derive(Clone, Copy, PartialEq)]
pub enum Ctl {
    Edit,
    Check,
    List,
    Tree,
}

/// Switch a native control to its dark variant, or back to the default light one.
pub fn style_control(h: HWND, kind: Ctl) {
    let dark = is_dark();
    unsafe {
        let _ = match (kind, dark) {
            (Ctl::Edit, true) => SetWindowTheme(h, w!("DarkMode_CFD"), PCWSTR::null()),
            (_, true) => SetWindowTheme(h, w!("DarkMode_Explorer"), PCWSTR::null()),
            (Ctl::List | Ctl::Tree, false) => SetWindowTheme(h, w!("Explorer"), PCWSTR::null()),
            // NULL, NULL restores the control's default theme (after a switch back from dark).
            _ => SetWindowTheme(h, PCWSTR::null(), PCWSTR::null()),
        };
    }
    if kind == Ctl::Tree {
        let p = pal();
        unsafe {
            // TVM_SETBKCOLOR / TVM_SETTEXTCOLOR / TVM_SETLINECOLOR
            SendMessageW(h, 0x111D, None, Some(LPARAM(colorref(p.card).0 as isize)));
            SendMessageW(h, 0x111E, None, Some(LPARAM(colorref(p.text).0 as isize)));
            SendMessageW(h, 0x1128, None, Some(LPARAM(colorref(p.card_border).0 as isize)));
            let _ = InvalidateRect(Some(h), None, true);
        }
    }
    if kind == Ctl::List {
        unsafe {
            let _ = SetWindowSubclass(h, Some(list_proc), 3, 0);
        }
        let p = pal();
        unsafe {
            // LVM_SETBKCOLOR / LVM_SETTEXTCOLOR / LVM_SETTEXTBKCOLOR
            SendMessageW(h, 0x1001, None, Some(LPARAM(colorref(p.field).0 as isize)));
            SendMessageW(h, 0x1024, None, Some(LPARAM(colorref(p.text).0 as isize)));
            SendMessageW(h, 0x1026, None, Some(LPARAM(colorref(p.field).0 as isize)));
            // Header: dark theme for the column titles
            let header = SendMessageW(h, 0x101F, None, None).0 as isize; // LVM_GETHEADER
            if header != 0 {
                let _ = SetWindowTheme(HWND(header as *mut _), if is_dark() { w!("DarkMode_ItemsView") } else { w!("Explorer") }, PCWSTR::null());
            }
        }
    }
}

/// Colours for `WM_CTLCOLOR*` of a form or panel: labels are transparent over the
/// background, edit boxes and list boxes use the field colour.
pub fn ctl_color(msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let dc = HDC(wp.0 as *mut core::ffi::c_void);
    let p = pal();
    let item = HWND(lp.0 as *mut _);
    let mut class = [0u16; 16];
    let n = unsafe { GetClassNameW(item, &mut class) }.max(0) as usize;
    let is_edit = String::from_utf16_lossy(&class[..n]).eq_ignore_ascii_case("Edit");
    let disabled = unsafe { !windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(item).as_bool() };
    unsafe {
        SetTextColor(dc, colorref(if disabled { p.disabled } else { p.text }));
        if is_edit || msg == WM_CTLCOLORLISTBOX {
            SetBkColor(dc, colorref(p.field));
            SetBkMode(dc, OPAQUE);
            return LRESULT(brush(p.field).0 as isize);
        }
        SetBkMode(dc, TRANSPARENT);
        LRESULT(GetStockObject(NULL_BRUSH).0 as isize)
    }
}

/// A borderless icon button (such as the panel's close ✕) that lights up on hover.
pub fn paint_flat(dis: &DRAWITEMSTRUCT) {
    let p = pal();
    let (dc, r) = (dis.hDC, dis.rcItem);
    let hot = HOT.with(|h| h.get()) == dis.hwndItem.0 as isize || dis.itemState.0 & 0x1 != 0;
    fill(dc, &r, p.bg);
    if hot {
        round_rect(dc, &r, 8, p.button_hover, p.button_hover);
    }
    let mut buf = [0u16; 8];
    let n = unsafe { GetWindowTextW(dis.hwndItem, &mut buf) }.max(0) as usize;
    let text = String::from_utf16_lossy(&buf[..n]);
    unsafe {
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        draw_text(dc, &text, &r, p.text, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, old);
    }
}

thread_local! {
    static HOT_TAB: Cell<i32> = const { Cell::new(-1) };
}

unsafe extern "system" fn tabs_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_ERASEBKGND => return LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let dc = unsafe { BeginPaint(hwnd, &mut ps) };
            paint_tabs(hwnd, dc);
            let _ = unsafe { EndPaint(hwnd, &ps) };
            return LRESULT(0);
        }
        WM_MOUSEMOVE => {
            // TCM_HITTEST
            let mut hit = [lp.0 as i16 as i32, (lp.0 >> 16) as i16 as i32, 0, 0];
            let item = unsafe { SendMessageW(hwnd, 0x130D, None, Some(LPARAM(hit.as_mut_ptr() as isize))) }.0 as i32;
            if HOT_TAB.with(|h| h.replace(item)) != item {
                let mut t = TRACKMOUSEEVENT { cbSize: size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: hwnd, dwHoverTime: 0 };
                let _ = unsafe { TrackMouseEvent(&mut t) };
                let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            }
        }
        WM_MOUSELEAVE => {
            HOT_TAB.with(|h| h.set(-1));
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
        }
        _ => {}
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

fn paint_tabs(hwnd: HWND, dc: HDC) {
    let p = pal();
    let mut client = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    fill(dc, &client, p.bg);
    let count = unsafe { SendMessageW(hwnd, 0x1304, None, None) }.0 as i32; // TCM_GETITEMCOUNT
    let current = unsafe { SendMessageW(hwnd, 0x130B, None, None) }.0 as i32; // TCM_GETCURSEL
    let hot = HOT_TAB.with(|h| h.get());
    unsafe {
        let old = SelectObject(dc, super::chrome::ui_font_handle().into());
        for i in 0..count {
            let mut r = RECT::default();
            SendMessageW(hwnd, 0x130A, Some(WPARAM(i as usize)), Some(LPARAM(&mut r as *mut _ as isize))); // TCM_GETITEMRECT
            let mut text = [0u16; 64];
            let mut item = TabItem { mask: 1, state: 0, state_mask: 0, text: text.as_mut_ptr(), text_max: 64, image: 0, param: 0 };
            SendMessageW(hwnd, 0x133C, Some(WPARAM(i as usize)), Some(LPARAM(&mut item as *mut _ as isize))); // TCM_GETITEMW
            let name = String::from_utf16_lossy(&text[..text.iter().position(|c| *c == 0).unwrap_or(0)]);
            let selected = i == current;
            if i == hot && !selected {
                let pill = RECT { left: r.left + 2, top: r.top + 2, right: r.right - 2, bottom: r.bottom - 4 };
                round_rect(dc, &pill, 8, p.button_hover, p.button_hover);
            }
            draw_text(dc, &name, &r, if selected { p.accent } else { p.text }, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
            if selected {
                let bar = RECT { left: r.left + 14, top: r.bottom - 5, right: r.right - 14, bottom: r.bottom - 2 };
                round_rect(dc, &bar, 3, p.accent, p.accent);
            }
        }
        SelectObject(dc, old);
    }
}

#[repr(C)]
struct TabItem {
    mask: u32,
    state: u32,
    state_mask: u32,
    text: *mut u16,
    text_max: i32,
    image: i32,
    param: isize,
}

/// Take over painting of a tab control: flat labels with an accent underline on the
/// selected one and a soft pill on hover. Selection and keyboard handling stay native.
pub fn style_tabs(tab: HWND) {
    unsafe {
        let _ = SetWindowSubclass(tab, Some(tabs_proc), 2, 0);
    }
}

/// Custom drawing of list-view column headers (they arrive as `NM_CUSTOMDRAW` from the header
/// control): a flat bar in the card colour with themed text, so dark mode stays readable.
/// Returns `Some(result)` when `lp` was such a notification.
pub fn header_custom_draw(lp: LPARAM) -> Option<LRESULT> {
    use windows::Win32::UI::Controls::NMCUSTOMDRAW;
    const NM_CUSTOMDRAW: u32 = (-12i32) as u32;
    let nm = unsafe { (lp.0 as *const NMCUSTOMDRAW).as_ref()? };
    if nm.hdr.code != NM_CUSTOMDRAW {
        return None;
    }
    let mut class = [0u16; 16];
    let n = unsafe { GetClassNameW(nm.hdr.hwndFrom, &mut class) }.max(0) as usize;
    if !String::from_utf16_lossy(&class[..n]).eq_ignore_ascii_case("SysHeader32") {
        return None;
    }
    let p = pal();
    match nm.dwDrawStage.0 {
        // CDDS_PREPAINT: paint the whole bar once, then ask about each item.
        0x1 => {
            let mut r = RECT::default();
            unsafe {
                let _ = GetClientRect(nm.hdr.hwndFrom, &mut r);
            }
            fill(nm.hdc, &r, p.card);
            fill(nm.hdc, &RECT { left: r.left, top: r.bottom - 1, right: r.right, bottom: r.bottom }, p.card_border);
            Some(LRESULT(0x20)) // CDRF_NOTIFYITEMDRAW
        }
        // CDDS_ITEMPREPAINT
        0x1_0001 => {
            let mut text = [0u16; 128];
            let mut item = windows::Win32::UI::Controls::HDITEMW { mask: windows::Win32::UI::Controls::HDI_TEXT, pszText: windows::core::PWSTR(text.as_mut_ptr()), cchTextMax: 128, ..Default::default() };
            unsafe {
                SendMessageW(nm.hdr.hwndFrom, 0x120B, Some(WPARAM(nm.dwItemSpec)), Some(LPARAM(&mut item as *mut _ as isize))); // HDM_GETITEMW
            }
            let label = String::from_utf16_lossy(&text[..text.iter().position(|c| *c == 0).unwrap_or(0)]);
            let cell = nm.rc;
            fill(nm.hdc, &RECT { left: cell.right - 1, top: cell.top + 6, right: cell.right, bottom: cell.bottom - 6 }, p.card_border);
            unsafe {
                let old = SelectObject(nm.hdc, semibold_font().into());
                draw_text(nm.hdc, &label, &RECT { left: cell.left + 8, top: cell.top, right: cell.right - 6, bottom: cell.bottom }, p.text, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
                SelectObject(nm.hdc, old);
            }
            Some(LRESULT(0x4)) // CDRF_SKIPDEFAULT
        }
        _ => Some(LRESULT(0)),
    }
}

/// Lists receive their header's `NM_CUSTOMDRAW` themselves, so they are subclassed to draw it.
unsafe extern "system" fn list_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM, _id: usize, _data: usize) -> LRESULT {
    if msg == WM_NOTIFY {
        if let Some(r) = header_custom_draw(lp) {
            return r;
        }
    }
    unsafe { DefSubclassProc(hwnd, msg, wp, lp) }
}

/// Rounded corners, Mica title bar and the light/dark caption for a dialog window.
pub fn frame(hwnd: HWND) {
    super::dwm::apply(hwnd, is_dark());
}

/// Let Windows draw the native popup menus (menu bar drop-downs, context menus) dark or light
/// to match the app. Uses the uxtheme entry points every dark-mode app relies on; where they
/// are missing (older Windows) the menus simply stay light.
fn native_menus(dark: bool) {
    unsafe {
        let Ok(ux) = LoadLibraryExW(w!("uxtheme.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32) else { return };
        // Ordinal 135: SetPreferredAppMode(2 = force dark, 3 = force light); 136: FlushMenuThemes.
        let set_mode = GetProcAddress(ux, PCSTR(135 as *const u8));
        let flush = GetProcAddress(ux, PCSTR(136 as *const u8));
        if let Some(f) = set_mode {
            let f: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            f(if dark { 2 } else { 3 });
        }
        if let Some(f) = flush {
            let f: extern "system" fn() = std::mem::transmute(f);
            f();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_differ_and_contrast() {
        set_dark(false);
        let light = pal();
        set_dark(true);
        let dark = pal();
        set_dark(false);
        assert_ne!(light.bg, dark.bg);
        let lum = |c: Rgb| 0.2126 * c.0 as f32 + 0.7152 * c.1 as f32 + 0.0722 * c.2 as f32;
        for p in [light, dark] {
            assert!((lum(p.text) - lum(p.card)).abs() > 120.0, "text on card");
            assert!((lum(p.accent_text) - lum(p.accent)).abs() > 80.0, "text on accent");
            assert!((lum(p.disabled) - lum(p.button)).abs() > 40.0 || lum(p.disabled) > 0.0);
        }
    }
}
