//! Text and line operations behind the Edit menu: case conversion, sorting,
//! blank handling, commenting, the column editor and date/time insertion.
//!
//! The transforms are pure functions over a block of text (and are unit
//! tested); `apply_lines` / `apply_range` apply one to the editor as a single
//! undo step.

use super::sci::{self, Send};

/// Split a block into lines, remembering whether it ended with a line break.
/// Only one trailing break is dropped, so a blank last line is preserved.
fn split(text: &str) -> (Vec<&str>, bool) {
    let (body, trailing) = if let Some(b) = text.strip_suffix("\r\n") {
        (b, true)
    } else if let Some(b) = text.strip_suffix(['\n', '\r']) {
        (b, true)
    } else {
        (text, false)
    };
    if body.is_empty() && !trailing {
        return (Vec::new(), false);
    }
    (body.split('\n').map(|l| l.trim_end_matches('\r')).collect(), trailing)
}

fn join(lines: &[String], eol: &str, trailing: bool) -> String {
    let mut s = lines.join(eol);
    if trailing {
        s.push_str(eol);
    }
    s
}

fn map_lines(text: &str, eol: &str, f: impl Fn(Vec<&str>) -> Vec<String>) -> String {
    let (lines, trailing) = split(text);
    if lines.is_empty() {
        return text.to_string();
    }
    join(&f(lines), eol, trailing)
}

// ── sorting ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortKind {
    Lexicographic,
    IgnoreCase,
    Integer,
    DecimalDot,
    DecimalComma,
}

/// The number a line starts with (after leading blanks), if any.
fn leading_number(line: &str, comma: bool, integer: bool) -> Option<f64> {
    let t = line.trim_start();
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        i += 1;
    }
    let digits = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits {
        return None;
    }
    let sep = if comma { b',' } else { b'.' };
    if !integer && i + 1 < b.len() && b[i] == sep && b[i + 1].is_ascii_digit() {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    t[..i].replace(',', ".").parse().ok()
}

pub fn sort_lines_by(text: &str, eol: &str, kind: SortKind, descending: bool) -> String {
    map_lines(text, eol, |mut l| {
        // Descending is a reversed comparison, so lines that compare equal keep their order.
        let dir = |o: std::cmp::Ordering| if descending { o.reverse() } else { o };
        match kind {
            SortKind::Lexicographic => l.sort_by(|a, b| dir(a.cmp(b))),
            SortKind::IgnoreCase => l.sort_by(|a, b| dir(a.to_lowercase().cmp(&b.to_lowercase()))),
            SortKind::Integer | SortKind::DecimalDot | SortKind::DecimalComma => {
                let comma = kind == SortKind::DecimalComma;
                let integer = kind == SortKind::Integer;
                // Lines without a number sort first.
                l.sort_by(|a, b| {
                    let (x, y) = (leading_number(a, comma, integer), leading_number(b, comma, integer));
                    dir(match (x, y) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
                    })
                });
            }
        }
        l.into_iter().map(String::from).collect()
    })
}

#[cfg(test)]
fn sort_lines(text: &str, eol: &str, descending: bool) -> String {
    sort_lines_by(text, eol, SortKind::Lexicographic, descending)
}

/// Small xorshift generator: shuffling and random case do not need a crate.
pub fn seed_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        | 1
}

fn next_rand(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

pub fn randomize_lines(text: &str, eol: &str, seed: u64) -> String {
    map_lines(text, eol, |mut l| {
        let mut st = seed | 1;
        for i in (1..l.len()).rev() {
            let j = (next_rand(&mut st) % (i as u64 + 1)) as usize;
            l.swap(i, j);
        }
        l.into_iter().map(String::from).collect()
    })
}

pub fn reverse_lines(text: &str, eol: &str) -> String {
    map_lines(text, eol, |mut l| {
        l.reverse();
        l.into_iter().map(String::from).collect()
    })
}

/// Keep the first occurrence of every line.
pub fn remove_duplicate_lines(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| {
        let mut seen = std::collections::HashSet::new();
        l.into_iter().filter(|x| seen.insert(*x)).map(String::from).collect()
    })
}

/// Collapse runs of identical neighbouring lines.
pub fn remove_consecutive_duplicates(text: &str, eol: &str) -> String {
    map_lines(text, eol, |mut l| {
        l.dedup();
        l.into_iter().map(String::from).collect()
    })
}

/// Lines with nothing on them at all.
pub fn remove_empty_lines(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().filter(|x| !x.is_empty()).map(String::from).collect())
}

/// Empty lines and lines holding only blanks.
pub fn remove_blank_lines(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().filter(|x| !x.trim_matches(BLANKS).is_empty()).map(String::from).collect())
}

pub fn join_lines(text: &str) -> String {
    let (lines, _) = split(text);
    lines.join(" ")
}

/// Break lines longer than `width` characters, preferring a blank to break at.
pub fn split_lines(text: &str, eol: &str, width: usize) -> String {
    let width = width.max(2);
    map_lines(text, eol, |lines| {
        let mut out = Vec::new();
        for line in lines {
            let mut rest: Vec<char> = line.chars().collect();
            while rest.len() > width {
                let cut = rest[..=width].iter().rposition(|c| *c == ' ' || *c == '\t').filter(|&p| p > 0);
                match cut {
                    Some(p) => {
                        out.push(rest[..p].iter().collect());
                        rest.drain(..=p);
                    }
                    None => {
                        out.push(rest[..width].iter().collect());
                        rest.drain(..width);
                    }
                }
            }
            out.push(rest.into_iter().collect());
        }
        out
    })
}

// ── blanks ───────────────────────────────────────────────────────────────────

/// Only spaces and tabs count as blanks, as in Notepad++ (not no-break or ideographic spaces).
const BLANKS: [char; 2] = [' ', '\t'];

pub fn trim_trailing(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_end_matches(BLANKS).to_string()).collect())
}

pub fn trim_leading(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_start_matches(BLANKS).to_string()).collect())
}

pub fn trim_both(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_matches(BLANKS).to_string()).collect())
}

/// Trim every line and drop the lines that were only blanks.
pub fn remove_unnecessary_blanks(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_matches(BLANKS)).filter(|x| !x.is_empty()).map(String::from).collect())
}

/// Replace each tab with the spaces that reach the next tab stop.
pub fn tabs_to_spaces(text: &str, eol: &str, tab: usize) -> String {
    let tab = tab.max(1);
    map_lines(text, eol, |lines| {
        lines
            .into_iter()
            .map(|line| {
                let mut out = String::new();
                let mut col = 0;
                for c in line.chars() {
                    if c == '\t' {
                        let n = tab - col % tab;
                        out.extend(std::iter::repeat_n(' ', n));
                        col += n;
                    } else {
                        out.push(c);
                        col += 1;
                    }
                }
                out
            })
            .collect()
    })
}

/// Turn runs of two or more spaces into tabs wherever they reach a tab stop.
pub fn spaces_to_tabs(text: &str, eol: &str, tab: usize, leading_only: bool) -> String {
    let tab = tab.max(2);
    map_lines(text, eol, |lines| {
        lines
            .into_iter()
            .map(|line| {
                let chars: Vec<char> = line.chars().collect();
                let (mut out, mut col, mut i) = (String::new(), 0usize, 0usize);
                let mut only_blank = true;
                while i < chars.len() {
                    let c = chars[i];
                    if c == ' ' && (!leading_only || only_blank) {
                        let mut j = i;
                        while j < chars.len() && chars[j] == ' ' {
                            j += 1;
                        }
                        let mut left = j - i;
                        if left >= 2 {
                            loop {
                                let stop = (col / tab + 1) * tab;
                                if stop - col <= left {
                                    out.push('\t');
                                    left -= stop - col;
                                    col = stop;
                                } else {
                                    break;
                                }
                            }
                        }
                        out.extend(std::iter::repeat_n(' ', left));
                        col += left;
                        i = j;
                        continue;
                    }
                    if c == '\t' {
                        col = (col / tab + 1) * tab;
                    } else {
                        if c != ' ' {
                            only_blank = false;
                        }
                        col += 1;
                    }
                    out.push(c);
                    i += 1;
                }
                out
            })
            .collect()
    })
}

// ── case ─────────────────────────────────────────────────────────────────────

/// "hello wORLD" -> "Hello World"; with `blend` only the first letters change.
pub fn proper_case(s: &str, blend: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
                start = false;
            } else if blend {
                out.push(c);
            } else {
                out.extend(c.to_lowercase());
            }
        } else {
            out.push(c);
            // An apostrophe inside a word does not start a new one.
            if !(c == '\'' && !start) {
                start = true;
            }
        }
    }
    out
}

/// Capitalise the first letter of each sentence.
pub fn sentence_case(s: &str, blend: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let (mut cap, mut ended) = (true, false);
    for c in s.chars() {
        if c.is_alphabetic() {
            if cap {
                out.extend(c.to_uppercase());
                cap = false;
            } else if blend {
                out.push(c);
            } else {
                out.extend(c.to_lowercase());
            }
            ended = false;
        } else {
            out.push(c);
            if matches!(c, '.' | '!' | '?') {
                ended = true;
            } else if c.is_whitespace() {
                if ended {
                    cap = true;
                    ended = false;
                }
            } else if !c.is_ascii_punctuation() || c.is_ascii_digit() {
                ended = false;
            }
        }
    }
    out
}

pub fn invert_case(s: &str) -> String {
    s.chars()
        .flat_map(|c| -> Vec<char> {
            if c.is_uppercase() {
                c.to_lowercase().collect()
            } else if c.is_lowercase() {
                c.to_uppercase().collect()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn random_case(s: &str, seed: u64) -> String {
    let mut st = seed | 1;
    s.chars()
        .flat_map(|c| -> Vec<char> {
            if !c.is_alphabetic() {
                vec![c]
            } else if next_rand(&mut st) & 1 == 0 {
                c.to_uppercase().collect()
            } else {
                c.to_lowercase().collect()
            }
        })
        .collect()
}

// ── comments ─────────────────────────────────────────────────────────────────

/// Line-comment prefix for a Lexilla lexer name.
pub fn comment_prefix(lexer: &str) -> Option<&'static str> {
    match lexer {
        "rust" | "cpp" | "json" | "dart" | "zig" | "phpscript" => Some("//"),
        "python" | "yaml" | "toml" | "bash" | "props" | "powershell" | "ruby" | "perl" | "r" | "nim" => Some("#"),
        "sql" | "lua" => Some("--"),
        "batch" => Some("REM"),
        "vb" => Some("'"),
        "latex" => Some("%"),
        "fortran" => Some("!"),
        "asm" => Some(";"),
        _ => None,
    }
}

/// Block-comment delimiters for a Lexilla lexer name.
pub fn comment_block(lexer: &str) -> Option<(&'static str, &'static str)> {
    match lexer {
        "rust" | "cpp" | "css" | "sql" | "dart" | "zig" | "phpscript" => Some(("/*", "*/")),
        "hypertext" | "xml" | "markdown" => Some(("<!--", "-->")),
        "lua" => Some(("--[[", "]]")),
        "pascal" => Some(("(*", "*)")),
        "powershell" => Some(("<#", "#>")),
        "ruby" => Some(("=begin", "=end")),
        _ => None,
    }
}

/// Is every non-empty line already commented with `prefix`?
fn all_commented(lines: &[&str], prefix: &str) -> bool {
    let mut non_empty = lines.iter().filter(|l| !l.trim().is_empty()).peekable();
    non_empty.peek().is_some() && non_empty.all(|l| l.trim_start().starts_with(prefix))
}

fn comment_line(l: &str, prefix: &str, indent: usize) -> String {
    if l.trim().is_empty() {
        l.to_string()
    } else {
        format!("{}{} {}", &l[..indent.min(l.len())], prefix, &l[indent.min(l.len())..])
    }
}

fn uncomment_line(l: &str, prefix: &str) -> String {
    let t = l.trim_start();
    match t.strip_prefix(prefix) {
        Some(rest) => format!("{}{}", &l[..l.len() - t.len()], rest.strip_prefix(' ').unwrap_or(rest)),
        None => l.to_string(),
    }
}

fn common_indent(lines: &[&str]) -> usize {
    lines.iter().filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0)
}

/// Comment every non-empty line, or uncomment them if all already are.
pub fn toggle_comment(text: &str, eol: &str, prefix: &str) -> String {
    map_lines(text, eol, |lines| {
        if all_commented(&lines, prefix) {
            lines.iter().map(|l| uncomment_line(l, prefix)).collect()
        } else {
            let indent = common_indent(&lines);
            lines.iter().map(|l| comment_line(l, prefix, indent)).collect()
        }
    })
}

pub fn comment_lines(text: &str, eol: &str, prefix: &str) -> String {
    map_lines(text, eol, |lines| {
        let indent = common_indent(&lines);
        lines.iter().map(|l| comment_line(l, prefix, indent)).collect()
    })
}

pub fn uncomment_lines(text: &str, eol: &str, prefix: &str) -> String {
    map_lines(text, eol, |lines| lines.iter().map(|l| uncomment_line(l, prefix)).collect())
}

pub fn block_comment(text: &str, open: &str, close: &str) -> String {
    format!("{open} {text} {close}")
}

/// Remove one pair of block delimiters if the (trimmed) text is wrapped in them.
pub fn block_uncomment(text: &str, open: &str, close: &str) -> String {
    let t = text.trim();
    match t.strip_prefix(open).and_then(|r| r.strip_suffix(close)) {
        Some(inner) => {
            let inner = inner.strip_prefix(' ').unwrap_or(inner);
            inner.strip_suffix(' ').unwrap_or(inner).to_string()
        }
        None => text.to_string(),
    }
}

// ── column editor ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NumFormat {
    Dec,
    Hex,
    Oct,
    Bin,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ColumnNumbers {
    pub initial: i64,
    pub step: i64,
    /// How many consecutive lines share a number.
    pub repeat: usize,
    pub format: NumFormat,
    pub leading_zeros: bool,
}

/// The numbers for `count` lines, padded to a common width when asked.
pub fn number_sequence(n: &ColumnNumbers, count: usize) -> Vec<String> {
    let repeat = n.repeat.max(1);
    let raw: Vec<String> = (0..count)
        .map(|i| {
            let v = n.initial.wrapping_add(n.step.wrapping_mul((i / repeat) as i64));
            match n.format {
                NumFormat::Dec => v.to_string(),
                NumFormat::Hex => format!("{:X}", v as u64),
                NumFormat::Oct => format!("{:o}", v as u64),
                NumFormat::Bin => format!("{:b}", v as u64),
            }
        })
        .collect();
    if !n.leading_zeros {
        return raw;
    }
    let width = raw.iter().map(|s| s.trim_start_matches('-').len()).max().unwrap_or(0);
    raw.into_iter()
        .map(|s| match s.strip_prefix('-') {
            Some(d) => format!("-{d:0>width$}"),
            None => format!("{s:0>width$}"),
        })
        .collect()
}

/// Insert `insert[i]` at character column `col` of each line, padding short lines.
pub fn insert_at_column(lines: &[&str], col: usize, insert: &[String]) -> Vec<String> {
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let chars: Vec<char> = l.chars().collect();
            let ins = insert.get(i).map(String::as_str).unwrap_or("");
            let head: String = chars.iter().take(col).collect();
            let pad = col.saturating_sub(chars.len());
            let tail: String = chars.iter().skip(col).collect();
            format!("{head}{}{ins}{tail}", " ".repeat(pad))
        })
        .collect()
}

// ── date and time ────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DateTime {
    pub year: i32,
    pub month: u32,   // 1-12
    pub day: u32,     // 1-31
    pub weekday: u32, // 0 = Sunday
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// Format with Windows-style picture letters: yyyy yy MMMM MMM MM M dddd ddd dd d
/// HH H hh h mm m ss s tt t. Text in single quotes is copied as is.
pub fn format_date_time(fmt: &str, t: &DateTime) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                out.push(chars[i]);
                i += 1;
            }
            i += 1;
            continue;
        }
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        let h12 = if t.hour % 12 == 0 { 12 } else { t.hour % 12 };
        let piece = match (c, n) {
            ('y', 1..=2) => format!("{:02}", t.year.rem_euclid(100)),
            ('y', _) => format!("{:04}", t.year),
            ('M', 1) => t.month.to_string(),
            ('M', 2) => format!("{:02}", t.month),
            ('M', 3) => MONTHS[(t.month as usize + 11) % 12][..3].to_string(),
            ('M', _) => MONTHS[(t.month as usize + 11) % 12].to_string(),
            ('d', 1) => t.day.to_string(),
            ('d', 2) => format!("{:02}", t.day),
            ('d', 3) => DAYS[t.weekday as usize % 7][..3].to_string(),
            ('d', _) => DAYS[t.weekday as usize % 7].to_string(),
            ('H', 1) => t.hour.to_string(),
            ('H', _) => format!("{:02}", t.hour),
            ('h', 1) => h12.to_string(),
            ('h', _) => format!("{h12:02}"),
            ('m', 1) => t.minute.to_string(),
            ('m', _) => format!("{:02}", t.minute),
            ('s', 1) => t.second.to_string(),
            ('s', _) => format!("{:02}", t.second),
            ('t', 1) => (if t.hour < 12 { "A" } else { "P" }).to_string(),
            ('t', _) => (if t.hour < 12 { "AM" } else { "PM" }).to_string(),
            _ => c.to_string().repeat(n),
        };
        out.push_str(&piece);
        i += n;
    }
    out
}

// ── web search ───────────────────────────────────────────────────────────────

/// (name, URL with `{q}` where the query goes)
pub const SEARCH_ENGINES: [(&str, &str); 5] = [
    ("Google", "https://www.google.com/search?q={q}"),
    ("Bing", "https://www.bing.com/search?q={q}"),
    ("DuckDuckGo", "https://duckduckgo.com/?q={q}"),
    ("Stack Overflow", "https://stackoverflow.com/search?q={q}"),
    ("GitHub", "https://github.com/search?q={q}"),
];

pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.trim().bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn search_url(engine: usize, query: &str) -> String {
    SEARCH_ENGINES[engine.min(SEARCH_ENGINES.len() - 1)].1.replace("{q}", &url_encode(query))
}

// ── applying to the editor ───────────────────────────────────────────────────

/// What a line operation covers when nothing is selected.
#[derive(Clone, Copy, PartialEq)]
pub enum Scope {
    WholeDocument,
    CurrentLine,
}

fn replace_range<S: Send>(s: &S, start: usize, end: usize, new: &str, select: bool) {
    s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
    s.send(sci::SCI_SETTARGETSTART, start, 0);
    s.send(sci::SCI_SETTARGETEND, end, 0);
    s.send(sci::SCI_REPLACETARGET, new.len(), new.as_ptr() as isize);
    s.send(sci::SCI_ENDUNDOACTION, 0, 0);
    if select {
        s.send(sci::SCI_SETSEL, start, (start + new.len()) as isize);
    }
}

/// Run `f` over the selected whole lines. With no selection it covers the whole
/// document or the caret's line, per `scope`. Returns true if the text changed.
pub fn apply_lines<S: Send>(s: &S, scope: Scope, f: &dyn Fn(&str) -> String) -> bool {
    let a = s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
    let b = s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
    let empty = a == b;
    let (start, end) = if empty && scope == Scope::WholeDocument {
        (0, sci::length(s).max(0) as usize)
    } else {
        let la = s.send(sci::SCI_LINEFROMPOSITION, a, 0) as usize;
        let mut lb = s.send(sci::SCI_LINEFROMPOSITION, b, 0) as usize;
        // A selection that ends at column 0 does not include that line.
        if lb > la && s.send(sci::SCI_POSITIONFROMLINE, lb, 0) as usize == b {
            lb -= 1;
        }
        (s.send(sci::SCI_POSITIONFROMLINE, la, 0) as usize, s.send(sci::SCI_GETLINEENDPOSITION, lb, 0) as usize)
    };
    let text = sci::get_text(s);
    let old = &text[start..end];
    let new = f(old);
    if new == old {
        return false;
    }
    replace_range(s, start, end, &new, !empty);
    if empty && scope == Scope::CurrentLine {
        s.send(sci::SCI_GOTOPOS, start + new.len(), 0);
    }
    true
}

/// Run `f` over exactly the selected text. With no selection it does nothing,
/// unless `line_if_empty` asks for the caret's line instead.
pub fn apply_range<S: Send>(s: &S, line_if_empty: bool, f: &dyn Fn(&str) -> String) -> bool {
    let a = s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
    let b = s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
    let (start, end) = if a != b {
        (a, b)
    } else if line_if_empty {
        let l = s.send(sci::SCI_LINEFROMPOSITION, a, 0) as usize;
        (s.send(sci::SCI_POSITIONFROMLINE, l, 0) as usize, s.send(sci::SCI_GETLINEENDPOSITION, l, 0) as usize)
    } else {
        return false;
    };
    let text = sci::get_text(s);
    let old = &text[start..end];
    let new = f(old);
    if new == old {
        return false;
    }
    replace_range(s, start, end, &new, true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_and_keeps_trailing_newline() {
        assert_eq!(sort_lines("b\r\na\r\nc\r\n", "\r\n", false), "a\r\nb\r\nc\r\n");
        assert_eq!(sort_lines("b\na\nc", "\n", true), "c\nb\na");
    }

    #[test]
    fn sort_kinds() {
        assert_eq!(sort_lines_by("b\nA\nc", "\n", SortKind::IgnoreCase, false), "A\nb\nc");
        assert_eq!(sort_lines_by("10\n9\n100\nx", "\n", SortKind::Integer, false), "x\n9\n10\n100");
        assert_eq!(sort_lines_by("1.5\n1.25\n-2", "\n", SortKind::DecimalDot, true), "1.5\n1.25\n-2");
        assert_eq!(sort_lines_by("1,5\n1,25\n-2", "\n", SortKind::DecimalComma, false), "-2\n1,25\n1,5");
    }

    #[test]
    fn dedupes_keeping_first() {
        assert_eq!(remove_duplicate_lines("a\nb\na\nc\nb", "\n"), "a\nb\nc");
        assert_eq!(remove_consecutive_duplicates("a\na\nb\na\na", "\n"), "a\nb\na");
    }

    #[test]
    fn empty_versus_blank_lines() {
        assert_eq!(remove_empty_lines("a\n\n  \nb\n", "\n"), "a\n  \nb\n");
        assert_eq!(remove_blank_lines("a\n\n  \nb\n", "\n"), "a\nb\n");
    }

    #[test]
    fn reverse_shuffle_join_split() {
        assert_eq!(reverse_lines("1\n2\n3", "\n"), "3\n2\n1");
        let shuffled = randomize_lines("1\n2\n3\n4\n5\n6", "\n", 42);
        let mut v: Vec<&str> = shuffled.split('\n').collect();
        v.sort();
        assert_eq!(v, ["1", "2", "3", "4", "5", "6"]);
        assert_eq!(join_lines("a\nb\r\nc\n"), "a b c");
        assert_eq!(split_lines("aaa bbb ccc ddd", "\n", 7), "aaa bbb\nccc ddd");
        assert_eq!(split_lines("abcdefghij", "\n", 4), "abcd\nefgh\nij");
    }

    #[test]
    fn blank_operations() {
        assert_eq!(trim_trailing("a  \nb\t\nc", "\n"), "a\nb\nc");
        assert_eq!(trim_leading("  a\n\tb", "\n"), "a\nb");
        assert_eq!(trim_both(" a \n b ", "\n"), "a\nb");
        assert_eq!(remove_unnecessary_blanks(" a \n\n  \n b", "\n"), "a\nb");
        assert_eq!(tabs_to_spaces("a\tb\n\tc", "\n", 4), "a   b\n    c");
        assert_eq!(spaces_to_tabs("    a    b", "\n", 4, false), "\ta\t b");
        assert_eq!(spaces_to_tabs("    a    b", "\n", 4, true), "\ta    b");
        assert_eq!(spaces_to_tabs("a b", "\n", 4, false), "a b");
    }

    #[test]
    fn case_conversions() {
        assert_eq!(proper_case("hello wORLD it's", false), "Hello World It's");
        assert_eq!(proper_case("hello wORLD", true), "Hello WORLD");
        assert_eq!(sentence_case("hELLO there. how ARE you? fine", false), "Hello there. How are you? Fine");
        assert_eq!(sentence_case("hELLO. wORLD", true), "HELLO. WORLD");
        assert_eq!(invert_case("aBc1"), "AbC1");
        let r = random_case("hello world", 7);
        assert_eq!(r.to_lowercase(), "hello world");
        assert_eq!(random_case("hello world", 7), r);
    }

    #[test]
    fn comment_toggle_round_trips() {
        let src = "fn a() {\n    x();\n\n    y();\n}";
        let commented = toggle_comment(src, "\n", "//");
        assert_eq!(commented, "// fn a() {\n//     x();\n\n//     y();\n// }");
        assert_eq!(toggle_comment(&commented, "\n", "//"), src);
    }

    #[test]
    fn comment_respects_common_indent() {
        assert_eq!(toggle_comment("    a\n      b", "\n", "#"), "    # a\n    #   b");
    }

    #[test]
    fn forced_comment_and_uncomment() {
        assert_eq!(comment_lines("# a\nb", "\n", "#"), "# # a\n# b");
        assert_eq!(uncomment_lines("# a\nb", "\n", "#"), "a\nb");
    }

    #[test]
    fn block_comments() {
        assert_eq!(block_comment("x = 1;", "/*", "*/"), "/* x = 1; */");
        assert_eq!(block_uncomment("/* x = 1; */", "/*", "*/"), "x = 1;");
        assert_eq!(block_uncomment("x = 1;", "/*", "*/"), "x = 1;");
        assert_eq!(comment_block("python"), None);
        assert_eq!(comment_block("cpp"), Some(("/*", "*/")));
    }

    #[test]
    fn number_sequences() {
        let n = ColumnNumbers { initial: 1, step: 1, repeat: 1, format: NumFormat::Dec, leading_zeros: false };
        assert_eq!(number_sequence(&n, 3), ["1", "2", "3"]);
        let n = ColumnNumbers { initial: 8, step: 2, repeat: 2, format: NumFormat::Dec, leading_zeros: true };
        assert_eq!(number_sequence(&n, 5), ["08", "08", "10", "10", "12"]);
        let n = ColumnNumbers { initial: 254, step: 1, repeat: 1, format: NumFormat::Hex, leading_zeros: true };
        assert_eq!(number_sequence(&n, 3), ["0FE", "0FF", "100"]);
        let n = ColumnNumbers { initial: 5, step: 1, repeat: 1, format: NumFormat::Bin, leading_zeros: false };
        assert_eq!(number_sequence(&n, 2), ["101", "110"]);
        let n = ColumnNumbers { initial: 7, step: 1, repeat: 1, format: NumFormat::Oct, leading_zeros: false };
        assert_eq!(number_sequence(&n, 2), ["7", "10"]);
    }

    #[test]
    fn column_insert_pads_short_lines() {
        let ins = vec!["X".to_string(), "Y".to_string(), "Z".to_string()];
        assert_eq!(insert_at_column(&["abcd", "ab", ""], 3, &ins), ["abcXd", "ab Y", "   Z"]);
    }

    #[test]
    fn date_time_formats() {
        let t = DateTime { year: 2026, month: 3, day: 7, weekday: 6, hour: 15, minute: 4, second: 9 };
        assert_eq!(format_date_time("yyyy-MM-dd HH:mm:ss", &t), "2026-03-07 15:04:09");
        assert_eq!(format_date_time("dddd, d MMMM yy", &t), "Saturday, 7 March 26");
        assert_eq!(format_date_time("h:mm tt", &t), "3:04 PM");
        assert_eq!(format_date_time("ddd MMM 'at' H", &t), "Sat Mar at 15");
    }

    #[test]
    fn search_urls_are_encoded() {
        assert_eq!(url_encode("a b&c/é"), "a+b%26c%2F%C3%A9");
        assert_eq!(search_url(0, "rust lang"), "https://www.google.com/search?q=rust+lang");
        assert_eq!(search_url(99, "x"), "https://github.com/search?q=x");
    }

    #[test]
    fn blank_last_line_survives_a_sort() {
        assert_eq!(sort_lines("b\na\n\n", "\n", false), "\na\nb\n");
        assert_eq!(sort_lines("\n", "\n", false), "\n");
    }

    #[test]
    fn empty_input_is_untouched() {
        assert_eq!(sort_lines("", "\n", false), "");
        assert_eq!(toggle_comment("", "\n", "//"), "");
    }

    #[test]
    fn prefixes() {
        assert_eq!(comment_prefix("python"), Some("#"));
        assert_eq!(comment_prefix("rust"), Some("//"));
        assert_eq!(comment_prefix("null"), None);
    }
}
