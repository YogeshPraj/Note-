//! Small tables and helpers shared by the generated code (copied from the Win32 shell).

#![allow(dead_code)]
use notepp_core::commands::*;
use notepp_core::doc::Enc;
use notepp_core::settings::Settings;
use notepp_core::{mdformat, settings};

pub const SLOT_NAMES: [&str; 5] = ["1st", "2nd", "3rd", "4th", "5th"];

/// (name, executable) for View > View Current File in.
pub const BROWSERS: [(&str, &str); 3] = [("Firefox", "firefox.exe"), ("Chrome", "chrome.exe"), ("Edge", "msedge.exe")];

/// The encodings offered by "Encode in" and "Convert to", in Notepad++'s order.
pub const ENCODE_IN: [Enc; 5] = [Enc::Ansi, Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Be, Enc::Utf16Le];

pub const WINDOW_DOCS_SHOWN: usize = 9;

/// The format a Markdown command id stands for.
pub fn md_fmt(id: usize) -> Option<mdformat::Fmt> {
    use mdformat::Fmt;
    Some(match id {
        ID_MD_BOLD => Fmt::Bold,
        ID_MD_ITALIC => Fmt::Italic,
        ID_MD_STRIKE => Fmt::Strike,
        ID_MD_CODE => Fmt::Code,
        ID_MD_H1 => Fmt::Heading(1),
        ID_MD_H2 => Fmt::Heading(2),
        ID_MD_H3 => Fmt::Heading(3),
        ID_MD_H4 => Fmt::Heading(4),
        ID_MD_H5 => Fmt::Heading(5),
        ID_MD_H6 => Fmt::Heading(6),
        ID_MD_QUOTE => Fmt::Quote,
        ID_MD_BULLET => Fmt::Bullet,
        ID_MD_NUMBERED => Fmt::Numbered,
        ID_MD_TASK => Fmt::Task,
        ID_MD_LINK => Fmt::Link,
        ID_MD_IMAGE => Fmt::Image,
        ID_MD_TABLE => Fmt::Table,
        ID_MD_CODEBLOCK => Fmt::CodeBlock,
        ID_MD_RULE => Fmt::Rule,
        _ => return None,
    })
}

/// Commands that make sense on a whiteboard tab (everything else targets the text editor).
pub fn wb_allows(id: usize) -> bool {
    matches!(
        id,
        ID_NEW | ID_NEW_WB | ID_OPEN | ID_SAVE | ID_SAVEAS | ID_SAVEALL | ID_CLOSE | ID_EXIT | ID_THEME | ID_NEXT_TAB | ID_PREV_TAB
            | ID_ABOUT | ID_PALETTE | ID_QUICKOPEN | ID_RECENT_CLEAR | ID_OPEN_FOLDER_EXPLORER | ID_OPEN_FOLDER_CMD
            | ID_OPEN_FOLDER_WS | ID_OPEN_VIEWER | ID_OPEN_WORKSPACE | ID_RELOAD | ID_SAVE_COPY | ID_RENAME | ID_CLOSE_ALL
            | ID_CLOSE_OTHERS | ID_CLOSE_LEFT | ID_CLOSE_RIGHT | ID_CLOSE_UNCHANGED | ID_RECYCLE | ID_LOAD_SESSION
            | ID_SAVE_SESSION | ID_RESTORE_CLOSED | ID_OPEN_ALL_RECENT | ID_WORKSPACE_TOGGLE | ID_PREFS | ID_STYLE_CFG | ID_SHORTCUTS
            | ID_IMPORT_THEME | ID_EXPORT_THEME | ID_EDIT_CONTEXT | ID_WINDOWS | ID_ENC_SETTINGS | ID_PASSWORDS | ID_PW_TOOLBAR | ID_PW_LOCK | ID_ENC_LOCK | ID_TAB_PIN | ID_TAB_COLOR_NONE | ID_TAB_REVEAL | ID_UPDATE_PILL | ID_UPDATE_CHECK | ID_UPDATE_AUTO
    ) || (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id)
        || (ID_WINDOW_DOC_BASE..ID_WINDOW_DOC_BASE + WINDOW_DOCS_SHOWN).contains(&id)
        || (ID_PRESET_BASE..ID_PRESET_BASE + 2 + settings::PRESETS.len()).contains(&id)
        || (ID_WINDOW_SORT_BASE..ID_WINDOW_SORT_BASE + 8).contains(&id)
}

pub const BINARY_FORMAT: &str = "Note++ Binary Text";


/// The theme colours as JSON ({"light": {...}, "dark": {...}}) for export.
pub fn theme_json(s: &Settings) -> String {
    let side = |base: notepp_core::sci::Theme, over: &std::collections::BTreeMap<String, String>| {
        let t = settings::themed(base, over);
        let map: serde_json::Map<String, serde_json::Value> =
            settings::ROLES.iter().filter_map(|(_, k)| Some((k.to_string(), serde_json::Value::String(settings::to_hex(settings::color_of(&t, k)?))))).collect();
        serde_json::Value::Object(map)
    };
    let v = serde_json::json!({ "light": side(notepp_core::sci::Theme::light(), &s.light_colors), "dark": side(notepp_core::sci::Theme::dark(), &s.dark_colors) });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}
