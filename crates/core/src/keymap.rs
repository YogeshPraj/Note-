//! The keymap: which key chord runs which command.
//!
//! It is built from the registry's defaults with the user's overrides (`settings.shortcuts`) applied on top. Two
//! commands on one chord cannot both work, so a clash is resolved and reported rather than silently shadowed: a
//! command that still has its default keeps it; between two overrides the lower command id wins; the loser's
//! override is dropped and it keeps its default. Menus show [`KeyMap::shown`], the effective shortcut.

use std::collections::{BTreeMap, HashMap};

use crate::registry::{self, SPECS};
use crate::shortcuts::{self, Chord};

/// Why an override was not applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// `command` asked for `chord`, but `held_by` has it; `command` keeps its default.
    Duplicate { command: usize, chord: String, held_by: usize },
    /// The text is not a shortcut; the command keeps its default.
    Invalid { command: usize, text: String },
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = |id: usize| registry::spec(id).map_or_else(|| format!("command {id}"), |s| s.palette.to_string());
        match self {
            Problem::Duplicate { command, chord, held_by } => write!(f, "{chord} is used by both {} and {}; {} keeps its default", name(*held_by), name(*command), name(*command)),
            Problem::Invalid { command, text } => write!(f, "\"{text}\" is not a shortcut; {} keeps its default", name(*command)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct KeyMap {
    /// Accepted overrides: the text to show and its chord (`None` = the shortcut was removed).
    overrides: BTreeMap<usize, (String, Option<Chord>)>,
    problems: Vec<Problem>,
}

impl KeyMap {
    /// Defaults from the registry plus `overrides` (command id as text -> shortcut text, "" removes the shortcut).
    pub fn build(overrides: &BTreeMap<String, String>) -> KeyMap {
        let mut accepted: BTreeMap<usize, (String, Option<Chord>)> = BTreeMap::new();
        let mut problems = Vec::new();
        for (key, text) in overrides {
            let Ok(id) = key.parse::<usize>() else { continue };
            let text = text.trim();
            if text.is_empty() {
                accepted.insert(id, (String::new(), None));
            } else if let Some(chord) = shortcuts::parse(text) {
                accepted.insert(id, (text.to_string(), Some(chord)));
            } else {
                problems.push(Problem::Invalid { command: id, text: text.to_string() });
            }
        }
        // Dropping a loser can hand its default chord back to a command that was using it, so repeat until stable.
        loop {
            let mut owners: HashMap<Chord, Vec<(usize, bool)>> = HashMap::new();
            for spec in SPECS {
                match accepted.get(&spec.id) {
                    Some((_, Some(chord))) => owners.entry(*chord).or_default().push((spec.id, true)),
                    Some((_, None)) => {}
                    None => {
                        if let Some(chord) = shortcuts::parse(spec.shortcut) {
                            owners.entry(chord).or_default().push((spec.id, false));
                        }
                    }
                }
            }
            for (id, (_, chord)) in &accepted {
                if registry::spec(*id).is_none() {
                    if let Some(chord) = chord {
                        owners.entry(*chord).or_default().push((*id, true));
                    }
                }
            }
            let mut dropped = Vec::new();
            for (chord, mut claims) in owners {
                if claims.len() < 2 {
                    continue;
                }
                claims.sort_by_key(|(id, overridden)| (*overridden, *id));
                let held_by = claims[0].0;
                for (id, overridden) in claims.into_iter().skip(1) {
                    if overridden {
                        dropped.push((id, shortcuts::format(&chord), held_by));
                    }
                }
            }
            if dropped.is_empty() {
                break;
            }
            for (command, chord, held_by) in dropped {
                accepted.remove(&command);
                problems.push(Problem::Duplicate { command, chord, held_by });
            }
        }
        problems.sort_by_key(|p| match p {
            Problem::Duplicate { command, .. } | Problem::Invalid { command, .. } => *command,
        });
        KeyMap { overrides: accepted, problems }
    }

    /// The chord that runs command `id` now.
    pub fn chord(&self, id: usize) -> Option<Chord> {
        match self.overrides.get(&id) {
            Some((_, chord)) => *chord,
            None => registry::spec(id).and_then(|s| shortcuts::parse(s.shortcut)),
        }
    }

    /// The shortcut text to show for `id` when the user changed it (empty = removed); `None` means the default.
    pub fn shown(&self, id: usize) -> Option<&str> {
        self.overrides.get(&id).map(|(text, _)| text.as_str())
    }

    /// The command a chord runs.
    pub fn command_for(&self, chord: &Chord) -> Option<usize> {
        let overridden = self.overrides.iter().find(|(_, (_, c))| c.as_ref() == Some(chord)).map(|(id, _)| *id);
        overridden.or_else(|| SPECS.iter().find(|s| !self.overrides.contains_key(&s.id) && shortcuts::parse(s.shortcut).as_ref() == Some(chord)).map(|s| s.id))
    }

    /// The accepted overrides in the form `settings.shortcuts` stores them.
    pub fn overrides(&self) -> BTreeMap<String, String> {
        self.overrides.iter().map(|(id, (text, _))| (id.to_string(), text.clone())).collect()
    }

    /// Overrides that could not be applied.
    pub fn problems(&self) -> &[Problem] {
        &self.problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::*;

    fn map(pairs: &[(usize, &str)]) -> KeyMap {
        KeyMap::build(&pairs.iter().map(|(id, t)| (id.to_string(), (*t).to_string())).collect())
    }

    fn chord(text: &str) -> Chord {
        shortcuts::parse(text).expect("a valid shortcut")
    }

    #[test]
    fn the_registry_defaults_do_not_clash() {
        let mut seen: HashMap<Chord, usize> = HashMap::new();
        for s in SPECS {
            if let Some(c) = shortcuts::parse(s.shortcut) {
                if let Some(other) = seen.insert(c, s.id) {
                    panic!("{} and {} both default to {}", other, s.id, s.shortcut);
                }
            }
        }
    }

    #[test]
    fn without_overrides_the_defaults_apply() {
        let k = map(&[]);
        assert_eq!(k.chord(ID_SAVE), Some(chord("Ctrl+S")));
        assert_eq!(k.shown(ID_SAVE), None);
        assert_eq!(k.command_for(&chord("Ctrl+S")), Some(ID_SAVE));
        assert!(k.problems().is_empty());
    }

    #[test]
    fn an_override_replaces_the_default() {
        let k = map(&[(ID_SAVE, "Ctrl+Alt+K")]);
        assert_eq!(k.chord(ID_SAVE), Some(chord("Ctrl+Alt+K")));
        assert_eq!(k.shown(ID_SAVE), Some("Ctrl+Alt+K"));
        assert_eq!(k.command_for(&chord("Ctrl+Alt+K")), Some(ID_SAVE));
        assert_eq!(k.command_for(&chord("Ctrl+S")), None, "the old chord is free");
    }

    #[test]
    fn an_empty_override_removes_the_shortcut() {
        let k = map(&[(ID_SAVE, "")]);
        assert_eq!(k.chord(ID_SAVE), None);
        assert_eq!(k.shown(ID_SAVE), Some(""));
    }

    #[test]
    fn a_chord_taken_by_a_default_is_refused_and_the_loser_keeps_its_default() {
        // Ctrl+O is Open's default.
        let k = map(&[(ID_SAVE, "Ctrl+O")]);
        assert_eq!(k.chord(ID_SAVE), Some(chord("Ctrl+S")));
        assert_eq!(k.shown(ID_SAVE), None);
        assert_eq!(k.command_for(&chord("Ctrl+O")), Some(ID_OPEN));
        assert_eq!(k.problems(), [Problem::Duplicate { command: ID_SAVE, chord: "Ctrl+O".into(), held_by: ID_OPEN }]);
    }

    #[test]
    fn two_overrides_on_one_chord_leave_the_lower_id() {
        let (a, b) = (ID_NEW, ID_SAVE);
        let k = map(&[(a, "Ctrl+Alt+J"), (b, "Ctrl+Alt+J")]);
        let (winner, loser) = if a < b { (a, b) } else { (b, a) };
        assert_eq!(k.chord(winner), Some(chord("Ctrl+Alt+J")));
        assert_eq!(k.shown(loser), None, "the loser's override is dropped");
        assert!(matches!(k.problems(), [Problem::Duplicate { command, held_by, .. }] if *command == loser && *held_by == winner));
    }

    #[test]
    fn moving_a_default_away_frees_it_for_another_command() {
        // Open gives up Ctrl+O, Save takes it: no clash.
        let k = map(&[(ID_OPEN, "Ctrl+Alt+O"), (ID_SAVE, "Ctrl+O")]);
        assert!(k.problems().is_empty());
        assert_eq!(k.chord(ID_SAVE), Some(chord("Ctrl+O")));
        assert_eq!(k.chord(ID_OPEN), Some(chord("Ctrl+Alt+O")));
    }

    #[test]
    fn swapping_two_defaults_works() {
        let k = map(&[(ID_OPEN, "Ctrl+S"), (ID_SAVE, "Ctrl+O")]);
        assert!(k.problems().is_empty());
        assert_eq!(k.command_for(&chord("Ctrl+S")), Some(ID_OPEN));
        assert_eq!(k.command_for(&chord("Ctrl+O")), Some(ID_SAVE));
    }

    #[test]
    fn text_that_is_not_a_shortcut_is_reported_and_ignored() {
        let k = map(&[(ID_SAVE, "Banana")]);
        assert_eq!(k.chord(ID_SAVE), Some(chord("Ctrl+S")));
        assert!(matches!(k.problems(), [Problem::Invalid { command, .. }] if *command == ID_SAVE));
    }

    #[test]
    fn overrides_round_trip_to_the_settings_form() {
        let k = map(&[(ID_SAVE, "Ctrl+Alt+K"), (ID_NEW, "")]);
        let o = k.overrides();
        assert_eq!(o.get(&ID_SAVE.to_string()).map(String::as_str), Some("Ctrl+Alt+K"));
        assert_eq!(o.get(&ID_NEW.to_string()).map(String::as_str), Some(""));
    }

    #[test]
    fn problems_read_as_sentences() {
        let k = map(&[(ID_SAVE, "Ctrl+O")]);
        let text = k.problems()[0].to_string();
        assert!(text.contains("Ctrl+O") && text.contains("keeps its default"), "{text}");
    }
}
