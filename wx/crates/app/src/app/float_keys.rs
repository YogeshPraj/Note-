//! Keys in a floating window. It has no menu bar, so nothing turns a key into a command by itself.

use notepp_core::shortcuts::Chord;

use super::*;

/// The menu shortcuts belong to the main window, so a key meant for one is looked up here. Everything else goes on to
/// the editor, which also keeps keys such as Del and Tab its own.
pub(super) fn wire(a: &Shared, frame: Frame) {
    let a = a.clone();
    frame.on_char_hook(move |e| {
        let wxdragon::event::WindowEventData::Keyboard(k) = &e else {
            e.skip(true);
            return;
        };
        let code = k.get_key_code().unwrap_or(0);
        let (ctrl, alt, shift) = (k.control_down(), k.alt_down(), k.shift_down());
        if crate::widgets::popup_menu::key(code, !ctrl && !alt) {
            return;
        }
        // The next tab is in the other window: Ctrl+Tab has nothing to go to, and must not insert a tab.
        if code == KEY_TAB && ctrl {
            return;
        }
        let chord = crate::dialogs::vk_of(code).map(|vk| Chord { ctrl, alt, shift, vk });
        let id = chord.and_then(|c| a.try_borrow().ok()?.keymap.command_for(&c));
        match id {
            Some(id) => crate::app::run_command(&a, id),
            None => e.skip(true),
        }
    });
}
