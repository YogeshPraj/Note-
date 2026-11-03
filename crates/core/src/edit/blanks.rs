//! Trimming blanks and converting between tabs and spaces.

use super::*;

// ── blanks ───────────────────────────────────────────────────────────────────

/// Only spaces and tabs count as blanks (not no-break or ideographic spaces).
pub(super) const BLANKS: [char; 2] = [' ', '\t'];

pub fn trim_trailing(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_end_matches(BLANKS).to_string()).collect())
}

pub fn trim_leading(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_start_matches(BLANKS).to_string()).collect())
}

pub fn trim_both(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_matches(BLANKS).to_string()).collect())
}

/// "Remove Unnecessary Blank and EOL": trim every line, then join them all into one.
pub fn remove_unnecessary_blanks(text: &str, eol: &str) -> String {
    join_all(&trim_both(text, eol))
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
