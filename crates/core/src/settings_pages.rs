//! The Settings window as plain data: its pages and rows, built from the current settings, and what each change means
//! (`change`). Any toolkit can draw it (see `docs/winui3-plan.md`, phase 1): the shell turns rows into its controls, builds
//! the three editors that are too big for a row where a row names them, and carries out the actions.

use crate::mcp::access::Level;
use crate::settings::{PRESETS, Settings};

mod change;
pub use change::*;

/// What a row lets the user set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control {
    /// A section heading between groups of rows (no value).
    Heading,
    Toggle(bool),
    Number {
        value: i64,
        min: i64,
        max: i64,
    },
    /// One of a few options, shown side by side.
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    /// One of many options, in a list that drops down.
    Dropdown {
        options: Vec<String>,
        selected: usize,
    },
    /// A push button with this label.
    Button(String),
    /// An editor the shell builds itself, too big for a row.
    Editor(Editor),
}

/// The editors that sit inside a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Editor {
    Colours,
    Shortcuts,
    ContextMenu,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Names the row in change reports; empty for headings and editors.
    pub key: &'static str,
    pub title: String,
    pub desc: String,
    pub control: Control,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub id: &'static str,
    pub title: String,
    /// Nesting in the navigation (0 = top level).
    pub level: u8,
    pub rows: Vec<Row>,
}

/// What the pages are built from, besides the settings.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    pub settings: Settings,
    /// Dark mode is on: the theme row shows the dark themes' choice.
    pub dark: bool,
    /// What the Security page says about encryption, and whether the vault is unlocked.
    pub encryption: String,
    pub unlocked: bool,
    /// What the AI Agents page says about the server, and which agents are allowed.
    pub agents_status: String,
    pub agents_allowed: String,
}

pub(crate) const ENCODINGS: [(&str, &str); 3] = [("utf8", "UTF-8"), ("utf8bom", "UTF-8 with BOM"), ("ansi", "ANSI")];
pub(crate) const LINE_ENDINGS: [(&str, &str); 3] = [("crlf", "Windows (CR LF)"), ("lf", "Unix (LF)"), ("cr", "Mac (CR)")];

fn row(key: &'static str, title: &str, desc: &str, control: Control) -> Row {
    Row { key, title: title.into(), desc: desc.into(), control }
}

fn heading(title: &str) -> Row {
    row("", title, "", Control::Heading)
}

fn toggle(key: &'static str, title: &str, desc: &str, on: bool) -> Row {
    row(key, title, desc, Control::Toggle(on))
}

fn number(key: &'static str, title: &str, desc: &str, value: i64, min: i64, max: i64) -> Row {
    row(key, title, desc, Control::Number { value, min, max })
}

fn choice(key: &'static str, title: &str, desc: &str, options: &[(&str, &str)], current: &str) -> Row {
    let selected = options.iter().position(|o| o.0 == current).unwrap_or(0);
    row(key, title, desc, Control::Choice { options: options.iter().map(|o| o.1.to_string()).collect(), selected })
}

fn button(key: &'static str, title: &str, desc: &str, label: &str) -> Row {
    row(key, title, desc, Control::Button(label.into()))
}

fn page(id: &'static str, title: &str, level: u8, rows: Vec<Row>) -> Page {
    Page { id, title: title.into(), level, rows }
}

/// The ready-made themes in menu order: the two defaults, then the presets.
pub fn theme_names() -> Vec<String> {
    ["Default (light)", "Default (dark)"].into_iter().map(String::from).chain(PRESETS.iter().map(|p| p.name.to_string())).collect()
}

/// The index in `theme_names` of the theme the settings hold, or one past the end ("Custom") when the colours were edited.
pub fn active_theme(s: &Settings, dark: bool) -> usize {
    let held = if dark { &s.dark_colors } else { &s.light_colors };
    let candidates = [(false, None), (true, None)].into_iter().chain(PRESETS.iter().map(|p| (p.dark, Some(p))));
    for (i, (d, p)) in candidates.enumerate() {
        let mut probe = s.clone();
        probe.apply_preset(d, p);
        if d == dark && (if d { &probe.dark_colors } else { &probe.light_colors }) == held {
            return i;
        }
    }
    theme_names().len()
}

fn themes(s: &Settings, dark: bool) -> Row {
    let mut options = theme_names();
    let selected = active_theme(s, dark);
    if selected == options.len() {
        options.push("Custom".into());
    }
    row("theme", "Colour theme", "Colours of the editor text, margins and syntax.", Control::Dropdown { options, selected })
}

/// The pages, filled from the current settings.
pub fn pages(f: &Facts) -> Vec<Page> {
    let s = &f.settings;
    vec![
        general(s),
        editor(s),
        appearance(s, f.dark),
        page("colours", "Colours", 1, vec![editor_row(Editor::Colours)]),
        page("shortcuts", "Shortcuts", 0, vec![editor_row(Editor::Shortcuts)]),
        page("context", "Context menu", 0, vec![editor_row(Editor::ContextMenu)]),
        security(f),
        brain(f),
        agents(f),
    ]
}

fn editor_row(e: Editor) -> Row {
    let (title, desc) = match e {
        Editor::Colours => ("Colours", "The colours of the theme in use, as #RRGGBB. Changes apply at once."),
        Editor::Shortcuts => ("Keyboard shortcuts", "Select a command and press the keys you want. Changes apply at once."),
        Editor::ContextMenu => ("Right-click menu", "Choose which commands the editor's right-click menu shows, and in what order. Changes apply at once."),
    };
    row("", title, desc, Control::Editor(e))
}

fn general(s: &Settings) -> Page {
    page(
        "general",
        "General",
        0,
        vec![
            heading("New documents"),
            choice("encoding", "Encoding", "The encoding a new document starts with.", &ENCODINGS, &s.new_doc_encoding),
            choice("eol", "Line ending", "The line ending a new document starts with.", &LINE_ENDINGS, &s.new_doc_eol),
            heading("Live preview"),
            toggle(
                "remote_images",
                "Load images from the web",
                "Show images that live on the internet in Markdown previews (applies after a restart).",
                s.preview_remote_images,
            ),
            heading("Updates"),
            toggle(
                "auto_update",
                "Update automatically",
                "Look for a new version in the background, download it, and install it the next time you start Note++.",
                s.auto_update,
            ),
        ],
    )
}

fn editor(s: &Settings) -> Page {
    page(
        "editor",
        "Editor",
        0,
        vec![
            heading("Font"),
            button("font", "Editor font", &format!("Now: {} {} pt.", s.font, s.font_size), "Choose font..."),
            number("font_size", "Font size", "The size of the editor text, in points.", i64::from(s.font_size), 6, 72),
            heading("Indentation"),
            number("tab", "Tab size", "How many columns a tab stop is wide.", i64::from(s.tab_width), 1, 16),
            toggle("spaces", "Insert spaces instead of tabs", "", s.use_spaces),
            heading("Guides and highlights"),
            number("edge", "Vertical guide column", "Draw a line at this column. 0 turns it off.", i64::from(s.edge_column), 0, 500),
            toggle("caret_line", "Highlight the current line", "", s.caret_line),
            toggle("smart", "Highlight matching words", "Mark every other occurrence of the selected word.", s.smart_highlight),
        ],
    )
}

fn appearance(s: &Settings, dark: bool) -> Page {
    page(
        "appearance",
        "Appearance",
        0,
        vec![
            heading("Theme"),
            themes(s, dark),
            heading("Window"),
            toggle("toolbar", "Toolbar", "", s.show_toolbar),
            toggle("tabbar", "Tab bar", "", s.show_tabbar),
            toggle("statusbar", "Status bar", "", s.show_statusbar),
            heading("Margins"),
            toggle("line_numbers", "Line numbers", "", s.line_numbers),
            toggle("fold", "Fold margin", "", s.fold_margin),
            toggle("bookmark", "Bookmark margin", "", s.bookmark_margin),
        ],
    )
}

fn security(f: &Facts) -> Page {
    page(
        "security",
        "Security",
        0,
        vec![
            row("", "Encryption", &f.encryption, Control::Heading),
            button("enc_setup", "Profile", "Set up the password that protects encrypted files, or replace the profile.", "Set up profile..."),
            button(
                "enc_lock",
                "Lock",
                "Forget the key until the password is entered again, or enter it now.",
                if f.unlocked { "Lock now" } else { "Unlock..." },
            ),
            button("enc_password", "Password", "Choose a new password. Encrypted files do not need to be touched.", "Change password..."),
            button("enc_recovery", "Recovery key", "Create a new recovery key. The old one stops working.", "New recovery key..."),
            button("enc_import", "Import", "Use an encryption profile saved elsewhere.", "Import profile..."),
            heading("Passwords"),
            number(
                "lock",
                "Lock the vault when idle",
                "Lock Bitwarden after this many idle minutes. 0 means never.",
                i64::from(f.settings.passwords_auto_lock_min),
                0,
                1440,
            ),
        ],
    )
}

/// What agents may do with the Brain: the key kept in the settings, and its words.
pub const BRAIN_AGENTS: [(&str, &str); 3] = [("off", "Nothing"), ("read", "Search and read"), ("add", "Search, read and add entries")];

fn brain(f: &Facts) -> Page {
    page(
        "brain",
        "Quick Notes",
        0,
        vec![
            heading("Notes"),
            button(
                "brain_prompt",
                "Ask Quick Notes",
                "Copies a note for your AI agent that says how to use Quick Notes: look at the overview, search, read, and answer with the notes it used.",
                "Copy agent prompt",
            ),
            choice(
                "brain_agents",
                "What AI agents may do with Quick Notes",
                "Search and read by default. An agent connects through Settings, AI Agents (turn that on and copy the agent setup); it can then search and read Quick Notes, and add entries if you allow it. Nothing leaves this computer unless the agent you attach sends it. An encrypted Quick Notes stays closed while the profile is locked.",
                &BRAIN_AGENTS,
                &f.settings.brain_agents,
            ),
            toggle(
                "brain_encrypt",
                "Encrypt Quick Notes",
                "Keep every quick note encrypted with your encryption profile. Turning this on or off rewrites the notes, and needs the profile unlocked. Encrypted notes can only be read, searched or added to while it is unlocked.",
                f.settings.brain_encrypt,
            ),
        ],
    )
}

fn agents(f: &Facts) -> Page {
    let s = &f.settings;
    let levels: Vec<(&str, &str)> = Level::ALL.iter().map(|l| (l.key(), l.label())).collect();
    page(
        "agents",
        "AI Agents",
        0,
        vec![
            row("", "AI agents", &f.agents_status, Control::Heading),
            toggle(
                "mcp_on",
                "Let AI agents connect",
                "Run an MCP server, so an agent can work in Note++ without sending keystrokes. It listens on this computer only.",
                s.mcp_enabled,
            ),
            choice(
                "mcp_level",
                "What agents may do",
                "Read only: list, read, search and open. Edit: also change and save documents, each change one undo step. Full: also change the view and tabs, and close unchanged tabs. Passwords and the clipboard are never available, and you are asked before an agent can use an encrypted document.",
                &levels,
                &s.mcp_level,
            ),
            number("mcp_port", "Port", "Change it if another program uses this port.", i64::from(s.mcp_port), 1024, 65535),
            button("mcp_copy", "Agent setup", "Copies the address and token, to paste into the agent's MCP settings.", "Copy agent setup"),
            button("mcp_token", "Token", "Make a new token if the old one may have been seen. Agents then need the new setup.", "New token"),
            button("mcp_forget", "Allowed agents", &f.agents_allowed, "Forget allowed agents"),
            button("mcp_log", "Activity", "What agents have done, when, and on which document.", "Show activity log"),
        ],
    )
}
