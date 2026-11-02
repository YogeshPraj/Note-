//! The Settings window: the pages (built from the current settings) and what each change does.
//!
//! Every change applies at once and is saved. Editors that are too big for a row (colours, shortcuts, the
//! right-click menu) sit inside their page ([custom]); the encryption actions are buttons that rebuild the page.

mod custom;

use std::rc::Rc;

use notepp_core::sci::Theme;
use notepp_core::settings::Settings;

use crate::app::{App, Shared};
use crate::widgets::settings_window::{Control, MakePages, OnChange, Page, Row, Value};

/// What the pages are built from.
pub struct Env {
    pub settings: Settings,
    pub theme: Theme,
    pub dark: bool,
    /// What the Security page says about encryption, and whether the vault is unlocked.
    pub encryption: String,
    pub unlocked: bool,
    /// The application the embedded editors act on (none in tests).
    pub app: Option<Shared>,
}

const ENCODINGS: [(&str, &str); 3] = [("utf8", "UTF-8"), ("utf8bom", "UTF-8 with BOM"), ("ansi", "ANSI")];
const LINE_ENDINGS: [(&str, &str); 3] = [("crlf", "Windows (CR LF)"), ("lf", "Unix (LF)"), ("cr", "Mac (CR)")];

fn toggle(key: &'static str, title: &str, desc: &str, on: bool) -> Row {
    Row::new(key, title, desc, Control::Toggle(on))
}

fn number(key: &'static str, title: &str, desc: &str, value: i64, min: i64, max: i64) -> Row {
    Row::new(key, title, desc, Control::Number { value, min, max })
}

fn choice(key: &'static str, title: &str, desc: &str, options: &[(&str, &str)], current: &str) -> Row {
    let selected = options.iter().position(|o| o.0 == current).unwrap_or(0);
    Row::new(key, title, desc, Control::Choice { options: options.iter().map(|o| o.1.to_string()).collect(), selected })
}

fn button(key: &'static str, title: &str, desc: &str, label: &str) -> Row {
    Row::new(key, title, desc, Control::Button(label.into()))
}

/// The pages, filled from the current settings.
pub fn pages(env: &Env) -> Vec<Page> {
    let s = &env.settings;
    vec![general(s), editor(s), appearance(env), shortcuts(env), context_menu(env), security(env)]
}

fn general(s: &Settings) -> Page {
    Page::new(
        "general",
        "General",
        0,
        vec![
            Row::heading("New documents"),
            choice("encoding", "Encoding", "The encoding a new document starts with.", &ENCODINGS, &s.new_doc_encoding),
            choice("eol", "Line ending", "The line ending a new document starts with.", &LINE_ENDINGS, &s.new_doc_eol),
            Row::heading("Live preview"),
            toggle(
                "remote_images",
                "Load images from the web",
                "Show images that live on the internet in Markdown previews (applies after a restart).",
                s.preview_remote_images,
            ),
            Row::heading("Updates"),
            toggle("auto_update", "Check for updates automatically", "Look for a new version when the program starts.", s.auto_update),
        ],
    )
}

fn editor(s: &Settings) -> Page {
    Page::new(
        "editor",
        "Editor",
        0,
        vec![
            Row::heading("Font"),
            button("font", "Editor font", &format!("Now: {} {} pt.", s.font, s.font_size), "Choose font..."),
            number("font_size", "Font size", "The size of the editor text, in points.", i64::from(s.font_size), 6, 72),
            Row::heading("Indentation"),
            number("tab", "Tab size", "How many columns a tab stop is wide.", i64::from(s.tab_width), 1, 16),
            toggle("spaces", "Insert spaces instead of tabs", "Pressing Tab inserts spaces.", s.use_spaces),
            Row::heading("Guides and highlights"),
            number("edge", "Vertical guide column", "Draw a line at this column. 0 turns it off.", i64::from(s.edge_column), 0, 500),
            toggle("caret_line", "Highlight the current line", "Tint the line the cursor is on.", s.caret_line),
            toggle("smart", "Highlight matching words", "Mark every other occurrence of the selected word.", s.smart_highlight),
        ],
    )
}

fn appearance(env: &Env) -> Page {
    let (s, dark) = (&env.settings, env.dark);
    Page::new(
        "appearance",
        "Appearance",
        0,
        vec![
            Row::heading("Theme"),
            choice("theme", "Colour theme", "Light or dark.", &[("light", "Light"), ("dark", "Dark")], if dark { "dark" } else { "light" }),
            custom::colours(env.app.as_ref(), &env.theme),
            Row::heading("Window"),
            toggle("toolbar", "Toolbar", "Show the toolbar.", s.show_toolbar),
            toggle("tabbar", "Tab bar", "Show the tab bar.", s.show_tabbar),
            toggle("statusbar", "Status bar", "Show the status bar.", s.show_statusbar),
            Row::heading("Margins"),
            toggle("line_numbers", "Line numbers", "Show line numbers in the margin.", s.line_numbers),
            toggle("fold", "Fold margin", "Show the code folding margin.", s.fold_margin),
            toggle("bookmark", "Bookmark margin", "Show the bookmark margin.", s.bookmark_margin),
        ],
    )
}

fn shortcuts(env: &Env) -> Page {
    Page::new("shortcuts", "Shortcuts", 0, vec![custom::shortcuts(env.app.as_ref())])
}

fn context_menu(env: &Env) -> Page {
    Page::new("context", "Context menu", 0, vec![custom::context_menu(env.app.as_ref())])
}

fn security(env: &Env) -> Page {
    let s = &env.settings;
    Page::new(
        "security",
        "Security",
        0,
        vec![
            Row::heading("Encryption"),
            Row::new("", "Status", &env.encryption, Control::Heading),
            button("enc_setup", "Profile", "Set up the password that protects encrypted files, or replace the profile.", "Set up / replace profile..."),
            button(
                "enc_lock",
                "Lock",
                "Forget the key until the password is entered again, or enter it now.",
                if env.unlocked { "Lock now" } else { "Unlock..." },
            ),
            button("enc_password", "Password", "Choose a new password. Encrypted files do not need to be touched.", "Change password..."),
            button("enc_recovery", "Recovery key", "Create a new recovery key. The old one stops working.", "New recovery key..."),
            button("enc_import", "Import", "Use an encryption profile saved elsewhere.", "Import profile..."),
            Row::heading("Passwords"),
            number(
                "lock",
                "Lock the vault when idle",
                "Lock Bitwarden after this many idle minutes. 0 means never.",
                i64::from(s.passwords_auto_lock_min),
                0,
                1440,
            ),
        ],
    )
}
fn choice_key(options: &[(&str, &str)], i: usize) -> String {
    options.get(i).map_or(options[0].0, |o| o.0).to_string()
}

/// Apply one value to the settings. Returns false for keys that are not plain settings (buttons, the theme).
pub fn apply_value(s: &mut Settings, key: &str, v: &Value) -> bool {
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
        _ => return false,
    }
    true
}

/// Open the window on `page`.
pub fn open(app: &Shared, page: &str) {
    let Ok(frame) = app.try_borrow().map(|a| a.frame) else { return };
    let for_pages = app.clone();
    let make: MakePages = Rc::new(move || {
        let Ok(a) = for_pages.try_borrow() else { return Vec::new() };
        pages(&Env {
            settings: a.ws.settings.clone(),
            theme: a.ws.theme.clone(),
            dark: a.ws.theme.dark,
            encryption: crate::encdialogs::status_text(&a.ws.vault),
            unlocked: a.ws.vault.unlocked(),
            app: Some(for_pages.clone()),
        })
    });
    let a = app.clone();
    let on_change: OnChange = Rc::new(move |key, v| changed(&a, key, &v));
    crate::widgets::settings_window::show(&frame, "Settings", &make, page, &on_change);
}

fn encryption_action(key: &str) -> Option<crate::encdialogs::Action> {
    use crate::encdialogs::Action;
    Some(match key {
        "enc_setup" => Action::Setup,
        "enc_lock" => Action::Lock,
        "enc_password" => Action::Password,
        "enc_recovery" => Action::Recovery,
        "enc_import" => Action::Import,
        _ => return None,
    })
}

/// Apply one change. Returns true when the pages must be built again.
fn changed(app: &Shared, key: &str, v: &Value) -> bool {
    let Ok(mut a) = app.try_borrow_mut() else { return false };
    if let (Some(action), Value::Pressed) = (encryption_action(key), v) {
        let (frame, mut vault) = (a.frame, a.ws.vault.clone());
        crate::encdialogs::act(&frame, &mut vault, action);
        a.ws.vault = vault;
        return true;
    }
    match (key, v) {
        ("font", Value::Pressed) => a.choose_font(),
        ("colours_reset", Value::Pressed) => {
            crate::handlers::AppearanceCommands::apply_style(&mut a, &crate::settings_ui::StyleEdit { reset: true, entered: Vec::new() });
            return true;
        }
        ("theme", Value::Choice(i)) => {
            a.ws.set_theme(if *i == 1 { Theme::dark() } else { Theme::light() });
            a.refresh();
            return true;
        }
        _ => {
            let mut s = a.ws.settings.clone();
            if apply_value(&mut s, key, v) {
                s.clamp();
                crate::settings_ui::apply_settings(&mut a.ws, s);
                a.sync_chrome();
                a.refresh();
            }
        }
    }
    false
}
impl App {
    fn choose_font(&mut self) {
        let cur = &self.ws.settings;
        if let Some((name, pt)) = crate::settings_ui::pick_font(&self.frame, &cur.font, cur.font_size) {
            let mut s = cur.clone();
            s.font = name;
            s.font_size = pt;
            s.clamp();
            crate::settings_ui::apply_settings(&mut self.ws, s);
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(dark: bool) -> Env {
        Env { settings: Settings::default(), theme: Theme::light(), dark, encryption: String::new(), unlocked: false, app: None }
    }

    #[test]
    fn choices_toggles_and_numbers_land_in_the_settings() {
        let mut s = Settings::default();
        assert!(apply_value(&mut s, "encoding", &Value::Choice(1)));
        assert_eq!(s.new_doc_encoding, "utf8bom");
        assert!(apply_value(&mut s, "eol", &Value::Choice(2)));
        assert_eq!(s.new_doc_eol, "cr");
        assert!(apply_value(&mut s, "tab", &Value::Int(8)));
        assert_eq!(s.tab_width, 8);
        assert!(apply_value(&mut s, "spaces", &Value::Bool(true)));
        assert!(s.use_spaces);
        assert!(apply_value(&mut s, "lock", &Value::Int(30)));
        assert_eq!(s.passwords_auto_lock_min, 30);
    }

    #[test]
    fn buttons_and_unknown_keys_are_not_settings() {
        let mut s = Settings::default();
        assert!(!apply_value(&mut s, "colours", &Value::Pressed));
        assert!(!apply_value(&mut s, "nope", &Value::Bool(true)));
        assert!(!apply_value(&mut s, "tab", &Value::Bool(true)));
    }

    #[test]
    fn every_row_key_is_handled_or_a_known_action() {
        let actions = ["font", "theme", "enc_setup", "enc_lock", "enc_password", "enc_recovery", "enc_import"];
        for page in pages(&env(false)) {
            for row in page.rows.iter().filter(|r| !matches!(r.control, Control::Heading | Control::Custom(_))) {
                let mut s = Settings::default();
                let probe = match &row.control {
                    Control::Toggle(_) => Value::Bool(true),
                    Control::Number { value, .. } => Value::Int(*value),
                    Control::Choice { .. } => Value::Choice(0),
                    _ => Value::Pressed,
                };
                assert!(actions.contains(&row.key) || apply_value(&mut s, row.key, &probe), "row {} is not wired", row.key);
            }
        }
    }

    #[test]
    fn the_theme_row_follows_the_current_theme() {
        let selected = |dark| {
            pages(&env(dark)).iter().flat_map(|p| p.rows.iter()).find_map(|r| match &r.control {
                Control::Choice { selected, .. } if r.key == "theme" => Some(*selected),
                _ => None,
            })
        };
        assert_eq!(selected(false), Some(0));
        assert_eq!(selected(true), Some(1));
    }
}
