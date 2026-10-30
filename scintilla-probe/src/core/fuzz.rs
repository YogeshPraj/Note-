//! Randomised robustness test: every pure text function must survive odd input (multi-byte
//! characters, lone CRs, empty strings, very long lines) without panicking.

use super::edit::*;
use super::mdformat::{self, Fmt};
use super::{functions, preview, search, textools, view};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

const PIECES: &[&str] = &[
    "a", "B", "hello", " ", "  ", "\t", "\r\n", "\n", "\r", "é", "日本", "😀", "# ", "## x", "- ", "* ", "1. ", "[x] ", "> ", "```", "|", "---", "**", "*", "_",
    "~~", "`", "[", "](", ")", "(", "{", "}", "/*", "*/", "//", "<!--", "-->", "&", "<b>", "</b>", "=", "'", "\"", "0", "-12", "3.5", "\u{a0}", "\u{3000}", "ß", "İ",
];

fn random_text(r: &mut Rng) -> String {
    let n = r.below(40);
    (0..n).map(|_| PIECES[r.below(PIECES.len())]).collect()
}

fn on_boundaries(s: &str, r: &mut Rng) -> (usize, usize) {
    let cuts: Vec<usize> = s.char_indices().map(|(i, _)| i).chain([s.len()]).collect();
    let a = cuts[r.below(cuts.len())];
    let b = cuts[r.below(cuts.len())];
    (a.min(b), a.max(b))
}

static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

#[test]
fn pure_functions_never_panic() {
    std::panic::set_hook(Box::new(|info| {
        let at = info.location().map_or(String::new(), |l| format!("{}:{}", l.file(), l.line()));
        let msg = info.payload().downcast_ref::<String>().cloned().or_else(|| info.payload().downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
        if let Ok(mut g) = LAST.lock() {
            *g = format!("{at}: {msg}");
        }
    }));
    let mut r = Rng(0x1234_5678_9ABC_DEF1);
    let mut failures: Vec<String> = Vec::new();
    for round in 0..150 {
        let text = random_text(&mut r);
        let eol = ["\n", "\r\n", "\r"][r.below(3)];
        let sel = on_boundaries(&text, &mut r);
        let width = r.below(12);
        let tab = r.below(9);
        let syn = comment_syntax(["Python", "C++", "CSS", "HTML", "Batch", "Fortran (fixed form)", "Normal text"][r.below(7)], "");
        let mut check = |name: &str, f: &mut dyn FnMut()| {
            if catch_unwind(AssertUnwindSafe(|| f())).is_err() {
                failures.push(format!("round {round}: {name} - {} on {text:?}", LAST.lock().map(|g| g.clone()).unwrap_or_default()));
            }
        };
        for kind in [SortKind::Lexicographic, SortKind::IgnoreCase, SortKind::Integer, SortKind::DecimalDot, SortKind::DecimalComma] {
            check("sort_lines_by", &mut || drop(sort_lines_by(&text, eol, kind, round % 2 == 0)));
        }
        check("randomize_lines", &mut || drop(randomize_lines(&text, eol, round as u64)));
        check("reverse_lines", &mut || drop(reverse_lines(&text, eol)));
        check("remove_duplicate_lines", &mut || drop(remove_duplicate_lines(&text, eol)));
        check("remove_consecutive_duplicates", &mut || drop(remove_consecutive_duplicates(&text, eol)));
        check("remove_empty_lines", &mut || drop(remove_empty_lines(&text, eol)));
        check("remove_blank_lines", &mut || drop(remove_blank_lines(&text, eol)));
        check("join_lines", &mut || drop(join_lines(&text)));
        check("split_lines", &mut || drop(split_lines(&text, eol, width)));
        check("trim_*", &mut || {
            drop(trim_trailing(&text, eol));
            drop(trim_leading(&text, eol));
            drop(trim_both(&text, eol));
            drop(remove_unnecessary_blanks(&text, eol));
        });
        check("tabs_to_spaces", &mut || drop(tabs_to_spaces(&text, eol, tab)));
        check("spaces_to_tabs", &mut || {
            drop(spaces_to_tabs(&text, eol, tab, false));
            drop(spaces_to_tabs(&text, eol, tab, true));
        });
        check("case", &mut || {
            drop(proper_case(&text, round % 2 == 0));
            drop(sentence_case(&text, round % 2 == 0));
            drop(invert_case(&text));
            drop(random_case(&text, round as u64));
        });
        for mode in [CommentMode::Toggle, CommentMode::Comment, CommentMode::Uncomment] {
            check("comment_single", &mut || drop(comment_single(&text, eol, syn, mode, round % 2 == 0)));
        }
        check("stream comment", &mut || {
            drop(stream_comment(&text, "/*", "*/"));
            drop(undo_stream_comment(&text, sel, "/*", "*/"));
            drop(undo_stream_comment(&text, sel, "<!--", "-->"));
            drop(undo_stream_comment(&text, sel, "\"", "\""));
        });
        check("column", &mut || {
            let lines: Vec<&str> = text.split('\n').collect();
            let n = ColumnNumbers { initial: r_i64(round), step: 3, repeat: round % 4, format: NumFormat::Hex, leading_zeros: round % 2 == 0 };
            drop(insert_at_column(&lines, round % 7, &number_sequence(&n, lines.len())));
        });
        check("search", &mut || {
            for o in [
                search::Opts { find: text.clone(), ..Default::default() },
                search::Opts { find: text.clone(), regex: true, ..Default::default() },
                search::Opts { find: text.clone(), whole_word: true, match_case: true, ..Default::default() },
                search::Opts { find: text.clone(), extended: true, replace: text.clone(), ..Default::default() },
                search::Opts { find: "(.)".into(), regex: true, replace: text.clone(), ..Default::default() },
            ] {
                if let Ok(re) = search::compile(&o) {
                    drop(search::all_matches(&text, &re));
                    drop(search::hits_in_text(&text, &re, None));
                    drop(search::next_match(&text, &re, sel.0));
                }
                drop(search::replacements(&text, &o));
            }
        });
        check("textools", &mut || {
            drop(textools::base64_decode(&text));
            drop(textools::base64_encode(text.as_bytes()));
            drop(textools::url_decode(&text));
            drop(textools::url_encode(&text));
            drop(textools::json_pretty(&text));
            drop(textools::json_minify(&text));
            drop(textools::xml_pretty(&text));
            drop(textools::xml_minify(&text));
        });
        for fmt in [Fmt::Bold, Fmt::Italic, Fmt::Strike, Fmt::Code, Fmt::Link, Fmt::Image, Fmt::Heading(2), Fmt::Quote, Fmt::Bullet, Fmt::Numbered, Fmt::Task, Fmt::CodeBlock, Fmt::Rule, Fmt::Table] {
            check("mdformat", &mut || {
                let e = mdformat::apply(fmt, &text, sel, eol);
                // The edit must be applicable to the document.
                assert!(e.start <= e.end && e.end <= text.len() && text.is_char_boundary(e.start) && text.is_char_boundary(e.end));
                let mut out = text.clone();
                out.replace_range(e.start..e.end, &e.text);
                assert!(e.sel.0 <= e.sel.1 && e.sel.1 <= out.len() && out.is_char_boundary(e.sel.0) && out.is_char_boundary(e.sel.1));
            });
        }
        check("preview", &mut || {
            drop(preview::markdown_to_html(&text));
            drop(preview::is_bare_mermaid(&text));
        });
        check("functions", &mut || {
            for lexer in ["python", "cpp", "rust", "markdown", "null", "sql"] {
                drop(functions::extract(&text, lexer));
            }
        });
        check("count_text", &mut || drop(view::count_text(&text)));
        check("format_date_time", &mut || {
            let t = DateTime { year: 2026, month: 10, day: 30, weekday: 5, hour: 15, minute: 7, second: 9 };
            drop(format_date_time(&text, &t));
        });
        check("complete", &mut || {
            drop(super::complete::word_before(&text, sel.1));
            drop(super::complete::words_with_prefix(&text, "a", sel.0));
            drop(super::complete::function_names(&text, "", sel.0));
            drop(super::complete::signature_for(&text, "a"));
            let _ = super::complete::path_fragment(&text);
        });
    }
    let _ = std::panic::take_hook();
    assert!(failures.is_empty(), "{} panics, first few:\n{}", failures.len(), failures.iter().take(12).cloned().collect::<Vec<_>>().join("\n"));
}

fn r_i64(round: usize) -> i64 {
    (round as i64 % 2000) - 1000
}
