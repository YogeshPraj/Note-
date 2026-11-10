//! Command handlers for window-level actions, one type per concern.

use notepp_core::commands::{ID_ABOUT, ID_EXIT, ID_FULLSCREEN, ID_IMPORT_THEME, ID_MINIMAP, ID_PRESET_BASE, ID_PREVIEW, ID_RELOAD, ID_REPORT_ISSUE};
use notepp_core::sci::Theme;
use notepp_core::settings;

use crate::app::App;
use crate::command::CommandHandler;
use crate::settings_ui;

/// Exit, reload, minimap, preview and full screen.
pub struct ViewCommands;

impl CommandHandler<App> for ViewCommands {
    fn name(&self) -> &'static str {
        "view"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            ID_EXIT => crate::tray::quit(&app.frame),
            ID_RELOAD => {
                if app.ws.reload_from_disk(&app.ui) {
                    app.whiteboard_reloaded();
                    app.flash = Some("Reloaded from disk".into());
                }
            }
            ID_MINIMAP => app.ws.minimap = !app.ws.minimap,
            ID_PREVIEW => {
                let on = !app.preview.on;
                app.set_preview(on);
            }
            ID_FULLSCREEN => {
                let on = !app.frame.is_full_screen();
                app.frame.show_full_screen(on);
            }
            _ => return false,
        }
        true
    }
}

/// Preferences, the Style Configurator, theme import and the colour presets.
pub struct AppearanceCommands;

impl AppearanceCommands {
    /// Apply the colours edited in the settings window.
    pub(crate) fn apply_style(app: &mut App, e: &settings_ui::StyleEdit) {
        let dark = app.ws.theme.dark;
        let base = if dark { Theme::dark() } else { Theme::light() };
        let diff = if e.reset { std::collections::BTreeMap::default() } else { settings::diff_from(&base, &e.entered) };
        let bad = e.entered.iter().filter(|(_, h)| settings::parse_hex(h).is_none()).count();
        let mut s = app.ws.settings.clone();
        if dark {
            s.dark_colors = diff;
        } else {
            s.light_colors = diff;
        }
        settings_ui::apply_settings(app, s);
        if bad > 0 {
            app.flash = Some(format!("{bad} colour(s) were not valid #RRGGBB and were ignored"));
        }
    }

    fn import_theme(app: &mut App) {
        let Some(p) = notepp_core::workspace::Ui::pick_open(&app.ui).into_iter().next() else { return };
        let imported = std::fs::read_to_string(&p).map_err(|e| notepp_core::error::Error::Io(e.to_string())).and_then(|t| {
            let mut s = app.ws.settings.clone();
            notepp_core::tables::import_theme(&mut s, &t).map(|n| (s, n))
        });
        match imported {
            Ok((s, n)) => {
                settings_ui::apply_settings(app, s);
                app.flash = Some(format!("Imported {n} colour(s)"));
            }
            Err(e) => notepp_core::workspace::Ui::error(&app.ui, &format!("Cannot import the theme\n\n{e}")),
        }
    }

    pub(crate) fn preset(app: &mut App, id: usize) {
        let i = id - ID_PRESET_BASE;
        let (dark, preset) = match i {
            0 => (false, None),
            1 => (true, None),
            _ => (settings::PRESETS[i - 2].dark, Some(&settings::PRESETS[i - 2])),
        };
        let mut s = app.ws.settings.clone();
        s.apply_preset(dark, preset);
        settings_ui::apply_settings(app, s);
        app.ws.set_theme(if dark { Theme::dark() } else { Theme::light() });
        app.flash = Some(format!("Theme: {}", preset.map_or(if dark { "Default (dark)" } else { "Default (light)" }, |p| p.name)));
    }
}

impl CommandHandler<App> for AppearanceCommands {
    fn name(&self) -> &'static str {
        "appearance"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            ID_IMPORT_THEME => Self::import_theme(app),
            _ if (ID_PRESET_BASE..ID_PRESET_BASE + 2 + settings::PRESETS.len()).contains(&id) => Self::preset(app, id),
            _ => return false,
        }
        true
    }
}

/// The About box.
pub struct HelpCommands;

impl CommandHandler<App> for HelpCommands {
    fn name(&self) -> &'static str {
        "help"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        if id == ID_REPORT_ISSUE {
            let build = if cfg!(debug_assertions) { "debug" } else { "release" };
            crate::platform::current().open_default(&notepp_core::update::issue_url(&format!("{} {}", std::env::consts::OS, std::env::consts::ARCH), build));
            return true;
        }
        if id != ID_ABOUT {
            return false;
        }
        let build = if cfg!(debug_assertions) { "debug" } else { "release" };
        let text = format!(
            "Note++\nVersion {} ({build} build)\n\nA developer-focused text editor in Rust.\n\nAuthor: Yogesh Prajapati\nhttps://github.com/YogeshPraj/Note-",
            notepp_core::update::VERSION
        );
        crate::widgets::ask::tell(&app.frame, "About Note++", &text, crate::widgets::ask::Icon::Info);
        true
    }
}
