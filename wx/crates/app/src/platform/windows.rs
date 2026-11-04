//! Windows: Explorer, cmd, the Recycle Bin and the regional date and time formats.
//!
//! Win32 structures use fixed-width integers, so the cast lints are allowed in this platform adapter only.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_precision_loss)]

use std::os::windows::process::CommandExt;
use std::path::Path;

use notepp_core::edit::DateTime;
use notepp_core::files::system_program;

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
    fn SetDllDirectoryW(path: *const u16) -> i32;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn ShellExecuteW(window: isize, verb: *const u16, file: *const u16, args: *const u16, dir: *const u16, show: i32) -> isize;
}

const CF_UNICODETEXT: u32 = 13;
const GMEM_MOVEABLE: u32 = 2;
const SW_SHOWNORMAL: i32 = 1;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// The file for [`Windows::recycle`]. It goes to the script as data: quotes in a file name, curly ones included, are
/// quotes to PowerShell, and a name built into the script would be code.
const RECYCLE_VAR: &str = "NOTEPP_RECYCLE_PATH";
const RECYCLE_SCRIPT: &str = "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($env:NOTEPP_RECYCLE_PATH, 'OnlyErrorDialogs', 'SendToRecycleBin')";

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

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn format_id(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(w.as_ptr()) }
}

impl Platform for Windows {
    fn harden_library_search(&self) {
        // Libraries loaded by name are no longer looked for in the current folder, which is the folder of a file that was
        // double-clicked. SetDefaultDllDirectories would be stricter but stops WebView2 from loading, and the web views
        // then fall back to Internet Explorer and stay blank. Ones the program links to: see docs/audit.md.
        let empty = wide("");
        unsafe { SetDllDirectoryW(empty.as_ptr()) };
    }

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
        let _ = std::process::Command::new(system_program("explorer.exe")).raw_arg(format!("/select,\"{}\"", path.display())).spawn();
    }

    fn open_terminal(&self, dir: Option<&Path>) {
        let mut cmd = std::process::Command::new(system_program("System32\\cmd.exe"));
        cmd.arg("/K");
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        let _ = cmd.creation_flags(0x10).spawn(); // CREATE_NEW_CONSOLE
    }

    fn open_default(&self, target: &str) {
        // Not `cmd /c start`: that would parse `&` and friends in addresses.
        let _ = std::process::Command::new(system_program("System32\\rundll32.exe")).arg("url.dll,FileProtocolHandler").arg(target).spawn();
    }

    fn open_with(&self, program: &str, path: &Path) {
        // The shell finds registered programs such as chrome.exe, and no command line parses the file's name. The folder
        // is given so that the search does not start in the current one.
        let (verb, file, args) = (wide("open"), wide(program), wide(&format!("\"{}\"", path.display())));
        let dir = wide(&system_program("System32").display().to_string());
        unsafe { ShellExecuteW(0, verb.as_ptr(), file.as_ptr(), args.as_ptr(), dir.as_ptr(), SW_SHOWNORMAL) };
    }

    fn recycle(&self, path: &Path) -> std::io::Result<()> {
        // The shell's own "send to Recycle Bin", through the .NET file-system helper.
        let out = std::process::Command::new(system_program("System32\\WindowsPowerShell\\v1.0\\powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", RECYCLE_SCRIPT])
            .env(RECYCLE_VAR, path)
            .creation_flags(CREATE_NO_WINDOW)
            .output()?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "moves a file to the real Recycle Bin"]
    fn a_quote_in_a_file_name_is_not_code() {
        let dir = std::env::temp_dir().join(format!("npp_recycle_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_current_dir(&dir).unwrap();
        let file = dir.join("x\u{2019}+(New-Item ran)+\u{2019}.txt");
        std::fs::write(&file, b"x").unwrap();
        Windows.recycle(&file).unwrap();
        assert!(!file.exists(), "it went to the Recycle Bin");
        assert!(!dir.join("ran").exists(), "the name ran as code");
        std::env::set_current_dir(std::env::temp_dir()).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
