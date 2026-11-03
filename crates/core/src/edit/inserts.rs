//! The Column Editor's numbers, Insert > Date Time, and search-engine addresses.

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
            let v = n.initial.wrapping_add(n.step.wrapping_mul(crate::num::to_i64(i / repeat)));
            match n.format {
                NumFormat::Dec => v.to_string(),
                NumFormat::Hex => format!("{:X}", crate::num::to_u64(v)),
                NumFormat::Oct => format!("{:o}", crate::num::to_u64(v)),
                NumFormat::Bin => format!("{:b}", crate::num::to_u64(v)),
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
            let ins = insert.get(i).map_or("", String::as_str);
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

pub(super) const MONTHS: [&str; 12] =
    ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
pub(super) const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

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
        let h12 = if t.hour.is_multiple_of(12) { 12 } else { t.hour % 12 };
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
