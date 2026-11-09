//! Every key of this thread, wherever the keyboard is (the editor, the tab strip, an open menu), before WinUI or
//! Scintilla sees it: a message hook on this thread only, like wx's char hook. `core::key_routing` decides; a key is
//! taken only when Note++ acts on it, and everything else reaches WinUI or the editor untouched.
#![allow(unsafe_code)]

use std::cell::{Cell, RefCell};

use notepp_core::key_routing::{self as kr, Key, Place, Press, Route, Situation};
use notepp_core::keymap::KeyMap;
use notepp_core::shortcuts::Chord;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_F10, VK_MENU, VK_SHIFT, VK_TAB};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HHOOK, MSG, PM_REMOVE, SetWindowsHookExW, WH_GETMESSAGE, WM_APP, WM_KEYDOWN, WM_NULL, WM_SYSKEYDOWN,
};

/// A key as `core` routed it, and whether the keyboard was in the editor.
#[derive(Clone, Debug, PartialEq)]
pub struct Routed {
    pub press: String,
    pub route: Route,
    pub in_editor: bool,
}

/// Probe message, posted to the editor: `wparam` = virtual key, `lparam` bits 1 ctrl, 2 alt, 4 shift, 8 "as if the
/// keyboard were in WinUI". Lets a test drive the routing without a keyboard (posted keys carry no modifier state).
pub const WM_PROBE_KEY: u32 = WM_APP + 1;

/// Probe message, posted to the editor: open Scintilla's autocomplete list (`wparam` 1) or a calltip (0).
pub const WM_PROBE_POPUPS: u32 = WM_APP + 2;

type Sink = Box<dyn Fn(Routed)>;

thread_local! {
    static HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
    static ROUTING: RefCell<Option<(KeyMap, Vec<String>)>> = const { RefCell::new(None) };
    static EDITOR: Cell<HWND> = const { Cell::new(std::ptr::null_mut()) };
}

/// Start routing this thread's keys: the key map, the menu titles (with `&` marks), and where routed keys go.
pub fn install(keymap: KeyMap, titles: Vec<String>, sink: impl Fn(Routed) + 'static) -> bool {
    ROUTING.with(|r| *r.borrow_mut() = Some((keymap, titles)));
    SINK.with(|s| *s.borrow_mut() = Some(Box::new(sink)));
    let hook = unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(hook), std::ptr::null_mut(), GetCurrentThreadId()) };
    HOOK.with(|h| h.set(hook));
    !hook.is_null()
}

/// The editor's window, so a key can tell whether it is the editor's.
pub fn set_editor(editor: HWND) {
    EDITOR.with(|e| e.set(editor));
}

/// The menu title whose `&` marks `c`, without the mark (what UI Automation calls it).
pub fn title_for(c: char) -> Option<String> {
    ROUTING.with(|r| r.borrow().as_ref().and_then(|(_, titles)| titles.iter().find(|t| marks(t, c)).map(|t| t.replace('&', ""))))
}

/// The first menu's title, without its mark.
pub fn first_title() -> Option<String> {
    ROUTING.with(|r| r.borrow().as_ref().and_then(|(_, titles)| titles.first().map(|t| t.replace('&', ""))))
}

fn marks(title: &str, c: char) -> bool {
    title.char_indices().any(|(i, ch)| ch == '&' && title[i + 1..].chars().next().is_some_and(|n| n.eq_ignore_ascii_case(&c)))
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

/// The main window's rules first (menu letters, F10, Ctrl+Tab); then the key map, because WinUI never turns a key into
/// a Note++ command by itself.
fn route(p: &Press) -> Route {
    ROUTING.with(|r| {
        let r = r.borrow();
        let Some((keymap, titles)) = r.as_ref() else { return Route::PassOn };
        let is_letter = |c: char| titles.iter().any(|t| marks(t, c));
        let main = kr::route(p, &Situation { place: Place::Main, menu_open: false, is_menu_letter: &is_letter, keymap });
        if main != Route::PassOn {
            return main;
        }
        kr::route(p, &Situation { place: Place::Floating, menu_open: false, is_menu_letter: &is_letter, keymap })
    })
}

/// Route a key; true when Note++ takes it. Ctrl+Tab outside the editor is left to WinUI's tab strip, which does it
/// itself; Ctrl+V is left to whoever has the keyboard (the spike has no picture paste).
fn take(p: Press, in_editor: bool) -> bool {
    let route = route(&p);
    let taken = match route {
        Route::MenuLetter(_) | Route::FirstMenu | Route::Command(_) | Route::Swallow => true,
        Route::SwitchTab(_) => in_editor,
        Route::ToOpenMenu | Route::PastePicture | Route::PassOn => false,
    };
    if route != Route::PassOn {
        let press = format!("{}{}{}{:?}", if p.ctrl { "Ctrl+" } else { "" }, if p.alt { "Alt+" } else { "" }, if p.shift { "Shift+" } else { "" }, p.key);
        SINK.with(|s| {
            if let Some(sink) = s.borrow().as_ref() {
                sink(Routed { press, route, in_editor });
            }
        });
    }
    taken
}

fn on_message(m: &MSG) -> bool {
    let vk = u16::try_from(m.wParam).unwrap_or(0);
    let in_editor = EDITOR.with(Cell::get) == m.hwnd;
    match m.message {
        WM_KEYDOWN | WM_SYSKEYDOWN if ![VK_CONTROL, VK_MENU, VK_SHIFT].contains(&vk) => {
            let down = |k: u16| unsafe { GetKeyState(i32::from(k)) } < 0;
            take(press_of(vk, down(VK_CONTROL), down(VK_MENU), down(VK_SHIFT)), in_editor)
        }
        WM_PROBE_KEY => {
            let l = m.lParam;
            take(press_of(vk, l & 1 != 0, l & 2 != 0, l & 4 != 0), l & 8 == 0);
            true
        }
        WM_PROBE_POPUPS => {
            crate::host::show_popups(m.wParam == 1);
            true
        }
        _ => false,
    }
}

unsafe extern "system" fn hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 && w == usize::try_from(PM_REMOVE).unwrap_or(1) {
        let m = unsafe { &mut *(l as *mut MSG) };
        if on_message(m) {
            m.message = WM_NULL;
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, w, l) }
}
