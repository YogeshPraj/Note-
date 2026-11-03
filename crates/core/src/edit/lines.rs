//! Sorting and the other whole-line operations (dedupe, reverse, shuffle, join, split).

use super::*;

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
pub(super) fn leading_number(line: &str, comma: bool, integer: bool) -> Option<f64> {
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

/// Two numbers of the same sign start at `ai` and `bi` (after the sign): compare them by value, however long.
pub(super) fn compare_numbers(a: &[char], b: &[char], ai: &mut usize, bi: &mut usize, sign: i64) -> i64 {
    let digit = |c: char| c.is_ascii_digit();
    if sign == -1 {
        *ai += 1;
        *bi += 1;
    }
    let end_of = |v: &[char], from: usize| v[from..].iter().position(|c| !digit(*c)).map_or(v.len(), |p| from + p);
    let (a_end, b_end) = (end_of(a, *ai), end_of(b, *bi));
    let (mut a_zeros, mut b_zeros) = (0i64, 0i64);
    while *ai < a.len() && a[*ai] == '0' {
        a_zeros += 1;
        *ai += 1;
    }
    while *bi < b.len() && b[*bi] == '0' {
        b_zeros += 1;
        *bi += 1;
    }
    let (a_len, b_len) = (a_end.saturating_sub(*ai), b_end.saturating_sub(*bi));
    if a_len != b_len {
        return if a_len > b_len { sign } else { -sign };
    }
    let mut cmp = 0;
    while cmp == 0 && *ai < a.len() && digit(a[*ai]) && *bi < b.len() && digit(b[*bi]) {
        cmp = (a[*ai] as i64 - b[*bi] as i64) * sign;
        *ai += 1;
        *bi += 1;
    }
    if cmp == 0 { b_zeros - a_zeros } else { cmp }
}

/// Two text chunks (up to the next digit or minus sign): their order, and where each ends.
pub(super) fn compare_text_chunk(a: &[char], b: &[char], mut ai: usize, mut bi: usize) -> (i64, usize, usize) {
    let digit = |c: char| c.is_ascii_digit();
    if a[ai] == '-' {
        ai += 1;
    }
    if b[bi] == '-' {
        bi += 1;
    }
    let chunk_end = |v: &[char], from: usize| v[from.min(v.len())..].iter().position(|c| digit(*c) || *c == '-').map_or(v.len(), |p| from.min(v.len()) + p);
    let (a_end, b_end) = (chunk_end(a, ai), chunk_end(b, bi));
    let c = match a[ai.min(a.len())..a_end].cmp(&b[bi.min(b.len())..b_end]) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    };
    (c, a_end, b_end)
}

/// "Sort Lines As Integers": a natural sort. Lines are compared chunk by chunk;
/// runs of digits (with an optional leading minus) compare by value however long they are, and
/// the rest compares as text, so "item2" sorts before "item10".
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let digit = |c: char| c.is_ascii_digit();
    let (mut ai, mut bi) = (0usize, 0usize);
    let mut cmp: i64 = 0;
    while cmp == 0 {
        if ai >= a.len() || bi >= b.len() {
            cmp = match a[ai.min(a.len())..].cmp(&b[bi.min(b.len())..]) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            };
            break;
        }
        let (mut a_num, mut b_num) = (digit(a[ai]), digit(b[bi]));
        let (mut a_sign, mut b_sign) = (1i64, 1i64);
        // A minus sign right before a digit starts a negative number.
        if !a_num && ai + 1 < a.len() {
            a_num = a[ai] == '-' && digit(a[ai + 1]);
            a_sign = -1;
        }
        if !b_num && bi + 1 < b.len() {
            b_num = b[bi] == '-' && digit(b[bi + 1]);
            b_sign = -1;
        }
        if a_num != b_num {
            // A number against text: compare the first characters, then go on.
            cmp = a[ai] as i64 - b[bi] as i64;
            ai += 1;
            bi += 1;
        } else if a_num {
            if a_sign == b_sign {
                cmp = compare_numbers(&a, &b, &mut ai, &mut bi, a_sign);
            } else {
                cmp = if a_sign == 1 { 1 } else { -1 };
            }
        } else {
            // Both are text: up to the next digit or minus sign.
            let (c, a_end, b_end) = compare_text_chunk(&a, &b, ai, bi);
            cmp = c;
            ai = a_end;
            bi = b_end;
        }
    }
    cmp.cmp(&0)
}

pub fn sort_lines_by(text: &str, eol: &str, kind: SortKind, descending: bool) -> String {
    map_lines(text, eol, |mut l| {
        // Descending is a reversed comparison, so lines that compare equal keep their order.
        let dir = |o: std::cmp::Ordering| if descending { o.reverse() } else { o };
        match kind {
            SortKind::Lexicographic => l.sort_by(|a, b| dir(a.cmp(b))),
            SortKind::IgnoreCase => l.sort_by(|a, b| dir(a.to_lowercase().cmp(&b.to_lowercase()))),
            SortKind::Integer => l.sort_by(|a, b| dir(natural_cmp(a, b))),
            SortKind::DecimalDot | SortKind::DecimalComma => {
                let comma = kind == SortKind::DecimalComma;
                let integer = false;
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
pub(super) fn sort_lines(text: &str, eol: &str, descending: bool) -> String {
    sort_lines_by(text, eol, SortKind::Lexicographic, descending)
}

/// Small xorshift generator: shuffling and random case do not need a crate.
pub fn seed_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x9E37_79B9_7F4A_7C15, |d| crate::num::to_u64(d.as_nanos())) | 1
}

pub(super) fn next_rand(state: &mut u64) -> u64 {
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
            let j = crate::num::to_usize(next_rand(&mut st) % (i as u64 + 1));
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

/// Join lines like Scintilla's Lines Join: one blank between them, unless the earlier line
/// already ends in one.
pub fn join_lines(text: &str) -> String {
    let (lines, _) = split(text);
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(l);
    }
    out
}

/// Scintilla's "Lines Join" over a whole block, as the "EOL to Space" command uses it: every line
/// break becomes one blank, unless the text before it already ends in one.
pub fn join_all(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            _ => out.push(c),
        }
    }
    out
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
                if let Some(p) = cut {
                    out.push(rest[..p].iter().collect());
                    rest.drain(..=p);
                } else {
                    out.push(rest[..width].iter().collect());
                    rest.drain(..width);
                }
            }
            out.push(rest.into_iter().collect());
        }
        out
    })
}
