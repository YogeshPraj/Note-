//! Comment syntax per language, single-line and stream comments.

use super::*;

// ── comments ─────────────────────────────────────────────────────────────────

/// The comment syntax of a language, from the usual language tables.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Comments {
    pub line: &'static str,
    pub start: &'static str,
    pub end: &'static str,
}

pub(super) const fn cm(line: &'static str, start: &'static str, end: &'static str) -> Comments {
    Comments { line, start, end }
}

/// Comment syntax by Language-menu label.
pub(super) const COMMENTS_BY_LABEL: &[(&str, Comments)] = &[
    ("ActionScript", cm("//", "/*", "*/")),
    ("Ada", cm("--", "", "")),
    ("Assembly", cm(";", "", "")),
    ("ASP", cm("'", "", "")),
    ("AutoIt", cm(";", "#CS", "#CE")),
    ("AviSynth", cm("#", "/*", "*/")),
    ("Batch", cm("REM", "", "")),
    ("C", cm("//", "/*", "*/")),
    ("C#", cm("//", "/*", "*/")),
    ("C++", cm("//", "/*", "*/")),
    ("Caml", cm("", "(*", "*)")),
    ("CMake", cm("#", "", "")),
    ("COBOL", cm("*>", "", "")),
    ("CoffeeScript", cm("#", "", "")),
    ("Csound", cm(";", "", "")),
    ("CSS", cm("", "/*", "*/")),
    ("D", cm("//", "/*", "*/")),
    ("ErrorList", cm("#", "", "")),
    ("Erlang", cm("%", "", "")),
    ("Forth", cm("\\", "(", ")")),
    ("Fortran (free form)", cm("!", "", "")),
    ("Fortran (fixed form)", cm("C", "", "")),
    ("GDScript", cm("#", "", "")),
    ("Go", cm("//", "/*", "*/")),
    ("Gui4Cli", cm("//", "", "")),
    ("Haskell", cm("--", "", "")),
    ("Hollywood", cm(";", "/*", "*/")),
    ("HTML", cm("", "<!--", "-->")),
    ("ini file", cm(";", "", "")),
    ("Inno Setup", cm(";", "", "")),
    ("Java", cm("//", "/*", "*/")),
    ("JavaScript", cm("//", "/*", "*/")),
    ("JSP", cm("//", "/*", "*/")),
    ("KIXtart", cm(";", "", "")),
    ("LaTeX", cm("%", "", "")),
    ("LISP", cm(";", "", "")),
    ("Lua", cm("--", "", "")),
    ("Makefile", cm("#", "", "")),
    ("Markdown", cm("", "<!--", "-->")),
    ("Matlab", cm("%", "", "")),
    ("MMIXAL", cm("%", "", "")),
    ("Nim", cm("#", "", "")),
    ("NSIS", cm(";", "/*", "*/")),
    ("Objective-C", cm("//", "/*", "*/")),
    ("OScript", cm("//", "/*", "*/")),
    ("Pascal", cm("", "{", "}")),
    ("Perl", cm("#", "", "")),
    ("PHP", cm("//", "/*", "*/")),
    ("PostScript", cm("%", "", "")),
    ("PowerShell", cm("#", "<#", "#>")),
    ("Properties file", cm("#", "", "")),
    ("Python", cm("#", "", "")),
    ("R", cm("#", "", "")),
    ("REBOL", cm(";", "", "")),
    ("Registry", cm(";;", "", "")),
    ("Resource file", cm("//", "/*", "*/")),
    ("Ruby", cm("#", "", "")),
    ("Rust", cm("//", "/*", "*/")),
    ("Scheme", cm(";", "", "")),
    ("Shell", cm("#", "", "")),
    ("Smalltalk", cm("", "\"", "\"")),
    ("Spice", cm("*", "", "")),
    ("SQL", cm("--", "/*", "*/")),
    ("Swift", cm("//", "/*", "*/")),
    ("TCL", cm("#", "", "")),
    ("TeX", cm("%", "", "")),
    ("TOML", cm("#", "", "")),
    ("TypeScript", cm("//", "/*", "*/")),
    ("Verilog", cm("//", "/*", "*/")),
    ("VHDL", cm("--", "/*", "*/")),
    ("Visual Basic", cm("'", "", "")),
    ("Visual Prolog", cm("%", "/*", "*/")),
    ("XML", cm("", "<!--", "-->")),
    ("YAML", cm("#", "", "")),
];

/// Languages with no entry here (or ours differ): fall back on the Lexilla lexer.
pub(super) fn comments_by_lexer(lexer: &str) -> Comments {
    match lexer {
        // Plain text has no syntax of its own; `//` is what people expect when they ask for a comment there.
        "null" => cm("//", "", ""),
        "julia" | "nix" | "raku" => cm("#", "", ""),
        "zig" | "dart" => cm("//", "/*", "*/"),
        "fsharp" => cm("//", "(*", "*)"),
        "freebasic" | "blitzbasic" | "purebasic" => cm("'", "", ""),
        _ => Comments::default(),
    }
}

/// The comment syntax for a Language-menu label (and its lexer).
pub fn comment_syntax(label: &str, lexer: &str) -> Comments {
    COMMENTS_BY_LABEL.iter().find(|(l, _)| *l == label).map_or_else(|| comments_by_lexer(lexer), |(_, c)| *c)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CommentMode {
    Toggle,
    Comment,
    Uncomment,
}

pub(super) fn starts_with_ci(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.is_char_boundary(prefix.len()) && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

pub(super) fn ends_with_ci(s: &str, suffix: &str) -> bool {
    s.len() >= suffix.len() && s.is_char_boundary(s.len() - suffix.len()) && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

/// Single-line comment / uncomment / toggle over whole lines, the usual way: every
/// line is handled on its own, at its own indentation, and blank lines are left alone. Languages
/// without a line-comment symbol wrap each line in the start and end symbols instead.
/// `column_zero` is for languages whose comments must start in the first column (Fortran 77).
/// Returns `None` when the language has no comment syntax at all.
pub fn comment_single(text: &str, eol: &str, syn: Comments, mode: CommentMode, column_zero: bool) -> Option<String> {
    let advanced = syn.line.is_empty();
    if advanced && (syn.start.is_empty() || syn.end.is_empty()) {
        return None;
    }
    let (open, close) = (format!("{} ", syn.start), format!(" {}", syn.end));
    let sym = format!("{} ", syn.line);
    Some(map_lines(text, eol, |lines| {
        lines
            .into_iter()
            .map(|line| {
                let at = if column_zero { 0 } else { line.len() - line.trim_start_matches(BLANKS).len() };
                let (head, body) = line.split_at(at);
                if line.trim_matches(BLANKS).is_empty() {
                    return line.to_string();
                }
                if mode != CommentMode::Comment {
                    if !advanced {
                        if starts_with_ci(body, syn.line) {
                            let rest = &body[syn.line.len()..];
                            return format!("{head}{}", rest.strip_prefix(' ').unwrap_or(rest));
                        }
                    } else if starts_with_ci(body, syn.start) && ends_with_ci(body, syn.end) && body.len() >= syn.start.len() + syn.end.len() {
                        let inner = &body[syn.start.len()..body.len() - syn.end.len()];
                        let inner = inner.strip_prefix(' ').unwrap_or(inner);
                        return format!("{head}{}", inner.strip_suffix(' ').unwrap_or(inner));
                    }
                }
                if mode == CommentMode::Uncomment {
                    return line.to_string();
                }
                if advanced { format!("{head}{open}{body}{close}") } else { format!("{head}{sym}{body}") }
            })
            .collect()
    }))
}

/// "Block Comment": wrap `text` in the start and end symbols, with a blank inside each.
pub fn stream_comment(text: &str, start: &str, end: &str) -> String {
    format!("{start} {text} {end}")
}

/// "Block Uncomment": find the comment around (or inside) the selection `sel` and remove its
/// delimiters and the one blank inside each. Returns the byte range to replace and its new text.
pub fn undo_stream_comment(text: &str, sel: (usize, usize), start: &str, end: &str) -> Option<(usize, usize, String)> {
    let around = |pos: usize| -> Option<(usize, usize)> {
        // The start symbol may begin at the caret itself, so look that far ahead (on a character boundary).
        let mut upto = (pos + start.len()).min(text.len());
        while !text.is_char_boundary(upto) {
            upto += 1;
        }
        let s = text[..upto].rfind(start)?;
        // The end symbol must come after the start symbol (they can be the same, as with quotes).
        let from = pos.max(s + start.len());
        let e = from + text[from..].find(end)?;
        let end_before = text[..pos].rfind(end);
        let start_after = text[from..].find(start).map(|i| from + i);
        (end_before.is_none_or(|x| s >= x) && start_after.is_none_or(|x| e <= x)).then_some((s, e))
    };
    let (s, e) = around(sel.0).or_else(|| around(sel.1)).or_else(|| {
        let s = sel.0 + text[sel.0..].find(start)?;
        let e = s + start.len() + text[s + start.len()..].find(end)?;
        (s < sel.1 && e > sel.0).then_some((s, e))
    })?;
    let inner = &text[s + start.len()..e];
    let inner = inner.strip_prefix(' ').unwrap_or(inner);
    let inner = inner.strip_suffix(' ').unwrap_or(inner);
    Some((s, e + end.len(), inner.to_string()))
}
