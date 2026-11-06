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
mod extended;
pub use extended::*;

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

pub(crate) fn replace_range<S: Send>(s: &S, a: usize, b: usize, with: &str) {
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

mod styles;
pub use styles::*;

mod caret;
pub use caret::*;
#[cfg(test)]
mod tests;
