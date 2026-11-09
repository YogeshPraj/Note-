//! Scintilla as a plain Win32 child of the WinUI window. windows-reactor does not hand out the window handle or an
//! element's place in it, so the window is found among this thread's windows, and the editor is laid over the
//! placeholder panel, which is the window's last row: its size comes from the panel, its place from the window's
//! bottom-right corner.
#![allow(unsafe_code)]

use std::cell::RefCell;

use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, EnumThreadWindows, GW_CHILD, GW_HWNDNEXT, GetClassNameW, GetClientRect, GetWindow, GetWindowRect, HWND_TOP, IsWindowVisible, LWA_ALPHA,
    SWP_SHOWWINDOW, SendMessageW, SetLayeredWindowAttributes, SetWindowPos, WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_TABSTOP, WS_VISIBLE,
};

const SCI_GETLENGTH: u32 = 2006;
const SCI_GETTEXT: u32 = 2182;
const SCI_SETTEXT: u32 = 2181;
const SCI_EMPTYUNDOBUFFER: u32 = 2175;
const SCI_SETSAVEPOINT: u32 = 2014;

struct Host {
    window: HWND,
    editor: HWND,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn class_of(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), 128) };
    String::from_utf16_lossy(&buf[..usize::try_from(n).unwrap_or(0)])
}

unsafe extern "system" fn collect(h: HWND, l: LPARAM) -> i32 {
    let found = unsafe { &mut *(l as *mut Vec<(HWND, String)>) };
    found.push((h, class_of(h)));
    1
}

/// This thread's top-level windows and their classes.
pub fn thread_windows() -> Vec<(HWND, String)> {
    let mut found: Vec<(HWND, String)> = Vec::new();
    unsafe { EnumThreadWindows(GetCurrentThreadId(), Some(collect), &mut found as *mut _ as LPARAM) };
    found
}

fn find_window() -> Option<HWND> {
    thread_windows().into_iter().find(|(h, c)| c == "WinUIDesktopWin32WindowClass" && unsafe { IsWindowVisible(*h) } != 0).map(|(h, _)| h)
}

fn create(window: HWND) -> Result<HWND, String> {
    let dll = std::env::var("NOTEPP_SPIKE_SCINTILLA").unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/../../scintilla/bin/Scintilla.dll").into());
    if unsafe { LoadLibraryW(wide(&dll).as_ptr()) }.is_null() {
        return Err(format!("could not load {dll}"));
    }
    let class = wide("Scintilla");
    let style = WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_TABSTOP;
    // A plain GDI child is drawn under WinUI's composition; a layered child gets its own surface (needs the manifest).
    let layered = std::env::var("NOTEPP_SPIKE_HOST").as_deref() != Ok("plain");
    let ex = if layered { WS_EX_LAYERED } else { 0 };
    let editor = unsafe {
        CreateWindowExW(ex, class.as_ptr(), std::ptr::null(), style, 0, 0, 10, 10, window, std::ptr::null_mut(), std::ptr::null_mut(), std::ptr::null())
    };
    if editor.is_null() {
        return Err(format!("CreateWindowExW(Scintilla, layered={layered}) failed"));
    }
    if layered {
        unsafe { SetLayeredWindowAttributes(editor, 0, 255, LWA_ALPHA) };
    }
    crate::keys::set_editor(editor);
    Ok(editor)
}

/// Lay the editor over the placeholder panel (`width` by `height` DIPs at `scale`), creating it the first time.
/// Returns a line for the report.
pub fn place(width: f64, height: f64, scale: f32) -> Result<String, String> {
    let (window, editor, fresh) = match HOST.with(|h| h.borrow().as_ref().map(|h| (h.window, h.editor))) {
        Some((w, e)) => (w, e, false),
        None => {
            let window = find_window().ok_or("no visible WinUIDesktopWin32WindowClass window on this thread")?;
            let editor = create(window)?;
            HOST.with(|h| *h.borrow_mut() = Some(Host { window, editor }));
            (window, editor, true)
        }
    };
    let mut client = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe { GetClientRect(window, &mut client) };
    let px = |dip: f64| (dip * f64::from(scale)).round() as i32;
    let (w, h) = (px(width), px(height));
    let (x, y) = (client.right - w, client.bottom - h);
    unsafe { SetWindowPos(editor, HWND_TOP, x, y, w, h, SWP_SHOWWINDOW) };
    Ok(format!("{} editor at {x},{y} {w}x{h} in client {}x{} (scale {scale})", if fresh { "created" } else { "moved" }, client.right, client.bottom))
}

fn send(msg: u32, w: usize, l: isize) -> isize {
    HOST.with(|h| h.borrow().as_ref().map_or(0, |h| unsafe { SendMessageW(h.editor, msg, w, l) }))
}

pub fn is_ready() -> bool {
    HOST.with(|h| h.borrow().is_some())
}

/// Replace the editor's text (a fresh document: no undo history, saved).
pub fn set_text(text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    send(SCI_SETTEXT, 0, bytes.as_ptr() as isize);
    send(SCI_EMPTYUNDOBUFFER, 0, 0);
    send(SCI_SETSAVEPOINT, 0, 0);
}

pub fn text() -> String {
    let len = usize::try_from(send(SCI_GETLENGTH, 0, 0)).unwrap_or(0);
    let mut buf = vec![0u8; len + 1];
    send(SCI_GETTEXT, len + 1, buf.as_mut_ptr() as isize);
    buf.truncate(len);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Run a Scintilla command message (undo, cut, ...).
pub fn command(msg: u32) {
    send(msg, 0, 0);
}

pub fn focus_editor() {
    HOST.with(|h| {
        if let Some(h) = h.borrow().as_ref() {
            unsafe { SetFocus(h.editor) };
        }
    });
}

pub fn handles() -> Option<(HWND, HWND)> {
    HOST.with(|h| h.borrow().as_ref().map(|h| (h.window, h.editor)))
}

/// The window's children, top of the z-order first: class, visible, screen rectangle.
pub fn children() -> Vec<String> {
    let Some((window, editor)) = handles() else { return Vec::new() };
    let mut out = Vec::new();
    let mut h = unsafe { GetWindow(window, GW_CHILD) };
    while !h.is_null() {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe { GetWindowRect(h, &mut r) };
        let mark = if h == editor { " (editor)" } else { "" };
        out.push(format!("{}{mark} visible={} {},{} {}x{}", class_of(h), unsafe { IsWindowVisible(h) }, r.left, r.top, r.right - r.left, r.bottom - r.top));
        h = unsafe { GetWindow(h, GW_HWNDNEXT) };
    }
    out
}

/// Keep the raw pointer type out of the component's code.
pub fn hwnd_value(h: HWND) -> usize {
    h as usize
}
