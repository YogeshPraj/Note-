//! Menu content for the commands that come in families: the recent files, the encodings, the languages.
//!
//! A provider turns current state into a list of [`MenuItemSpec`]s: a typed [`Command`] and its text. It holds no
//! action logic. Choosing an item runs `execute()` on its command, like every other command; the menu code only lays
//! the items out. A new family is one provider and one `Command` variant.

use std::path::PathBuf;

use crate::doc::CHARSETS;
use crate::lang::LANGUAGES;
use crate::registry::Command;
use crate::tables::ENCODE_IN;

/// One generated menu item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItemSpec {
    pub command: Command,
    /// Menu text (mnemonics included, literal ampersands doubled).
    pub label: String,
    /// The entry in the Command Palette, when the palette lists it.
    pub palette: Option<String>,
}

/// A titled submenu of generated items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuGroup {
    pub title: String,
    pub items: Vec<MenuItemSpec>,
}

pub trait MenuProvider {
    /// The items for the current state, in menu order.
    fn items(&self) -> Vec<MenuItemSpec>;
}

/// Providers whose items fall into submenus (character sets by region, languages by initial).
pub trait GroupedMenuProvider {
    fn groups(&self) -> Vec<MenuGroup>;
}

fn escape(text: &str) -> String {
    text.replace('&', "&&")
}

/// The recent-files list: `&1: path`, up to nine mnemonics and `1&0` for the tenth.
#[derive(Clone, Copy, Debug)]
pub struct RecentFiles<'a> {
    pub files: &'a [PathBuf],
}

impl MenuProvider for RecentFiles<'_> {
    fn items(&self) -> Vec<MenuItemSpec> {
        self.files
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mnemonic = if i < 9 { format!("&{}", i + 1) } else { "1&0".to_string() };
                MenuItemSpec { command: Command::OpenRecent(i), label: format!("{mnemonic}: {}", escape(&p.display().to_string())), palette: None }
            })
            .collect()
    }
}

/// "Encode in ..." (reopen the file in an encoding).
#[derive(Clone, Copy, Debug)]
pub struct EncodeInItems;

impl MenuProvider for EncodeInItems {
    fn items(&self) -> Vec<MenuItemSpec> {
        ENCODE_IN
            .iter()
            .enumerate()
            .map(|(i, e)| MenuItemSpec {
                command: Command::EncodeIn(i),
                label: format!("Encode in {}", e.label()),
                palette: Some(format!("Encoding: Encode in {}", e.label())),
            })
            .collect()
    }
}

/// "Convert to ..." (change the text's encoding).
#[derive(Clone, Copy, Debug)]
pub struct ConvertToItems;

impl MenuProvider for ConvertToItems {
    fn items(&self) -> Vec<MenuItemSpec> {
        ENCODE_IN
            .iter()
            .enumerate()
            .map(|(i, e)| MenuItemSpec {
                command: Command::ConvertTo(i),
                label: format!("Convert to {}", e.label()),
                palette: Some(format!("Encoding: Convert to {}", e.label())),
            })
            .collect()
    }
}

/// Character sets, one group per region; Windows-1252 (the ANSI code page) leads Western European.
#[derive(Clone, Copy, Debug)]
pub struct CharsetGroups;

impl GroupedMenuProvider for CharsetGroups {
    fn groups(&self) -> Vec<MenuGroup> {
        let mut groups: Vec<MenuGroup> = Vec::new();
        for (i, c) in CHARSETS.iter().enumerate() {
            let at = match groups.iter().position(|g| g.title == c.group) {
                Some(at) => at,
                None => {
                    let mut items = Vec::new();
                    if c.group == "Western European" {
                        items.push(MenuItemSpec { command: Command::SetCharset(CHARSETS.len()), label: "Windows-1252".into(), palette: None });
                    }
                    groups.push(MenuGroup { title: c.group.to_string(), items });
                    groups.len() - 1
                }
            };
            groups[at].items.push(MenuItemSpec {
                command: Command::SetCharset(i),
                label: c.label.to_string(),
                palette: Some(format!("Encoding: Character set {} ({})", c.label, c.group)),
            });
        }
        groups
    }
}

/// Languages: "Normal Text" first, then one group per initial letter, each sorted by name.
#[derive(Clone, Copy, Debug)]
pub struct Languages;

fn language_item(i: usize) -> MenuItemSpec {
    let label = LANGUAGES[i].0;
    MenuItemSpec { command: Command::SetLanguage(i), label: escape(label), palette: Some(format!("Language: {label}")) }
}

impl MenuProvider for Languages {
    /// Normal Text, which sits outside the lettered groups.
    fn items(&self) -> Vec<MenuItemSpec> {
        vec![MenuItemSpec { label: "Normal Text".into(), ..language_item(0) }]
    }
}

impl GroupedMenuProvider for Languages {
    fn groups(&self) -> Vec<MenuGroup> {
        let mut by_letter: std::collections::BTreeMap<char, Vec<usize>> = std::collections::BTreeMap::new();
        for (i, (label, _)) in LANGUAGES.iter().enumerate().skip(1) {
            by_letter.entry(label.chars().next().unwrap_or('#').to_ascii_uppercase()).or_default().push(i);
        }
        by_letter
            .into_iter()
            .map(|(letter, mut idx)| {
                idx.sort_by_key(|i| LANGUAGES[*i].0.to_lowercase());
                MenuGroup { title: letter.to_string(), items: idx.into_iter().map(language_item).collect() }
            })
            .collect()
    }
}

/// Every family item the Command Palette lists, in the order the palette shows them.
pub fn palette_items() -> Vec<MenuItemSpec> {
    let mut all: Vec<MenuItemSpec> = EncodeInItems.items().into_iter().zip(ConvertToItems.items()).flat_map(|(a, b)| [a, b]).collect();
    all.extend((0..CHARSETS.len()).map(|i| {
        let c = &CHARSETS[i];
        MenuItemSpec {
            command: Command::SetCharset(i),
            label: c.label.to_string(),
            palette: Some(format!("Encoding: Character set {} ({})", c.label, c.group)),
        }
    }));
    all.extend((0..LANGUAGES.len()).map(language_item));
    all
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_files_get_numbered_mnemonics_and_escaped_ampersands() {
        let files: Vec<PathBuf> = (0..11).map(|i| PathBuf::from(format!("a&b{i}.txt"))).collect();
        let items = RecentFiles { files: &files }.items();
        assert_eq!(items.len(), 11);
        assert_eq!(items[0].label, "&1: a&&b0.txt");
        assert_eq!(items[9].label, "1&0: a&&b9.txt");
        assert_eq!(items[3].command, Command::OpenRecent(3));
    }

    #[test]
    fn no_recent_files_no_items() {
        assert!(RecentFiles { files: &[] }.items().is_empty());
    }

    #[test]
    fn every_item_command_round_trips_through_its_id() {
        let mut all = EncodeInItems.items();
        all.extend(ConvertToItems.items());
        all.extend(CharsetGroups.groups().into_iter().flat_map(|g| g.items));
        all.extend(Languages.items());
        all.extend(Languages.groups().into_iter().flat_map(|g| g.items));
        assert!(all.len() > 50);
        for item in all {
            assert_eq!(Command::from_id(item.command.id()), Some(item.command), "{}", item.label);
        }
    }

    #[test]
    fn windows_1252_leads_western_european() {
        let groups = CharsetGroups.groups();
        let west = groups.iter().find(|g| g.title == "Western European").expect("group exists");
        assert_eq!(west.items[0].label, "Windows-1252");
        assert_eq!(west.items[0].command, Command::SetCharset(CHARSETS.len()));
    }

    #[test]
    fn languages_are_grouped_by_initial_and_sorted() {
        let groups = Languages.groups();
        let letters: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
        let mut sorted = letters.clone();
        sorted.sort_unstable();
        assert_eq!(letters, sorted);
        for g in &groups {
            let names: Vec<String> = g.items.iter().map(|i| i.label.to_lowercase()).collect();
            let mut s = names.clone();
            s.sort();
            assert_eq!(names, s);
        }
        assert_eq!(Languages.items()[0].command, Command::SetLanguage(0));
    }
}
