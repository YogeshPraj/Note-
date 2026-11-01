//! Windows: Explorer, cmd, the Recycle Bin, regional date formats, window styles and native menus.

use std::os::windows::process::CommandExt;
use std::path::Path;

use notepp_core::edit::DateTime;

use super::{Platform, winmenu};

pub struct Windows;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct SystemTime {
    year: u16,
    month: u16,
    weekday: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    ms: u16,
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetLocalTime(t: *mut SystemTime);
    fn GetDateFormatEx(locale: *const u16, flags: u32, date: *const SystemTime, format: *const u16, out: *mut u16, cch: i32, calendar: *const u16) -> i32;
    fn GetTimeFormatEx(locale: *const u16, flags: u32, time: *const SystemTime, format: *const u16, out: *mut u16, cch: i32) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn RedrawWindow(hwnd: isize, rect: *const i32, region: isize, flags: u32) -> i32;
}

#[link(name = "user32")]
unsafe extern "system" {
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

impl Platform for Windows {
    fn reveal_in_file_manager(&self, path: &Path) {
        // Explorer wants `/select,"path"` as one argument with its own quotes.
        let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
    }

    fn open_terminal(&self, dir: Option<&Path>) {
        let mut cmd = std::process::Command::new("cmd");
        cmd.arg("/K");
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        let _ = cmd.creation_flags(0x10).spawn(); // CREATE_NEW_CONSOLE
    }

    fn open_default(&self, target: &str) {
        // Not `cmd /c start`: that would parse `&` and friends in addresses.
        let _ = std::process::Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(target).spawn();
    }

    fn open_with(&self, program: &str, path: &Path) {
        // `start` resolves registered programs such as chrome.exe; raw_arg keeps the quoting.
        let _ = std::process::Command::new("cmd").raw_arg(format!("/C start \"\" {program} \"{}\"", path.display())).creation_flags(0x0800_0000).spawn();
    }

    fn recycle(&self, path: &Path) -> Result<(), String> {
        // The shell's own "send to Recycle Bin", through the .NET file-system helper.
        let script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
            path.display().to_string().replace('\'', "''")
        );
        let out = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
    }

    fn local_time(&self) -> DateTime {
        let mut t = SystemTime::default();
        unsafe { GetLocalTime(&mut t) };
        DateTime { year: t.year as i32, month: t.month as u32, day: t.day as u32, weekday: t.weekday as u32, hour: t.hour as u32, minute: t.minute as u32, second: t.second as u32 }
    }

    fn date_time_text(&self, long: bool) -> String {
        let mut now = SystemTime::default();
        let (mut date, mut time) = ([0u16; 128], [0u16; 64]);
        unsafe {
            GetLocalTime(&mut now);
            let d = GetDateFormatEx(std::ptr::null(), if long { 2 } else { 1 }, &now, std::ptr::null(), date.as_mut_ptr(), date.len() as i32, std::ptr::null());
            let t = GetTimeFormatEx(std::ptr::null(), 0, &now, std::ptr::null(), time.as_mut_ptr(), time.len() as i32);
            let part = |buf: &[u16], n: i32| String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())]);
            format!("{} {}", part(&time, t), part(&date, d))
        }
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
            if GetAncestor(window, 2) == window {  // GA_ROOT: a top-level window, owned or not
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
}
