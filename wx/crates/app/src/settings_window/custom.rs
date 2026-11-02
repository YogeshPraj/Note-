//! The editors that sit inside the Settings window's pages: shortcuts, the right-click menu and colours.

use std::rc::Rc;

use notepp_core::sci::Theme;

use crate::app::Shared;
use crate::dialogs;
use crate::widgets::settings_window::{Builder, Control, Custom, Row, Value};

fn editor(title: &str, desc: &str, size: (i32, i32), build: Builder) -> Row {
    Row::new("", title, desc, Control::Custom(Custom { size, build }))
}

/// Run `f` on the application if nothing else is using it.
fn with_app(app: &Shared, f: impl FnOnce(&mut crate::app::App)) {
    if let Ok(mut a) = app.try_borrow_mut() {
        f(&mut a);
    }
}

pub fn shortcuts(app: Option<&Shared>) -> Row {
    let rows = app.and_then(|a| a.try_borrow().ok().map(|a| a.shortcut_rows())).unwrap_or_default();
    let app = app.cloned();
    let build: Builder = Rc::new(move |panel, _| {
        let Some(app) = app.clone() else { return };
        let apply: Rc<dyn Fn(Vec<dialogs::MapRow>)> = Rc::new(move |rows| with_app(&app, |a| a.apply_shortcuts(rows)));
        dialogs::embed_shortcut_mapper(panel, rows.clone(), apply);
    });
    editor("Keyboard shortcuts", "Select a command and press the keys you want. Changes apply at once.", dialogs::MAPPER_SIZE, build)
}

pub fn context_menu(app: Option<&Shared>) -> Row {
    let app = app.cloned();
    let build: Builder = Rc::new(move |panel, _| {
        let Some(app) = app.clone() else { return };
        let (all, current, default) = crate::app::App::context_menu_lists();
        let (save, text) = (app.clone(), app);
        let apply: Rc<dyn Fn(Vec<String>)> = Rc::new(move |menu| with_app(&save, |a| a.save_context_menu(&menu)));
        let open_text: Rc<dyn Fn()> = Rc::new(move || with_app(&text, crate::app::App::open_context_menu_file));
        dialogs::embed_context_menu(panel, all, current, default, apply, open_text);
    });
    editor(
        "Right-click menu",
        "Choose which commands the editor's right-click menu shows, and in what order. Changes apply at once.",
        dialogs::CONTEXT_SIZE,
        build,
    )
}

pub fn colours(app: Option<&Shared>, theme: &Theme) -> Row {
    let app = app.cloned();
    let theme = theme.clone();
    let frame = app.as_ref().and_then(|a| a.try_borrow().ok().map(|a| a.frame));
    let build: Builder = Rc::new(move |panel, report| {
        let (Some(app), Some(frame)) = (app.clone(), frame) else { return };
        let apply: Rc<dyn Fn(crate::settings_ui::StyleEdit)> = Rc::new(move |e| with_app(&app, |a| crate::handlers::AppearanceCommands::apply_style(a, &e)));
        let report = report.clone();
        let reset: Rc<dyn Fn()> = Rc::new(move || report("colours_reset", Value::Pressed));
        crate::settings_ui::embed_colours(panel, &frame, &theme, apply, reset);
    });
    editor(
        "Colours",
        "The colours of the theme in use, as #RRGGBB. Changes apply at once.",
        crate::settings_ui::colours_size(notepp_core::settings::ROLES.len()),
        build,
    )
}
