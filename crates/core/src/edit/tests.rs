use super::*;

#[test]
fn sorts_and_keeps_trailing_newline() {
    assert_eq!(sort_lines("b\r\na\r\nc\r\n", "\r\n", false), "a\r\nb\r\nc\r\n");
    assert_eq!(sort_lines("b\na\nc", "\n", true), "c\nb\na");
}

#[test]
fn sort_kinds() {
    assert_eq!(sort_lines_by("b\nA\nc", "\n", SortKind::IgnoreCase, false), "A\nb\nc");
    assert_eq!(sort_lines_by("10\n9\n100\nx", "\n", SortKind::Integer, false), "9\n10\n100\nx");
    // Natural order: runs of digits compare by value, however long; text compares as text.
    assert_eq!(sort_lines_by("item10\nitem2\nitem1\nitem-3\nitem007", "\n", SortKind::Integer, false), "item-3\nitem1\nitem2\nitem007\nitem10");
    assert_eq!(
        sort_lines_by("99999999999999999999\n100000000000000000000\n5", "\n", SortKind::Integer, true),
        "100000000000000000000\n99999999999999999999\n5"
    );
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
fn line_operations_work_on_classic_mac_documents() {
    // Lines that end in a lone CR used to be seen as one line, so nothing was sorted, reversed or deduplicated.
    assert_eq!(sort_lines_by("b\rc\ra\r", "\r", SortKind::Lexicographic, false), "a\rb\rc\r");
    assert_eq!(reverse_lines("1\r2\r3", "\r"), "3\r2\r1");
    assert_eq!(remove_duplicate_lines("x\ry\rx", "\r"), "x\ry");
    assert_eq!(join_lines("a\rb"), "a b");
    assert_eq!(lines_of("a\rb\rc"), ["a", "b", "c"]);
    // A stray CR inside a line of an LF or CR LF document is left alone.
    assert_eq!(lines_of("a\rb\nc\r\n"), ["a\rb", "c", ""]);
}

#[test]
fn reverse_shuffle_join_split() {
    assert_eq!(reverse_lines("1\n2\n3", "\n"), "3\n2\n1");
    let shuffled = randomize_lines("1\n2\n3\n4\n5\n6", "\n", 42);
    let mut v: Vec<&str> = shuffled.split('\n').collect();
    v.sort_unstable();
    assert_eq!(v, ["1", "2", "3", "4", "5", "6"]);
    assert_eq!(join_lines("a\nb\r\nc\n"), "a b c");
    assert_eq!(join_lines("a \nb"), "a b");
    assert_eq!(join_all("a\r\nb\nc\r"), "a b c ");
    assert_eq!(join_all("a \r\nb"), "a b");
    assert_eq!(join_lines("a \nb"), "a b");
    assert_eq!(join_all("a\r\nb\nc\r"), "a b c ");
    assert_eq!(join_all("a \r\nb"), "a b");
    assert_eq!(split_lines("aaa bbb ccc ddd", "\n", 7), "aaa bbb\nccc ddd");
    assert_eq!(split_lines("abcdefghij", "\n", 4), "abcd\nefgh\nij");
}

#[test]
fn blank_operations() {
    assert_eq!(trim_trailing("a  \nb\t\nc", "\n"), "a\nb\nc");
    assert_eq!(trim_leading("  a\n\tb", "\n"), "a\nb");
    assert_eq!(trim_both(" a \n b ", "\n"), "a\nb");
    // Trimmed, then everything on one line (the Remove Unnecessary Blank and EOL).
    assert_eq!(remove_unnecessary_blanks(" a \n\n  \n b", "\n"), "a b");
    assert_eq!(tabs_to_spaces("a\tb\n\tc", "\n", 4), "a   b\n    c");
    assert_eq!(spaces_to_tabs("    a    b", "\n", 4, false), "\ta\t b");
    assert_eq!(spaces_to_tabs("    a    b", "\n", 4, true), "\ta    b");
    assert_eq!(spaces_to_tabs("a b", "\n", 4, false), "a b");
}

#[test]
fn sentence_case_follows_notepad_plus_plus() {
    assert_eq!(sentence_case("hello. WORLD! how are you? i think i am fine.", false), "Hello. World! How are you? I think I am fine.");
    assert_eq!(sentence_case("one\n\ntwo\r\n\r\nthree", false), "One\n\nTwo\r\n\r\nThree");
    assert_eq!(sentence_case("keep iPhone. but A.b stays", true), "Keep iPhone. But A.b stays");
    assert_eq!(proper_case("don\u{2019}t stop", false), "Don\u{2019}t Stop");
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

fn syn(label: &str) -> Comments {
    comment_syntax(label, "")
}

#[test]
fn toggle_works_line_by_line_at_each_indent() {
    let py = syn("Python");
    // Each line gets the symbol at its own indentation; blank lines are left alone.
    assert_eq!(comment_single("    a\n      b\n\nc", "\n", py, CommentMode::Toggle, false).unwrap(), "    # a\n      # b\n\n# c");
    // A mixed selection toggles per line: commented lines are uncommented, the rest commented.
    assert_eq!(comment_single("# a\nb", "\n", py, CommentMode::Toggle, false).unwrap(), "a\n# b");
    let src = "fn a() {\n    x();\n}";
    let once = comment_single(src, "\n", syn("Rust"), CommentMode::Toggle, false).unwrap();
    assert_eq!(once, "// fn a() {\n    // x();\n// }");
    assert_eq!(comment_single(&once, "\n", syn("Rust"), CommentMode::Toggle, false).unwrap(), src);
}

#[test]
fn forced_comment_and_uncomment() {
    let py = syn("Python");
    assert_eq!(comment_single("# a\nb", "\n", py, CommentMode::Comment, false).unwrap(), "# # a\n# b");
    assert_eq!(comment_single("# a\nb\n#c", "\n", py, CommentMode::Uncomment, false).unwrap(), "a\nb\nc");
    // The batch symbol is matched in any case, and a blank after it goes too.
    assert_eq!(comment_single("rem x\nREM y", "\n", syn("Batch"), CommentMode::Uncomment, false).unwrap(), "x\ny");
}

#[test]
fn languages_without_line_comments_wrap_each_line() {
    let css = syn("CSS");
    assert_eq!(comment_single("a {}\n  b {}", "\n", css, CommentMode::Comment, false).unwrap(), "/* a {} */\n  /* b {} */");
    assert_eq!(comment_single("/* a {} */", "\n", css, CommentMode::Toggle, false).unwrap(), "a {}");
    assert_eq!(comment_single("x", "\n", syn("Normal text"), CommentMode::Toggle, false), None);
    assert_eq!(comment_single("x", "\n", syn("JSON"), CommentMode::Toggle, false), None);
    // Fortran 77 comments start in column one.
    assert_eq!(comment_single("   x = 1", "\n", syn("Fortran (fixed form)"), CommentMode::Comment, true).unwrap(), "C    x = 1");
}

#[test]
fn block_comments() {
    assert_eq!(stream_comment("x = 1;", "/*", "*/"), "/* x = 1; */");
    let doc = "a /* x = 1; */ b";
    // Caret or selection inside the comment is enough.
    assert_eq!(undo_stream_comment(doc, (7, 7), "/*", "*/"), Some((2, 14, "x = 1;".into())));
    assert_eq!(undo_stream_comment(doc, (2, 14), "/*", "*/"), Some((2, 14, "x = 1;".into())));
    assert_eq!(undo_stream_comment("a b", (1, 2), "/*", "*/"), None);
    assert_eq!(comment_syntax("Python", "python").start, "");
    assert_eq!(comment_syntax("C++", "cpp").start, "/*");
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
}
