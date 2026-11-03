//! Braces, characters in a range, the selected text, smart highlight and Go To.

use super::*;

// ---- braces ------------------------------------------------------------------

/// The bracket next to the caret: the one before it, else the one after.
/// Only `()[]{}` count (Scintilla would also pair `<` and `>`).
pub(super) fn brace_at_caret<S: Send>(s: &S) -> Option<usize> {
    let pos = crate::num::to_usize(s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
    let is_brace = |p: usize| matches!(crate::num::to_u8(s.send(sci::SCI_GETCHARAT, p, 0)), b'(' | b')' | b'[' | b']' | b'{' | b'}');
    [pos.checked_sub(1), Some(pos)].into_iter().flatten().find(|&p| p < crate::num::to_usize(sci::length(s).max(0)) && is_brace(p))
}

/// The brace at the caret and its partner.
pub fn matching_brace<S: Send>(s: &S) -> Option<Span> {
    let p = brace_at_caret(s)?;
    let m = s.send(sci::SCI_BRACEMATCH, p, 0);
    (m >= 0).then_some((p, crate::num::to_usize(m)))
}

/// Mark the pair next to the caret (or the lone bracket that has no partner).
pub fn highlight_braces<S: Send>(s: &S) {
    match brace_at_caret(s) {
        Some(p) => {
            let m = s.send(sci::SCI_BRACEMATCH, p, 0);
            if m >= 0 {
                s.send(sci::SCI_BRACEHIGHLIGHT, p, m);
            } else {
                s.send(sci::SCI_BRACEBADLIGHT, p, 0);
            }
        }
        None => {
            s.send(sci::SCI_BRACEHIGHLIGHT, usize::MAX, -1);
        }
    }
}

pub fn goto_matching_brace<S: Send>(s: &S) -> bool {
    match matching_brace(s) {
        Some((_, m)) => {
            s.send(sci::SCI_GOTOPOS, m, 0);
            s.send(sci::SCI_SCROLLCARET, 0, 0);
            true
        }
        None => false,
    }
}

/// Select what lies between the brace and its partner (braces excluded).
pub fn select_between_braces<S: Send>(s: &S) -> bool {
    match matching_brace(s) {
        Some((p, m)) => {
            s.send(sci::SCI_SETSEL, p.min(m) + 1, crate::num::to_isize(p.max(m)));
            true
        }
        None => false,
    }
}

// ---- find characters in a range ---------------------------------------------

/// The next character whose code point is within `lo..=hi`, from byte offset
/// `from` (exclusive of text before it when going forward, of text after it
/// when going backward), optionally wrapping around.
pub fn find_char_in_range(text: &str, lo: u32, hi: u32, from: usize, forward: bool, wrap: bool) -> Option<Span> {
    let hit = |(i, c): &(usize, char)| (lo..=hi).contains(&(*c as u32)).then_some((*i, *i + c.len_utf8()));
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let split = chars.partition_point(|(i, _)| *i < from);
    if forward {
        let first = chars[split..].iter().find_map(hit);
        first.or_else(|| if wrap { chars[..split].iter().find_map(hit) } else { None })
    } else {
        let first = chars[..split].iter().rev().find_map(hit);
        first.or_else(|| if wrap { chars[split..].iter().rev().find_map(hit) } else { None })
    }
}

/// Search the editor text for a character in range and select it.
pub fn find_char<S: Send>(s: &S, lo: u32, hi: u32, forward: bool, wrap: bool) -> bool {
    let (a, b) = selection(s);
    let text = sci::get_text(s);
    match find_char_in_range(&text, lo, hi, if forward { b } else { a }, forward, wrap) {
        Some((x, y)) => {
            select(s, x, y);
            true
        }
        None => false,
    }
}

/// The selected text, but only for a single ordinary selection. Column
/// (rectangular) and multiple selections return an empty string.
///
/// Reads through the search target, whose length is exact. `SCI_GETSELTEXT`
/// must not be used here: for column selections it writes more than it reports
/// when sized with a null pointer, overflowing the buffer (heap corruption).
pub fn selected_text<S: Send>(s: &S) -> String {
    if s.send(sci::SCI_GETSELECTIONS, 0, 0) != 1 || s.send(sci::SCI_SELECTIONISRECTANGLE, 0, 0) != 0 {
        return String::new();
    }
    let (a, b) = selection(s);
    if b <= a || b - a > 1_000_000 {
        return String::new();
    }
    s.send(sci::SCI_SETTARGETRANGE, a, crate::num::to_isize(b));
    let n = crate::num::to_usize(s.send(sci::SCI_GETTARGETTEXT, 0, 0).max(0));
    if n == 0 {
        return String::new();
    }
    let mut buf = vec![0u8; n + 1];
    s.send(sci::SCI_GETTARGETTEXT, 0, buf.as_mut_ptr() as isize);
    buf.truncate(n);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Highlight every occurrence of the single word currently selected, for
/// "smart highlighting". Large documents are skipped.
pub fn smart_highlight<S: Send>(s: &S) {
    clear_marks(s, INDIC_SMART);
    if sci::length(s) > 2_000_000 {
        return;
    }
    let word = selected_text(s);
    let is_word = !word.is_empty() && word.len() <= 100 && word.chars().all(|c| c.is_alphanumeric() || c == '_');
    if is_word {
        // Default: whole words, any case.
        let o = Opts { find: word, match_case: false, whole_word: true, ..Default::default() };
        let _ = mark_all(s, &o, INDIC_SMART);
    }
}

/// Move to 1-based `line`, clamped to the document. Returns the line used.
pub fn goto_line<S: Send>(s: &S, line: usize) -> usize {
    let total = crate::num::to_usize(sci::line_count(s).max(1));
    let l = line.clamp(1, total);
    s.send(sci::SCI_ENSUREVISIBLE, l - 1, 0);
    s.send(sci::SCI_GOTOLINE, l - 1, 0);
    s.send(sci::SCI_SCROLLCARET, 0, 0);
    l
}
