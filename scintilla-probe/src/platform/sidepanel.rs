//! The right-hand side panel: either the Character Panel (an ASCII table whose
//! entries are inserted on double-click) or the Clipboard History (recent
//! copies, pasted on double-click, Delete removes one).

use std::cell::{Cell, RefCell};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PWSTR, w};

use super::chrome::ui_font_handle;

pub const WIDTH: i32 = 250;
const MAX_HISTORY: usize = 50;
const VK_DELETE: u16 = 0x2E;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mode {
    Characters,
    Clipboard,
}

thread_local! {
    static LIST: Cell<Option<HWND>> = const { Cell::new(None) };
    static MODE: Cell<Option<Mode>> = const { Cell::new(None) };
    static HISTORY: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

const CONTROL_NAMES: [&str; 32] = [
    "NUL", "SOH", "STX", "ETX", "EOT", "ENQ", "ACK", "BEL", "BS", "TAB", "LF", "VT", "FF", "CR", "SO", "SI", "DLE", "DC1", "DC2", "DC3", "DC4",
    "NAK", "SYN", "ETB", "CAN", "EM", "SUB", "ESC", "FS", "GS", "RS", "US",
];

/// The character a table row stands for (Windows-1252 above 127).
pub fn char_for(n: u8) -> char {
    if n < 128 {
        n as char
    } else {
        encoding_rs::WINDOWS_1252.decode(&[n]).0.chars().next().unwrap_or('?')
    }
}

fn label_for(n: u8) -> String {
    match n {
        0..=31 => CONTROL_NAMES[n as usize].to_string(),
        127 => "DEL".into(),
        128..=159 if char_for(n) == '\u{FFFD}' || char_for(n).is_control() => String::new(),
        _ => char_for(n).to_string(),
    }
}

pub fn mode() -> Option<Mode> {
    MODE.with(|m| m.get())
}

pub fn hwnd() -> Option<HWND> {
    LIST.with(|c| c.get())
}

pub fn create(host: HWND) -> HWND {
    unsafe {
        let _ = InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_LISTVIEW_CLASSES,
        });
        // LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS
        let style = WS_CHILD | WS_CLIPSIBLINGS | WS_BORDER | WS_TABSTOP | WINDOW_STYLE(0x1 | 0x4 | 0x8);
        let list = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SysListView32"),
            None,
            style,
            0,
            0,
            WIDTH,
            100,
            Some(host),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap();
        SendMessageW(list, WM_SETFONT, Some(WPARAM(ui_font_handle().0 as usize)), Some(LPARAM(1)));
        SendMessageW(list, LVM_SETEXTENDEDLISTVIEWSTYLE, Some(WPARAM(0x20)), Some(LPARAM(0x20))); // full-row select
        LIST.with(|c| c.set(Some(list)));
        list
    }
}

fn set_cell(list: HWND, row: i32, col: i32, text: &str) {
    let mut t: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let mut item = LVITEMW { mask: LVIF_TEXT, iItem: row, iSubItem: col, pszText: PWSTR(t.as_mut_ptr()), ..Default::default() };
    unsafe {
        let msg = if col == 0 { LVM_INSERTITEMW } else { LVM_SETITEMTEXTW };
        SendMessageW(list, msg, Some(WPARAM(row as usize)), Some(LPARAM(&mut item as *mut _ as isize)));
    }
}

fn set_columns(list: HWND, cols: &[(&str, i32)]) {
    unsafe {
        while SendMessageW(list, LVM_DELETECOLUMN, Some(WPARAM(0)), None).0 != 0 {}
        for (i, (name, width)) in cols.iter().enumerate() {
            let mut t: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let col = LVCOLUMNW { mask: LVCF_TEXT | LVCF_WIDTH, cx: *width, pszText: PWSTR(t.as_mut_ptr()), ..Default::default() };
            SendMessageW(list, LVM_INSERTCOLUMNW, Some(WPARAM(i)), Some(LPARAM(&col as *const _ as isize)));
        }
    }
}

fn fill() {
    let Some(list) = hwnd() else { return };
    unsafe {
        SendMessageW(list, LVM_DELETEALLITEMS, None, None);
    }
    match mode() {
        Some(Mode::Characters) => {
            set_columns(list, &[("Value", 60), ("Hex", 60), ("Char", 100)]);
            for n in 0..=255u8 {
                set_cell(list, n as i32, 0, &n.to_string());
                set_cell(list, n as i32, 1, &format!("{n:02X}"));
                set_cell(list, n as i32, 2, &label_for(n));
            }
        }
        Some(Mode::Clipboard) => {
            set_columns(list, &[("Clipboard History", WIDTH - 24)]);
            HISTORY.with(|h| {
                for (i, s) in h.borrow().iter().enumerate() {
                    set_cell(list, i as i32, 0, &preview(s));
                }
            });
        }
        None => {}
    }
}

/// One-line, length-limited form of a clipboard entry.
fn preview(s: &str) -> String {
    let flat: String = s.chars().map(|c| if c == '\r' || c == '\n' { '\u{21B5}' } else if c == '\t' { ' ' } else { c }).collect();
    let mut out: String = flat.chars().take(120).collect();
    if flat.chars().count() > 120 {
        out.push('\u{2026}');
    }
    out
}

pub fn show_mode(mode: Option<Mode>) {
    MODE.with(|m| m.set(mode));
    fill();
}

/// Remember a copied text (newest first, no repeats).
pub fn record_clipboard(text: &str) {
    if text.is_empty() {
        return;
    }
    HISTORY.with(|h| {
        let mut h = h.borrow_mut();
        h.retain(|x| x != text);
        h.insert(0, text.to_string());
        h.truncate(MAX_HISTORY);
    });
    if mode() == Some(Mode::Clipboard) {
        fill();
    }
}

fn selected() -> Option<usize> {
    let list = hwnd()?;
    let n = unsafe { SendMessageW(list, LVM_GETNEXTITEM, Some(WPARAM(usize::MAX)), Some(LPARAM(LVNI_SELECTED as isize))) }.0;
    (n >= 0).then_some(n as usize)
}

/// Handle a `WM_NOTIFY` from the list. Returns text to insert into the editor.
pub fn notify(lparam: LPARAM) -> Option<String> {
    let hdr = unsafe { &*(lparam.0 as *const NMHDR) };
    if Some(hdr.hwndFrom) != hwnd() {
        return None;
    }
    match (hdr.code, mode()) {
        (NM_DBLCLK, Some(Mode::Characters)) => selected().map(|n| char_for(n as u8).to_string()),
        (NM_DBLCLK, Some(Mode::Clipboard)) => selected().and_then(|n| HISTORY.with(|h| h.borrow().get(n).cloned())),
        (LVN_KEYDOWN, Some(Mode::Clipboard)) => {
            let key = unsafe { &*(lparam.0 as *const NMLVKEYDOWN) };
            if key.wVKey == VK_DELETE {
                if let Some(n) = selected() {
                    HISTORY.with(|h| {
                        let mut h = h.borrow_mut();
                        if n < h.len() {
                            h.remove(n);
                        }
                    });
                    fill();
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_map_like_windows_1252() {
        assert_eq!(char_for(65), 'A');
        assert_eq!(char_for(0xE9), 'é');
        assert_eq!(char_for(0x80), '€');
        assert_eq!(label_for(10), "LF");
        assert_eq!(label_for(127), "DEL");
    }

    #[test]
    fn previews_are_single_line_and_capped() {
        assert_eq!(preview("a\r\nb\tc"), "a\u{21B5}\u{21B5}b c");
        assert_eq!(preview(&"x".repeat(200)).chars().count(), 121);
    }
}
