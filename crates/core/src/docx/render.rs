//! The blocks and runs read from a Word file, written out as Markdown.

#[derive(Clone, Default, PartialEq)]
pub(super) struct Run {
    pub(super) text: String,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) code: bool,
    pub(super) link: Option<String>,
}

pub(super) enum Block {
    Text(String),
    Heading(usize, String),
    /// Ordered, level, text, list number id, first number.
    Item(bool, usize, String, String, usize),
    Rule,
    Quote(String),
    Code(String),
    Table(Vec<Vec<String>>),
}

/// The level a list style's name ends in ("list bullet 2" is the second level), from 0.
pub(super) fn style_level(name: &str) -> usize {
    name.chars().last().and_then(|c| c.to_digit(10)).map_or(0, |d| usize::try_from(d).unwrap_or(1).saturating_sub(1))
}

/// Plain text made safe in Markdown.
fn escape(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, c) in chars.iter().enumerate() {
        let alnum = |j: Option<usize>| j.and_then(|j| chars.get(j)).is_some_and(|c| c.is_alphanumeric());
        match c {
            '\\' | '*' | '`' | '[' | ']' | '<' | '>' => {
                out.push('\\');
                out.push(*c);
            }
            '_' if !(alnum(i.checked_sub(1)) && alnum(Some(i + 1))) => out.push_str("\\_"),
            c => out.push(*c),
        }
    }
    out
}

/// The runs as Markdown text: neighbours that look alike are one, and spaces stay outside the markers.
pub(super) fn inline(runs: &[Run], plain: bool) -> String {
    let mut merged: Vec<Run> = Vec::new();
    for r in runs {
        match merged.last_mut() {
            Some(last) if last.bold == r.bold && last.italic == r.italic && last.strike == r.strike && last.code == r.code && last.link == r.link => {
                last.text.push_str(&r.text);
            }
            _ => merged.push(r.clone()),
        }
    }
    let mut out = String::new();
    for r in merged {
        let body = r.text.trim_matches([' ', '\t', '\n']);
        if body.is_empty() {
            out.push_str(&r.text.replace(['\t'], " "));
            continue;
        }
        let lead = &r.text[..r.text.len() - r.text.trim_start_matches([' ', '\t', '\n']).len()];
        let trail = &r.text[r.text.trim_end_matches([' ', '\t', '\n']).len()..];
        let mut s = if r.code && !plain {
            let ticks = if body.contains('`') { "``" } else { "`" };
            format!("{ticks}{body}{ticks}")
        } else {
            escape(body)
        };
        if !plain {
            if r.strike {
                s = format!("~~{s}~~");
            }
            if r.italic {
                s = format!("*{s}*");
            }
            if r.bold {
                s = format!("**{s}**");
            }
        }
        if let Some(url) = &r.link {
            s = format!("[{s}]({url})");
        }
        out.push_str(lead);
        out.push_str(&s);
        out.push_str(trail);
    }
    out
}

fn first_line_safe(s: &str) -> String {
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    let numbered = digits > 0 && s[digits..].starts_with(['.', ')']);
    if s.starts_with(['#', '>', '+', '-']) || numbered { format!("\\{s}") } else { s.to_string() }
}

pub(super) fn render(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut previous: Option<&Block> = None;
    let mut numbers: Vec<usize> = Vec::new();
    for b in blocks {
        let both_items = match (previous, b) {
            (Some(Block::Item(_, _, _, before, _)), Block::Item(_, level, _, num, _)) => *level > 0 || before == num,
            _ => false,
        };
        let both_code = matches!((previous, b), (Some(Block::Code(_)), Block::Code(_)));
        if previous.is_some() {
            out.push_str(if both_items || both_code { "\n" } else { "\n\n" });
        }
        if !both_items {
            numbers.clear();
        }
        match b {
            Block::Text(t) => out.push_str(&first_line_safe(t)),
            Block::Heading(n, t) => out.push_str(&format!("{} {}", "#".repeat(*n), t.trim())),
            Block::Rule => out.push_str("---"),
            Block::Item(ordered, level, t, _, start) => {
                numbers.truncate(level + 1);
                numbers.resize(level + 1, 0);
                if numbers[*level] == 0 {
                    numbers[*level] = start.saturating_sub(1);
                }
                numbers[*level] += 1;
                let marker = if *ordered { format!("{}.", numbers[*level]) } else { "-".to_string() };
                out.push_str(&format!("{}{marker} {}", "   ".repeat(*level), t.trim()));
            }
            Block::Quote(t) => out.push_str(&t.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n")),
            Block::Code(t) => {
                if both_code {
                    // Joins the fence above: take it back and carry on.
                    out.truncate(out.trim_end_matches('\n').strip_suffix("```").map_or(out.len(), str::len));
                    out.push_str(&format!("{}\n```", t.trim_end_matches('\n')));
                } else {
                    out.push_str(&format!("```\n{}\n```", t.trim_end_matches('\n')));
                }
            }
            Block::Table(rows) => {
                let width = rows.iter().map(Vec::len).max().unwrap_or(0);
                let line = |r: &Vec<String>| format!("| {} |", (0..width).map(|i| r.get(i).map_or("", String::as_str)).collect::<Vec<_>>().join(" | "));
                out.push_str(&line(&rows[0]));
                out.push('\n');
                out.push_str(&format!("|{}|", vec![" --- "; width].join("|")));
                for r in &rows[1..] {
                    out.push('\n');
                    out.push_str(&line(r));
                }
            }
        }
        previous = Some(b);
    }
    out.push('\n');
    out
}
