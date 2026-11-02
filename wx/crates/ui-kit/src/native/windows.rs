//! Windows: dark title bars, window styles and showing popups without taking the focus.

use super::Native;

pub struct Windows;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn RedrawWindow(hwnd: isize, rect: *const i32, region: isize, flags: u32) -> i32;
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
    fn GetMenu(hwnd: isize) -> isize;
    fn SetMenu(hwnd: isize, menu: isize) -> i32;
    fn ShowWindow(hwnd: isize, command: i32) -> i32;
    fn SetWindowsHookExW(id: i32, proc: unsafe extern "system" fn(i32, usize, isize) -> isize, module: isize, thread: u32) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
    fn GetKeyState(vk: i32) -> i16;
    fn GetForegroundWindow() -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, process: *mut u32) -> u32;
    fn EnumChildWindows(parent: isize, proc: unsafe extern "system" fn(isize, isize) -> i32, lparam: isize) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetCurrentProcessId() -> u32;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attr: u32, value: *const i32, size: u32) -> i32;
}

#[link(name = "uxtheme")]
unsafe extern "system" {
    fn SetWindowTheme(hwnd: isize, app: *const u16, ids: *const u16) -> i32;
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
const WS_EX_NOACTIVATE: isize = 0x0800_0000;
const GWL_STYLE: i32 = -16;
const WS_BORDER: isize = 0x0080_0000;
const WS_EX_EDGES: isize = 0x0000_0200 | 0x0002_0000; // WS_EX_CLIENTEDGE | WS_EX_STATICEDGE
const SW_SHOWNOACTIVATE: i32 = 4;
const WH_KEYBOARD: i32 = 2;
const VK_CONTROL: i32 = 0x11;

type KeyFn = fn(u32, bool) -> bool;

thread_local! {
    static KEYS: std::cell::Cell<Option<KeyFn>> = const { std::cell::Cell::new(None) };
    static HOOK: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    // The key whose press was consumed: its release is consumed too.
    static EATEN: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

unsafe extern "system" fn key_hook(code: i32, wparam: usize, lparam: isize) -> isize {
    if code == 0
        && let Some(f) = KEYS.with(std::cell::Cell::get)
    {
        let released = lparam & (1 << 31) != 0;
        if released {
            if EATEN.with(std::cell::Cell::get) == wparam {
                return 1;
            }
        } else {
            let alt = lparam & (1 << 29) != 0;
            let ctrl = unsafe { GetKeyState(VK_CONTROL) } < 0;
            if f(wparam as u32, !alt && !ctrl) {
                EATEN.with(|e| e.set(wparam));
                return 1;
            }
        }
    }
    unsafe { CallNextHookEx(HOOK.with(std::cell::Cell::get), code, wparam, lparam) }
}

impl Native for Windows {
    fn style_window(&self, window: isize, dark: bool) {
        unsafe {
            if GetAncestor(window, 2) == window {
                // GA_ROOT: a top-level window, owned or not.
                let on = i32::from(dark);
                DwmSetWindowAttribute(window, 20, &raw const on, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
                SetWindowPos(window, 0, 0, 0, 0, 0, 0x37);
                RedrawWindow(window, std::ptr::null(), 0, 0x0001 | 0x0004 | 0x0400 | 0x0100); // INVALIDATE | ERASE | FRAME | UPDATENOW
            }
            theme_child(window, isize::from(dark));
            EnumChildWindows(window, theme_child, isize::from(dark));
        }
    }

    fn detach_menu_bar(&self, window: isize) -> isize {
        unsafe {
            let menu = GetMenu(window);
            SetMenu(window, 0);
            menu
        }
    }

    fn grab_keys(&self, on_key: Option<fn(u32, bool) -> bool>) -> bool {
        KEYS.with(|k| k.set(on_key));
        let installed = HOOK.with(std::cell::Cell::get);
        match on_key {
            Some(_) if installed == 0 => {
                let h = unsafe { SetWindowsHookExW(WH_KEYBOARD, key_hook, 0, GetCurrentThreadId()) };
                HOOK.with(|c| c.set(h));
                h != 0
            }
            Some(_) => true,
            None => {
                if installed != 0 {
                    unsafe { UnhookWindowsHookEx(installed) };
                    HOOK.with(|c| c.set(0));
                }
                true
            }
        }
    }

    fn remove_edge(&self, window: isize) {
        unsafe {
            let style = GetWindowLongPtrW(window, GWL_STYLE);
            SetWindowLongPtrW(window, GWL_STYLE, style & !WS_BORDER);
            let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, ex & !WS_EX_EDGES);
            SetWindowPos(window, 0, 0, 0, 0, 0, 0x37); // NOMOVE | NOSIZE | NOZORDER | FRAMECHANGED | NOACTIVATE
        }
    }

    fn is_foreground(&self) -> bool {
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(GetForegroundWindow(), &raw mut pid);
            pid == GetCurrentProcessId()
        }
    }

    fn show_without_activating(&self, window: isize) -> bool {
        unsafe {
            let ex = GetWindowLongPtrW(window, GWL_EXSTYLE);
            SetWindowLongPtrW(window, GWL_EXSTYLE, ex | WS_EX_NOACTIVATE);
            ShowWindow(window, SW_SHOWNOACTIVATE);
        }
        true
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
