//! The Brain's list in the side panel: the recent days and the tags with how many entries carry each
//! (docs/brain-plan.md). A day opens the Brain's file; a tag runs a search for it.

use std::path::PathBuf;

use notepp_core::brain::store::{Index, Store};
use notepp_core::brain::{self, Note};
use notepp_core::crypto::Dek;

use super::{Activate, Content};

/// How many recent days the list shows.
const DAYS: usize = 14;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Note(PathBuf),
    Tag(String),
    Nothing,
}

#[derive(Default)]
pub struct BrainList {
    index: Index,
    rows: Vec<(Vec<String>, Target)>,
}

fn row(a: &str, b: &str, t: Target) -> (Vec<String>, Target) {
    (vec![a.to_string(), b.to_string()], t)
}

/// The rows for `notes`: recent days (newest first, with their entry counts), then tags.
fn build(notes: &[Note]) -> Vec<(Vec<String>, Target)> {
    let mut per_day: std::collections::BTreeMap<brain::Date, (usize, PathBuf)> = std::collections::BTreeMap::new();
    for n in notes {
        for day in brain::entries(&n.text).iter().filter_map(|e| e.day) {
            per_day.entry(day).or_insert_with(|| (0, n.path.clone())).0 += 1;
        }
    }
    let mut rows = Vec::new();
    for (day, (count, path)) in per_day.iter().rev().take(DAYS) {
        rows.push(row(&day.to_string(), &count.to_string(), Target::Note(path.clone())));
    }
    for (tag, count) in brain::tag_counts(notes) {
        rows.push(row(&format!("#{tag}"), &count.to_string(), Target::Tag(tag)));
    }
    if rows.is_empty() {
        rows.push(row("Nothing captured yet", "", Target::Nothing));
    }
    rows
}

impl BrainList {
    /// Read the notes again (only those that changed are read) and rebuild the rows. False when the notes cannot be read.
    pub fn reload(&mut self, store: &Store, key: Option<&Dek>) -> bool {
        if let Ok(notes) = self.index.notes(store, key) {
            self.rows = build(&notes);
            return true;
        }
        self.rows = vec![row("Unlock encryption to read Quick Notes", "", Target::Nothing)];
        false
    }

    /// The Brain is locked: say so, and show nothing else.
    pub fn locked(&mut self) {
        self.rows = vec![row("Quick Notes is locked", "", Target::Nothing)];
    }
}

impl Content for BrainList {
    fn columns(&self) -> Vec<(&'static str, i32)> {
        vec![("Quick Notes", super::SIDE_W - 74), ("", 50)]
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.rows.iter().map(|(c, _)| c.clone()).collect()
    }

    fn activate(&self, row: usize) -> Option<Activate> {
        match &self.rows.get(row)?.1 {
            Target::Note(p) => Some(Activate::Note(p.clone())),
            Target::Tag(t) => Some(Activate::Tag(t.clone())),
            Target::Nothing => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_come_newest_first_then_tags_with_counts() {
        let text = "# 2026-11-11 Wednesday\n\n## 09:00\nfix #deploy\n\n# 2026-11-12 Thursday\n\n## 09:00\nship #deploy\n\n## 10:00\nmore\n";
        let notes = [Note { path: PathBuf::from("quick-notes.md"), name: "brain".into(), text: text.into() }];
        let list = BrainList { rows: build(&notes), ..BrainList::default() };
        let names: Vec<String> = list.rows().into_iter().map(|r| r[0].clone()).collect();
        assert_eq!(names, ["2026-11-12", "2026-11-11", "#deploy"]);
        assert_eq!(list.rows()[0][1], "2");
        assert_eq!(list.rows()[2][1], "2");
        assert_eq!(list.activate(0), Some(Activate::Note("quick-notes.md".into())));
        assert_eq!(list.activate(2), Some(Activate::Tag("deploy".into())));
    }

    #[test]
    fn an_empty_brain_says_so_and_a_row_of_words_opens_nothing() {
        let list = BrainList { rows: build(&[]), ..BrainList::default() };
        assert_eq!(list.rows()[0][0], "Nothing captured yet");
        assert_eq!(list.activate(0), None);
    }
}
