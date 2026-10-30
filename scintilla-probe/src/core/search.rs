//! Search: find / replace / count / mark.
//!
//! Matching is done in Rust (fancy-regex: PCRE-like, with backreferences and
//! lookaround, much closer to Notepad++'s Boost.Regex than Scintilla's built-in
//! engine, whose `\w`/`\s`/`\b` proved unreliable). Scintilla positions are
//! UTF-8 byte offsets, which is exactly what `&str` matching yields, so match
//! ranges map straight onto document positions.
//!
//! The pure functions at the top are unit-tested; the Scintilla-facing ones
//! below only fetch the text and apply the result.

use super::doc;
use super::sci::{self, Send};
use fancy_regex::{Captures, Regex, RegexBuilder};
use std::path::{Path, PathBuf};

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

/// Expand the escapes of Notepad++'s Extended search mode.
pub fn unescape_ext(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars().peekable();
    // Up to max digits of adix, as a character.
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
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.peek().copied() {
            Some('n') => { it.next(); out.push('\n') }
            Some('r') => { it.next(); out.push('\r') }
            Some('t') => { it.next(); out.push('\t') }
            Some('0') => { it.next(); out.push('\0') }
            Some('\\') => { it.next(); out.push('\\') }
            Some(k @ ('x' | 'd' | 'o' | 'b' | 'u')) => {
                it.next();
                let (radix, max) = match k {
                    'x' => (16, 2),
                    'd' => (10, 3),
                    'o' => (8, 3),
                    'b' => (2, 8),
                    _ => (16, 4),
                };
                match number(&mut it, radix, max) {
                    Some(ch) => out.push(ch),
                    None => {
                        out.push('\\');
                        out.push(k);
                    }
                }
            }
            _ => out.push('\\'),
        }
    }
    out
}

pub fn compile(o: &Opts) -> Result<Regex, String> {
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
    // (?m): ^ and $ match at every line, as in Notepad++. (?R): a CR LF pair counts as one line break.
    RegexBuilder::new(&format!("(?mR){body}"))
        .case_insensitive(!o.match_case)
        .dot_matches_new_line(o.regex && o.dot_all)
        .build()
        .map_err(|e| e.to_string())
}

/// Turn matches into line-annotated hits.
pub fn hits_in_text(text: &str, re: &Regex, path: Option<&Path>) -> Vec<Hit> {
    let mut out = Vec::new();
    let (mut line, mut line_start, mut scanned) = (1usize, 0usize, 0usize);
    for (a, b) in all_matches(text, re) {
        line += text[scanned..a].matches('\n').count();
        if let Some(nl) = text[scanned..a].rfind('\n') {
            line_start = scanned + nl + 1;
        }
        scanned = a;
        let line_end = text[a..].find(['\r', '\n']).map(|i| a + i).unwrap_or(text.len());
        out.push(Hit {
            path: path.map(Path::to_path_buf),
            line,
            start: a,
            end: b,
            text: text[line_start..line_end].trim().chars().take(240).collect(),
        });
    }
    out
}

/// `*` and `?` wildcard match, case-insensitive.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) if a == b => go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    go(&p, &n)
}

/// Does `name` pass a filter list like `*.rs;*.toml;!*.min.js`?
pub fn passes_filter(filters: &str, name: &str) -> bool {
    let parts: Vec<&str> = filters.split([';', ',', ' ']).map(str::trim).filter(|s| !s.is_empty()).collect();
    let includes: Vec<&&str> = parts.iter().filter(|p| !p.starts_with('!')).collect();
    let excluded = parts.iter().filter_map(|p| p.strip_prefix('!')).any(|p| glob_match(p, name));
    !excluded && (includes.is_empty() || includes.iter().any(|p| glob_match(p, name)))
}

const SKIP_DIRS: [&str; 5] = ["node_modules", "target", ".git", ".svn", ".hg"];
pub const MAX_HITS: usize = 5000;
/// Most Find All results listed in the panel.
pub const MAX_LIST: usize = 20_000;

/// Search every file under `dir` whose name passes `filters`.
pub fn find_in_files(dir: &Path, filters: &str, o: &Opts) -> Result<Vec<Hit>, String> {
    let re = compile(o)?;
    let mut hits = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    stack.push(p);
                }
            } else if passes_filter(filters, &name) {
                if e.metadata().map(|m| m.len() > 8_000_000).unwrap_or(true) {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&p) else { continue };
                let head = &bytes[..bytes.len().min(8000)];
                let utf16 = bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]);
                if !utf16 && head.contains(&0) {
                    continue;
                }
                let (text, _, _) = doc::decode(&bytes);
                hits.extend(hits_in_text(&text, &re, Some(&p)));
                if hits.len() >= MAX_HITS {
                    hits.truncate(MAX_HITS);
                    return Ok(hits);
                }
            }
        }
    }
    Ok(hits)
}

/// Every match in the current document, for the "Find All" results list.
pub fn find_all<S: Send>(s: &S, o: &Opts) -> Result<Vec<Hit>, String> {
    let range = limit(s, o);
    match prepare(s, o) {
        Ok((text, re)) => Ok(hits_in_text(&text, &re, None).into_iter().filter(|h| inside(range, (h.start, h.end))).collect()),
        Err(e) if e.is_empty() => Ok(Vec::new()),
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
/// replacement (the Boost.Regex format Notepad++ uses).
pub fn expand(rep: &str, caps: &Captures<'_, str>) -> String {
    #[derive(Clone, Copy, PartialEq)]
    enum Case {
        Keep,
        Upper,
        Lower,
    }
    fn put(out: &mut String, s: &str, mode: Case, once: &mut Case) {
        for c in s.chars() {
            let m = if *once != Case::Keep { std::mem::replace(once, Case::Keep) } else { mode };
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
                if let Some(g) = caps.get(d.to_digit(10).unwrap() as usize) {
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
pub fn replacements(text: &str, o: &Opts) -> Result<Vec<(usize, usize, String)>, String> {
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
    s.send(sci::SCI_ENSUREVISIBLE, line as usize, 0);
    s.send(sci::SCI_SETSEL, a, b as isize);
    s.send(sci::SCI_SCROLLCARET, 0, 0);
}

fn selection<S: Send>(s: &S) -> Span {
    (
        s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize,
        s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize,
    )
}

fn prepare<S: Send>(s: &S, o: &Opts) -> Result<(String, Regex), String> {
    if o.find.is_empty() {
        return Err(String::new());
    }
    Ok((sci::get_text(s), compile(o)?))
}

pub fn find<S: Send>(s: &S, o: &Opts, forward: bool) -> Found {
    let (text, re) = match prepare(s, o) {
        Ok(v) => v,
        Err(e) if e.is_empty() => return Found::NotFound,
        Err(e) => return Found::BadPattern(e),
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
        s.send(sci::SCI_SETSEL, end, end as isize);
        replaced = true;
    }
    (replaced, find(s, o, true))
}

pub fn replace_all<S: Send>(s: &S, o: &Opts) -> Result<usize, String> {
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

pub fn count<S: Send>(s: &S, o: &Opts) -> Result<usize, String> {
    let range = limit(s, o);
    match prepare(s, o) {
        Ok((text, re)) => Ok(all_matches(&text, &re).into_iter().filter(|m| inside(range, *m)).count()),
        Err(e) if e.is_empty() => Ok(0),
        Err(e) => Err(e),
    }
}

pub fn clear_marks<S: Send>(s: &S, indicator: usize) {
    s.send(sci::SCI_SETINDICATORCURRENT, indicator, 0);
    s.send(sci::SCI_INDICATORCLEARRANGE, 0, sci::length(s).max(0));
}

pub fn mark_all<S: Send>(s: &S, o: &Opts, indicator: usize) -> Result<usize, String> {
    clear_marks(s, indicator);
    fill_marks(s, o, indicator)
}

/// Mark every match without clearing what is already marked.
fn fill_marks<S: Send>(s: &S, o: &Opts, indicator: usize) -> Result<usize, String> {
    let (text, re) = match prepare(s, o) {
        Ok(v) => v,
        Err(e) if e.is_empty() => return Ok(0),
        Err(e) => return Err(e),
    };
    let range = limit(s, o);
    let marks: Vec<Span> = all_matches(&text, &re).into_iter().filter(|m| inside(range, *m)).collect();
    s.send(sci::SCI_SETINDICATORCURRENT, indicator, 0);
    for (a, b) in &marks {
        s.send(sci::SCI_INDICATORFILLRANGE, *a, (*b - *a) as isize);
    }
    Ok(marks.len())
}

// ---- token styles (Search > Style ... Token) -------------------------------

/// Five user colours, like Notepad++'s "1st" to "5th" styles.
pub const STYLE_SLOTS: usize = 5;
const INDIC_STYLE_BASE: usize = 10;
pub const STYLE_COLOURS: [(u8, u8, u8); STYLE_SLOTS] =
    [(0xFF, 0x80, 0x00), (0xFF, 0xFF, 0x00), (0x00, 0xE0, 0x40), (0x80, 0x40, 0xFF), (0xFF, 0x40, 0x40)];

pub fn style_indicator(slot: usize) -> usize {
    INDIC_STYLE_BASE + slot.min(STYLE_SLOTS - 1)
}

fn is_word_text(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// Byte range of the word under the caret.
fn word_range_at_caret<S: Send>(s: &S) -> Option<Span> {
    let pos = s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
    let a = s.send(sci::SCI_WORDSTARTPOSITION, pos, 1).max(0) as usize;
    let b = s.send(sci::SCI_WORDENDPOSITION, pos, 1).max(0) as usize;
    (b > a && b - a <= 200).then_some((a, b))
}

/// Select the word under the caret when nothing is selected.
pub fn select_word_at_caret<S: Send>(s: &S) {
    let (a, b) = selection(s);
    if a == b {
        if let Some((x, y)) = word_range_at_caret(s) {
            select(s, x, y);
        }
    }
}

/// The selection, or the word under the caret when nothing is selected.
pub fn token_at_caret<S: Send>(s: &S) -> String {
    let sel = selected_text(s);
    if !sel.is_empty() {
        return sel;
    }
    let Some((a, b)) = word_range_at_caret(s) else { return String::new() };
    s.send(sci::SCI_SETTARGETRANGE, a, b as isize);
    let n = s.send(sci::SCI_GETTARGETTEXT, 0, 0).max(0) as usize;
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
    s.send(sci::SCI_INDICATORFILLRANGE, a, (b - a) as isize);
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
    let len = sci::length(s).max(0) as usize;
    let (mut out, mut p) = (Vec::new(), 0usize);
    while p < len {
        let end = s.send(sci::SCI_INDICATOREND, ind, p as isize).max(0) as usize;
        if end <= p {
            break;
        }
        if s.send(sci::SCI_INDICATORVALUEAT, ind, p as isize) != 0 {
            out.push((p, end));
        }
        p = end;
    }
    out
}

/// The range after (or, backwards, before) `from`, wrapping around.
pub fn next_range(ranges: &[Span], from: usize, forward: bool) -> Option<Span> {
    if forward {
        ranges.iter().find(|r| r.0 >= from).or(ranges.first()).copied()
    } else {
        ranges.iter().rev().find(|r| r.0 < from).or(ranges.last()).copied()
    }
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
    ranges.sort();
    ranges.dedup();
    let text = sci::get_text(s);
    ranges.iter().filter_map(|(a, b)| text.get(*a..*b)).collect::<Vec<_>>().join("\r\n")
}

// ---- braces ------------------------------------------------------------------

/// The bracket next to the caret: the one before it, else the one after, like Notepad++.
/// Only `()[]{}` count (Scintilla would also pair `<` and `>`).
fn brace_at_caret<S: Send>(s: &S) -> Option<usize> {
    let pos = s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
    let is_brace = |p: usize| matches!(s.send(sci::SCI_GETCHARAT, p, 0) as u8, b'(' | b')' | b'[' | b']' | b'{' | b'}');
    [pos.checked_sub(1), Some(pos)].into_iter().flatten().find(|&p| p < sci::length(s).max(0) as usize && is_brace(p))
}

/// The brace at the caret and its partner.
pub fn matching_brace<S: Send>(s: &S) -> Option<Span> {
    let p = brace_at_caret(s)?;
    let m = s.send(sci::SCI_BRACEMATCH, p, 0);
    (m >= 0).then_some((p, m as usize))
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
            s.send(sci::SCI_SETSEL, p.min(m) + 1, p.max(m) as isize);
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
    s.send(sci::SCI_SETTARGETRANGE, a, b as isize);
    let n = s.send(sci::SCI_GETTARGETTEXT, 0, 0).max(0) as usize;
    if n == 0 {
        return String::new();
    }
    let mut buf = vec![0u8; n + 1];
    s.send(sci::SCI_GETTARGETTEXT, 0, buf.as_mut_ptr() as isize);
    buf.truncate(n);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Highlight every occurrence of the single word currently selected, like
/// Notepad++'s "smart highlighting". Large documents are skipped.
pub fn smart_highlight<S: Send>(s: &S) {
    clear_marks(s, INDIC_SMART);
    if sci::length(s) > 2_000_000 {
        return;
    }
    let word = selected_text(s);
    let is_word = !word.is_empty() && word.len() <= 100 && word.chars().all(|c| c.is_alphanumeric() || c == '_');
    if is_word {
        // Like Notepad++'s default: whole words, any case.
        let o = Opts { find: word, match_case: false, whole_word: true, ..Default::default() };
        let _ = mark_all(s, &o, INDIC_SMART);
    }
}

/// Move to 1-based `line`, clamped to the document. Returns the line used.
pub fn goto_line<S: Send>(s: &S, line: usize) -> usize {
    let total = sci::line_count(s).max(1) as usize;
    let l = line.clamp(1, total);
    s.send(sci::SCI_ENSUREVISIBLE, l - 1, 0);
    s.send(sci::SCI_GOTOLINE, l - 1, 0);
    s.send(sci::SCI_SCROLLCARET, 0, 0);
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(find: &str) -> Opts {
        Opts { find: find.into(), ..Default::default() }
    }

    #[test]
    fn range_navigation_wraps() {
        let r = [(2, 4), (10, 12), (20, 25)];
        assert_eq!(next_range(&r, 4, true), Some((10, 12)));
        assert_eq!(next_range(&r, 26, true), Some((2, 4)));
        assert_eq!(next_range(&r, 10, false), Some((2, 4)));
        assert_eq!(next_range(&r, 0, false), Some((20, 25)));
        assert_eq!(next_range(&[], 0, true), None);
    }

    #[test]
    fn finds_characters_in_a_code_point_range() {
        let t = "ab\u{e9}cd\u{20ac}";
        assert_eq!(find_char_in_range(t, 128, u32::MAX, 0, true, false), Some((2, 4)));
        assert_eq!(find_char_in_range(t, 128, u32::MAX, 4, true, false), Some((6, 9)));
        assert_eq!(find_char_in_range(t, 128, u32::MAX, 9, true, false), None);
        assert_eq!(find_char_in_range(t, 128, u32::MAX, 9, true, true), Some((2, 4)));
        assert_eq!(find_char_in_range(t, 0, 127, 6, false, false), Some((5, 6)));
        assert_eq!(find_char_in_range(t, 0, 127, 0, false, true), Some((5, 6)));
        assert_eq!(find_char_in_range("abc", 0x62, 0x62, 0, true, false), Some((1, 2)));
    }
    fn count_of(text: &str, o: &Opts) -> usize {
        all_matches(text, &compile(o).unwrap()).len()
    }

    const TEXT: &str = "alpha beta alpha\r\nAlpha gamma 123 456\r\nfoo() bar() alphabet\r\n";

    #[test]
    fn case_and_whole_word() {
        assert_eq!(count_of(TEXT, &opts("alpha")), 4);
        assert_eq!(count_of(TEXT, &Opts { match_case: true, ..opts("alpha") }), 3);
        assert_eq!(count_of(TEXT, &Opts { whole_word: true, ..opts("alpha") }), 3);
        assert_eq!(count_of(TEXT, &Opts { whole_word: true, match_case: true, ..opts("alpha") }), 2);
    }

    #[test]
    fn plain_text_is_literal() {
        assert_eq!(count_of(TEXT, &opts("foo()")), 1);
        assert_eq!(count_of("a.b axb", &opts("a.b")), 1);
    }

    #[test]
    fn regex_classes_work_in_every_case_mode() {
        for match_case in [true, false] {
            let o = |f: &str| Opts { regex: true, match_case, ..opts(f) };
            assert_eq!(count_of(TEXT, &o(r"\d+")), 2);
            assert_eq!(count_of(TEXT, &o(r"\w+")), 10);
            assert_eq!(count_of(TEXT, &o(r"\s")), 13, "match_case={match_case}");
            assert_eq!(count_of(TEXT, &o(r"\bfoo")), 1);
        }
    }

    #[test]
    fn anchors_match_per_line() {
        let o = Opts { regex: true, match_case: true, ..opts("^Alpha") };
        assert_eq!(count_of(TEXT, &o), 1);
        let o = Opts { regex: true, ..opts(r"^\w+") };
        assert_eq!(count_of(TEXT, &o), 3);
    }

    #[test]
    fn replacements_expand_groups() {
        let o = Opts { regex: true, replace: r"\2-\1".into(), ..opts(r"(\d+) (\d+)") };
        let r = replacements(TEXT, &o).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].2, "456-123");
        let o = Opts { regex: true, replace: "[$1]".into(), ..opts(r"(\w+)\(\)") };
        let r: Vec<_> = replacements(TEXT, &o).unwrap().into_iter().map(|x| x.2).collect();
        assert_eq!(r, vec!["[foo]", "[bar]"]);
    }

    #[test]
    fn plain_replacement_is_literal() {
        let o = Opts { replace: r"\1$1".into(), ..opts("beta") };
        assert_eq!(replacements(TEXT, &o).unwrap()[0].2, r"\1$1");
    }

    #[test]
    fn next_and_prev() {
        let re = compile(&opts("alpha")).unwrap();
        assert_eq!(next_match(TEXT, &re, 0), Some((0, 5)));
        assert_eq!(next_match(TEXT, &re, 1), Some((11, 16)));
        assert_eq!(prev_match(TEXT, &re, 11), Some((0, 5)));
        assert_eq!(prev_match(TEXT, &re, 4), None);
    }

    #[test]
    fn unicode_offsets_are_bytes() {
        let re = compile(&opts("é")).unwrap();
        assert_eq!(all_matches("aéé", &re), vec![(1, 3), (3, 5)]);
    }

    #[test]
    fn hits_carry_line_numbers() {
        let re = compile(&opts("alpha")).unwrap();
        let hits = hits_in_text(TEXT, &re, None);
        let lines: Vec<usize> = hits.iter().map(|h| h.line).collect();
        assert_eq!(lines, vec![1, 1, 2, 3]);
        assert_eq!(hits[3].text, "foo() bar() alphabet");
        assert_eq!((hits[2].start, hits[2].end), (18, 23));
    }

    #[test]
    fn glob_and_filters() {
        assert!(glob_match("*.rs", "Main.RS"));
        assert!(!glob_match("*.rs", "main.rss"));
        assert!(glob_match("a?c.*", "abc.txt"));
        assert!(passes_filter("*.*", "x.txt"));
        assert!(passes_filter("*.rs;*.toml", "Cargo.toml"));
        assert!(!passes_filter("*.rs;*.toml", "a.js"));
        assert!(!passes_filter("*.js;!*.min.js", "a.min.js"));
        assert!(passes_filter("", "anything"));
    }

    #[test]
    fn find_in_files_walks_and_filters() {
        let d = std::env::temp_dir().join("notepp_fif_native");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("a.txt"), "hello\nWorld here\n").unwrap();
        std::fs::write(d.join("sub").join("b.rs"), "nothing\nworld again").unwrap();
        std::fs::write(d.join("node_modules").join("c.txt"), "world").unwrap();
        std::fs::write(d.join("bin.dat"), b"world\0\0\0").unwrap();
        let hits = find_in_files(&d, "*.*", &opts("world")).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().any(|h| h.line == 2 && h.path.as_ref().unwrap().ends_with("a.txt")));
        let only_rs = find_in_files(&d, "*.rs", &opts("world")).unwrap();
        assert_eq!(only_rs.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dot_matches_newline_option() {
        let o = Opts { regex: true, ..opts("a.b") };
        assert_eq!(count_of("a\nb", &o), 0);
        let o = Opts { regex: true, dot_all: true, ..opts("a.b") };
        assert_eq!(count_of("a\nb", &o), 1);
    }

    #[test]
    fn probe_crlf_and_edges() {
        let re = |f: &str, o: Opts| compile(&Opts { regex: true, find: f.into(), ..o }).unwrap();
        let crlf = "ab\r\ncd\r\n";
        // `$` must match before a CRLF, and `.` must not swallow the CR.
        assert_eq!(all_matches(crlf, &re("b$", Opts::default())), vec![(1, 2)], "dollar before CRLF");
        assert_eq!(all_matches(crlf, &re(".*", Opts::default())).iter().filter(|m| m.1 > m.0).count(), 2);
        assert_eq!(all_matches(crlf, &re(".+", Opts::default())), vec![(0, 2), (4, 6)], "dot excludes CR");
        // Whole word with a pattern that starts or ends with punctuation.
        let w = compile(&Opts { find: "@foo".into(), whole_word: true, ..Default::default() }).unwrap();
        assert_eq!(all_matches("x @foo y", &w).len(), 1, "whole word @foo");
        let w = compile(&Opts { find: "foo()".into(), whole_word: true, ..Default::default() }).unwrap();
        assert_eq!(all_matches("a foo() b", &w).len(), 1, "whole word foo()");
    }

    #[test]
    fn replacement_case_switches() {
        let rep = |f: &str, r: &str, text: &str| {
            let o = Opts { regex: true, replace: r.into(), ..opts(f) };
            let reps = replacements(text, &o).unwrap();
            let mut out = text.to_string();
            for (a, b, w) in reps.iter().rev() {
                out.replace_range(*a..*b, w);
            }
            out
        };
        assert_eq!(rep(r"(\w+) (\w+)", r"\U$1\E $2", "hello world"), "HELLO world");
        assert_eq!(rep(r"(\w+)", r"\u$1", "hello"), "Hello");
        assert_eq!(rep(r"(\w+)", r"\L$1", "HELLO"), "hello");
        assert_eq!(rep(r"(\w)(\w*)", r"\u$1\L$2", "wORLD"), "World");
    }

    #[test]
    fn crlf_anchor_with_lookaround() {
        let re = compile(&Opts { regex: true, find: r"(?<=a)b$".into(), ..Default::default() }).unwrap();
        assert_eq!(all_matches("ab\r\ncb\r\n", &re), vec![(1, 2)]);
    }

    #[test]
    fn extended_mode_escapes() {
        assert_eq!(unescape_ext(r"a\tb\n\x41\d066\o103\u00e9\0"), "a\tb\nABC\u{e9}\0");
        assert_eq!(unescape_ext(r"\\n"), "\\n");
        assert_eq!(unescape_ext(r"\q"), r"\q");
        let re = compile(&Opts { extended: true, find: r"a\nb".into(), ..Default::default() }).unwrap();
        assert_eq!(all_matches("a\nb a b", &re).len(), 1);
        let re = compile(&Opts { extended: true, find: r"two\r\nthree".into(), ..Default::default() }).unwrap();
        assert_eq!(all_matches("one two\r\nthree one\r\n", &re), vec![(4, 14)]);
        let full = Opts { extended: true, find: r"two\r\nthree".into(), replace: r"2\t3".into(), wrap: true, ..Default::default() };
        assert_eq!(replacements("one two\r\nthree one\r\n", &full).unwrap(), vec![(4, 14, "2\t3".to_string())]);
        let o = Opts { extended: true, find: "x".into(), replace: r"1\t2".into(), ..Default::default() };
        assert_eq!(replacements("x", &o).unwrap()[0].2, "1\t2");
    }

    #[test]
    fn bad_pattern_reports_error() {
        assert!(compile(&Opts { regex: true, ..opts("(unclosed") }).is_err());
    }

    #[test]
    fn empty_matches_terminate() {
        let re = compile(&Opts { regex: true, ..opts("x*") }).unwrap();
        assert_eq!(all_matches("ab", &re).len(), 3);
    }
}
