//! Text and line operations: sort, de-duplicate, trim, comment toggling.
//!
//! The transforms are pure functions over a block of text (and are unit
//! tested); `apply_lines` applies one to the selected lines, or the whole
//! document when nothing is selected, as a single undo step.

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

pub fn sort_lines(text: &str, eol: &str, descending: bool) -> String {
    map_lines(text, eol, |mut l| {
        l.sort();
        if descending {
            l.reverse();
        }
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

pub fn remove_empty_lines(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().filter(|x| !x.trim().is_empty()).map(String::from).collect())
}

pub fn trim_trailing(text: &str, eol: &str) -> String {
    map_lines(text, eol, |l| l.into_iter().map(|x| x.trim_end().to_string()).collect())
}

/// Line-comment prefix for a Lexilla lexer name.
pub fn comment_prefix(lexer: &str) -> Option<&'static str> {
    match lexer {
        "rust" | "cpp" | "json" => Some("//"),
        "python" | "yaml" | "toml" | "bash" | "props" | "powershell" | "ruby" | "perl" | "r" => Some("#"),
        "sql" | "lua" => Some("--"),
        "batch" => Some("REM"),
        _ => None,
    }
}

/// Comment every non-empty line, or uncomment them if all already are.
pub fn toggle_comment(text: &str, eol: &str, prefix: &str) -> String {
    map_lines(text, eol, |lines| {
        let non_empty: Vec<&&str> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
        let all_commented = !non_empty.is_empty() && non_empty.iter().all(|l| l.trim_start().starts_with(prefix));
        let indent = non_empty.iter().map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        lines
            .iter()
            .map(|l| {
                if l.trim().is_empty() {
                    l.to_string()
                } else if all_commented {
                    let ws = l.len() - l.trim_start().len();
                    let rest = &l.trim_start()[prefix.len()..];
                    format!("{}{}", &l[..ws], rest.strip_prefix(' ').unwrap_or(rest))
                } else {
                    format!("{}{} {}", &l[..indent], prefix, &l[indent..])
                }
            })
            .collect()
    })
}

/// Run `f` over the selected whole lines (or the whole document if there is no
/// selection). Returns true if the text changed.
pub fn apply_lines<S: Send>(s: &S, f: &dyn Fn(&str) -> String) -> bool {
    let a = s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
    let b = s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
    let whole = a == b;
    let (start, end) = if whole {
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
    s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
    s.send(sci::SCI_SETTARGETSTART, start, 0);
    s.send(sci::SCI_SETTARGETEND, end, 0);
    s.send(sci::SCI_REPLACETARGET, new.len(), new.as_ptr() as isize);
    s.send(sci::SCI_ENDUNDOACTION, 0, 0);
    if !whole {
        s.send(sci::SCI_SETSEL, start, (start + new.len()) as isize);
    }
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
    fn dedupes_keeping_first() {
        assert_eq!(remove_duplicate_lines("a\nb\na\nc\nb", "\n"), "a\nb\nc");
    }

    #[test]
    fn removes_empty_and_blank_lines() {
        assert_eq!(remove_empty_lines("a\n\n  \nb\n", "\n"), "a\nb\n");
    }

    #[test]
    fn trims_trailing_whitespace() {
        assert_eq!(trim_trailing("a  \nb\t\nc", "\n"), "a\nb\nc");
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
