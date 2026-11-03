//! Search: find / replace / count / mark.
//!
//! Matching is done in Rust (fancy-regex: PCRE-like, with backreferences and
//! lookaround, much closer to Boost.Regex than Scintilla's built-in
//! engine, whose `\w`/`\s`/`\b` proved unreliable). Scintilla positions are
//! UTF-8 byte offsets, which is exactly what `&str` matching yields, so match
//! ranges map straight onto document positions.
//!
//! The pure functions at the top are unit-tested; the Scintilla-facing ones
//! below only fetch the text and apply the result.

use super::sci::{self, Send};
use crate::error::Error;
use fancy_regex::{Captures, Regex, RegexBuilder};
use std::path::{Path, PathBuf};

mod in_files;
pub use in_files::*;

pub const INDIC_FIND: usize = 8;
pub const INDIC_SMART: usize = 9;

#[derive(Clone, Default, Debug, PartialEq)]
pub struct Opts {
    pub find: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub wrap: bool,
    /// Regex only: `.` also matches line breaks.
    pub dot_all: bool,
    /// Extended search mode: `\n \r \t \0 \xHH \dNNN \oNNN \bBBBBBBBB \uHHHH` in the text stand for characters.
    pub extended: bool,
    /// Count, Find All, Replace All and Mark All only look inside the current selection.
    pub in_selection: bool,
}

/// One search result: where it is and the line it is on.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub path: Option<PathBuf>,
    /// 1-based line number.
    pub line: usize,
    /// Byte offsets of the match within the (decoded, UTF-8) text.
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub enum Found {
    At { wrapped: bool },
    NotFound,
    BadPattern(String),
}

type Span = (usize, usize);

// ---- pure engine ----------------------------------------------------------

// Up to max digits of `radix`, as a character.
fn number(it: &mut std::iter::Peekable<std::str::Chars<'_>>, radix: u32, max: usize) -> Option<char> {
    let mut n = 0u32;
    let mut got = 0;
    while got < max {
        match it.peek().and_then(|c| c.to_digit(radix)) {
            Some(d) => {
                n = n * radix + d;
                it.next();
                got += 1;
            }
            None => break,
        }
    }
    if got == 0 { None } else { char::from_u32(n) }
}

/// Expand the escapes of the Extended search mode.
pub fn unescape_ext(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some('n') => {
                it.next();
                out.push('\n');
            }
            Some('r') => {
                it.next();
                out.push('\r');
            }
            Some('t') => {
                it.next();
                out.push('\t');
            }
            Some('0') => {
                it.next();
                out.push('\0');
            }
            Some('\\') => {
                it.next();
                out.push('\\');
            }
            Some(k @ ('x' | 'd' | 'o' | 'b' | 'u')) => {
                it.next();
                let (radix, max) = match k {
                    'x' => (16, 2),
                    'd' => (10, 3),
                    'o' => (8, 3),
                    'b' => (2, 8),
                    _ => (16, 4),
                };
                if let Some(ch) = number(&mut it, radix, max) {
                    out.push(ch);
                } else {
                    out.push('\\');
                    out.push(k);
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}

pub fn compile(o: &Opts) -> Result<Regex, Error> {
    let literal = if o.extended { unescape_ext(&o.find) } else { o.find.clone() };
    let body = if o.regex { o.find.clone() } else { fancy_regex::escape(&literal).into_owned() };
    // Whole word: no word character may touch the match. An edge that is itself punctuation
    // (such as "@name" or "f()") has nothing to separate from, as in Scintilla's whole-word search.
    let body = if o.whole_word {
        let is_word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let (before, after) = if o.regex { (true, true) } else { (is_word(literal.chars().next()), is_word(literal.chars().next_back())) };
        format!("{}(?:{body}){}", if before { r"(?<!\w)" } else { "" }, if after { r"(?!\w)" } else { "" })
    } else {
        body
    };
    // (?m): ^ and $ match at every line. (?R): a CR LF pair counts as one line break.
    RegexBuilder::new(&format!("(?mR){body}"))
        .case_insensitive(!o.match_case)
        .dot_matches_new_line(o.regex && o.dot_all)
        .build()
        .map_err(|e| Error::Pattern(e.to_string()))
}

/// Turn matches into line-annotated hits.
pub fn hits_in_text(text: &str, re: &Regex, path: Option<&Path>) -> Vec<Hit> {
    let mut out = Vec::new();
    // A classic Mac document breaks its lines with a lone CR and has no LF at all.
    let brk = if !text.contains('\n') && text.contains('\r') { '\r' } else { '\n' };
    let (mut line, mut line_start, mut scanned) = (1usize, 0usize, 0usize);
    for (a, b) in all_matches(text, re) {
        line += text[scanned..a].matches(brk).count();
        if let Some(nl) = text[scanned..a].rfind(brk) {
            line_start = scanned + nl + 1;
        }
        scanned = a;
        let line_end = text[a..].find(['\r', '\n']).map_or(text.len(), |i| a + i);
        out.push(Hit { path: path.map(Path::to_path_buf), line, start: a, end: b, text: text[line_start..line_end].trim().chars().take(240).collect() });
    }
    out
}

/// Every match in the current document, for the "Find All" results list.
pub fn find_all<S: Send>(s: &S, o: &Opts) -> Result<Vec<Hit>, Error> {
    let range = limit(s, o);
    match prepare(s, o) {
        Ok((text, re)) => Ok(hits_in_text(&text, &re, None).into_iter().filter(|h| inside(range, (h.start, h.end))).collect()),
        Err(Error::EmptyPattern) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Select `start..end` (byte offsets) and scroll to it.
pub fn select_range<S: Send>(s: &S, start: usize, end: usize) {
    select(s, start, end);
}

/// Every non-overlapping match, in order. Empty matches advance by one char.
pub fn all_matches(text: &str, re: &Regex) -> Vec<Span> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= text.len() {
        match re.find_from_pos(text, pos) {
            Ok(Some(m)) => {
                out.push((m.start(), m.end()));
                pos = if m.end() > m.start() { m.end() } else { next_boundary(text, m.end()) };
            }
            _ => break,
        }
    }
    out
}

fn next_boundary(text: &str, pos: usize) -> usize {
    let mut p = pos + 1;
    while p < text.len() && !text.is_char_boundary(p) {
        p += 1;
    }
    p
}

/// First match starting at or after `from`.
pub fn next_match(text: &str, re: &Regex, from: usize) -> Option<Span> {
    if from > text.len() {
        return None;
    }
    let m = re.find_from_pos(text, from).ok()??;
    Some((m.start(), m.end()))
}

/// Last match that ends at or before `limit`.
#[cfg(test)]
pub fn prev_match(text: &str, re: &Regex, limit: usize) -> Option<Span> {
    all_matches(text, re).into_iter().rev().find(|m| m.1 <= limit)
}

/// Expand `\1`, `$1`, `\n`, `\r`, `\t`, `\\` and the case switches `\U \L \E \u \l` in a regex
/// replacement (the Boost.Regex format).
pub fn expand(rep: &str, caps: &Captures<'_, str>) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Case {
        Keep,
        Upper,
        Lower,
    }
    fn put(out: &mut String, s: &str, mode: Case, once: &mut Case) {
        for c in s.chars() {
            let m = if *once == Case::Keep { mode } else { std::mem::replace(once, Case::Keep) };
            match m {
                Case::Upper => out.extend(c.to_uppercase()),
                Case::Lower => out.extend(c.to_lowercase()),
                Case::Keep => out.push(c),
            }
        }
    }
    let mut out = String::new();
    let (mut mode, mut once) = (Case::Keep, Case::Keep);
    let mut it = rep.chars().peekable();
    while let Some(c) = it.next() {
        match (c, it.peek().copied()) {
            ('\\' | '$', Some(d)) if d.is_ascii_digit() => {
                it.next();
                if let Some(g) = d.to_digit(10).and_then(|n| caps.get(n as usize)) {
                    put(&mut out, g.as_str(), mode, &mut once);
                }
            }
            ('\\', Some(d @ ('n' | 'r' | 't' | '\\' | 'U' | 'L' | 'E' | 'u' | 'l'))) => {
                it.next();
                match d {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    '\\' => out.push('\\'),
                    'U' => mode = Case::Upper,
                    'L' => mode = Case::Lower,
                    'E' => (mode, once) = (Case::Keep, Case::Keep),
                    'u' => once = Case::Upper,
                    _ => once = Case::Lower,
                }
            }
            _ => put(&mut out, c.encode_utf8(&mut [0; 4]), mode, &mut once),
        }
    }
    out
}
fn replacement_for(text: &str, re: &Regex, o: &Opts, span: Span) -> String {
    if !o.regex {
        return if o.extended { unescape_ext(&o.replace) } else { o.replace.clone() };
    }
    match re.captures_from_pos(text, span.0) {
        Ok(Some(c)) => expand(&o.replace, &c),
        _ => o.replace.clone(),
    }
}

/// (start, end, replacement) for every match, ready to apply back to front.
pub fn replacements(text: &str, o: &Opts) -> Result<Vec<(usize, usize, String)>, Error> {
    let re = compile(o)?;
    Ok(all_matches(text, &re).into_iter().map(|s| (s.0, s.1, replacement_for(text, &re, o, s))).collect())
}

// ---- Scintilla-facing -----------------------------------------------------

/// The selection when the options say to search only inside it (and one exists).
fn limit<S: Send>(s: &S, o: &Opts) -> Option<Span> {
    if !o.in_selection {
        return None;
    }
    let (a, b) = selection(s);
    (b > a).then_some((a, b))
}

fn inside(range: Option<Span>, m: Span) -> bool {
    range.is_none_or(|(a, b)| m.0 >= a && m.1 <= b)
}

fn select<S: Send>(s: &S, a: usize, b: usize) {
    let line = s.send(sci::SCI_LINEFROMPOSITION, a, 0);
    s.send(sci::SCI_ENSUREVISIBLE, crate::num::to_usize(line), 0);
    s.send(sci::SCI_SETSEL, a, crate::num::to_isize(b));
    s.send(sci::SCI_SCROLLCARET, 0, 0);
}

fn selection<S: Send>(s: &S) -> Span {
    (crate::num::to_usize(s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0)), crate::num::to_usize(s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0)))
}

fn prepare<S: Send>(s: &S, o: &Opts) -> Result<(String, Regex), Error> {
    if o.find.is_empty() {
        return Err(Error::EmptyPattern);
    }
    Ok((sci::get_text(s), compile(o)?))
}

pub fn find<S: Send>(s: &S, o: &Opts, forward: bool) -> Found {
    let (text, re) = match prepare(s, o) {
        Ok(v) => v,
        Err(Error::EmptyPattern) => return Found::NotFound,
        Err(e) => return Found::BadPattern(e.to_string()),
    };
    let (sel_a, sel_b) = selection(s);
    // A zero-length match that is already the (empty) selection must not be found again.
    let stuck = |m: &Span| sel_a == sel_b && m.0 == m.1 && m.0 == sel_a;
    let hit = if forward {
        let step = |from: usize| match next_match(&text, &re, from) {
            Some(m) if stuck(&m) => next_match(&text, &re, next_boundary(&text, m.1)),
            other => other,
        };
        step(sel_b).map(|m| (m, false)).or_else(|| if o.wrap { step(0).map(|m| (m, true)) } else { None })
    } else {
        let before = |limit: usize| all_matches(&text, &re).into_iter().rev().find(|m| m.1 <= limit && !stuck(m));
        before(sel_a).map(|m| (m, false)).or_else(|| if o.wrap { before(text.len()).map(|m| (m, true)) } else { None })
    };
    match hit {
        Some(((a, b), wrapped)) => {
            select(s, a, b);
            Found::At { wrapped }
        }
        None => Found::NotFound,
    }
}

fn replace_range<S: Send>(s: &S, a: usize, b: usize, with: &str) {
    s.send(sci::SCI_SETTARGETSTART, a, 0);
    s.send(sci::SCI_SETTARGETEND, b, 0);
    s.send(sci::SCI_REPLACETARGET, with.len(), with.as_ptr() as isize);
}

/// Replace the selection if it is a match, then move to the next one.
/// Returns whether something was replaced.
pub fn replace_current<S: Send>(s: &S, o: &Opts) -> (bool, Found) {
    let Ok((text, re)) = prepare(s, o) else { return (false, find(s, o, true)) };
    let (a, b) = selection(s);
    let mut replaced = false;
    if next_match(&text, &re, a) == Some((a, b)) {
        let with = replacement_for(&text, &re, o, (a, b));
        replace_range(s, a, b, &with);
        let end = a + with.len();
        s.send(sci::SCI_SETSEL, end, crate::num::to_isize(end));
        replaced = true;
    }
    (replaced, find(s, o, true))
}

pub fn replace_all<S: Send>(s: &S, o: &Opts) -> Result<usize, Error> {
    if o.find.is_empty() {
        return Ok(0);
    }
    let range = limit(s, o);
    let reps: Vec<_> = replacements(&sci::get_text(s), o)?.into_iter().filter(|r| inside(range, (r.0, r.1))).collect();
    s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
    for (a, b, with) in reps.iter().rev() {
        replace_range(s, *a, *b, with);
    }
    s.send(sci::SCI_ENDUNDOACTION, 0, 0);
    Ok(reps.len())
}

pub fn count<S: Send>(s: &S, o: &Opts) -> Result<usize, Error> {
    let range = limit(s, o);
    match prepare(s, o) {
        Ok((text, re)) => Ok(all_matches(&text, &re).into_iter().filter(|m| inside(range, *m)).count()),
        Err(Error::EmptyPattern) => Ok(0),
        Err(e) => Err(e),
    }
}

pub fn clear_marks<S: Send>(s: &S, indicator: usize) {
    s.send(sci::SCI_SETINDICATORCURRENT, indicator, 0);
    s.send(sci::SCI_INDICATORCLEARRANGE, 0, sci::length(s).max(0));
}

pub fn mark_all<S: Send>(s: &S, o: &Opts, indicator: usize) -> Result<usize, Error> {
    clear_marks(s, indicator);
    fill_marks(s, o, indicator)
}

/// Mark every match without clearing what is already marked.
fn fill_marks<S: Send>(s: &S, o: &Opts, indicator: usize) -> Result<usize, Error> {
    let (text, re) = match prepare(s, o) {
        Ok(v) => v,
        Err(Error::EmptyPattern) => return Ok(0),
        Err(e) => return Err(e),
    };
    let range = limit(s, o);
    let marks: Vec<Span> = all_matches(&text, &re).into_iter().filter(|m| inside(range, *m)).collect();
    s.send(sci::SCI_SETINDICATORCURRENT, indicator, 0);
    for (a, b) in &marks {
        s.send(sci::SCI_INDICATORFILLRANGE, *a, crate::num::to_isize(*b - *a));
    }
    Ok(marks.len())
}

// ---- token styles (Search > Style ... Token) -------------------------------

/// Five user colours, named "1st" to "5th".
pub const STYLE_SLOTS: usize = 5;
const INDIC_STYLE_BASE: usize = 10;
pub const STYLE_COLOURS: [(u8, u8, u8); STYLE_SLOTS] = [(0xFF, 0x80, 0x00), (0xFF, 0xFF, 0x00), (0x00, 0xE0, 0x40), (0x80, 0x40, 0xFF), (0xFF, 0x40, 0x40)];

pub fn style_indicator(slot: usize) -> usize {
    INDIC_STYLE_BASE + slot.min(STYLE_SLOTS - 1)
}

fn is_word_text(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Byte range of the word under the caret.
fn word_range_at_caret<S: Send>(s: &S) -> Option<Span> {
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

// ---- braces ------------------------------------------------------------------

/// The bracket next to the caret: the one before it, else the one after.
/// Only `()[]{}` count (Scintilla would also pair `<` and `>`).
fn brace_at_caret<S: Send>(s: &S) -> Option<usize> {
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

#[cfg(test)]
mod tests;
