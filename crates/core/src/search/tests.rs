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
fn hits_in_a_classic_mac_document_have_their_line_numbers() {
    let re = compile(&opts("x")).unwrap();
    let hits = hits_in_text("a\rx one\rb\rx two", &re, None);
    assert_eq!(hits.iter().map(|h| (h.line, h.text.as_str())).collect::<Vec<_>>(), [(2, "x one"), (4, "x two")]);
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
