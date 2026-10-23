//! Per-language lexer set-up: keywords and colours.
//!
//! Lexilla lexers number their styles independently, so each language needs
//! its own palette. Palettes are applied to the *view* (styles are not stored
//! per document), so they are re-applied whenever the active tab changes.

use super::lexer;
use super::sci::{self, Send, Theme, rgb};
use std::ffi::CString;

type Rgb = (u8, u8, u8);

pub fn palette(lexer: &str, t: &Theme) -> Vec<(usize, Rgb)> {
    let (c, n, k, s, ty, m, f, p) = (t.comment, t.number, t.keyword, t.string, t.ty, t.mac, t.func, t.prop);
    match lexer {
        // SCE_RUST_*
        "rust" => vec![
            (1, c), (2, c), (3, c), (4, c),
            (5, n), (6, k), (7, ty), (8, ty),
            (13, s), (14, s), (15, s), (18, ty), (19, m),
            (21, s), (22, s), (23, s),
        ],
        // SCE_C_*
        "cpp" => vec![
            (1, c), (2, c), (3, c), (15, c),
            (4, n), (5, k), (16, ty), (6, s), (7, s),
            (13, s), (14, s), (9, m),
        ],
        // SCE_P_*
        "python" => vec![
            (1, c), (12, c), (2, n), (3, s), (4, s),
            (6, s), (7, s), (16, s), (5, k), (14, ty),
            (8, ty), (9, f), (15, m),
        ],
        // SCE_JSON_*
        "json" => vec![(1, n), (2, s), (4, p), (6, c), (7, c), (11, k), (12, k)],
        // SCE_YAML_*
        "yaml" => vec![(1, c), (3, k), (4, n), (5, m), (6, m)],
        // SCE_MARKDOWN_*
        "markdown" => vec![(6, k), (7, k), (8, k), (9, k), (10, k), (11, k), (13, m), (14, m), (15, c), (17, c), (18, p), (19, s), (20, s), (21, s)],
        // SCE_H_* (HTML and XML share these)
        "hypertext" | "xml" => vec![(1, k), (2, k), (11, k), (3, p), (4, p), (5, n), (6, s), (7, s), (9, c), (10, m)],
        // SCE_CSS_*
        "css" => vec![(1, k), (2, ty), (3, m), (4, m), (6, p), (7, p), (8, s), (9, c), (10, m), (11, k), (12, k), (13, s), (14, s), (15, p), (16, p)],
        _ => Vec::new(),
    }
}

const RUST_KW: &str = "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while";
const RUST_TYPES: &str = "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec Option Result Box Rc Arc HashMap HashSet";
const CPP_KW: &str = "alignas alignof auto bool break case catch char class const constexpr continue default delete do double else enum explicit export extern false final float for friend goto if inline int long mutable namespace new noexcept nullptr operator override private protected public register return short signed sizeof static struct switch template this throw true try typedef typename union unsigned using virtual void volatile while function var let async await import from yield of in instanceof typeof null undefined interface implements extends package func go defer chan fallthrough";
const CPP_TYPES: &str = "int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t size_t string vector map set";
const PY_KW: &str = "False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield";

pub fn keyword_sets(lexer: &str) -> &'static [&'static str] {
    match lexer {
        "rust" => &[RUST_KW, RUST_TYPES],
        "cpp" => &[CPP_KW, CPP_TYPES],
        "python" => &[PY_KW],
        "json" => &["true false null"],
        "yaml" => &["true false yes no on off null"],
        _ => &[],
    }
}

pub fn display_name(lexer: &str) -> String {
    match lexer {
        "null" => "Normal text".into(),
        "cpp" => "C / C++ / JS".into(),
        "json" => "JSON".into(),
        "yaml" => "YAML".into(),
        "xml" => "XML".into(),
        "hypertext" => "HTML".into(),
        "css" => "CSS".into(),
        "sql" => "SQL".into(),
        "toml" => "TOML".into(),
        "powershell" => "PowerShell".into(),
        other => {
            let mut c = other.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

/// Attach the lexer for `name` to the current document and enable folding.
pub fn apply_lexer<S: Send>(sci: &S, name: &str, theme: &Theme) {
    let lex = if name == "null" { std::ptr::null_mut() } else { lexer::create(name) };
    sci.send(sci::SCI_SETILEXER, 0, lex as isize);
    for (i, kw) in keyword_sets(name).iter().enumerate() {
        let c = CString::new(*kw).unwrap();
        sci.send(sci::SCI_SETKEYWORDS, i, c.as_ptr() as isize);
    }
    sci::setup_folding(sci, theme);
}

/// Colour the lexer styles for `name`, resetting every other style to plain.
pub fn apply_palette<S: Send>(sci: &S, name: &str, theme: &Theme) {
    for style in 0..32usize {
        sci.send(sci::SCI_STYLESETFORE, style, rgb(theme.fg.0, theme.fg.1, theme.fg.2));
    }
    for (style, c) in palette(name, theme) {
        sci.send(sci::SCI_STYLESETFORE, style, rgb(c.0, c.1, c.2));
    }
}

/// Languages offered in the Language menu: (menu label, Lexilla name).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("Normal text", "null"),
    ("C / C++ / JS / Java / Go", "cpp"),
    ("CSS", "css"),
    ("HTML", "hypertext"),
    ("JSON", "json"),
    ("Markdown", "markdown"),
    ("Python", "python"),
    ("Rust", "rust"),
    ("XML", "xml"),
    ("YAML", "yaml"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palettes_stay_inside_lexer_style_range() {
        for l in ["rust", "cpp", "python", "json", "yaml", "markdown", "hypertext", "xml", "css"] {
            assert!(palette(l, &Theme::light()).iter().all(|(s, _)| *s < 32), "{l}");
        }
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("null"), "Normal text");
        assert_eq!(display_name("rust"), "Rust");
        assert_eq!(display_name("json"), "JSON");
    }
}
