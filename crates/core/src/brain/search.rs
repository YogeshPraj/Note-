//! Finding things again: a query of words, `#tags` and dates over every entry of every note.

use std::path::PathBuf;

use super::date::Date;
use super::parse::{Entry, entries};

/// How long a result's snippet may be, in characters.
const SNIPPET: usize = 160;

/// A note as the search reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub path: PathBuf,
    /// The file's name without `.md`.
    pub name: String,
    pub text: String,
}

/// What the user asked for. Everything must match.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    /// Words (lower case) that must each appear in the entry, its heading, its source or its note's name.
    pub words: Vec<String>,
    /// Tags (lower case) the entry must carry; `deploy` also finds `deploy/azure`.
    pub tags: Vec<String>,
    /// Only entries under a day from the first day to the last.
    pub days: Option<(Date, Date)>,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.tags.is_empty() && self.days.is_none()
    }
}

/// Read a query: `#tag`, `date:2026-11` (a day, a month or a year), `today`, `yesterday`, `week` (the last seven
/// days), `month` (the last thirty-one); every other word must appear in the entry.
pub fn parse_query(text: &str, today: Date) -> Query {
    let mut q = Query::default();
    for token in text.split_whitespace() {
        let lower = token.to_lowercase();
        if let Some(tag) = lower.strip_prefix('#').filter(|t| !t.is_empty()) {
            q.tags.push(tag.trim_end_matches('/').to_string());
        } else if let Some(span) = lower.strip_prefix("date:").and_then(Date::span) {
            q.days = Some(span);
        } else {
            match lower.as_str() {
                "today" => q.days = Some((today, today)),
                "yesterday" => q.days = Some((today.days_back(1), today.days_back(1))),
                "week" => q.days = Some((today.days_back(6), today)),
                "month" => q.days = Some((today.days_back(30), today)),
                _ => q.words.push(lower),
            }
        }
    }
    q
}

/// One result: where to go, and what to show for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    /// The entry's line in the note, from 1.
    pub line: usize,
    /// `2026-11-12 14:32` for a capture, `Azure › Slots` for a section of a note with no day.
    pub title: String,
    /// The line of the entry that best shows the match.
    pub snippet: String,
    pub tags: Vec<String>,
    pub day: Option<Date>,
}

fn has_tag(entry: &Entry, tag: &str) -> bool {
    entry.tags.iter().any(|t| t == tag || t.strip_prefix(tag).is_some_and(|rest| rest.starts_with('/')))
}

/// How well `entry` answers the query's words: None when a word is missing; a word in the heading or the note's name
/// counts double.
fn score(note: &Note, entry: &Entry, q: &Query) -> Option<usize> {
    let head = format!("{} {}", note.name, entry.heading).to_lowercase();
    let body = format!("{} {}", entry.text, entry.from.as_deref().unwrap_or("")).to_lowercase();
    let mut total = 0;
    for w in &q.words {
        total += match (head.contains(w.as_str()), body.contains(w.as_str())) {
            (true, _) => 2,
            (false, true) => 1,
            (false, false) => return None,
        };
    }
    Some(total)
}

fn clip(line: &str) -> String {
    let line = line.trim();
    if line.chars().count() <= SNIPPET { line.to_string() } else { format!("{}…", line.chars().take(SNIPPET - 1).collect::<String>().trim_end()) }
}

/// The first line of the entry that holds one of the words, else its first line.
fn snippet(entry: &Entry, q: &Query) -> String {
    let lines = || entry.text.lines().filter(|l| !l.trim().is_empty());
    let holding = lines().find(|l| {
        let low = l.to_lowercase();
        q.words.iter().any(|w| low.contains(w.as_str()))
    });
    clip(holding.or_else(|| lines().next()).unwrap_or(&entry.heading))
}

fn title(note: &Note, entry: &Entry) -> String {
    match (&entry.day, &entry.time) {
        (Some(day), Some(time)) => format!("{day} {time}"),
        (Some(day), None) => day.to_string(),
        (None, _) if entry.heading.is_empty() || entry.heading == note.name => note.name.clone(),
        (None, _) => format!("{} \u{203a} {}", note.name, entry.heading),
    }
}

/// The entries that answer `q`, best first: by how well the words match, then newest first (topics after days), at
/// most `limit`. An empty query gives the newest entries.
pub fn search(notes: &[Note], q: &Query, limit: usize) -> Vec<Hit> {
    let mut found: Vec<(usize, Hit)> = Vec::new();
    for note in notes {
        for entry in entries(&note.text) {
            if let Some((first, last)) = q.days
                && !entry.day.is_some_and(|d| first <= d && d <= last)
            {
                continue;
            }
            if !q.tags.iter().all(|t| has_tag(&entry, t)) {
                continue;
            }
            let Some(points) = score(note, &entry, q) else { continue };
            let hit = Hit {
                path: note.path.clone(),
                line: entry.line,
                title: title(note, &entry),
                snippet: snippet(&entry, q),
                tags: entry.tags.clone(),
                day: entry.day,
            };
            found.push((points, hit));
        }
    }
    found.sort_by(|(pa, a), (pb, b)| pb.cmp(pa).then(b.day.cmp(&a.day)).then(a.path.cmp(&b.path)).then(b.line.cmp(&a.line)));
    found.into_iter().take(limit).map(|(_, h)| h).collect()
}

/// Every tag with how many entries carry it: the most used first, then by name.
pub fn tag_counts(notes: &[Note]) -> Vec<(String, usize)> {
    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for entry in notes.iter().flat_map(|n| entries(&n.text)) {
        for tag in entry.tags {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    let mut list: Vec<(String, usize)> = counts.into_iter().collect();
    list.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(d: u32) -> Date {
        Date { year: 2026, month: 11, day: d }
    }

    fn notes() -> Vec<Note> {
        vec![
            Note {
                path: "quick-notes.md".into(),
                name: "brain".into(),
                text: "# 2026-11-10 Tuesday\n\n## 09:00 #deploy/azure\nThe swap fails without the flag.\n\n## 11:30\nLunch with the data team.\n\n# 2026-11-12 Thursday\n\n## 14:32 #deploy\nStaging slot: set the flag first.\nfrom: D:\\work\\deploy.ps1:120\n".into(),
            },
            Note {
                path: "topics/Azure.md".into(),
                name: "Azure".into(),
                text: "# Azure\n\n## Slots\nA slot swap is not instant. #deploy\n\n## Costs\nWatch the egress.\n".into(),
            },
        ]
    }

    #[test]
    fn a_query_is_words_tags_and_dates() {
        let today = day(12);
        let q = parse_query("Flag #Deploy date:2026-11 swap", today);
        assert_eq!(q, Query { words: vec!["flag".into(), "swap".into()], tags: vec!["deploy".into()], days: Some((day(1), day(30))) });
        assert_eq!(parse_query("today", today).days, Some((today, today)));
        assert_eq!(parse_query("yesterday", today).days, Some((day(11), day(11))));
        assert_eq!(parse_query("week", today).days, Some((day(6), today)));
        assert_eq!(parse_query("date:soon #", today), Query { words: vec!["date:soon".into(), "#".into()], ..Query::default() });
        assert!(parse_query("  ", today).is_empty());
    }

    #[test]
    fn nothing_asked_gives_the_newest_first() {
        let hits = search(&notes(), &Query::default(), 3);
        let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["2026-11-12 14:32", "2026-11-10 11:30", "2026-11-10 09:00"]);
        assert_eq!(hits[0].line, 11, "the line in the one file");
    }

    #[test]
    fn every_word_must_be_there_and_a_heading_counts_more() {
        let today = day(12);
        let hits = search(&notes(), &parse_query("slot", today), 10);
        let titles: Vec<&str> = hits.iter().map(|h| h.title.as_str()).collect();
        assert_eq!(titles, ["Azure \u{203a} Slots", "2026-11-12 14:32"], "the section called Slots comes first");
        assert_eq!(hits[1].snippet, "Staging slot: set the flag first.");
        assert!(search(&notes(), &parse_query("slot lunch", today), 10).is_empty());
        let by_source = search(&notes(), &parse_query("deploy.ps1", today), 10);
        assert_eq!(by_source.len(), 1, "where it came from is searched too");
    }

    #[test]
    fn a_tag_finds_its_children_and_a_date_keeps_to_day_notes() {
        let today = day(12);
        let tagged = search(&notes(), &parse_query("#deploy", today), 10);
        assert_eq!(tagged.len(), 3, "{tagged:?}");
        assert_eq!(search(&notes(), &parse_query("#deploy/azure", today), 10).len(), 1);
        assert!(search(&notes(), &parse_query("#dep", today), 10).is_empty(), "part of a tag's name is not the tag");
        let dated = search(&notes(), &parse_query("#deploy today", today), 10);
        assert_eq!(dated.iter().map(|h| h.title.as_str()).collect::<Vec<_>>(), ["2026-11-12 14:32"]);
    }

    #[test]
    fn tags_are_counted_per_entry() {
        assert_eq!(tag_counts(&notes()), [("deploy".to_string(), 2), ("deploy/azure".to_string(), 1)]);
    }

    #[test]
    fn a_long_line_is_cut_for_the_list() {
        let long = Note { path: "t.md".into(), name: "t".into(), text: format!("## h\n{}\n", "word ".repeat(100)) };
        let hit = &search(&[long], &Query::default(), 1)[0];
        assert!(hit.snippet.chars().count() <= SNIPPET && hit.snippet.ends_with('…'));
    }
}
