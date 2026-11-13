//! Reading a note back: its entries (a heading and the text under it), `#tags` and `[[links]]`.

use super::date::Date;

/// One entry of a note: a heading and what is under it: one capture, or one section of a note the user wrote. Text before the first heading is an entry with an empty heading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The heading without its `#` marks.
    pub heading: String,
    /// `14:32` when the heading starts with a time, as captures do.
    pub time: Option<String>,
    /// The text under the heading, without the `from:` line.
    pub text: String,
    /// Where a captured selection came from (the `from:` line).
    pub from: Option<String>,
    /// Every `#tag` in the heading and the text, lower case, each once, in order of appearance.
    pub tags: Vec<String>,
    /// The day this entry is under: the last `# 2026-11-12 Thursday` heading before it.
    pub day: Option<Date>,
    /// The heading's line in the note, from 1 (the first line of the text for an entry without a heading).
    pub line: usize,
}

/// The heading on `line`, without its marks: `## 14:32 #x` gives `14:32 #x`. None when the line is not a heading.
fn heading_of(line: &str) -> Option<&str> {
    let marks = line.bytes().take_while(|b| *b == b'#').count();
    let rest = &line[marks..];
    ((1..=6).contains(&marks) && (rest.is_empty() || rest.starts_with(' '))).then(|| rest.trim())
}

/// `14:32` at the start of a heading.
fn time_of(heading: &str) -> Option<String> {
    let t = heading.get(..5)?;
    let b = t.as_bytes();
    let digits = b[0].is_ascii_digit() && b[1].is_ascii_digit() && b[3].is_ascii_digit() && b[4].is_ascii_digit();
    let ends = heading.len() == 5 || heading[5..].starts_with(' ');
    (digits && b[2] == b':' && ends).then(|| t.to_string())
}

/// The day a first-level heading names: `# 2026-11-12 Thursday` gives 2026-11-12.
fn day_of(line: &str) -> Option<Date> {
    line.strip_prefix("# ").and_then(|h| Date::parse(h.trim()))
}

/// The day of the last day heading in `note`.
pub fn last_day(note: &str) -> Option<Date> {
    note.lines().rev().find_map(day_of)
}

fn finish(heading: &str, line: usize, body: &[&str], day: Option<Date>) -> Option<Entry> {
    let from = body.iter().rev().find(|l| !l.trim().is_empty()).and_then(|l| l.strip_prefix("from: ")).map(|f| f.trim().to_string());
    let kept: Vec<&str> = body.iter().copied().filter(|l| from.is_none() || !l.starts_with("from: ")).collect();
    let text = kept.join("\n").trim().to_string();
    if heading.is_empty() && text.is_empty() {
        return None;
    }
    let tags = tags_in(&format!("{heading}\n{text}"));
    Some(Entry { heading: heading.to_string(), time: time_of(heading), text, from, tags, day, line })
}

/// The entries of `note`, in order. A day's title (`# 2026-11-12 Thursday`, a first-level heading with nothing under
/// it but other headings) is not an entry. Headings inside a fenced code block are text.
pub fn entries(note: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let (mut heading, mut at, mut body): (String, usize, Vec<&str>) = (String::new(), 1, Vec::new());
    let (mut fenced, mut title_pending) = (false, false);
    let (mut today, mut began): (Option<Date>, Option<Date>) = (None, None);
    for (i, line) in note.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if let Some(h) = heading_of(line).filter(|_| !fenced) {
            // A title with nothing of its own under it is the note's name, not an entry.
            let empty_title = title_pending && body.iter().all(|l| l.trim().is_empty());
            if !empty_title {
                out.extend(finish(&heading, at, &body, began));
            }
            let title = if fenced { None } else { day_of(line) };
            title_pending = title.is_some() || (line.starts_with("# ") && out.is_empty());
            today = title.or(today);
            began = today;
            (heading, at, body) = (h.to_string(), i + 1, Vec::new());
        } else {
            if heading.is_empty() && body.is_empty() && !line.trim().is_empty() {
                at = i + 1;
            }
            body.push(line);
        }
    }
    let empty_title = title_pending && body.iter().all(|l| l.trim().is_empty());
    if !empty_title {
        out.extend(finish(&heading, at, &body, began));
    }
    out
}

/// Every `#tag` in `text`, lower case, each once, in order. A tag starts at a `#` that begins a word and is followed
/// by a letter; it goes on through letters, digits, `_`, `-` and `/`. So `## heading`, `C#`, `#1` and the `#part` of
/// a web address are not tags.
pub fn tags_in(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let starts_word = i == 0 || chars[i - 1].is_whitespace() || matches!(chars[i - 1], '(' | '[' | '{' | ',' | ';');
        if chars[i] == '#' && starts_word && chars.get(i + 1).is_some_and(|c| c.is_alphabetic()) {
            let end = (i + 1..chars.len()).find(|&k| !(chars[k].is_alphanumeric() || matches!(chars[k], '_' | '-' | '/'))).unwrap_or(chars.len());
            let tag: String = chars[i + 1..end].iter().collect::<String>().trim_end_matches(['-', '/']).to_lowercase();
            if !tags.contains(&tag) {
                tags.push(tag);
            }
            i = end;
        } else {
            i += 1;
        }
    }
    tags
}

/// Every `[[name]]` in `text`: the names, each once, in order. `[[name|shown]]` links to `name`.
pub fn links_in(text: &str) -> Vec<String> {
    let mut links: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let after = &rest[open + 2..];
        let Some(close) = after.find("]]") else { break };
        let inner = &after[..close];
        let name = inner.split('|').next().unwrap_or("").trim();
        if !name.is_empty() && !name.contains('\n') && !links.iter().any(|l| l == name) {
            links.push(name.to_string());
        }
        rest = &after[close + 2..];
    }
    links
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str =
        "# 2026-11-12 Thursday\n\n## 14:32 #deploy #Azure\nThe slot needs the flag.\nfrom: D:\\work\\deploy.ps1:120\n\n## 16:05\nAsked Priya about #deploy.\n";

    #[test]
    fn a_days_captures_come_back_as_entries() {
        let e = entries(DAY);
        assert_eq!(e.len(), 2, "{e:?}");
        assert_eq!(e[0].time.as_deref(), Some("14:32"));
        assert_eq!(e[0].text, "The slot needs the flag.");
        assert_eq!(e[0].from.as_deref(), Some("D:\\work\\deploy.ps1:120"));
        assert_eq!(e[0].tags, ["deploy", "azure"]);
        assert_eq!(e[0].line, 3);
        assert_eq!((e[1].time.as_deref(), e[1].line, e[1].from.as_deref()), (Some("16:05"), 7, None));
        assert_eq!(e[1].tags, ["deploy"]);
    }

    #[test]
    fn what_the_user_wrote_without_a_heading_is_an_entry_too() {
        let e = entries("# 2026-11-12 Thursday\n\nloose words\n\n## 09:00\nx\n");
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].heading.as_str(), e[0].text.as_str(), e[0].line), ("2026-11-12 Thursday", "loose words", 1));
        let plain = entries("just text\nmore\n");
        assert_eq!((plain[0].heading.as_str(), plain[0].text.as_str(), plain[0].line), ("", "just text\nmore", 1));
    }

    #[test]
    fn a_topic_notes_sections_are_entries_and_code_is_not_cut_up() {
        let e = entries("# Azure\n\n## Slots\nswap\n```\n## not a heading\n```\n### Deeper\nmore\n");
        let got: Vec<(&str, &str)> = e.iter().map(|x| (x.heading.as_str(), x.text.as_str())).collect();
        assert_eq!(got, [("Slots", "swap\n```\n## not a heading\n```"), ("Deeper", "more")]);
        assert!(e.iter().all(|x| x.time.is_none()));
        assert!(entries("").is_empty() && entries("# Only a title\n").is_empty());
    }

    #[test]
    fn every_entry_knows_the_day_it_is_under() {
        let e = entries("# 2026-11-11 Wednesday\n\n## 09:00\na\n\n# 2026-11-12 Thursday\n\n## 09:00\nb\n\n## 10:00\nc\n");
        let days: Vec<String> = e.iter().map(|x| x.day.unwrap().to_string()).collect();
        assert_eq!(days, ["2026-11-11", "2026-11-12", "2026-11-12"], "{e:?}");
        assert_eq!(e[1].line, 8);
        assert_eq!(last_day("# 2026-11-11 x\n\n# 2026-11-12 y\n\n## 09:00\nz\n").unwrap().to_string(), "2026-11-12");
        assert_eq!(last_day("# Azure\n"), None);
    }

    #[test]
    fn tags_are_words_that_start_with_a_hash() {
        assert_eq!(tags_in("#one two #Two-b, (#three/x) #one"), ["one", "two-b", "three/x"]);
        assert!(tags_in("## heading, C#, #1, a#b, http://x.y/#part, # alone").is_empty());
        assert_eq!(tags_in("ends with dash #tag- and #päivä"), ["tag", "päivä"]);
    }

    #[test]
    fn links_name_topics() {
        assert_eq!(links_in("see [[Azure deploy]] and [[ Slots | the slots ]], again [[Azure deploy]]"), ["Azure deploy", "Slots"]);
        assert!(links_in("[[]] [[unclosed and [single]").is_empty());
    }
}
