//! Windows clipboard access: text, the HTML / RTF formats, a private binary
//! format, and the change listener that feeds the Clipboard History panel.

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows::core::PCWSTR;

const CF_UNICODETEXT: u32 = 13;

/// Holds the clipboard open; closes it when dropped.
struct Open;

impl Open {
    fn new(owner: HWND) -> Option<Open> {
        // Another program may be using the clipboard for a moment.
        for _ in 0..10 {
            if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
                return Some(Open);
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        None
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn format_id(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

fn put(format: u32, bytes: &[u8]) -> bool {
    unsafe {
        let Ok(mem) = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) else { return false };
        let p = GlobalLock(mem) as *mut u8;
        if p.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        let _ = GlobalUnlock(mem);
        SetClipboardData(format, Some(HANDLE(mem.0))).is_ok()
    }
}

fn read(format: u32) -> Option<Vec<u8>> {
    unsafe {
        let h = GetClipboardData(format).ok()?;
        let mem = HGLOBAL(h.0);
        let size = GlobalSize(mem);
        let p = GlobalLock(mem) as *const u8;
        if p.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(p, size).to_vec();
        let _ = GlobalUnlock(mem);
        Some(v)
    }
}

pub fn set_text(owner: HWND, text: &str) -> bool {
    let Some(_c) = Open::new(owner) else { return false };
    let mut units: Vec<u16> = text.encode_utf16().collect();
    units.push(0);
    let bytes: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    unsafe {
        let _ = EmptyClipboard();
    }
    put(CF_UNICODETEXT, &bytes)
}

pub fn get_text(owner: HWND) -> Option<String> {
    let _c = Open::new(owner)?;
    let bytes = read(CF_UNICODETEXT)?;
    let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

/// Raw bytes of a registered clipboard format, if the clipboard holds it.
pub fn get_bytes(owner: HWND, name: &str) -> Option<Vec<u8>> {
    let id = format_id(name);
    let _c = Open::new(owner)?;
    if unsafe { IsClipboardFormatAvailable(id) }.is_err() {
        return None;
    }
    read(id)
}

/// Replace the clipboard with `bytes` in a private format, plus a readable text copy.
pub fn set_binary(owner: HWND, name: &str, bytes: &[u8]) -> bool {
    let id = format_id(name);
    let Some(_c) = Open::new(owner) else { return false };
    unsafe {
        let _ = EmptyClipboard();
    }
    let shown: String = bytes.iter().map(|b| if *b == 0 { ' ' } else { *b as char }).collect();
    let mut units: Vec<u16> = shown.encode_utf16().collect();
    units.push(0);
    let text: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    put(id, bytes) && put(CF_UNICODETEXT, &text)
}

/// Replace the clipboard with a rich format (`"Rich Text Format"` or `"HTML Format"`)
/// plus a plain-text copy for programs that only paste text.
pub fn set_rich(owner: HWND, name: &str, bytes: &[u8], plain: &str) -> bool {
    let id = format_id(name);
    let Some(_c) = Open::new(owner) else { return false };
    unsafe {
        let _ = EmptyClipboard();
    }
    let mut units: Vec<u16> = plain.encode_utf16().collect();
    units.push(0);
    let text: Vec<u8> = units.iter().flat_map(|u| u.to_le_bytes()).collect();
    let mut payload = bytes.to_vec();
    payload.push(0);
    put(id, &payload) && put(CF_UNICODETEXT, &text)
}

/// A CF_HTML clipboard payload (with its offset header) around an HTML fragment.
pub fn html_payload(fragment: &str) -> Vec<u8> {
    let head = "<html><body><!--StartFragment-->";
    let tail = "<!--EndFragment--></body></html>";
    let header = |s: usize, e: usize, total: usize| {
        format!("Version:0.9\r\nStartHTML:{:010}\r\nEndHTML:{total:010}\r\nStartFragment:{s:010}\r\nEndFragment:{e:010}\r\n", 0)
    };
    let hlen = header(0, 0, 0).len();
    let start = hlen + head.len();
    let end = start + fragment.len();
    let total = end + tail.len();
    let mut out = header(start, end, total).replace("StartHTML:0000000000", &format!("StartHTML:{hlen:010}"));
    out.push_str(head);
    out.push_str(fragment);
    out.push_str(tail);
    out.into_bytes()
}

/// Start (or stop) receiving `WM_CLIPBOARDUPDATE` on `hwnd`.
pub fn listen(hwnd: HWND, on: bool) {
    unsafe {
        if on {
            let _ = AddClipboardFormatListener(hwnd);
        } else {
            let _ = RemoveClipboardFormatListener(hwnd);
        }
    }
}

/// The HTML fragment of a CF_HTML payload (offsets come from its header).
pub fn html_fragment(payload: &[u8]) -> String {
    let text = String::from_utf8_lossy(payload);
    let offset = |key: &str| -> Option<usize> {
        let at = text.find(key)? + key.len();
        text[at..].lines().next()?.trim().parse().ok()
    };
    match (offset("StartFragment:"), offset("EndFragment:")) {
        (Some(a), Some(b)) if a <= b && b <= payload.len() => String::from_utf8_lossy(&payload[a..b]).into_owned(),
        _ => text.trim_end_matches('\0').to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_fragment_uses_header_offsets() {
        let body = "<html><body><!--StartFragment--><b>hi</b><!--EndFragment--></body></html>";
        let header = |s: usize, e: usize| format!("Version:0.9\r\nStartHTML:0000000000\r\nEndHTML:0000000000\r\nStartFragment:{s:010}\r\nEndFragment:{e:010}\r\n");
        let hlen = header(0, 0).len();
        let s = hlen + body.find("<b>").unwrap();
        let e = hlen + body.find("<!--EndFragment").unwrap();
        let payload = format!("{}{}", header(s, e), body);
        assert_eq!(html_fragment(payload.as_bytes()), "<b>hi</b>");
        assert_eq!(html_fragment(b"<p>plain</p>\0"), "<p>plain</p>");
    }

    #[test]
    fn html_payload_round_trips() {
        let p = html_payload("<b>héllo</b>");
        assert_eq!(html_fragment(&p), "<b>héllo</b>");
        let text = String::from_utf8(p).unwrap();
        let start_html: usize = text.split("StartHTML:").nth(1).unwrap()[..10].parse().unwrap();
        assert!(text[start_html..].starts_with("<html>"));
    }
}
