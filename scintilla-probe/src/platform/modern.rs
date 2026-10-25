//! Light/dark theming for the app's native Windows controls.
//!
//! Nothing here paints controls. Buttons, check boxes, tabs, lists and edit boxes are the
//! system's own (visual styles come from the app manifest); this module only
//!   * remembers whether the app is dark,
//!   * switches each control to its dark theme (`SetWindowTheme`) or back,
//!   * supplies the flat window/field colours that `WM_CTLCOLOR*` and `WM_ERASEBKGND` need,
//!   * flips the native popup menus and the window frame between light and dark.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows::Win32::UI::Controls::SetWindowTheme;
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
    pub text: Rgb,
    pub disabled: Rgb,
    pub field: Rgb,
}

pub fn pal() -> Pal {
    if is_dark() {
        Pal { bg: (0x20, 0x20, 0x20), card: (0x2B, 0x2B, 0x2B), text: (0xFF, 0xFF, 0xFF), disabled: (0x7A, 0x7A, 0x7A), field: (0x2D, 0x2D, 0x2D) }
    } else {
        Pal { bg: (0xF0, 0xF0, 0xF0), card: (0xFF, 0xFF, 0xFF), text: (0x00, 0x00, 0x00), disabled: (0x6D, 0x6D, 0x6D), field: (0xFF, 0xFF, 0xFF) }
    }
}

pub fn colorref((r, g, b): Rgb) -> COLORREF {
    COLORREF(r as u32 | (g as u32) << 8 | (b as u32) << 16)
}

thread_local! {
    static BRUSHES: RefCell<HashMap<u32, HBRUSH>> = RefCell::new(HashMap::new());
}

/// A shared solid brush (kept for the life of the process; there are only a handful of colours).
pub fn brush(c: Rgb) -> HBRUSH {
    BRUSHES.with(|m| *m.borrow_mut().entry(colorref(c).0).or_insert_with(|| unsafe { CreateSolidBrush(colorref(c)) }))
}

/// Fill the whole client area of `hwnd` with the window colour (for `WM_ERASEBKGND`).
pub fn erase_background(hwnd: HWND, wp: WPARAM) -> LRESULT {
    let dc = HDC(wp.0 as *mut core::ffi::c_void);
    let mut r = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut r);
        FillRect(dc, &r, brush(pal().bg));
    }
    LRESULT(1)
}

/// Colours for `WM_CTLCOLOR*`: labels are transparent over the window colour and edit boxes
/// and list boxes use the field colour. Buttons and check boxes draw themselves.
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
        SetBkColor(dc, colorref(p.bg));
        SetBkMode(dc, TRANSPARENT);
        LRESULT(brush(p.bg).0 as isize)
    }
}

/// Which native control `style_control` is preparing.
#[derive(Clone, Copy, PartialEq)]
pub enum Ctl {
    Edit,
    /// Push buttons, check boxes, radio buttons, group boxes, tabs and list boxes.
    Button,
    List,
    Tree,
}

/// Switch a native control to its dark theme, or back to the default light one.
pub fn style_control(h: HWND, kind: Ctl) {
    let dark = is_dark();
    unsafe {
        let _ = match (kind, dark) {
            (Ctl::Edit, true) => SetWindowTheme(h, w!("DarkMode_CFD"), PCWSTR::null()),
            (Ctl::List, true) => SetWindowTheme(h, w!("DarkMode_ItemsView"), PCWSTR::null()),
            (_, true) => SetWindowTheme(h, w!("DarkMode_Explorer"), PCWSTR::null()),
            (Ctl::List | Ctl::Tree, false) => SetWindowTheme(h, w!("Explorer"), PCWSTR::null()),
            // NULL, NULL restores the control's default theme (after a switch back from dark).
            _ => SetWindowTheme(h, PCWSTR::null(), PCWSTR::null()),
        };
    }
    let p = pal();
    unsafe {
        match kind {
            Ctl::List => {
                let _ = SetWindowSubclass(h, Some(list_proc), 3, 0);
                // LVM_SETBKCOLOR / LVM_SETTEXTCOLOR / LVM_SETTEXTBKCOLOR
                SendMessageW(h, 0x1001, None, Some(LPARAM(colorref(p.field).0 as isize)));
                SendMessageW(h, 0x1024, None, Some(LPARAM(colorref(p.text).0 as isize)));
                SendMessageW(h, 0x1026, None, Some(LPARAM(colorref(p.field).0 as isize)));
            }
            Ctl::Tree => {
                // TVM_SETBKCOLOR / TVM_SETTEXTCOLOR
                SendMessageW(h, 0x111D, None, Some(LPARAM(colorref(p.card).0 as isize)));
                SendMessageW(h, 0x111E, None, Some(LPARAM(colorref(p.text).0 as isize)));
            }
            _ => {}
        }
        let _ = InvalidateRect(Some(h), None, true);
    }
}

/// Dark mode only: lists keep a white native header, so their `NM_CUSTOMDRAW` (which the list
/// control receives from its header) is answered to paint a dark bar with themed text. Header
/// items are static, so this cannot flicker.
fn header_custom_draw(lp: LPARAM) -> Option<LRESULT> {
    use windows::Win32::UI::Controls::{HDI_TEXT, HDITEMW, NMCUSTOMDRAW};
    const NM_CUSTOMDRAW: u32 = (-12i32) as u32;
    if !is_dark() {
        return None;
    }
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
        // CDDS_PREPAINT: paint the whole bar, then ask about each item.
        0x1 => {
            let mut r = RECT::default();
            unsafe {
                let _ = GetClientRect(nm.hdr.hwndFrom, &mut r);
                FillRect(nm.hdc, &r, brush(p.card));
            }
            Some(LRESULT(0x30)) // CDRF_NOTIFYITEMDRAW | CDRF_NOTIFYPOSTPAINT
        }
        // CDDS_POSTPAINT: cover the filler the header draws after the last column.
        0x2 => {
            let count = unsafe { SendMessageW(nm.hdr.hwndFrom, 0x1200, None, None) }.0 as usize; // HDM_GETITEMCOUNT
            let mut client = RECT::default();
            let mut last = RECT::default();
            unsafe {
                let _ = GetClientRect(nm.hdr.hwndFrom, &mut client);
                if count > 0 {
                    SendMessageW(nm.hdr.hwndFrom, 0x1207, Some(WPARAM(count - 1)), Some(LPARAM(&mut last as *mut _ as isize))); // HDM_GETITEMRECT
                }
                if last.right < client.right {
                    FillRect(nm.hdc, &RECT { left: last.right, top: client.top, right: client.right, bottom: client.bottom }, brush(p.card));
                }
            }
            Some(LRESULT(0))
        }
        // CDDS_ITEMPREPAINT
        0x1_0001 => {
            let mut text = [0u16; 128];
            let mut item = HDITEMW { mask: HDI_TEXT, pszText: windows::core::PWSTR(text.as_mut_ptr()), cchTextMax: 128, ..Default::default() };
            unsafe {
                SendMessageW(nm.hdr.hwndFrom, 0x120B, Some(WPARAM(nm.dwItemSpec)), Some(LPARAM(&mut item as *mut _ as isize))); // HDM_GETITEMW
            }
            let cell = nm.rc;
            let len = text.iter().position(|c| *c == 0).unwrap_or(0);
            unsafe {
                // Column divider, then the title in the text colour.
                FillRect(nm.hdc, &RECT { left: cell.right - 1, top: cell.top + 4, right: cell.right, bottom: cell.bottom - 4 }, brush((0x55, 0x55, 0x55)));
                SetBkMode(nm.hdc, TRANSPARENT);
                SetTextColor(nm.hdc, colorref(p.text));
                let mut area = RECT { left: cell.left + 6, top: cell.top, right: cell.right - 4, bottom: cell.bottom };
                DrawTextW(nm.hdc, &mut text[..len], &mut area, DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS);
            }
            Some(LRESULT(0x4)) // CDRF_SKIPDEFAULT
        }
        _ => Some(LRESULT(0)),
    }
}

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
    super::dwm::apply_opaque(hwnd, is_dark());
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
            assert!((lum(p.text) - lum(p.bg)).abs() > 120.0, "text on window colour");
            assert!((lum(p.text) - lum(p.field)).abs() > 120.0, "text in fields");
        }
    }
}
