//! Windows 11 window frame: caption tint, Mica backdrop, rounded corners.
//!
//! Only documented DWM attributes. Every call fails harmlessly where the OS
//! doesn't support it — Mica needs Windows 11 22H2 (build 22621+), immersive
//! dark mode needs Windows 10 1809+ — so there is no version sniffing: older
//! systems simply keep their default frame.
//!
//! Deliberately absent: DWMWA_CAPTION_COLOR / DWMWA_TEXT_COLOR. Setting an
//! explicit caption colour switches Mica off in the title bar.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWINDOWATTRIBUTE, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute};
use windows::Win32::UI::Controls::MARGINS;

const DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1: i32 = 19; // Windows 10 1809–1909
const DWMWA_USE_IMMERSIVE_DARK_MODE: i32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: i32 = 33;
const DWMWA_SYSTEMBACKDROP_TYPE: i32 = 38;

const DWMWCP_ROUND: u32 = 2;
const DWMSBT_MAINWINDOW: u32 = 2; // Mica
const DWMSBT_NONE: u32 = 1;

fn set(hwnd: HWND, attr: i32, value: u32) -> bool {
    unsafe {
        DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(attr), &value as *const u32 as *const core::ffi::c_void, 4)
            .is_ok()
    }
}

/// Apply the frame styling to a top-level window. Returns whether the OS
/// accepted the Mica backdrop.
pub fn apply(hwnd: HWND, dark: bool) -> bool {
    set_dark(hwnd, dark);
    set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    set(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW)
}

/// Dark or light caption and rounded corners, without the Mica backdrop. For dialogs and popups
/// whose client area is painted opaque (Mica would show through unpainted pixels and make
/// native controls look patchy).
pub fn apply_opaque(hwnd: HWND, dark: bool) {
    set_dark(hwnd, dark);
    set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
    set(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_NONE);
}

/// Dark or light caption — and, with Mica on, a dark or light Mica tint.
pub fn set_dark(hwnd: HWND, dark: bool) {
    let d = dark as u32;
    if !set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, d) {
        set(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1, d);
    }
}

/// Rounded corners only (for popups that have no title bar).
pub fn round(hwnd: HWND) {
    set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND);
}

/// Let the backdrop show through the top `px` of the client area, wherever
/// the window paints pixels with alpha below 255.
pub fn extend_top(hwnd: HWND, px: i32) {
    let m = MARGINS { cyTopHeight: px, ..Default::default() };
    unsafe {
        let _ = DwmExtendFrameIntoClientArea(hwnd, &m);
    }
}
