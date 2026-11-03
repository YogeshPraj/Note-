//! The keyboard in an open menu: arrows, Enter, Esc, Home/End and letters.

use super::*;

pub(super) const KEY_RETURN: i32 = 13;
pub(super) const KEY_ESCAPE: i32 = 27;
pub(super) const KEY_SPACE: i32 = 32;
pub(super) const KEY_NUMPAD_ENTER: i32 = 370;
pub(super) const KEY_END: i32 = 312;
pub(super) const KEY_HOME: i32 = 313;
pub(super) const KEY_LEFT: i32 = 314;
pub(super) const KEY_UP: i32 = 315;
pub(super) const KEY_RIGHT: i32 = 316;
pub(super) const KEY_DOWN: i32 = 317;

pub(super) fn set_hot(s: &Shared, i: usize, row: Option<usize>) {
    if let Some(l) = s.levels.borrow_mut().get_mut(i) {
        l.hot = row;
    }
    refresh(s, i);
}

/// Keyboard: the deepest open level is the one being driven.
pub(super) fn on_key(s: &Shared, code: i32, plain: bool) {
    let i = s.levels.borrow().len().saturating_sub(1);
    let (row, entries) = {
        let lv = s.levels.borrow();
        let Some(l) = lv.get(i) else { return };
        (l.hot, l.entries.clone())
    };
    let is_sub = row.is_some_and(|r| matches!(entries[r], Entry::Sub { .. }));
    match code {
        KEY_ESCAPE => defer_close(s),
        KEY_UP => set_hot(s, i, step(&entries, row, -1)),
        KEY_DOWN => set_hot(s, i, step(&entries, row, 1)),
        KEY_HOME => set_hot(s, i, first(&entries)),
        KEY_END => set_hot(s, i, last(&entries)),
        KEY_RETURN | KEY_NUMPAD_ENTER | KEY_SPACE => {
            if let Some(r) = row {
                choose(s, i, r);
            }
        }
        KEY_RIGHT if is_sub => sync_sub(s, i, true),
        KEY_RIGHT => navigate(s, i, 1),
        KEY_LEFT if i > 0 => close_from(s, i),
        KEY_LEFT => navigate(s, i, -1),
        65..=90 | 48..=57 if plain => on_letter(s, i, &entries, row, char::from(u8::try_from(code).unwrap_or(b' '))),
        _ => {}
    }
}

/// A letter picks the row it underlines; when several share it, repeated presses move between them.
pub(super) fn on_letter(s: &Shared, i: usize, entries: &[Entry], row: Option<usize>, key: char) {
    let Some(r) = by_mnemonic(entries, key, row) else { return };
    set_hot(s, i, Some(r));
    let unique = by_mnemonic(entries, key, Some(r)).is_none_or(|n| n == r);
    if unique {
        choose(s, i, r);
    }
}

pub(super) fn defer_close(s: &Shared) {
    let s = s.clone();
    later(&s.owner.clone(), move || close_from(&s, 0));
}

pub(super) fn navigate(s: &Shared, i: usize, dir: i32) {
    if i > 0 {
        return;
    }
    let s = s.clone();
    later(&s.owner.clone(), move || {
        close_from(&s, 0);
        (s.handlers.nav)(dir);
    });
}
