//! Proper, sentence, inverted and random case.

use super::*;

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
            if !((c == '\'' || c == '\u{2019}') && !start) {
                start = true;
            }
        }
    }
    out
}

/// Capitalise the first letter of each sentence: a sentence ends at ``.`` ``!``
/// ``?`` followed by a non-letter, and at a blank line; a lone "i" becomes "I". With ``blend`` the
/// other letters keep their case.
pub fn sentence_case(s: &str, blend: bool) -> String {
    // One character in, one character out (no "ß" -> "SS"), so positions stay put.
    fn one(mut it: impl Iterator<Item = char>, same: char) -> char {
        match (it.next(), it.next()) {
            (Some(c), None) => c,
            _ => same,
        }
    }
    let mut out: Vec<char> = s.chars().collect();
    let n = out.len();
    let (mut new_sentence, mut was_r, mut was_n) = (true, false, false);
    for i in 0..n {
        let c = out[i];
        if c.is_alphabetic() {
            if new_sentence {
                out[i] = one(c.to_uppercase(), c);
                new_sentence = false;
            } else if !blend {
                out[i] = one(c.to_lowercase(), c);
            }
            (was_r, was_n) = (false, false);
            let before_ok = i > 0 && (out[i - 1].is_whitespace() || out[i - 1] == '(' || out[i - 1] == '"');
            let after_ok = i + 1 < n && (out[i + 1].is_whitespace() || out[i + 1] == '\'');
            if out[i] == 'i' && before_ok && after_ok {
                out[i] = 'I';
            }
        } else if matches!(c, '.' | '!' | '?') {
            new_sentence = !(i + 1 == n || out[i + 1].is_alphanumeric());
        } else if c == '\r' {
            if was_r {
                new_sentence = true;
            }
            was_r = true;
        } else if c == '\n' {
            if was_n {
                new_sentence = true;
            }
            was_n = true;
        }
    }
    out.into_iter().collect()
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
