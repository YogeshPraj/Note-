//! The command registry: every user action (menu item, toolbar button, shortcut, palette entry) is one
//! [`CommandSpec`] here. Menus, the toolbar, the palette and the keybindings are generated from this table, so
//! a label, a default shortcut or an enabled rule is written once.
//!
//! Commands keep the numeric ids of the established command set (`commands::ID_*`); [`Command`] gives the
//! frequent ones a typed name. Families of commands (recent files, encodings, languages, ...) are generated from
//! their data and carry an id range instead of a row here.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::commands::{
    ID_ABOUT, ID_AC_FUNC, ID_AC_HINT, ID_AC_PATH, ID_AC_WORD, ID_B_EOL_SPACE, ID_B_SPACE_TAB_ALL, ID_B_SPACE_TAB_LEAD, ID_B_TAB_SPACE, ID_B_TRIM_BOTH,
    ID_B_TRIM_LEAD, ID_B_UNNECESSARY, ID_B64_DEC, ID_B64_ENC, ID_BEGIN_END_COL, ID_BEGIN_END_SEL, ID_BM_CLEAR, ID_BM_COPY, ID_BM_CUT, ID_BM_DELETE,
    ID_BM_DELETE_UNMARKED, ID_BM_INVERT, ID_BM_NEXT, ID_BM_PASTE, ID_BM_PREV, ID_BM_TOGGLE, ID_BRACE_GOTO, ID_BRACE_SEL, ID_C_LOWER, ID_C_UPPER,
    ID_CASE_INVERT, ID_CASE_PROPER, ID_CASE_PROPER_BLEND, ID_CASE_RANDOM, ID_CASE_SENTENCE, ID_CASE_SENTENCE_BLEND, ID_CHAR_PANEL, ID_CHARSET_BASE,
    ID_CLEAR_RO, ID_CLIP_HISTORY, ID_CLONE_OTHER_VIEW, ID_CLOSE, ID_CLOSE_ALL, ID_CLOSE_LEFT, ID_CLOSE_OTHERS, ID_CLOSE_RIGHT, ID_CLOSE_UNCHANGED,
    ID_CMT_BLOCK, ID_CMT_LINE, ID_COLUMN_EDITOR, ID_COLUMN_MODE, ID_COMMENT, ID_CONVERT_BASE, ID_COPY, ID_CP_ALLNAMES, ID_CP_ALLPATHS, ID_CP_DIR,
    ID_CP_FULLPATH, ID_CP_NAME, ID_CUT, ID_DECRYPT, ID_DELETE, ID_DISTRACTION, ID_DOC_LIST, ID_EDIT_CONTEXT, ID_ENC_LOCK, ID_ENC_SETTINGS, ID_ENCODE_BASE,
    ID_ENCRYPT, ID_ENGINE_BASE, ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_EXIT, ID_EXPORT_THEME, ID_FIF, ID_FIND, ID_FIND_CHARS, ID_FINDNEXT, ID_FINDPREV,
    ID_FOCUS_OTHER, ID_FOLD_ALL, ID_FOLD_CUR, ID_FULLSCREEN, ID_FUNC_LIST, ID_GOTO, ID_HASH_BASE, ID_HIDE_LINES, ID_HTTP_SEND, ID_IMPORT_THEME, ID_INCR,
    ID_INDENT_DEC, ID_INDENT_INC, ID_INS_DATE_CUSTOM, ID_INS_DATE_LONG, ID_INS_DATE_SHORT, ID_JSON_MIN, ID_JSON_PRETTY, ID_L_COPY, ID_L_CUT, ID_L_DEDUP,
    ID_L_DEL, ID_L_DOWN, ID_L_DUP, ID_L_INS_ABOVE, ID_L_INS_BELOW, ID_L_JOIN, ID_L_NOEMPTY, ID_L_RANDOM, ID_L_REVERSE, ID_L_RM_BLANK, ID_L_RM_CONSEC,
    ID_L_SORT_ASC, ID_L_SORT_COMMA_ASC, ID_L_SORT_COMMA_DESC, ID_L_SORT_DESC, ID_L_SORT_DOT_ASC, ID_L_SORT_DOT_DESC, ID_L_SORT_INT_ASC, ID_L_SORT_INT_DESC,
    ID_L_SORT_NOCASE_ASC, ID_L_SORT_NOCASE_DESC, ID_L_SPLIT, ID_L_TRANSPOSE, ID_L_TRIM, ID_L_UP, ID_LANG_BASE, ID_LOG_FOLLOW, ID_LTR, ID_MARK, ID_MD_BOLD,
    ID_MD_BULLET, ID_MD_CODE, ID_MD_CODEBLOCK, ID_MD_H1, ID_MD_H2, ID_MD_H3, ID_MD_H4, ID_MD_H5, ID_MD_H6, ID_MD_IMAGE, ID_MD_ITALIC, ID_MD_LINK,
    ID_MD_NUMBERED, ID_MD_QUOTE, ID_MD_RULE, ID_MD_STRIKE, ID_MD_TABLE, ID_MD_TASK, ID_MD_TOOLBAR, ID_MINIMAP, ID_MONITOR, ID_MOVE_NEW_INSTANCE,
    ID_MOVE_OTHER_VIEW, ID_MOVE_TAB_BACK, ID_MOVE_TAB_FWD, ID_NEW, ID_NEW_WB, ID_NEXT_RESULT, ID_NEXT_TAB, ID_ON_TOP, ID_OPEN, ID_OPEN_ALL_RECENT,
    ID_OPEN_FOLDER_CMD, ID_OPEN_FOLDER_EXPLORER, ID_OPEN_FOLDER_WS, ID_OPEN_NEW_INSTANCE, ID_OPEN_VIEWER, ID_OPEN_WORKSPACE, ID_PAGE_SETUP, ID_PALETTE,
    ID_PASSWORDS, ID_PASTE, ID_POSTIT, ID_PREFS, ID_PRESET_BASE, ID_PREV_RESULT, ID_PREV_TAB, ID_PREVIEW, ID_PRINT, ID_PRINT_NOW, ID_PS_COPY_BIN,
    ID_PS_CUT_BIN, ID_PS_HTML, ID_PS_PASTE_BIN, ID_PS_RTF, ID_PW_LOCK, ID_QUICKOPEN, ID_READONLY, ID_RECENT_BASE, ID_RECENT_CLEAR, ID_RECYCLE, ID_REDO,
    ID_RELOAD, ID_REPLACE, ID_REPORT_ISSUE, ID_RESTORE_CLOSED, ID_RESULTS, ID_RTL, ID_SAVE, ID_SAVE_COPY, ID_SAVEALL, ID_SAVEAS, ID_SEL_FIND_NEXT,
    ID_SEL_FIND_PREV, ID_SEL_FOLDER, ID_SEL_OPEN, ID_SEL_SEARCH, ID_SELALL, ID_SHORTCUTS, ID_STYLE_ALL_BASE, ID_STYLE_CFG, ID_SUMMARY, ID_SYM_ALL, ID_SYM_CTRL,
    ID_SYM_EOL, ID_SYM_INDENT, ID_SYM_NONPRINT, ID_SYM_WRAP, ID_SYM_WS, ID_SYNC_H, ID_SYNC_V, ID_TAB_CMP_CLIP, ID_TAB_CMP_FILE, ID_TAB_COLOR_CUSTOM,
    ID_TAB_COLOR_NONE, ID_TAB_PIN, ID_TAB_REVEAL, ID_THEME, ID_UNCMT_BLOCK, ID_UNCMT_LINE, ID_UNDO, ID_UNFOLD_ALL, ID_UNFOLD_CUR, ID_UNFOLD_LEVEL_BASE,
    ID_UPDATE_AUTO, ID_UPDATE_CHECK, ID_URL_DEC, ID_URL_ENC, ID_VOL_NEXT, ID_VOL_PREV, ID_WINDOWS, ID_WORKSPACE_TOGGLE, ID_WRAP, ID_XML_MIN, ID_XML_PRETTY,
    ID_ZOOMIN, ID_ZOOMOUT, ID_ZOOMRESET,
};
use crate::sci::{self, Send};
use crate::workspace::Workspace;

/// What the enabled rules may look at: a small read-only snapshot of the application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandState {
    pub has_selection: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_path: bool,
    /// The active tab is a whiteboard, so the editor's text is not what the user sees (see [`needs_text`]).
    pub whiteboard: bool,
}

impl CommandState {
    pub fn of<S: Send + Copy>(ws: &Workspace<S>) -> CommandState {
        let sci = ws.sci;
        CommandState {
            has_selection: sci.send(sci::SCI_GETSELECTIONSTART, 0, 0) != sci.send(sci::SCI_GETSELECTIONEND, 0, 0),
            can_undo: sci.send(sci::SCI_CANUNDO, 0, 0) != 0,
            can_redo: sci.send(sci::SCI_CANREDO, 0, 0) != 0,
            has_path: ws.tabs.get(ws.active).is_some_and(|t| t.path.is_some()),
            whiteboard: ws.is_whiteboard(),
        }
    }
}

/// Does command `id` read or change the editor's text? A whiteboard tab has none: the editor behind it still holds the
/// last text tab's document, so these commands must not run while a whiteboard is in front (they would edit a tab the
/// user is not looking at). The Edit, Search, Encoding and Language menus, folding, the text tools and the like.
pub fn needs_text(id: usize) -> bool {
    // In the Edit or Search menu but about the file, a setting, a panel or the Find in Files results.
    let exempt = matches!(
        id,
        ID_CP_FULLPATH..=ID_CP_ALLPATHS
            | ID_ENGINE_BASE..=2329
            | ID_COLUMN_MODE
            | ID_CHAR_PANEL
            | ID_CLIP_HISTORY
            | ID_CLEAR_RO
            | ID_FIF
            | ID_RESULTS
            | ID_NEXT_RESULT
            | ID_PREV_RESULT
    );
    let text = matches!(
        id,
        ID_UNDO..=ID_CLEAR_RO
            | ID_FIND..=ID_BM_INVERT
            | ID_HIDE_LINES
            | ID_FOLD_ALL..=ID_UNFOLD_LEVEL_LAST
            | ID_SUMMARY
            | ID_RTL
            | ID_LTR
            | ID_MONITOR
            | ID_B64_ENC..=ID_XML_MIN
            | ID_ENCODE_BASE..=ID_CONVERT_LAST
            | ID_LANG_BASE..=ID_LANG_LAST
    );
    // Tools > hashes: Generate (a dialog) and from files are fine; "from selection" reads the editor.
    let hash_of_selection = (ID_HASH_BASE..ID_B64_ENC).contains(&id) && (id - ID_HASH_BASE) % 10 == 2;
    !exempt && (text || hash_of_selection)
}

/// The last id of the Unfold Level family, the Convert to family and the Language family.
const ID_UNFOLD_LEVEL_LAST: usize = ID_UNFOLD_LEVEL_BASE + 7;
const ID_CONVERT_LAST: usize = ID_CONVERT_BASE + 99;
const ID_LANG_LAST: usize = ID_LANG_BASE + 999;

pub type Enabled = fn(CommandState) -> bool;

fn always(_: CommandState) -> bool {
    true
}
fn has_selection(s: CommandState) -> bool {
    s.has_selection
}
fn can_undo(s: CommandState) -> bool {
    s.can_undo
}
fn can_redo(s: CommandState) -> bool {
    s.can_redo
}
fn has_path(s: CommandState) -> bool {
    s.has_path
}

/// One command: how it is named in menus and in the palette, its default shortcut, and when it can run.
#[derive(Clone, Copy)]
pub struct CommandSpec {
    pub id: usize,
    /// Menu text with its mnemonic (`&New`); empty when the command is only in the palette.
    pub menu: &'static str,
    /// Palette text (`File: New`); empty when the command is not offered there.
    pub palette: &'static str,
    /// Default shortcut as [`crate::shortcuts`] text; empty for none.
    pub shortcut: &'static str,
    /// Shortcut text the menu shows when it differs (`Ctrl+Z or Alt+Backspace`); empty = same as `shortcut`.
    pub menu_key: &'static str,
    pub enabled: Enabled,
    /// True when nabled can ever say no (so the menus and toolbar need updating as the state changes).
    pub gated: bool,
}

impl std::fmt::Debug for CommandSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandSpec").field("id", &self.id).field("palette", &self.palette).finish_non_exhaustive()
    }
}

impl CommandSpec {
    /// The text a tool-tip shows: the name without mnemonic, and the shortcut when there is one.
    pub fn tooltip(&self) -> String {
        self.tooltip_with(None)
    }

    /// Like [`CommandSpec::tooltip`] with the user's own shortcut text (`Some("")` = none) instead of the default.
    pub fn tooltip_with(&self, key: Option<&str>) -> String {
        let name = if self.menu.is_empty() { self.palette.rsplit(": ").next().unwrap_or(self.palette) } else { self.menu };
        let name = name.replace('&', "").trim_end_matches("...").to_string();
        let key = key.unwrap_or_else(|| self.hint());
        if key.is_empty() { name } else { format!("{name} ({key})") }
    }
    /// The shortcut text the palette shows beside the command.
    pub fn hint(&self) -> &'static str {
        if self.shortcut.is_empty() { self.menu_key } else { self.shortcut }
    }

    pub fn menu_key_or_shortcut(&self) -> &'static str {
        if self.menu_key.is_empty() { self.shortcut } else { self.menu_key }
    }

    /// Menu item text, `Name\tShortcut`, with `key` (the user's own choice) replacing the default when given.
    pub fn menu_text(&self, key: Option<&str>) -> String {
        let key = key.unwrap_or_else(|| self.menu_key_or_shortcut());
        if key.is_empty() { self.menu.to_string() } else { format!("{}\t{key}", self.menu) }
    }
}

const fn row(id: usize, menu: &'static str, palette: &'static str, shortcut: &'static str, menu_key: &'static str) -> CommandSpec {
    CommandSpec { id, menu, palette, shortcut, menu_key, enabled: always, gated: false }
}

const fn gated(id: usize, menu: &'static str, palette: &'static str, shortcut: &'static str, menu_key: &'static str, enabled: Enabled) -> CommandSpec {
    CommandSpec { id, menu, palette, shortcut, menu_key, enabled, gated: true }
}

mod table;
pub use table::*;

fn index() -> &'static HashMap<usize, &'static CommandSpec> {
    static INDEX: OnceLock<HashMap<usize, &'static CommandSpec>> = OnceLock::new();
    INDEX.get_or_init(|| SPECS.iter().map(|s| (s.id, s)).collect())
}

/// The spec of command `id`, if it is a fixed command (not a member of a generated family).
pub fn spec(id: usize) -> Option<&'static CommandSpec> {
    index().get(&id).copied()
}

/// Is command `id` allowed to run now? Text commands never run on a whiteboard; otherwise commands without a spec are
/// always allowed.
pub fn is_enabled(id: usize, state: CommandState) -> bool {
    !(state.whiteboard && needs_text(id)) && spec(id).is_none_or(|s| (s.enabled)(state))
}

/// Every fixed command, in table order.
pub fn all() -> impl Iterator<Item = &'static CommandSpec> {
    SPECS.iter()
}

/// The commands whose availability changes with the state of the application.
pub fn state_dependent() -> impl Iterator<Item = &'static CommandSpec> {
    SPECS.iter().filter(|s| s.gated)
}

/// The commands the palette lists, in order.
pub fn palette() -> impl Iterator<Item = &'static CommandSpec> {
    SPECS.iter().filter(|s| !s.palette.is_empty())
}

/// The default shortcut of `id` when it is one the keyboard layer can use.
pub fn default_shortcut(id: usize) -> Option<&'static str> {
    spec(id).map(|s| s.shortcut).filter(|k| crate::shortcuts::parse(k).is_some())
}

/// How many recent files the File menu can hold.
pub const RECENT_SLOTS: usize = 100;

/// Commands under a typed name, so code does not pass magic numbers around. The first group are the frequent fixed
/// commands; the rest are families, one command per item of a list that is built from current state (the recent
/// files, the encodings, the languages). A family command carries its index into that list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    FileNew,
    FileOpen,
    FileSave,
    FileSaveAs,
    FileClose,
    EditUndo,
    EditRedo,
    EditCut,
    EditCopy,
    EditPaste,
    EditSelectAll,
    SearchFind,
    SearchReplace,
    SearchFindNext,
    ViewToggleWordWrap,
    ViewZoomIn,
    ViewZoomOut,
    /// Open entry `n` of the recent-files list.
    OpenRecent(usize),
    /// Reopen the file as the n-th of [`crate::tables::ENCODE_IN`].
    EncodeIn(usize),
    /// Convert the text to the n-th of [`crate::tables::ENCODE_IN`].
    ConvertTo(usize),
    /// Reopen as character set n of [`crate::doc::CHARSETS`]; `CHARSETS.len()` stands for Windows-1252.
    SetCharset(usize),
    /// Use language n of [`crate::lang::LANGUAGES`] (0 is Normal Text).
    SetLanguage(usize),
}

impl Command {
    /// The fixed commands, for tests and tooling.
    pub const FIXED: [Command; 17] = [
        Command::FileNew,
        Command::FileOpen,
        Command::FileSave,
        Command::FileSaveAs,
        Command::FileClose,
        Command::EditUndo,
        Command::EditRedo,
        Command::EditCut,
        Command::EditCopy,
        Command::EditPaste,
        Command::EditSelectAll,
        Command::SearchFind,
        Command::SearchReplace,
        Command::SearchFindNext,
        Command::ViewToggleWordWrap,
        Command::ViewZoomIn,
        Command::ViewZoomOut,
    ];

    pub fn id(self) -> usize {
        match self {
            Command::FileNew => ID_NEW,
            Command::FileOpen => ID_OPEN,
            Command::FileSave => ID_SAVE,
            Command::FileSaveAs => ID_SAVEAS,
            Command::FileClose => ID_CLOSE,
            Command::EditUndo => ID_UNDO,
            Command::EditRedo => ID_REDO,
            Command::EditCut => ID_CUT,
            Command::EditCopy => ID_COPY,
            Command::EditPaste => ID_PASTE,
            Command::EditSelectAll => ID_SELALL,
            Command::SearchFind => ID_FIND,
            Command::SearchReplace => ID_REPLACE,
            Command::SearchFindNext => ID_FINDNEXT,
            Command::ViewToggleWordWrap => ID_WRAP,
            Command::ViewZoomIn => ID_ZOOMIN,
            Command::ViewZoomOut => ID_ZOOMOUT,
            Command::OpenRecent(i) => ID_RECENT_BASE + i,
            Command::EncodeIn(i) => ID_ENCODE_BASE + i,
            Command::ConvertTo(i) => ID_CONVERT_BASE + i,
            Command::SetCharset(i) => ID_CHARSET_BASE + i,
            Command::SetLanguage(i) => ID_LANG_BASE + i,
        }
    }

    /// The command behind a numeric id (what a menu or the keyboard reports), family members included.
    pub fn from_id(id: usize) -> Option<Command> {
        let member = |base: usize, len: usize| id.checked_sub(base).filter(|i| *i < len);
        if let Some(c) = Command::FIXED.into_iter().find(|c| c.id() == id) {
            return Some(c);
        }
        member(ID_RECENT_BASE, RECENT_SLOTS)
            .map(Command::OpenRecent)
            .or_else(|| member(ID_ENCODE_BASE, crate::tables::ENCODE_IN.len()).map(Command::EncodeIn))
            .or_else(|| member(ID_CONVERT_BASE, crate::tables::ENCODE_IN.len()).map(Command::ConvertTo))
            .or_else(|| member(ID_CHARSET_BASE, crate::doc::CHARSETS.len() + 1).map(Command::SetCharset))
            .or_else(|| member(ID_LANG_BASE, crate::lang::LANGUAGES.len()).map(Command::SetLanguage))
    }

    /// The registry row of a fixed command (family members have none: their text comes from a menu provider).
    pub fn spec(self) -> Option<&'static CommandSpec> {
        spec(self.id())
    }
}
#[cfg(test)]
mod tests;
