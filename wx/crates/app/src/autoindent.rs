//! Typing aids in every editor. They act on the key before the editor sees it, so each is one undo step:
//!
//! * Enter keeps the indentation of the line above (its own tabs or spaces, so nobody has to look at the style), adds
//!   one level after an opening bracket and, between a pair of brackets, puts the closing one on its own line.
//! * An opening bracket or double quote also types its closing partner; typing that partner steps over it. With text
//!   selected, the pair goes around the selection instead.
//! * A closing bracket typed on a line that holds nothing else moves back one level.
//!
//! A closing character typed for the user is marked with a hidden indicator, which stays with the text through every
//! edit and belongs to the document, so tabs, undo and other editing never confuse it. With several carets the keys
//! are left to the editor.

use notepp_core::sci::{self, Send};
use wxdragon::event::WindowEventData;
use wxdragon::prelude::*;

use crate::platform::scintilla::WxSci;

/// The indicator that marks closing characters typed for the user.
const MARK: usize = 25;
const INDIC_HIDDEN: usize = 5;
const SCI_AUTOCACTIVE: u32 = 2102;
const SCI_SCROLLCARET: u32 = 2169;

fn closer_of(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        _ => None,
    }
}

fn is_closer(c: i32) -> bool {
    matches!(u8::try_from(c), Ok(b')' | b']' | b'}'))
}

fn is_blank(c: i32) -> bool {
    matches!(c, 0 | 9 | 10 | 13 | 32)
}

fn is_word(c: i32) -> bool {
    u8::try_from(c).is_ok_and(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn len(s: &str) -> i32 {
    i32::try_from(s.len()).unwrap_or(0)
}

/// The leading tabs and spaces of `text`.
fn lead_of(text: &str) -> String {
    text.chars().take_while(|c| matches!(c, ' ' | '\t')).collect()
}

/// One indentation level in this document: what its first indented line uses, else the editor's setting.
fn unit_of(stc: &StyledTextCtrl) -> String {
    for l in 0..stc.get_line_count().min(300) {
        let text = stc.get_line_text(l);
        let lead = lead_of(&text);
        if !lead.is_empty() && !text.trim().is_empty() {
            return if lead.starts_with('\t') { "\t".into() } else { " ".repeat(lead.len().min(8)) };
        }
    }
    if stc.get_use_tabs() { "\t".into() } else { " ".repeat(usize::try_from(stc.get_tab_width().max(1)).unwrap_or(4)) }
}

/// Several edits that undo as one.
fn grouped(s: &WxSci, edit: impl FnOnce()) {
    s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
    edit();
    s.send(sci::SCI_ENDUNDOACTION, 0, 0);
}

fn marked(s: &WxSci, pos: i32) -> bool {
    s.send(sci::SCI_SETINDICATORCURRENT, MARK, 0);
    s.send(sci::SCI_INDICATORVALUEAT, MARK, pos as isize) != 0
}

fn mark(s: &WxSci, pos: i32) {
    s.send(sci::SCI_SETINDICATORCURRENT, MARK, 0);
    s.send(sci::SCI_INDICATORFILLRANGE, usize::try_from(pos).unwrap_or(0), 1);
}

fn unmark(s: &WxSci, pos: i32) {
    s.send(sci::SCI_SETINDICATORCURRENT, MARK, 0);
    s.send(sci::SCI_INDICATORCLEARRANGE, usize::try_from(pos).unwrap_or(0), 1);
}

/// The typed character, when it is plain text the aids may act on.
fn typed_char(k: &wxdragon::event::window_events::KeyboardEvent) -> Option<char> {
    // AltGr (how many keyboards reach `{` and `[`) reports Ctrl and Alt together.
    if k.control_down() != k.alt_down() {
        return None;
    }
    let code = k.get_unicode_key().filter(|c| *c > 0).or_else(|| k.get_key_code())?;
    char::from_u32(u32::try_from(code).ok()?)
}

fn on_char(stc: &StyledTextCtrl, e: &WindowEventData) {
    let WindowEventData::Keyboard(k) = e else {
        e.skip(true);
        return;
    };
    let s = WxSci::of(stc);
    let plain = s.send(sci::SCI_GETSELECTIONS, 0, 0) == 1 && s.send(sci::SCI_GETREADONLY, 0, 0) == 0 && s.send(SCI_AUTOCACTIVE, 0, 0) == 0;
    let used = plain
        && match typed_char(k) {
            Some('\r' | '\n') => enter_key(stc, &s),
            Some(c) => typed(stc, &s, c),
            None => false,
        };
    e.skip(!used);
}

/// Enter pressed on its own: indent the new line, or leave it to the editor.
fn on_key_down(stc: &StyledTextCtrl, e: &WindowEventData) {
    const KEY_ENTER: i32 = 13;
    const KEY_NUMPAD_ENTER: i32 = 370;
    let WindowEventData::Keyboard(k) = e else {
        e.skip(true);
        return;
    };
    let enter = matches!(k.get_key_code(), Some(KEY_ENTER | KEY_NUMPAD_ENTER));
    let s = WxSci::of(stc);
    let plain = !k.control_down()
        && !k.alt_down()
        && !k.shift_down()
        && s.send(sci::SCI_GETSELECTIONS, 0, 0) == 1
        && s.send(sci::SCI_GETREADONLY, 0, 0) == 0
        && s.send(SCI_AUTOCACTIVE, 0, 0) == 0;
    e.skip(!(enter && plain && enter_key(stc, &s)));
}

/// An opening or closing character was typed: true when the aids dealt with it.
fn typed(stc: &StyledTextCtrl, s: &WxSci, ch: char) -> bool {
    let (a, b) = (stc.get_selection_start(), stc.get_selection_end());
    let code = ch as i32;
    if a != b {
        // The pair around the selection, which stays selected.
        let Some(close) = closer_of(ch) else { return false };
        let text = stc.get_selected_text();
        grouped(s, || {
            stc.replace_selection(&format!("{ch}{text}{close}"));
            stc.set_selection(a + len(&ch.to_string()), a + len(&ch.to_string()) + len(&text));
        });
        return true;
    }
    let pos = a;
    let next = stc.get_char_at(pos);
    // Stepping over a closing character typed for the user.
    if (is_closer(code) || ch == '"') && next == code && marked(s, pos) {
        unmark(s, pos);
        stc.goto_pos(pos + 1);
        return true;
    }
    if is_closer(code) {
        return dedent(stc, s, pos, ch);
    }
    let Some(close) = closer_of(ch) else { return false };
    let before = if pos >= 1 { stc.get_char_at(pos - 1) } else { 0 };
    let quote = ch == '"';
    if !(is_blank(next) || is_closer(next)) || (quote && (is_word(before) || before == code)) {
        return false;
    }
    grouped(s, || {
        stc.insert_text(pos, &format!("{ch}{close}"));
        mark(s, pos + 1);
        stc.goto_pos(pos + 1);
    });
    true
}

/// Enter: a new line indented like this one. True when it was done here.
fn enter_key(stc: &StyledTextCtrl, s: &WxSci) -> bool {
    let (a, b) = (stc.get_selection_start(), stc.get_selection_end());
    let line = stc.line_from_position(a);
    let start = stc.position_from_line(line);
    let text = stc.get_line_text(line);
    let before = text.get(..usize::try_from(a - start).unwrap_or(0)).unwrap_or("");
    let lead = lead_of(before);
    let opens = before.trim_end().ends_with(['{', '[', '(']);
    let split = opens && is_closer(stc.get_char_at(b));
    if lead.is_empty() && !opens {
        return false;
    }
    let eol = match s.send(sci::SCI_GETEOLMODE, 0, 0) {
        0 => "\r\n",
        1 => "\r",
        _ => "\n",
    };
    let here = if opens { format!("{lead}{}", unit_of(stc)) } else { lead.clone() };
    let first = format!("{eol}{here}");
    grouped(s, || {
        stc.replace_selection(&if split { format!("{first}{eol}{lead}") } else { first.clone() });
        stc.goto_pos(a + len(&first));
    });
    s.send(SCI_SCROLLCARET, 0, 0);
    true
}

/// A closing bracket typed at the end of a line's indentation: back one level, then the bracket.
fn dedent(stc: &StyledTextCtrl, s: &WxSci, pos: i32, ch: char) -> bool {
    let start = stc.position_from_line(stc.line_from_position(pos));
    let width = pos - start;
    if width <= 0 || (start..pos).any(|p| !matches!(stc.get_char_at(p), 9 | 32)) {
        return false;
    }
    let cut = if stc.get_char_at(pos - 1) == 9 {
        1
    } else {
        let unit = unit_of(stc);
        let step = if unit == "\t" { stc.get_tab_width().max(1) } else { len(&unit).max(1) };
        let extra = width % step;
        (if extra == 0 { step } else { extra }).min(width)
    };
    grouped(s, || {
        stc.delete_range(pos - cut, cut);
        stc.insert_text(pos - cut, &ch.to_string());
        stc.goto_pos(pos - cut + 1);
    });
    true
}

/// Wire the aids to `stc`.
pub(crate) fn wire(stc: &StyledTextCtrl) {
    WxSci::of(stc).send(sci::SCI_INDICSETSTYLE, MARK, INDIC_HIDDEN as isize);
    let s = *stc;
    stc.on_char(move |e| on_char(&s, &e));
    // The editor turns Enter into a command on the key-down, so no character event follows: take it here.
    let s = *stc;
    stc.on_key_down(move |e| on_key_down(&s, &e));
}
