//! Search > Style ... Token: the five style slots, jumping between and copying styled text.

use super::*;

// ---- token styles (Search > Style ... Token) -------------------------------

/// Five user colours, named "1st" to "5th".
pub const STYLE_SLOTS: usize = 5;
pub(super) const INDIC_STYLE_BASE: usize = 10;
pub const STYLE_COLOURS: [(u8, u8, u8); STYLE_SLOTS] = [(0xFF, 0x80, 0x00), (0xFF, 0xFF, 0x00), (0x00, 0xE0, 0x40), (0x80, 0x40, 0xFF), (0xFF, 0x40, 0x40)];

pub fn style_indicator(slot: usize) -> usize {
    INDIC_STYLE_BASE + slot.min(STYLE_SLOTS - 1)
}

pub(super) fn is_word_text(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Byte range of the word under the caret.
pub(super) fn word_range_at_caret<S: Send>(s: &S) -> Option<Span> {
    let pos = crate::num::to_usize(s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
    let a = crate::num::to_usize(s.send(sci::SCI_WORDSTARTPOSITION, pos, 1).max(0));
    let b = crate::num::to_usize(s.send(sci::SCI_WORDENDPOSITION, pos, 1).max(0));
    (b > a && b - a <= 200).then_some((a, b))
}

/// Select the word under the caret when nothing is selected.
pub fn select_word_at_caret<S: Send>(s: &S) {
    let (a, b) = selection(s);
    if a == b
        && let Some((x, y)) = word_range_at_caret(s)
    {
        select(s, x, y);
    }
}

/// The selection, or the word under the caret when nothing is selected.
pub fn token_at_caret<S: Send>(s: &S) -> String {
    let sel = selected_text(s);
    if !sel.is_empty() {
        return sel;
    }
    let Some((a, b)) = word_range_at_caret(s) else { return String::new() };
    s.send(sci::SCI_SETTARGETRANGE, a, crate::num::to_isize(b));
    let n = crate::num::to_usize(s.send(sci::SCI_GETTARGETTEXT, 0, 0).max(0));
    let mut buf = vec![0u8; n + 1];
    s.send(sci::SCI_GETTARGETTEXT, 0, buf.as_mut_ptr() as isize);
    buf.truncate(n);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Colour every occurrence of `token` with a style slot. Returns how many.
pub fn style_all<S: Send>(s: &S, slot: usize, token: &str) -> usize {
    let o = Opts { find: token.to_string(), match_case: true, whole_word: is_word_text(token), ..Default::default() };
    fill_marks(s, &o, style_indicator(slot)).unwrap_or(0)
}

/// Colour just the current selection with a style slot.
pub fn style_selection<S: Send>(s: &S, slot: usize) -> bool {
    let (a, b) = selection(s);
    if b <= a {
        return false;
    }
    s.send(sci::SCI_SETINDICATORCURRENT, style_indicator(slot), 0);
    s.send(sci::SCI_INDICATORFILLRANGE, a, crate::num::to_isize(b - a));
    true
}

/// Remove one style slot, or all of them.
pub fn clear_style<S: Send>(s: &S, slot: Option<usize>) {
    match slot {
        Some(i) => clear_marks(s, style_indicator(i)),
        None => (0..STYLE_SLOTS).for_each(|i| clear_marks(s, style_indicator(i))),
    }
}

/// The coloured ranges of one style slot, in order.
pub fn style_ranges<S: Send>(s: &S, slot: usize) -> Vec<Span> {
    let ind = style_indicator(slot);
    let len = crate::num::to_usize(sci::length(s).max(0));
    let (mut out, mut p) = (Vec::new(), 0usize);
    while p < len {
        let end = crate::num::to_usize(s.send(sci::SCI_INDICATOREND, ind, crate::num::to_isize(p)).max(0));
        if end <= p {
            break;
        }
        if s.send(sci::SCI_INDICATORVALUEAT, ind, crate::num::to_isize(p)) != 0 {
            out.push((p, end));
        }
        p = end;
    }
    out
}

/// The range after (or, backwards, before) `from`, wrapping around.
pub fn next_range(ranges: &[Span], from: usize, forward: bool) -> Option<Span> {
    if forward { ranges.iter().find(|r| r.0 >= from).or(ranges.first()).copied() } else { ranges.iter().rev().find(|r| r.0 < from).or(ranges.last()).copied() }
}

/// Select the next coloured range of a slot. Returns false if there is none.
pub fn jump<S: Send>(s: &S, slot: usize, forward: bool) -> bool {
    let ranges = style_ranges(s, slot);
    let (a, b) = selection(s);
    match next_range(&ranges, if forward { b } else { a }, forward) {
        Some((x, y)) => {
            select(s, x, y);
            true
        }
        None => false,
    }
}

/// The text under the given style slots, one piece per line, in document order.
pub fn styled_text<S: Send>(s: &S, slots: &[usize]) -> String {
    let mut ranges: Vec<Span> = slots.iter().flat_map(|i| style_ranges(s, *i)).collect();
    ranges.sort_unstable();
    ranges.dedup();
    let text = sci::get_text(s);
    ranges.iter().filter_map(|(a, b)| text.get(*a..*b)).collect::<Vec<_>>().join("\r\n")
}
