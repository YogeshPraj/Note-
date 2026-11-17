//! The smaller parts of a Word file: style names, link targets and list numbering.

use std::collections::HashMap;

use super::xml::{Tok, attr, tokens};

/// Style id to style name, lower case.
pub(super) fn style_names(xml: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut current: Option<String> = None;
    for t in tokens(xml) {
        match t {
            Tok::Start(n, a) if n == "style" => current = attr(&a, "styleId").map(str::to_string),
            Tok::Start(n, a) if n == "name" => {
                if let (Some(id), Some(v)) = (&current, attr(&a, "val")) {
                    out.insert(id.clone(), v.to_lowercase());
                }
            }
            _ => {}
        }
    }
    out
}

pub(super) fn link_targets(xml: &str) -> HashMap<String, String> {
    tokens(xml)
        .into_iter()
        .filter_map(|t| match t {
            Tok::Start(n, a) if n == "Relationship" => Some((attr(&a, "Id")?.to_string(), attr(&a, "Target")?.to_string())),
            _ => None,
        })
        .collect()
}

/// Which list levels count (1, 2, 3) rather than bullet.
#[derive(Default)]
pub(super) struct Numbering {
    num_to_abstract: HashMap<String, String>,
    /// (abstract id, level) to whether it is a numbered level.
    ordered: HashMap<(String, usize), bool>,
    /// The first number of a level: the list's own override, else the style's.
    start: HashMap<(String, usize), usize>,
    over: HashMap<(String, usize), usize>,
}

impl Numbering {
    pub(super) fn read(xml: &str) -> Self {
        let mut n = Self::default();
        let (mut abstract_id, mut num_id, mut level): (Option<String>, Option<String>, usize) = (None, None, 0);
        let mut level_of_override = 0;
        for t in tokens(xml) {
            match t {
                Tok::Start(name, a) => match name.as_str() {
                    "abstractNum" => abstract_id = attr(&a, "abstractNumId").map(str::to_string),
                    "lvl" => level = attr(&a, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0),
                    "numFmt" => {
                        if let (Some(id), Some(f)) = (&abstract_id, attr(&a, "val")) {
                            n.ordered.entry((id.clone(), level)).or_insert(f != "bullet" && f != "none");
                        }
                    }
                    "num" => num_id = attr(&a, "numId").map(str::to_string),
                    "lvlOverride" => level_of_override = attr(&a, "ilvl").and_then(|v| v.parse().ok()).unwrap_or(0),
                    "start" if abstract_id.is_some() => {
                        if let (Some(id), Some(v)) = (&abstract_id, attr(&a, "val").and_then(|v| v.parse().ok())) {
                            n.start.entry((id.clone(), level)).or_insert(v);
                        }
                    }
                    "startOverride" => {
                        if let (Some(id), Some(v)) = (&num_id, attr(&a, "val").and_then(|v| v.parse().ok())) {
                            n.over.insert((id.clone(), level_of_override), v);
                        }
                    }
                    "abstractNumId" if num_id.is_some() => {
                        if let (Some(num), Some(v)) = (&num_id, attr(&a, "val")) {
                            n.num_to_abstract.insert(num.clone(), v.to_string());
                        }
                    }
                    _ => {}
                },
                Tok::End(name) => match name.as_str() {
                    "abstractNum" => abstract_id = None,
                    "num" => num_id = None,
                    _ => {}
                },
                Tok::Text(_) => {}
            }
        }
        n
    }

    pub(super) fn first(&self, num: &str, level: usize) -> usize {
        let from_style = self.num_to_abstract.get(num).and_then(|a| self.start.get(&(a.clone(), level)).copied());
        self.over.get(&(num.to_string(), level)).copied().or(from_style).unwrap_or(1)
    }

    pub(super) fn is_ordered(&self, num: &str, level: usize) -> bool {
        self.num_to_abstract.get(num).is_some_and(|a| self.ordered.get(&(a.clone(), level)).copied().unwrap_or(false))
    }
}
