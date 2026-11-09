//! Keys before the editor or a page sees them: what `core::key_routing` decides, carried out. This file only turns wx key
//! codes into a `Press` and acts on the `Route`.

use notepp_core::key_routing::{self as kr, Key, Place, Press, Route, Situation};
use notepp_core::shortcuts::Chord;

use super::*;

/// wx's key code for F10 (`WXK_F10`).
const KEY_F10: i32 = 344;

/// A wx key event as the rules read it.
fn press_of(k: &wxdragon::event::window_events::KeyboardEvent) -> (Press, i32) {
    let code = k.get_key_code().unwrap_or(0);
    let (ctrl, alt, shift) = (k.control_down(), k.alt_down(), k.shift_down());
    let key = match code {
        KEY_TAB => Key::Tab,
        KEY_F10 => Key::F10,
        c => u8::try_from(c).ok().filter(u8::is_ascii_graphic).map_or(Key::Other, |b| Key::Char(char::from(b))),
    };
    let chord = crate::dialogs::vk_of(code).map(|vk| Chord { ctrl, alt, shift, vk });
    (Press { key, ctrl, alt, shift, chord }, code)
}

/// What the key in hand does in `place`.
fn route_for(a: &Shared, press: &Press, place: Place) -> Route {
    let Ok(app) = a.try_borrow() else { return Route::PassOn };
    let titles = app.chrome.menu_titles();
    let is_menu_letter = |c: char| crate::widgets::menu_bar::menu_for_key(&titles, c).is_some();
    kr::route(press, &Situation { place, menu_open: crate::widgets::popup_menu::is_open(), is_menu_letter: &is_menu_letter, keymap: &app.keymap })
}

/// Handle a key of a window whose keyboard is in `place`; the editor or page gets it when nothing does.
pub(super) fn on_key(a: &Shared, e: &wxdragon::event::WindowEventData, place: Place) {
    let wxdragon::event::WindowEventData::Keyboard(k) = e else {
        e.skip(true);
        return;
    };
    let (press, code) = press_of(k);
    let used = match route_for(a, &press, place) {
        Route::MenuLetter(c) => {
            open_menu_for_key(a, &c.to_string());
            true
        }
        Route::FirstMenu => {
            open_menu(a, 0, true);
            true
        }
        Route::ToOpenMenu => crate::widgets::popup_menu::key(code, !press.ctrl && !press.alt),
        Route::SwitchTab(delta) => {
            switch_tab(a, delta);
            true
        }
        Route::PastePicture => paste_picture_from_editor(a),
        Route::Command(id) => {
            crate::app::run_command(a, id);
            true
        }
        Route::Swallow => true,
        Route::PassOn => false,
    };
    if !used {
        e.skip(true);
    }
}

/// Ctrl+V in the editor: open the picture on the clipboard, if that is all it holds. True when the key was used.
fn paste_picture_from_editor(app: &Shared) -> bool {
    let Ok(mut a) = app.try_borrow_mut() else { return false };
    a.stc().has_focus() && a.paste_image()
}
