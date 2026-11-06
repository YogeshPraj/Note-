//! The text work of the document tools, without an editor: splitting lines, line endings, exact matches.

use crate::doc::Eol;
use crate::error::Error;

/// A part of a document, by whole lines where it can be.
#[derive(Debug, PartialEq, Eq)]
pub struct Slice<'a> {
    pub text: &'a str,
    /// The first and last line given, from 1.
    pub first: usize,
    pub last: usize,
    pub total: usize,
    /// Where to go on reading, when there is more.
    pub next: Option<usize>,
    /// The one line given was too long to give whole.
    pub cut: bool,
}

/// Where each line starts, for any of the three line endings.
fn line_starts(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let mut starts = vec![0];
    for (i, b) in bytes.iter().enumerate() {
        let ends = *b == b'\n' || (*b == b'\r' && bytes.get(i + 1) != Some(&b'\n'));
        if ends {
            starts.push(i + 1);
        }
    }
    starts
}

/// The largest length not above `max` that ends between characters.
fn fit(text: &str, max: usize) -> usize {
    (0..=max.min(text.len())).rev().find(|&n| text.is_char_boundary(n)).unwrap_or(0)
}

/// Up to `count` lines from line `start` (from 1), at most `max_bytes` of them. A single longer line is cut, and said to be.
pub fn lines(text: &str, start: usize, count: Option<usize>, max_bytes: usize) -> Slice<'_> {
    let starts = line_starts(text);
    let total = starts.len();
    let first = start.clamp(1, total);
    let wanted_last = count.map_or(total, |c| first.saturating_add(c.max(1) - 1).min(total));
    let begin = starts[first - 1];
    let end_of = |line: usize| starts.get(line).copied().unwrap_or(text.len());
    let mut last = wanted_last;
    while last > first && end_of(last) - begin > max_bytes {
        last -= 1;
    }
    let body = &text[begin..end_of(last)];
    let next = (last < total).then_some(last + 1);
    if body.len() > max_bytes {
        return Slice { text: &body[..fit(body, max_bytes)], first, last, total, next, cut: true };
    }
    Slice { text: body, first, last, total, next, cut: false }
}

/// The 1-based line a byte offset is on.
pub fn line_of(text: &str, offset: usize) -> usize {
    line_starts(text).partition_point(|&s| s <= offset)
}

/// `text` with every line ending made the document's own: an agent writes `\n` whatever the file uses.
pub fn with_eol(text: &str, eol: Eol) -> String {
    let plain = text.replace("\r\n", "\n").replace('\r', "\n");
    match eol {
        Eol::Lf => plain,
        Eol::Crlf => plain.replace('\n', "\r\n"),
        Eol::Cr => plain.replace('\n', "\r"),
    }
}

/// What `old` must be to match in `text`: as given, or with the document's line endings. Exactly one match, or `all`.
///
/// # Errors
/// There is no match, or several and `all` was not asked for: the edit would be a guess.
pub fn exact_match(text: &str, old: &str, eol: Eol, all: bool) -> Result<(String, usize), Error> {
    if old.is_empty() {
        return Err(Error::Agent("\"old_text\" must not be empty.".into()));
    }
    let wanted = if text.contains(old) { old.to_string() } else { with_eol(old, eol) };
    let found = text.matches(wanted.as_str()).count();
    if found == 0 {
        return Err(Error::Agent("\"old_text\" was not found. Read the document again: it may have changed.".into()));
    }
    if found > 1 && !all {
        return Err(Error::Agent(format!("\"old_text\" was found {found} times. Give more of the text around it, or set \"all\".")));
    }
    Ok((wanted, found))
}

/// The Mermaid source in `text`: its first `mermaid` block, or all of it when it has none. None when there is nothing.
pub fn mermaid_source(text: &str) -> Option<String> {
    let mut lines = text.lines().skip_while(|l| !l.trim_start().starts_with("```mermaid"));
    let source =
        if lines.next().is_some() { lines.take_while(|l| !l.trim_start().starts_with("```")).collect::<Vec<_>>().join("\n") } else { text.to_string() };
    (!source.trim().is_empty()).then_some(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diagram_is_found_in_a_mermaid_file_or_a_markdown_block() {
        assert_eq!(mermaid_source("graph TD\n A-->B\n").as_deref(), Some("graph TD\n A-->B\n"));
        let doc = "# Title\n\n```js\nx\n```\n```mermaid\ngraph TD\n A-->B\n```\n\n```mermaid\nsecond\n```\n";
        assert_eq!(mermaid_source(doc).as_deref(), Some("graph TD\n A-->B"));
        assert_eq!(mermaid_source("  \n"), None);
        assert_eq!(mermaid_source("```mermaid\n```"), None);
    }

    #[test]
    fn lines_are_found_for_every_line_ending() {
        for text in ["a\nb\nc", "a\r\nb\r\nc", "a\rb\rc"] {
            let s = lines(text, 2, Some(1), 100);
            assert_eq!((s.text.trim_end(), s.first, s.last, s.total, s.next), ("b", 2, 2, 3, Some(3)), "{text:?}");
        }
    }

    #[test]
    fn reading_clamps_to_the_document() {
        let s = lines("a\nb", 9, None, 100);
        assert_eq!((s.text, s.first, s.next), ("b", 2, None));
        let s = lines("a\nb", 0, Some(0), 100);
        assert_eq!((s.text, s.first, s.last), ("a\n", 1, 1));
        let s = lines("", 1, None, 100);
        assert_eq!((s.text, s.total, s.next), ("", 1, None));
        assert_eq!(lines("a\n", 1, None, 100).total, 2, "as the editor counts them");
    }

    #[test]
    fn a_large_read_stops_at_a_line_and_says_where_to_go_on() {
        let text = "aaaa\nbbbb\ncccc\n";
        let s = lines(text, 1, None, 11);
        assert_eq!((s.text, s.last, s.next), ("aaaa\nbbbb\n", 2, Some(3)));
        let rest = lines(text, s.next.unwrap(), None, 11);
        assert_eq!(rest.text, "cccc\n");
    }

    #[test]
    fn one_line_longer_than_the_limit_is_cut_between_characters() {
        let text = "éééééé";
        let s = lines(text, 1, None, 5);
        assert_eq!((s.text, s.next, s.cut), ("éé", None, true));
        let two = lines(
            "éééééé
b",
            1,
            None,
            5,
        );
        assert_eq!((two.next, two.cut), (Some(2), true), "reading goes on past it, never back to it");
    }

    #[test]
    fn offsets_become_line_numbers() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\r\nb", 3), 2);
    }

    #[test]
    fn line_endings_become_the_documents() {
        assert_eq!(with_eol("a\nb\r\nc\rd", Eol::Crlf), "a\r\nb\r\nc\r\nd");
        assert_eq!(with_eol("a\r\nb", Eol::Lf), "a\nb");
        assert_eq!(with_eol("a\nb", Eol::Cr), "a\rb");
    }

    #[test]
    fn an_edit_must_be_exact_and_unique() {
        assert_eq!(exact_match("one two", "two", Eol::Lf, false), Ok(("two".into(), 1)));
        assert!(exact_match("one two", "three", Eol::Lf, false).is_err());
        assert!(exact_match("one two", "", Eol::Lf, false).is_err());
        assert!(exact_match("a a", "a", Eol::Lf, false).is_err());
        assert_eq!(exact_match("a a", "a", Eol::Lf, true), Ok(("a".into(), 2)));
    }

    #[test]
    fn a_match_finds_the_text_whatever_line_endings_the_agent_used() {
        assert_eq!(exact_match("a\r\nb", "a\nb", Eol::Crlf, false), Ok(("a\r\nb".into(), 1)));
    }
}
