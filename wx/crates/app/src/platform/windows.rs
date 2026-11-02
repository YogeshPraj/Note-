//! Windows: Explorer, cmd, the Recycle Bin and the regional date and time formats.
//!
//! Win32 structures use fixed-width integers, so the cast lints are allowed in this platform adapter only.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_precision_loss)]

use std::os::windows::process::CommandExt;
use std::path::Path;

use notepp_core::edit::DateTime;

use super::Platform;

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

#[link(name = "user32")]
unsafe extern "system" {
    fn OpenClipboard(owner: isize) -> i32;
    fn CloseClipboard() -> i32;
    fn EmptyClipboard() -> i32;
    fn RegisterClipboardFormatW(name: *const u16) -> u32;
    fn IsClipboardFormatAvailable(format: u32) -> i32;
    fn GetClipboardData(format: u32) -> isize;
    fn SetClipboardData(format: u32, mem: isize) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GlobalAlloc(flags: u32, bytes: usize) -> isize;
    fn GlobalFree(mem: isize) -> isize;
    fn GlobalLock(mem: isize) -> *mut u8;
    fn GlobalUnlock(mem: isize) -> i32;
    fn GlobalSize(mem: isize) -> usize;
    fn GetLocalTime(t: *mut SystemTime);
    fn GetDateFormatEx(locale: *const u16, flags: u32, date: *const SystemTime, format: *const u16, out: *mut u16, cch: i32, calendar: *const u16) -> i32;
    fn GetTimeFormatEx(locale: *const u16, flags: u32, time: *const SystemTime, format: *const u16, out: *mut u16, cch: i32) -> i32;
}

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 2;

/// The clipboard, held open; closed when dropped.
struct Clipboard;

#[allow(clippy::unused_self, reason = "`read` and `put` take the guard to prove the clipboard is open")]
impl Clipboard {
    fn open(owner: isize) -> Option<Clipboard> {
        // Another program may have it open for a moment.
        for _ in 0..10 {
            if unsafe { OpenClipboard(owner) } != 0 {
                return Some(Clipboard);
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        None
    }

    fn read(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            if IsClipboardFormatAvailable(format) == 0 {
                return None;
            }
            let mem = GetClipboardData(format);
            let p = if mem == 0 { std::ptr::null_mut() } else { GlobalLock(mem) };
            if p.is_null() {
                return None;
            }
            let bytes = std::slice::from_raw_parts(p, GlobalSize(mem)).to_vec();
            GlobalUnlock(mem);
            Some(bytes)
        }
    }

    /// Add `bytes` in `format` (the clipboard owns the memory once this succeeds).
    fn put(&self, format: u32, bytes: &[u8]) -> bool {
        unsafe {
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
            let p = if mem == 0 { std::ptr::null_mut() } else { GlobalLock(mem) };
            if p.is_null() {
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            GlobalUnlock(mem);
            if SetClipboardData(format, mem) == 0 {
                GlobalFree(mem);
                return false;
            }
            true
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

fn format_id(name: &str) -> u32 {
    let w: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { RegisterClipboardFormatW(w.as_ptr()) }
}

impl Platform for Windows {
    fn clipboard_format(&self, owner: isize, name: &str) -> Option<Vec<u8>> {
        let id = format_id(name);
        Clipboard::open(owner)?.read(id)
    }

    fn set_clipboard_format(&self, owner: isize, name: &str, bytes: &[u8], text: &str) -> bool {
        let id = format_id(name);
        let Some(clip) = Clipboard::open(owner) else { return false };
        let units: Vec<u8> = text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect();
        unsafe { EmptyClipboard() };
        clip.put(id, bytes) && clip.put(CF_UNICODETEXT, &units)
    }

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

    fn recycle(&self, path: &Path) -> std::io::Result<()> {
        // The shell's own "send to Recycle Bin", through the .NET file-system helper.
        let script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
            path.display().to_string().replace('\'', "''")
        );
        let out = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output()?;
        if out.status.success() { Ok(()) } else { Err(std::io::Error::other(String::from_utf8_lossy(&out.stderr).trim().to_string())) }
    }

    fn local_time(&self) -> DateTime {
        let mut t = SystemTime::default();
        unsafe { GetLocalTime(&raw mut t) };
        DateTime {
            year: i32::from(t.year),
            month: u32::from(t.month),
            day: u32::from(t.day),
            weekday: u32::from(t.weekday),
            hour: u32::from(t.hour),
            minute: u32::from(t.minute),
            second: u32::from(t.second),
        }
    }

    fn date_time_text(&self, long: bool) -> String {
        let mut now = SystemTime::default();
        let (mut date, mut time) = ([0u16; 128], [0u16; 64]);
        unsafe {
            GetLocalTime(&raw mut now);
            let d = GetDateFormatEx(
                std::ptr::null(),
                if long { 2 } else { 1 },
                &raw const now,
                std::ptr::null(),
                date.as_mut_ptr(),
                date.len() as i32,
                std::ptr::null(),
            );
            let t = GetTimeFormatEx(std::ptr::null(), 0, &raw const now, std::ptr::null(), time.as_mut_ptr(), time.len() as i32);
            let part = |buf: &[u16], n: i32| String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())]);
            format!("{} {}", part(&time, t), part(&date, d))
        }
    }
}
