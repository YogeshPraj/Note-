//! What a change in the Settings window means: a setting that now has a new value, or an action for the shell to carry out.

use super::{ENCODINGS, LINE_ENDINGS, theme_names};
use crate::mcp::DEFAULT_PORT;
use crate::mcp::access::Level;
use crate::settings::Settings;

/// A value the user entered, as the shell reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Choice(usize),
    Pressed,
}

/// What the Security page's buttons ask for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncryptionAction {
    Setup,
    /// Lock when unlocked, ask for the password when locked.
    Lock,
    Password,
    Recovery,
    Import,
}

/// Something for the shell to do, often with a dialog of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ChooseFont,
    /// Use theme `n` of `theme_names`.
    Theme(usize),
    ResetColours,
    Encryption(EncryptionAction),
    CopyAgentSetup,
    NewAgentToken,
    ForgetAgents,
    ShowAgentLog,
}

impl Action {
    /// After this, the pages say something new (a button label, a status line), so they must be built again.
    pub fn rebuilds(self) -> bool {
        !matches!(self, Action::ChooseFont)
    }
}

/// What a change means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The settings were changed (and kept in range): apply them. `rebuild` when a page shows the result (the agents' status).
    Setting {
        rebuild: bool,
    },
    Action(Action),
    /// Not a change this window knows: nothing to do.
    Nothing,
}

fn choice_key(options: &[(&str, &str)], i: usize) -> String {
    options.get(i).map_or(options[0].0, |o| o.0).to_string()
}

/// The button actions, by row key.
fn action_for(key: &str) -> Option<Action> {
    Some(match key {
        "font" => Action::ChooseFont,
        "colours_reset" => Action::ResetColours,
        "enc_setup" => Action::Encryption(EncryptionAction::Setup),
        "enc_lock" => Action::Encryption(EncryptionAction::Lock),
        "enc_password" => Action::Encryption(EncryptionAction::Password),
        "enc_recovery" => Action::Encryption(EncryptionAction::Recovery),
        "enc_import" => Action::Encryption(EncryptionAction::Import),
        "mcp_copy" => Action::CopyAgentSetup,
        "mcp_token" => Action::NewAgentToken,
        "mcp_forget" => Action::ForgetAgents,
        "mcp_log" => Action::ShowAgentLog,
        _ => return None,
    })
}

/// Set a plain setting from row `key`. False when `key` is not one, or the value is not of its kind.
fn set(s: &mut Settings, key: &str, v: &Value) -> bool {
    let to_i32 = |n: i64| i32::try_from(n).unwrap_or(0);
    match (key, v) {
        ("encoding", Value::Choice(i)) => s.new_doc_encoding = choice_key(&ENCODINGS, *i),
        ("eol", Value::Choice(i)) => s.new_doc_eol = choice_key(&LINE_ENDINGS, *i),
        ("remote_images", Value::Bool(b)) => s.preview_remote_images = *b,
        ("auto_update", Value::Bool(b)) => s.auto_update = *b,
        ("font_size", Value::Int(n)) => s.font_size = to_i32(*n),
        ("tab", Value::Int(n)) => s.tab_width = to_i32(*n),
        ("spaces", Value::Bool(b)) => s.use_spaces = *b,
        ("edge", Value::Int(n)) => s.edge_column = to_i32(*n),
        ("caret_line", Value::Bool(b)) => s.caret_line = *b,
        ("smart", Value::Bool(b)) => s.smart_highlight = *b,
        ("toolbar", Value::Bool(b)) => s.show_toolbar = *b,
        ("tabbar", Value::Bool(b)) => s.show_tabbar = *b,
        ("statusbar", Value::Bool(b)) => s.show_statusbar = *b,
        ("line_numbers", Value::Bool(b)) => s.line_numbers = *b,
        ("fold", Value::Bool(b)) => s.fold_margin = *b,
        ("bookmark", Value::Bool(b)) => s.bookmark_margin = *b,
        ("lock", Value::Int(n)) => s.passwords_auto_lock_min = u32::try_from(*n).unwrap_or(0),
        ("mcp_on", Value::Bool(b)) => s.mcp_enabled = *b,
        ("mcp_level", Value::Choice(i)) => s.mcp_level = Level::ALL.get(*i).copied().unwrap_or(Level::Read).key().to_string(),
        ("mcp_port", Value::Int(n)) => s.mcp_port = u16::try_from(*n).unwrap_or(DEFAULT_PORT),
        _ => return false,
    }
    true
}

/// What the user's change to row `key` means. A setting is changed in `s` and kept in range.
pub fn change(s: &mut Settings, key: &str, v: &Value) -> Change {
    if let (Some(action), Value::Pressed) = (action_for(key), v) {
        return Change::Action(action);
    }
    if let ("theme", Value::Choice(i)) = (key, v) {
        // The extra "Custom" entry is where the user already is.
        return if *i < theme_names().len() { Change::Action(Action::Theme(*i)) } else { Change::Nothing };
    }
    if set(s, key, v) {
        s.clamp();
        return Change::Setting { rebuild: key == "mcp_on" };
    }
    Change::Nothing
}

#[cfg(test)]
mod tests {
    use super::super::{Control, Facts, active_theme, pages};
    use super::*;
    use crate::settings::PRESETS;

    #[test]
    fn choices_toggles_and_numbers_land_in_the_settings() {
        let mut s = Settings::default();
        assert_eq!(change(&mut s, "encoding", &Value::Choice(1)), Change::Setting { rebuild: false });
        assert_eq!(s.new_doc_encoding, "utf8bom");
        change(&mut s, "eol", &Value::Choice(2));
        assert_eq!(s.new_doc_eol, "cr");
        change(&mut s, "tab", &Value::Int(8));
        assert_eq!(s.tab_width, 8);
        change(&mut s, "spaces", &Value::Bool(true));
        assert!(s.use_spaces);
        change(&mut s, "lock", &Value::Int(30));
        assert_eq!(s.passwords_auto_lock_min, 30);
    }

    #[test]
    fn values_are_kept_in_range() {
        let mut s = Settings::default();
        change(&mut s, "font_size", &Value::Int(500));
        assert_eq!(s.font_size, 72);
    }

    #[test]
    fn buttons_are_actions_and_unknown_keys_or_wrong_kinds_are_nothing() {
        let mut s = Settings::default();
        assert_eq!(change(&mut s, "font", &Value::Pressed), Change::Action(Action::ChooseFont));
        assert_eq!(change(&mut s, "enc_lock", &Value::Pressed), Change::Action(Action::Encryption(EncryptionAction::Lock)));
        assert_eq!(change(&mut s, "mcp_copy", &Value::Pressed), Change::Action(Action::CopyAgentSetup));
        assert_eq!(change(&mut s, "nope", &Value::Bool(true)), Change::Nothing);
        assert_eq!(change(&mut s, "tab", &Value::Bool(true)), Change::Nothing);
        assert_eq!(s, Settings::default(), "nothing was set");
    }

    #[test]
    fn a_theme_is_an_action_and_the_custom_entry_is_nothing() {
        let mut s = Settings::default();
        assert_eq!(change(&mut s, "theme", &Value::Choice(2)), Change::Action(Action::Theme(2)));
        assert_eq!(change(&mut s, "theme", &Value::Choice(theme_names().len())), Change::Nothing);
    }

    #[test]
    fn turning_agents_on_or_off_rebuilds_the_page_that_says_so() {
        let mut s = Settings::default();
        assert_eq!(change(&mut s, "mcp_on", &Value::Bool(true)), Change::Setting { rebuild: true });
        assert!(s.mcp_enabled);
        change(&mut s, "mcp_level", &Value::Choice(9));
        assert_eq!(Level::from_key(&s.mcp_level), Level::Read, "anything unknown is the lowest level");
        change(&mut s, "mcp_port", &Value::Int(99999));
        assert_eq!(s.mcp_port, DEFAULT_PORT);
        assert!(!Action::ChooseFont.rebuilds() && Action::NewAgentToken.rebuilds());
    }

    #[test]
    fn every_row_does_something() {
        for page in pages(&Facts::default()) {
            for row in page.rows.iter().filter(|r| !matches!(r.control, Control::Heading | Control::Editor(_))) {
                let mut s = Settings::default();
                let probe = match &row.control {
                    Control::Toggle(_) => Value::Bool(true),
                    Control::Number { value, .. } => Value::Int(*value),
                    Control::Choice { .. } | Control::Dropdown { .. } => Value::Choice(0),
                    _ => Value::Pressed,
                };
                assert_ne!(change(&mut s, row.key, &probe), Change::Nothing, "row {} does nothing", row.key);
            }
        }
    }

    #[test]
    fn the_theme_row_follows_the_current_theme() {
        let selected = |dark| {
            pages(&Facts { dark, ..Facts::default() }).iter().flat_map(|p| p.rows.iter()).find_map(|r| match &r.control {
                Control::Dropdown { selected, .. } if r.key == "theme" => Some(*selected),
                _ => None,
            })
        };
        assert_eq!(selected(false), Some(0));
        assert_eq!(selected(true), Some(1));
    }

    #[test]
    fn a_preset_is_found_again_and_edited_colours_are_custom() {
        let mut s = Settings::default();
        s.apply_preset(true, Some(&PRESETS[0]));
        assert_eq!(active_theme(&s, true), 2);
        s.dark_colors.insert("fg".into(), "123456".into());
        assert_eq!(active_theme(&s, true), theme_names().len());
    }

    #[test]
    fn the_pages_say_what_the_facts_say() {
        let facts = Facts { unlocked: true, agents_allowed: "Allowed: claude.".into(), ..Facts::default() };
        let rows: Vec<_> = pages(&facts).into_iter().flat_map(|p| p.rows).collect();
        let row = |key: &str| rows.iter().find(|r| r.key == key).cloned();
        assert_eq!(row("enc_lock").map(|r| r.control), Some(Control::Button("Lock now".into())));
        assert_eq!(row("mcp_forget").map(|r| r.desc), Some("Allowed: claude.".into()));
    }
}
