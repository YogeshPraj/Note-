//! Scintilla as a plain Win32 child of the WinUI window. windows-reactor does not hand out the window handle or an
//! element's place in it, so the window is found among this thread's windows, and the editor is laid over the
//! placeholder panel, which is the window's last row: its size comes from the panel, its place from the window's
//! bottom-right corner.
#![allow(unsafe_code)]

use std::cell::RefCell;

use notepp_core::key_routing::{self as kr, Key, Place, Press, Route, Situation};
use notepp_core::keymap::KeyMap;
use notepp_core::shortcuts::Chord;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::LoadLibraryW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, SetFocus, VK_CONTROL, VK_F10, VK_MENU, VK_SHIFT, VK_TAB};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, EnumThreadWindows, GW_CHILD, GW_HWNDNEXT, GWL_EXSTYLE, GetClassNameW, GetClientRect, GetWindow, GetWindowLongPtrW, GetWindowRect,
    HWND_TOP, IsWindowVisible, LWA_ALPHA, SWP_SHOWWINDOW, SendMessageW, SetLayeredWindowAttributes, SetWindowPos, WM_APP, WM_KEYDOWN, WM_PAINT, WM_SYSKEYDOWN,
    WS_CHILD, WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_TABSTOP, WS_VISIBLE,
};

/// A key the editor saw, as `core` routed it.
#[derive(Clone, Debug, PartialEq)]
pub struct Routed {
    pub press: String,
    pub route: Route,
}

/// Probe message: `wparam` = virtual key, `lparam` bits 1 ctrl, 2 alt, 4 shift. Lets a test drive the routing
/// without a keyboard (posted key messages carry no modifier state).
pub const WM_PROBE_KEY: u32 = WM_APP + 1;

const SCI_GETLENGTH: u32 = 2006;
const SCI_GETTEXT: u32 = 2182;
const SCI_SETTEXT: u32 = 2181;
const SCI_EMPTYUNDOBUFFER: u32 = 2175;
const SCI_SETSAVEPOINT: u32 = 2014;

struct Host {
    window: HWND,
    editor: HWND,
}

type KeySink = Box<dyn Fn(Routed)>;

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
    static SINK: RefCell<Option<KeySink>> = const { RefCell::new(None) };
    static PAINTS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static ROUTING: RefCell<Option<(KeyMap, Vec<String>)>> = const { RefCell::new(None) };
}

/// Where routed keys go, and what the routing needs: the key map and the menu titles (with `&` marks).
pub fn set_routing(keymap: KeyMap, titles: Vec<String>, sink: impl Fn(Routed) + 'static) {
    ROUTING.with(|r| *r.borrow_mut() = Some((keymap, titles)));
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
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
    unsafe { SetWindowSubclass(editor, Some(keys), 1, 0) };
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

fn press_of(vk: u16, ctrl: bool, alt: bool, shift: bool) -> Press {
    let key = match vk {
        VK_TAB => Key::Tab,
        VK_F10 => Key::F10,
        v if (0x30..=0x5A).contains(&v) => u8::try_from(v).map_or(Key::Other, |b| Key::Char(char::from(b))),
        _ => Key::Other,
    };
    Press { key, ctrl, alt, shift, chord: Some(Chord { ctrl, alt, shift, vk }) }
}

/// Route a key through `core`. In this shell the editor is a Win32 window, so WinUI never sees its keys: shortcuts are
/// looked up here as in a floating window, menu letters as in the main one.
fn route(p: &Press) -> Route {
    ROUTING.with(|r| {
        let r = r.borrow();
        let Some((keymap, titles)) = r.as_ref() else { return Route::PassOn };
        let is_letter =
            |c: char| titles.iter().any(|t| t.char_indices().any(|(i, ch)| ch == '&' && t[i + 1..].chars().next().is_some_and(|n| n.eq_ignore_ascii_case(&c))));
        let main = kr::route(p, &Situation { place: Place::Main, menu_open: false, is_menu_letter: &is_letter, keymap });
        if main != Route::PassOn {
            return main;
        }
        kr::route(p, &Situation { place: Place::Floating, menu_open: false, is_menu_letter: &is_letter, keymap })
    })
}

fn deliver(p: Press) -> bool {
    let route = route(&p);
    let used = !matches!(route, Route::PassOn | Route::PastePicture);
    let press = format!("{}{}{}{:?}", if p.ctrl { "Ctrl+" } else { "" }, if p.alt { "Alt+" } else { "" }, if p.shift { "Shift+" } else { "" }, p.key);
    SINK.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink(Routed { press, route });
        }
    });
    used
}

unsafe extern "system" fn keys(h: HWND, msg: u32, w: WPARAM, l: LPARAM, _id: usize, _data: usize) -> LRESULT {
    let down = |vk: u16| unsafe { GetKeyState(i32::from(vk)) } < 0;
    match msg {
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let vk = u16::try_from(w).unwrap_or(0);
            if ![VK_CONTROL, VK_MENU, VK_SHIFT].contains(&vk) && deliver(press_of(vk, down(VK_CONTROL), down(VK_MENU), down(VK_SHIFT))) {
                return 0;
            }
        }
        WM_PAINT => PAINTS.with(|p| p.set(p.get() + 1)),
        WM_PROBE_KEY => {
            let vk = u16::try_from(w).unwrap_or(0);
            deliver(press_of(vk, l & 1 != 0, l & 2 != 0, l & 4 != 0));
            return 0;
        }
        _ => {}
    }
    unsafe { DefSubclassProc(h, msg, w, l) }
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

/// Paint messages the editor has had, its text length, and the WinUI window's extended style.
pub fn paint_facts() -> String {
    let ex = HOST.with(|h| h.borrow().as_ref().map_or(0, |h| unsafe { GetWindowLongPtrW(h.window, GWL_EXSTYLE) }));
    format!("paints={} length={} window exstyle={ex:#x} (NOREDIRECTIONBITMAP={})", PAINTS.with(|p| p.get()), send(SCI_GETLENGTH, 0, 0), ex & 0x0020_0000 != 0)
}

/// Keep the raw pointer type out of the component's code.
pub fn hwnd_value(h: HWND) -> usize {
    h as usize
}
