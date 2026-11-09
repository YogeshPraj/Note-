//! The Settings window: draws the pages `core::settings_pages` describes, and carries out what a change means.
//!
//! Every change applies at once and is saved. The editors that are too big for a row (colours, shortcuts, the right-click
//! menu) are built here ([custom]) where a page names them; the actions are buttons whose result rebuilds the page.

mod custom;

use std::rc::Rc;

use notepp_core::commands::ID_PRESET_BASE;
use notepp_core::sci::Theme;
use notepp_core::settings_pages::{self as sp, Action, Change, EncryptionAction, Facts};

use crate::app::{App, Shared};
use crate::widgets::settings_window::{Control, MakePages, OnChange, Page, Row, Value};

/// A row of `core`'s pages as the kit draws it. Editors are built for the application `app` (none in tests).
fn kit_row(row: sp::Row, app: Option<&Shared>, theme: &Theme) -> Row {
    let control = match row.control {
        sp::Control::Heading => Control::Heading,
        sp::Control::Toggle(on) => Control::Toggle(on),
        sp::Control::Number { value, min, max } => Control::Number { value, min, max },
        sp::Control::Choice { options, selected } => Control::Choice { options, selected },
        sp::Control::Dropdown { options, selected } => Control::Dropdown { options, selected },
        sp::Control::Button(label) => Control::Button(label),
        sp::Control::Editor(sp::Editor::Colours) => return custom::colours(app, theme),
        sp::Control::Editor(sp::Editor::Shortcuts) => return custom::shortcuts(app),
        sp::Control::Editor(sp::Editor::ContextMenu) => return custom::context_menu(app),
    };
    Row::new(row.key, &row.title, &row.desc, control)
}

fn kit_pages(pages: Vec<sp::Page>, app: Option<&Shared>, theme: &Theme) -> Vec<Page> {
    pages.into_iter().map(|p| Page::new(p.id, &p.title, p.level, p.rows.into_iter().map(|r| kit_row(r, app, theme)).collect())).collect()
}

/// The kit's report of a change, as `core` reads it. Text is never reported by these pages.
fn core_value(v: &Value) -> Option<sp::Value> {
    Some(match v {
        Value::Bool(b) => sp::Value::Bool(*b),
        Value::Int(n) => sp::Value::Int(*n),
        Value::Choice(i) => sp::Value::Choice(*i),
        Value::Pressed => sp::Value::Pressed,
        Value::Text(_) => return None,
    })
}

/// Open the window on `page`.
pub fn open(app: &Shared, page: &str) {
    let Ok(frame) = app.try_borrow().map(|a| a.frame) else { return };
    let for_pages = app.clone();
    let make: MakePages = Rc::new(move || {
        let Ok(a) = for_pages.try_borrow() else { return Vec::new() };
        let facts = Facts {
            settings: a.ws.settings.clone(),
            dark: a.ws.theme.dark,
            encryption: crate::encdialogs::status_text(&a.ws.vault),
            unlocked: a.ws.vault.unlocked(),
            agents_status: a.mcp_status(),
            agents_allowed: a.mcp_trusted(),
        };
        kit_pages(sp::pages(&facts), Some(&for_pages), &a.ws.theme)
    });
    let a = app.clone();
    let on_change: OnChange = Rc::new(move |key, v| changed(&a, key, &v));
    crate::widgets::settings_window::show(&frame, "Settings", &make, page, &on_change);
}

/// Apply one change. Returns true when the pages must be built again.
fn changed(app: &Shared, key: &str, v: &Value) -> bool {
    let Some(value) = core_value(v) else { return false };
    let Ok(mut a) = app.try_borrow_mut() else { return false };
    let mut s = a.ws.settings.clone();
    match sp::change(&mut s, key, &value) {
        Change::Nothing => false,
        Change::Setting { rebuild } => {
            crate::settings_ui::apply_settings(&mut a, s);
            a.sync_chrome();
            a.refresh();
            rebuild
        }
        Change::Action(action) => {
            act(&mut a, action);
            action.rebuilds()
        }
    }
}

/// Carry out an action of the window.
fn act(a: &mut App, action: Action) {
    match action {
        Action::ChooseFont => a.choose_font(),
        Action::Theme(i) => {
            crate::handlers::AppearanceCommands::preset(a, ID_PRESET_BASE + i);
            a.sync_chrome();
            a.refresh();
        }
        Action::ResetColours => {
            crate::handlers::AppearanceCommands::apply_style(a, &crate::settings_ui::StyleEdit { reset: true, entered: Vec::new() });
        }
        Action::Encryption(e) => encryption(a, e),
        Action::CopyAgentSetup | Action::NewAgentToken | Action::ForgetAgents | Action::ShowAgentLog => {
            match action {
                Action::CopyAgentSetup => a.mcp_copy_setup(),
                Action::NewAgentToken => a.mcp_new_token(),
                Action::ForgetAgents => a.mcp_forget_trusted(),
                _ => a.mcp_show_log(),
            }
            a.refresh();
        }
    }
}

/// The Security page's buttons: Lock locks at once when unlocked; the rest open their dialogs.
fn encryption(a: &mut App, e: EncryptionAction) {
    use crate::encdialogs::Action as Dialog;
    if e == EncryptionAction::Lock && a.ws.vault.unlocked() {
        if let Err(err) = a.ws.lock_encryption() {
            notepp_core::workspace::Ui::error(&a.ui, &err.to_string());
        }
        return;
    }
    let dialog = match e {
        EncryptionAction::Setup => Dialog::Setup,
        EncryptionAction::Lock => Dialog::Lock,
        EncryptionAction::Password => Dialog::Password,
        EncryptionAction::Recovery => Dialog::Recovery,
        EncryptionAction::Import => Dialog::Import,
    };
    let (frame, mut vault) = (a.frame, a.ws.vault.clone());
    crate::encdialogs::act(&frame, &mut vault, dialog);
    a.ws.vault = vault;
}

impl App {
    fn choose_font(&mut self) {
        let cur = &self.ws.settings;
        if let Some((name, pt)) = crate::settings_ui::pick_font(&self.frame, &cur.font, cur.font_size) {
            let mut s = cur.clone();
            s.font = name;
            s.font_size = pt;
            s.clamp();
            crate::settings_ui::apply_settings(self, s);
            self.refresh();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_and_row_reaches_the_kit_as_core_describes_it() {
        let pages = kit_pages(sp::pages(&Facts::default()), None, &Theme::light());
        let ids: Vec<&str> = pages.iter().map(|p| p.id).collect();
        assert_eq!(ids, ["general", "editor", "appearance", "colours", "shortcuts", "context", "security", "agents"]);
        let editors = pages.iter().flat_map(|p| p.rows.iter()).filter(|r| matches!(r.control, Control::Custom(_))).count();
        assert_eq!(editors, 3, "colours, shortcuts and the right-click menu are built here");
        let theme = pages.iter().flat_map(|p| p.rows.iter()).find(|r| r.key == "theme").map(|r| r.control.clone());
        assert!(matches!(theme, Some(Control::Dropdown { selected: 0, .. })));
    }

    #[test]
    fn the_kits_values_reach_core_and_text_does_not() {
        assert_eq!(core_value(&Value::Choice(2)), Some(sp::Value::Choice(2)));
        assert_eq!(core_value(&Value::Pressed), Some(sp::Value::Pressed));
        assert_eq!(core_value(&Value::Text("x".into())), None);
    }
}
