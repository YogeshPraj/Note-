use super::*;

/// Apply an edit to `doc` the way the editor does.
fn run(fmt: Fmt, doc: &str, sel: (usize, usize)) -> (String, (usize, usize)) {
    let e = apply(fmt, doc, sel, "\n");
    let mut out = String::new();
    out.push_str(&doc[..e.start]);
    out.push_str(&e.text);
    out.push_str(&doc[e.end..]);
    (out, e.sel)
}

fn picked(doc: &str, sel: (usize, usize)) -> &str {
    &doc[sel.0..sel.1]
}

#[test]
fn bold_wraps_and_unwraps_a_selection() {
    let (d, sel) = run(Fmt::Bold, "say hello now", (4, 9));
    assert_eq!(d, "say **hello** now");
    assert_eq!(picked(&d, sel), "hello");
    // Selecting the inside of **hello** and clicking Bold again removes the markers.
    let (d2, sel2) = run(Fmt::Bold, &d, sel);
    assert_eq!(d2, "say hello now");
    assert_eq!(picked(&d2, sel2), "hello");
    // So does selecting the markers too.
    let (d3, _) = run(Fmt::Bold, &d, (4, 13));
    assert_eq!(d3, "say hello now");
}

#[test]
fn caret_in_a_word_formats_the_word() {
    let (d, sel) = run(Fmt::Italic, "one two three", (6, 6));
    assert_eq!(d, "one *two* three");
    assert_eq!(picked(&d, sel), "two");
}

#[test]
fn empty_caret_inserts_a_selected_placeholder() {
    let (d, sel) = run(Fmt::Bold, "a  b", (2, 2));
    assert_eq!(d, "a **bold text** b");
    assert_eq!(picked(&d, sel), "bold text");
    let (d, sel) = run(Fmt::Code, "", (0, 0));
    assert_eq!((d.as_str(), picked(&d, sel)), ("`code`", "code"));
}

#[test]
fn italic_is_not_confused_with_bold() {
    // Italic on bold text adds a star pair instead of stripping one star of the bold marker.
    let (d, _) = run(Fmt::Italic, "**hi**", (2, 4));
    assert_eq!(d, "***hi***");
    // And bold markers are not seen as italic when selected whole.
    let (d, _) = run(Fmt::Italic, "**hi**", (0, 6));
    assert_eq!(d, "***hi***");
    let (d, _) = run(Fmt::Italic, "*hi*", (1, 3));
    assert_eq!(d, "hi");
}

#[test]
fn multi_line_selection_wraps_each_line() {
    let (d, _) = run(Fmt::Bold, "one\n\n two ", (0, 10));
    assert_eq!(d, "**one**\n\n **two** ");
}

#[test]
fn strike_and_code() {
    assert_eq!(run(Fmt::Strike, "gone", (0, 4)).0, "~~gone~~");
    assert_eq!(run(Fmt::Code, "x = 1", (0, 5)).0, "`x = 1`");
    assert_eq!(run(Fmt::Strike, "~~gone~~", (0, 8)).0, "gone");
}

#[test]
fn unicode_selections_stay_on_character_boundaries() {
    let doc = "café au lait";
    let (d, sel) = run(Fmt::Bold, doc, (3, 4)); // inside the multi-byte é: snaps to boundaries
    assert!(d.contains("**"), "{d}");
    assert!(d.is_char_boundary(sel.0) && d.is_char_boundary(sel.1));
    assert_eq!(run(Fmt::Bold, doc, (0, 5)).0, "**café** au lait");
}

#[test]
fn links_and_images() {
    let (d, sel) = run(Fmt::Link, "see docs here", (4, 8));
    assert_eq!(d, "see [docs](https://) here");
    assert_eq!(picked(&d, sel), "https://");
    let (d, sel) = run(Fmt::Link, "https://example.com", (0, 19));
    assert_eq!(d, "[link text](https://example.com)");
    assert_eq!(picked(&d, sel), "link text");
    let (d, sel) = run(Fmt::Image, "", (0, 0));
    assert_eq!(d, "![alt text](https://)");
    assert_eq!(picked(&d, sel), "alt text");
    let (d, _) = run(Fmt::Image, "logo", (0, 4));
    assert_eq!(d, "![logo](https://)");
}

#[test]
fn headings_set_switch_and_toggle() {
    assert_eq!(run(Fmt::Heading(2), "Title", (0, 0)).0, "## Title");
    assert_eq!(run(Fmt::Heading(1), "## Title", (3, 3)).0, "# Title");
    assert_eq!(run(Fmt::Heading(2), "## Title", (0, 0)).0, "Title");
    // Several lines: blank lines are skipped.
    assert_eq!(run(Fmt::Heading(3), "a\n\nb", (0, 4)).0, "### a\n\n### b");
    // Not a heading: a hash without a following space.
    assert_eq!(run(Fmt::Heading(1), "#tag", (0, 0)).0, "# #tag");
}

#[test]
fn caret_follows_the_inserted_prefix() {
    let (d, sel) = run(Fmt::Heading(2), "Title", (5, 5));
    assert_eq!((d.as_str(), sel), ("## Title", (8, 8)));
    let (d, sel) = run(Fmt::Heading(2), &d, sel);
    assert_eq!((d.as_str(), sel), ("Title", (5, 5)));
}

#[test]
fn quote_toggles() {
    assert_eq!(run(Fmt::Quote, "a\nb", (0, 3)).0, "> a\n> b");
    assert_eq!(run(Fmt::Quote, "> a\n> b", (0, 7)).0, "a\nb");
    assert_eq!(run(Fmt::Quote, "a\n\nb", (0, 4)).0, "> a\n>\n> b");
}

#[test]
fn lists_convert_between_kinds() {
    assert_eq!(run(Fmt::Bullet, "a\nb", (0, 3)).0, "- a\n- b");
    assert_eq!(run(Fmt::Numbered, "a\nb\n\nc", (0, 6)).0, "1. a\n2. b\n\n3. c");
    assert_eq!(run(Fmt::Task, "a", (0, 0)).0, "- [ ] a");
    // Switching kind replaces the old marker; applying the same kind removes it.
    assert_eq!(run(Fmt::Numbered, "- a\n- b", (0, 7)).0, "1. a\n2. b");
    assert_eq!(run(Fmt::Bullet, "- a\n- b", (0, 7)).0, "a\nb");
    assert_eq!(run(Fmt::Task, "- [x] done", (0, 0)).0, "done");
    assert_eq!(run(Fmt::Bullet, "- [ ] todo", (0, 0)).0, "- todo");
    // Indentation is kept.
    assert_eq!(run(Fmt::Bullet, "  nested", (0, 0)).0, "  - nested");
}

#[test]
fn selection_ending_at_a_line_start_excludes_that_line() {
    assert_eq!(run(Fmt::Bullet, "a\nb\nc", (0, 4)).0, "- a\n- b\nc");
}

#[test]
fn code_blocks_fence_and_unfence() {
    let (d, sel) = run(Fmt::CodeBlock, "", (0, 0));
    assert_eq!(d, "```\n\n```");
    assert_eq!(sel, (4, 4));
    assert_eq!(run(Fmt::CodeBlock, "let x = 1;\nx", (0, 12)).0, "```\nlet x = 1;\nx\n```");
    assert_eq!(run(Fmt::CodeBlock, "```\ncode\n```", (0, 12)).0, "code");
}

#[test]
fn rule_and_table_leave_blank_lines_around() {
    assert_eq!(run(Fmt::Rule, "text", (2, 2)).0, "text\n\n---\n");
    assert_eq!(run(Fmt::Rule, "", (0, 0)).0, "---\n");
    let (d, sel) = run(Fmt::Table, "intro", (0, 0));
    assert!(d.starts_with("intro\n\n| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n|  |  |  |\n"), "{d}");
    assert_eq!(picked(&d, sel), "Column 1");
}

#[test]
fn crlf_documents_keep_their_line_endings() {
    let doc = "a\r\nb";
    let e = apply(Fmt::Bullet, doc, (0, doc.len()), "\r\n");
    assert_eq!(e.text, "- a\r\n- b");
}

#[test]
fn caret_between_cr_and_lf_keeps_the_line_break() {
    let doc = "hello\r\nworld";
    // Offset 6 is between \r and \n.
    let e = apply(Fmt::Heading(2), doc, (6, 6), "\r\n");
    let mut out = doc[..e.start].to_string();
    out.push_str(&e.text);
    out.push_str(&doc[e.end..]);
    assert_eq!(out, "## hello\r\nworld");
    // An empty CRLF line with the caret inside the break.
    let doc = "a\r\n\r\nb";
    let e = apply(Fmt::Bullet, doc, (4, 4), "\r\n");
    assert_eq!(&doc[..e.start], "a\r\n");
    assert!(doc[e.end..].starts_with("\r\nb"), "{e:?}");
}

#[test]
fn out_of_range_selection_is_clamped() {
    // The caret ends up after "hi", which touches the word, so the word is formatted.
    let (d, _) = run(Fmt::Bold, "hi", (5, 9));
    assert_eq!(d, "**hi**");
    let (d, _) = run(Fmt::Bold, "hi ", (9, 9));
    assert_eq!(d, "hi **bold text**");
}
