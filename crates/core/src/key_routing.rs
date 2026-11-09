//! What a key does, wherever the keyboard is: before the editor or a page sees it, the window may want it (a menu letter, Ctrl+Tab,
//! a shortcut in a window with no menu bar). Plain rules on plain facts, so any shell can follow them (see
//! `docs/winui3-plan.md`, phase 1); the shell turns its key codes into a [`Press`] and carries out the [`Route`].

use crate::keymap::KeyMap;
use crate::shortcuts::Chord;

/// The keys these rules know by name; anything else is `Other`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Tab,
    F10,
    /// A printable character, as the key reports it.
    Char(char),
    Other,
}

/// A key press.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// The press as a shortcut, when it can be one.
    pub chord: Option<Chord>,
}

/// Where the keyboard is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// The main window, which has the menu bar and the tabs.
    Main,
    /// A floating window: no menu bar, and the other tabs are elsewhere.
    Floating,
}

/// What to do with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Open the menu whose title marks this letter.
    MenuLetter(char),
    /// Open the first menu (F10).
    FirstMenu,
    /// An open menu takes it.
    ToOpenMenu,
    /// Go to the next (1) or previous (-1) tab.
    SwitchTab(i32),
    /// Open the picture on the clipboard in a tab, if that is all the clipboard holds; otherwise the key goes on.
    PastePicture,
    /// Run this command.
    Command(usize),
    /// Spend the key: it has nothing to do here, and must not reach the editor either.
    Swallow,
    /// Let the editor or the page have it.
    PassOn,
}

/// What the rules need to know besides the key.
pub struct Situation<'a> {
    pub place: Place,
    /// A menu is open: it takes the keys.
    pub menu_open: bool,
    /// Whether a menu title marks this letter.
    pub is_menu_letter: &'a dyn Fn(char) -> bool,
    pub keymap: &'a KeyMap,
}

impl std::fmt::Debug for Situation<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Situation").field("place", &self.place).field("menu_open", &self.menu_open).finish_non_exhaustive()
    }
}

/// What `p` does in `s`.
pub fn route(p: &Press, s: &Situation<'_>) -> Route {
    let plain_alt = p.alt && !p.ctrl;
    // Alt+letter opens its menu, also while another menu is open (that one is replaced). Only the main window has menus.
    if s.place == Place::Main
        && plain_alt
        && let Key::Char(c) = p.key
        && c.is_ascii_graphic()
        && (s.is_menu_letter)(c)
    {
        return Route::MenuLetter(c);
    }
    if s.menu_open {
        return Route::ToOpenMenu;
    }
    let ctrl_only = p.ctrl && !p.alt;
    if p.key == Key::Tab && ctrl_only {
        // Tab is a navigation key the toolkit and the editor take before any shortcut sees it. In a floating window the other
        // tabs are in the other window, and the key must not insert a tab either.
        return match s.place {
            Place::Main => Route::SwitchTab(if p.shift { -1 } else { 1 }),
            Place::Floating => Route::Swallow,
        };
    }
    match s.place {
        Place::Main if p.key == Key::Char('V') && ctrl_only && !p.shift => Route::PastePicture,
        Place::Main if p.key == Key::F10 && !p.alt && !p.ctrl => Route::FirstMenu,
        // The main window's menu bar turns its own shortcuts into commands; a floating window has none.
        Place::Main => Route::PassOn,
        Place::Floating => p.chord.and_then(|c| s.keymap.command_for(&c)).map_or(Route::PassOn, Route::Command),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn press(key: Key, ctrl: bool, alt: bool, shift: bool) -> Press {
        Press { key, ctrl, alt, shift, chord: None }
    }

    fn route_in(p: Press, place: Place, menu_open: bool) -> Route {
        let keymap = KeyMap::build(&BTreeMap::new());
        let letters = |c: char| matches!(c.to_ascii_uppercase(), 'F' | 'E');
        route(&p, &Situation { place, menu_open, is_menu_letter: &letters, keymap: &keymap })
    }

    #[test]
    fn alt_and_a_menu_letter_open_that_menu_in_the_main_window_only() {
        assert_eq!(route_in(press(Key::Char('F'), false, true, false), Place::Main, false), Route::MenuLetter('F'));
        assert_eq!(route_in(press(Key::Char('F'), false, true, false), Place::Main, true), Route::MenuLetter('F'), "replaces an open menu");
        assert_eq!(route_in(press(Key::Char('F'), false, true, false), Place::Floating, false), Route::PassOn);
        assert_eq!(route_in(press(Key::Char('F'), true, true, false), Place::Main, false), Route::PassOn, "Ctrl+Alt is not a menu key");
    }

    #[test]
    fn a_letter_no_menu_has_goes_to_an_open_menu_or_on() {
        assert_eq!(route_in(press(Key::Char('Q'), false, true, false), Place::Main, true), Route::ToOpenMenu);
        assert_eq!(route_in(press(Key::Char('Q'), false, true, false), Place::Main, false), Route::PassOn);
    }

    #[test]
    fn an_open_menu_takes_every_other_key() {
        assert_eq!(route_in(press(Key::Other, false, false, false), Place::Main, true), Route::ToOpenMenu);
        assert_eq!(route_in(press(Key::Tab, true, false, false), Place::Main, true), Route::ToOpenMenu);
    }

    #[test]
    fn ctrl_tab_switches_tabs_in_the_main_window_and_is_spent_in_a_floating_one() {
        assert_eq!(route_in(press(Key::Tab, true, false, false), Place::Main, false), Route::SwitchTab(1));
        assert_eq!(route_in(press(Key::Tab, true, false, true), Place::Main, false), Route::SwitchTab(-1));
        assert_eq!(route_in(press(Key::Tab, true, false, false), Place::Floating, false), Route::Swallow);
        assert_eq!(route_in(press(Key::Tab, false, false, false), Place::Main, false), Route::PassOn, "plain Tab is the editor's");
        assert_eq!(route_in(press(Key::Tab, true, true, false), Place::Main, false), Route::PassOn);
    }

    #[test]
    fn ctrl_v_tries_a_picture_and_f10_opens_the_first_menu() {
        assert_eq!(route_in(press(Key::Char('V'), true, false, false), Place::Main, false), Route::PastePicture);
        assert_eq!(route_in(press(Key::Char('V'), true, false, true), Place::Main, false), Route::PassOn);
        assert_eq!(route_in(press(Key::F10, false, false, false), Place::Main, false), Route::FirstMenu);
        assert_eq!(route_in(press(Key::F10, false, false, false), Place::Floating, false), Route::PassOn);
    }

    #[test]
    fn a_floating_window_turns_shortcuts_into_commands_and_leaves_the_rest() {
        let save = Press { chord: crate::shortcuts::parse("Ctrl+S"), ..press(Key::Char('S'), true, false, false) };
        assert_eq!(route_in(save, Place::Floating, false), Route::Command(crate::commands::ID_SAVE));
        assert_eq!(route_in(save, Place::Main, false), Route::PassOn, "the menu bar runs it");
        let typing = Press { chord: crate::shortcuts::parse("A"), ..press(Key::Char('A'), false, false, false) };
        assert_eq!(route_in(typing, Place::Floating, false), Route::PassOn);
    }

    #[test]
    fn the_users_own_shortcut_is_the_one_a_floating_window_runs() {
        let mut overrides = BTreeMap::new();
        overrides.insert(crate::commands::ID_SAVE.to_string(), "Ctrl+Alt+K".to_string());
        let keymap = KeyMap::build(&overrides);
        let p = Press { chord: crate::shortcuts::parse("Ctrl+Alt+K"), ..press(Key::Char('K'), true, true, false) };
        let none = |_: char| false;
        assert_eq!(
            route(&p, &Situation { place: Place::Floating, menu_open: false, is_menu_letter: &none, keymap: &keymap }),
            Route::Command(crate::commands::ID_SAVE)
        );
    }
}
