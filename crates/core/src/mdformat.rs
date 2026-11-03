//! Markdown formatting commands for the toolbar and the command palette.
//!
//! Pure text in, one edit out: `apply` looks at the document and the selection and says
//! what to replace and where the selection should land. The editor applies it as a single
//! undo step. Positions are byte offsets into the document text.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    Image,
    /// Heading level 1..=6; applying the same level again removes the heading.
    Heading(u8),
    Quote,
    Bullet,
    Numbered,
    Task,
    CodeBlock,
    Rule,
    Table,
}

/// Replace `start..end` of the document with `text`, then select `sel` (positions in the new text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub sel: (usize, usize),
}

pub fn apply(fmt: Fmt, doc: &str, sel: (usize, usize), eol: &str) -> Edit {
    let len = doc.len();
    let (mut s, mut e) = (sel.0.min(sel.1).min(len), sel.0.max(sel.1).min(len));
    while !doc.is_char_boundary(s) {
        s -= 1;
    }
    while !doc.is_char_boundary(e) {
        e += 1;
    }
    match fmt {
        Fmt::Bold => wrap(doc, (s, e), "**", "bold text", eol),
        Fmt::Italic => wrap(doc, (s, e), "*", "italic text", eol),
        Fmt::Strike => wrap(doc, (s, e), "~~", "strikethrough", eol),
        Fmt::Code => wrap(doc, (s, e), "`", "code", eol),
        Fmt::Link => link(doc, (s, e), false),
        Fmt::Image => link(doc, (s, e), true),
        Fmt::Heading(n) => lines_edit(doc, (s, e), eol, |lines| heading(lines, n.clamp(1, 6))),
        Fmt::Quote => lines_edit(doc, (s, e), eol, quote),
        Fmt::Bullet => lines_edit(doc, (s, e), eol, |l| list(l, ListKind::Bullet)),
        Fmt::Numbered => lines_edit(doc, (s, e), eol, |l| list(l, ListKind::Numbered)),
        Fmt::Task => lines_edit(doc, (s, e), eol, |l| list(l, ListKind::Task)),
        Fmt::CodeBlock => code_block(doc, (s, e), eol),
        Fmt::Rule => insert_after_line(doc, e, eol, "---", None),
        Fmt::Table => {
            let table = format!("| Column 1 | Column 2 | Column 3 |{eol}| --- | --- | --- |{eol}|  |  |  |");
            insert_after_line(doc, e, eol, &table, Some((2, 10)))
        }
    }
}

// ── positions ────────────────────────────────────────────────────────────────

fn line_start(doc: &str, pos: usize) -> usize {
    doc[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// End of the line containing `pos`, before its line break (`\r\n` counts as one break, even
/// when `pos` sits between the two characters).
fn line_end(doc: &str, pos: usize) -> usize {
    let end = doc[pos..].find('\n').map_or(doc.len(), |i| pos + i);
    if end > 0 && doc.as_bytes()[end - 1] == b'\r' { end - 1 } else { end }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The word touching `pos`, if any.
fn word_at(doc: &str, pos: usize) -> Option<(usize, usize)> {
    let before = doc[..pos].char_indices().rev().take_while(|(_, c)| is_word(*c)).last().map(|(i, _)| i);
    let after = doc[pos..].char_indices().take_while(|(_, c)| is_word(*c)).last().map(|(i, c)| pos + i + c.len_utf8());
    match (before, after) {
        (None, None) => None,
        (b, a) => Some((b.unwrap_or(pos), a.unwrap_or(pos))),
    }
}

// ── inline wrapping ──────────────────────────────────────────────────────────

fn wrap(doc: &str, (s, e): (usize, usize), marker: &str, placeholder: &str, _eol: &str) -> Edit {
    let m = marker.len();
    // No selection: use the word under the caret, or insert the marker pair around a placeholder.
    let (s, e) = if s == e {
        match word_at(doc, s) {
            Some(w) => w,
            None => {
                return Edit { start: s, end: e, text: format!("{marker}{placeholder}{marker}"), sel: (s + m, s + m + placeholder.len()) };
            }
        }
    } else {
        (s, e)
    };
    let selected = &doc[s..e];

    // A single `*` must not be confused with the first star of `**`.
    let single_star = marker == "*";
    let stars_before = doc[..s].chars().rev().take_while(|c| *c == '*').count();
    let stars_after = doc[e..].chars().take_while(|c| *c == '*').count();
    let outside = doc[..s].ends_with(marker) && doc[e..].starts_with(marker) && (!single_star || (stars_before == 1 && stars_after == 1));
    if outside {
        return Edit { start: s - m, end: e + m, text: selected.to_string(), sel: (s - m, s - m + selected.len()) };
    }
    let inside = selected.len() >= 2 * m && selected.starts_with(marker) && selected.ends_with(marker) && (!single_star || !selected.starts_with("**"));
    if inside {
        let inner = &selected[m..selected.len() - m];
        return Edit { start: s, end: e, text: inner.to_string(), sel: (s, s + inner.len()) };
    }
    // Wrapping across lines would break the markup, so each line gets its own pair.
    if selected.contains('\n') {
        let mut out = String::new();
        for (i, raw) in selected.split('\n').enumerate() {
            if i > 0 {
                out.push('\n');
            }
            let cr = raw.ends_with('\r');
            let line = raw.trim_end_matches('\r');
            let (lead, rest) = line.split_at(line.len() - line.trim_start().len());
            let body = rest.trim_end();
            let trail = &rest[body.len()..];
            if body.is_empty() {
                out.push_str(line);
            } else {
                out.push_str(&format!("{lead}{marker}{body}{marker}{trail}"));
            }
            if cr {
                out.push('\r');
            }
        }
        let n = out.len();
        return Edit { start: s, end: e, text: out, sel: (s, s + n) };
    }
    Edit { start: s, end: e, text: format!("{marker}{selected}{marker}"), sel: (s + m, e + m) }
}

fn link(doc: &str, (s, e): (usize, usize), image: bool) -> Edit {
    let bang = if image { "!" } else { "" };
    let selected = &doc[s..e];
    let looks_like_url = selected.starts_with("http://") || selected.starts_with("https://") || selected.starts_with("www.");
    let (label, url) = if selected.is_empty() {
        (if image { "alt text" } else { "link text" }, "https://")
    } else if looks_like_url {
        (if image { "alt text" } else { "link text" }, selected)
    } else {
        (selected, "https://")
    };
    let text = format!("{bang}[{label}]({url})");
    let (a, b) = if selected.is_empty() || looks_like_url {
        // Select the label placeholder so typing replaces it.
        (s + bang.len() + 1, s + bang.len() + 1 + label.len())
    } else {
        // The label is the user's text: select the URL placeholder instead.
        let url_start = s + bang.len() + 1 + label.len() + 2;
        (url_start, url_start + url.len())
    };
    Edit { start: s, end: e, text, sel: (a, b) }
}

// ── whole-line commands ──────────────────────────────────────────────────────

/// Apply `f` to the lines the selection touches. `f` returns the new lines.
fn lines_edit(doc: &str, (s, e): (usize, usize), eol: &str, f: impl Fn(&[String]) -> Vec<String>) -> Edit {
    let (ls, le) = line_range(doc, (s, e));
    let region = &doc[ls..le];
    let old: Vec<String> = region.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    let new = f(&old);
    let text = new.join(eol);
    let sel = if s == e {
        // Keep the caret where it was, shifted by what changed at the start of its line.
        let delta = crate::num::to_isize(new[0].len()) - crate::num::to_isize(old[0].len());
        let caret = crate::num::to_usize((crate::num::to_isize(s) + delta).clamp(crate::num::to_isize(ls), crate::num::to_isize(ls + text.len())));
        (caret, caret)
    } else {
        (ls, ls + text.len())
    };
    Edit { start: ls, end: le, text, sel }
}

/// Start and end of the whole lines covered by a selection (a selection that ends at the very
/// start of a line does not include that line).
fn line_range(doc: &str, (s, e): (usize, usize)) -> (usize, usize) {
    let ls = line_start(doc, s);
    let last = if e > s && doc.as_bytes()[e - 1] == b'\n' { e - 1 } else { e };
    (ls, line_end(doc, last.max(ls)).max(ls))
}

fn indent_of(line: &str) -> (&str, &str) {
    line.split_at(line.len() - line.trim_start().len())
}

fn heading_level(line: &str) -> Option<(usize, &str)> {
    let (_, rest) = indent_of(line);
    let hashes = rest.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && rest[hashes..].starts_with([' ', '\t']) { Some((hashes, rest[hashes..].trim_start())) } else { None }
}

fn heading(lines: &[String], level: u8) -> Vec<String> {
    let blank = |l: &String| l.trim().is_empty();
    let all_same = lines.iter().filter(|l| !blank(l)).all(|l| heading_level(l).is_some_and(|(n, _)| n == level as usize));
    lines
        .iter()
        .map(|l| {
            if blank(l) {
                return l.clone();
            }
            let text = heading_level(l).map_or_else(|| l.trim_start().to_string(), |(_, t)| t.to_string());
            if all_same { text } else { format!("{} {}", "#".repeat(level as usize), text) }
        })
        .collect()
}

fn quote(lines: &[String]) -> Vec<String> {
    let all_quoted = lines.iter().filter(|l| !l.trim().is_empty()).all(|l| l.trim_start().starts_with('>'));
    lines
        .iter()
        .map(|l| {
            if all_quoted {
                let (indent, rest) = indent_of(l);
                match rest.strip_prefix('>') {
                    Some(r) => format!("{indent}{}", r.strip_prefix(' ').unwrap_or(r)),
                    None => l.clone(),
                }
            } else if l.trim().is_empty() {
                ">".to_string()
            } else {
                format!("> {l}")
            }
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum ListKind {
    Bullet,
    Numbered,
    Task,
}

/// The list marker at the start of `rest` (after indentation): its kind and byte length.
fn list_marker(rest: &str) -> Option<(ListKind, usize)> {
    for task in ["- [ ] ", "- [x] ", "- [X] ", "* [ ] ", "* [x] ", "* [X] ", "+ [ ] ", "+ [x] ", "+ [X] "] {
        if rest.starts_with(task) {
            return Some((ListKind::Task, task.len()));
        }
    }
    if ["- ", "* ", "+ "].iter().any(|m| rest.starts_with(m)) {
        return Some((ListKind::Bullet, 2));
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 9 && [". ", ") "].iter().any(|m| rest[digits..].starts_with(m)) {
        return Some((ListKind::Numbered, digits + 2));
    }
    None
}

fn list(lines: &[String], kind: ListKind) -> Vec<String> {
    let blank = |l: &String| l.trim().is_empty();
    let all_same = lines.iter().filter(|l| !blank(l)).all(|l| list_marker(indent_of(l).1).is_some_and(|(k, _)| k == kind));
    let mut n = 0;
    lines
        .iter()
        .map(|l| {
            if blank(l) {
                return l.clone();
            }
            let (indent, rest) = indent_of(l);
            let body = list_marker(rest).map_or(rest, |(_, len)| &rest[len..]);
            if all_same {
                return format!("{indent}{body}");
            }
            n += 1;
            match kind {
                ListKind::Bullet => format!("{indent}- {body}"),
                ListKind::Task => format!("{indent}- [ ] {body}"),
                ListKind::Numbered => format!("{indent}{n}. {body}"),
            }
        })
        .collect()
}

fn code_block(doc: &str, (s, e): (usize, usize), eol: &str) -> Edit {
    let (ls, le) = line_range(doc, (s, e));
    let region = &doc[ls..le];
    let lines: Vec<&str> = region.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let fenced = lines.len() >= 2 && lines[0].trim_start().starts_with("```") && lines[lines.len() - 1].trim() == "```";
    if fenced {
        let inner = lines[1..lines.len() - 1].join(eol);
        let n = inner.len();
        return Edit { start: ls, end: le, text: inner, sel: (ls, ls + n) };
    }
    if region.trim().is_empty() {
        let text = format!("```{eol}{eol}```");
        return Edit { start: ls, end: le, text, sel: (ls + 3 + eol.len(), ls + 3 + eol.len()) };
    }
    let text = format!("```{eol}{}{eol}```", lines.join(eol));
    let n = text.len();
    Edit { start: ls, end: le, text, sel: (ls + 3 + eol.len(), ls + n - 3 - eol.len()) }
}

/// Insert a block after the line containing `pos`, with a blank line around it when needed.
/// `select` is (offset, length) inside `block` for the selection afterwards.
fn insert_after_line(doc: &str, pos: usize, eol: &str, block: &str, select: Option<(usize, usize)>) -> Edit {
    let le = line_end(doc, pos);
    let current_blank = doc[line_start(doc, pos)..le].trim().is_empty();
    // Replace an empty line in place; otherwise leave a blank line between this line and the block.
    let (start, lead) = if current_blank { (line_start(doc, pos), String::new()) } else { (le, format!("{eol}{eol}")) };
    let end = le;
    let text = format!("{lead}{block}{eol}");
    let base = start + lead.len();
    let sel = match select {
        // The table template's selection is given as (line, length): select that cell on that line.
        Some((line, len)) => {
            let off = block.split(eol).take(line).map(|l| l.len() + eol.len()).sum::<usize>();
            let col = block.split(eol).nth(line).map_or(0, |l| l.find("  ").map_or(0, |i| i + 1));
            let _ = len;
            let at = base + off + col;
            (at, at)
        }
        None => (base + block.len() + eol.len(), base + block.len() + eol.len()),
    };
    // For the table, select the first header cell instead of a caret in the first body row.
    let sel = if block.starts_with("| Column 1") { (base + 2, base + 10) } else { sel };
    Edit { start, end, text, sel }
}

#[cfg(test)]
mod tests;
