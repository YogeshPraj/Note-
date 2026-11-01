//! Windows: dark mode, window styles and the native menu loop.

use super::{Native, winmenu};

pub struct Windows;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn RedrawWindow(hwnd: isize, rect: *const i32, region: isize, flags: u32) -> i32;
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
    fn EnumChildWindows(parent: isize, proc: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const i32, size: u32) -> i32;
}

#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(hwnd: isize, app: *const u16, ids: *const u16) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn LoadLibraryW(name: *const u16) -> isize;
    fn GetProcAddress(module: isize, name: usize) -> usize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

unsafe extern "system" fn theme_child(hwnd: isize, dark: isize) -> i32 {
    let name = wide(if dark != 0 { "DarkMode_Explorer" } else { "Explorer" });
    unsafe { SetWindowTheme(hwnd, name.as_ptr(), std::ptr::null()) };
    1
}

const GWL_EXSTYLE: i32 = -20;
const WS_EX_TOPMOST: isize = 0x8;

impl Native for Windows {
    fn set_app_dark(&self, dark: bool) {
        // Undocumented uxtheme ordinals (what Explorer uses): 135 SetPreferredAppMode, 136 FlushMenuThemes.
        type SetMode = unsafe extern "system" fn(i32) -> i32;
        type Flush = unsafe extern "system" fn();
        unsafe {
            let lib = LoadLibraryW(wide("uxtheme.dll").as_ptr());
            if lib == 0 {
                return;
            }
            let (set, flush) = (GetProcAddress(lib, 135), GetProcAddress(lib, 136));
            if set != 0 {
                std::mem::transmute::<usize, SetMode>(set)(if dark { 2 } else { 3 }); // ForceDark / ForceLight
            }
            if flush != 0 {
                std::mem::transmute::<usize, Flush>(flush)();
            }
        }
    }

    fn style_window(&self, window: isize, dark: bool) {
        unsafe {
            if GetAncestor(window, 2) == window {
                // GA_ROOT: a top-level window, owned or not.
                let on = dark as i32;
                DwmSetWindowAttribute(window, 20, &on, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
                SetWindowPos(window, 0, 0, 0, 0, 0, 0x37);
                RedrawWindow(window, std::ptr::null(), 0, 0x0001 | 0x0004 | 0x0400 | 0x0100); // INVALIDATE | ERASE | FRAME | UPDATENOW
            }
            theme_child(window, dark as isize);
            EnumChildWindows(window, theme_child, dark as isize);
        }
    }

    fn detach_menu_bar(&self, window: isize) -> isize {
        winmenu::detach(window)
    }

    fn popup_menu(&self, bar: isize, window: isize, index: i32, count: i32, anchor: &dyn Fn(i32) -> (i32, i32)) {
        winmenu::popup(bar, window, index, count, anchor)
    }

    fn set_rtl(&self, window: isize, rtl: bool) {
        let bits: isize = 0x0000_1000 | 0x0000_2000 | 0x0000_4000; // WS_EX_RIGHT | WS_EX_RTLREADING | WS_EX_LEFTSCROLLBAR
        unsafe {
            let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, if rtl { ex | bits } else { ex & !bits });
            SetWindowPos(window, 0, 0, 0, 0, 0, 0x37); // NOMOVE | NOSIZE | NOZORDER | FRAMECHANGED | NOACTIVATE
        }
    }

    fn is_topmost(&self, window: isize) -> bool {
        unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_TOPMOST != 0 }
    }

    fn set_topmost(&self, window: isize, on: bool) {
        unsafe { SetWindowPos(window, if on { -1 } else { -2 }, 0, 0, 0, 0, 0x13) }; // NOMOVE | NOSIZE | NOACTIVATE
    }
}
