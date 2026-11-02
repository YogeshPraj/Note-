//! The command registry: every user action (menu item, toolbar button, shortcut, palette entry) is one
//! [`CommandSpec`] here. Menus, the toolbar, the palette and the keybindings are generated from this table, so
//! a label, a default shortcut or an enabled rule is written once.
//!
//! Commands keep the numeric ids of the Notepad++-compatible command set (`commands::ID_*`); [`Command`] gives the
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
    ID_ENCRYPT, ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_EXIT, ID_EXPORT_THEME, ID_FIF, ID_FIND, ID_FIND_CHARS, ID_FINDNEXT, ID_FINDPREV, ID_FOCUS_OTHER,
    ID_FOLD_ALL, ID_FOLD_CUR, ID_FULLSCREEN, ID_FUNC_LIST, ID_GOTO, ID_HASH_BASE, ID_HIDE_LINES, ID_HTTP_SEND, ID_IMPORT_THEME, ID_INCR, ID_INDENT_DEC,
    ID_INDENT_INC, ID_INS_DATE_CUSTOM, ID_INS_DATE_LONG, ID_INS_DATE_SHORT, ID_JSON_MIN, ID_JSON_PRETTY, ID_L_COPY, ID_L_CUT, ID_L_DEDUP, ID_L_DEL, ID_L_DOWN,
    ID_L_DUP, ID_L_INS_ABOVE, ID_L_INS_BELOW, ID_L_JOIN, ID_L_NOEMPTY, ID_L_RANDOM, ID_L_REVERSE, ID_L_RM_BLANK, ID_L_RM_CONSEC, ID_L_SORT_ASC,
    ID_L_SORT_COMMA_ASC, ID_L_SORT_COMMA_DESC, ID_L_SORT_DESC, ID_L_SORT_DOT_ASC, ID_L_SORT_DOT_DESC, ID_L_SORT_INT_ASC, ID_L_SORT_INT_DESC,
    ID_L_SORT_NOCASE_ASC, ID_L_SORT_NOCASE_DESC, ID_L_SPLIT, ID_L_TRANSPOSE, ID_L_TRIM, ID_L_UP, ID_LANG_BASE, ID_LOG_FOLLOW, ID_LTR, ID_MARK, ID_MD_BOLD,
    ID_MD_BULLET, ID_MD_CODE, ID_MD_CODEBLOCK, ID_MD_H1, ID_MD_H2, ID_MD_H3, ID_MD_H4, ID_MD_H5, ID_MD_H6, ID_MD_IMAGE, ID_MD_ITALIC, ID_MD_LINK,
    ID_MD_NUMBERED, ID_MD_QUOTE, ID_MD_RULE, ID_MD_STRIKE, ID_MD_TABLE, ID_MD_TASK, ID_MD_TOOLBAR, ID_MINIMAP, ID_MONITOR, ID_MOVE_NEW_INSTANCE,
    ID_MOVE_OTHER_VIEW, ID_MOVE_TAB_BACK, ID_MOVE_TAB_FWD, ID_NEW, ID_NEW_WB, ID_NEXT_RESULT, ID_NEXT_TAB, ID_ON_TOP, ID_OPEN, ID_OPEN_ALL_RECENT,
    ID_OPEN_FOLDER_CMD, ID_OPEN_FOLDER_EXPLORER, ID_OPEN_FOLDER_WS, ID_OPEN_NEW_INSTANCE, ID_OPEN_VIEWER, ID_OPEN_WORKSPACE, ID_PAGE_SETUP, ID_PALETTE,
    ID_PASSWORDS, ID_PASTE, ID_POSTIT, ID_PREFS, ID_PRESET_BASE, ID_PREV_RESULT, ID_PREV_TAB, ID_PREVIEW, ID_PRINT, ID_PRINT_NOW, ID_PS_COPY_BIN,
    ID_PS_CUT_BIN, ID_PS_HTML, ID_PS_PASTE_BIN, ID_PS_RTF, ID_PW_LOCK, ID_QUICKOPEN, ID_READONLY, ID_RECENT_BASE, ID_RECENT_CLEAR, ID_RECYCLE, ID_REDO,
    ID_RELOAD, ID_REPLACE, ID_RESTORE_CLOSED, ID_RESULTS, ID_RTL, ID_SAVE, ID_SAVE_COPY, ID_SAVEALL, ID_SAVEAS, ID_SEL_FIND_NEXT, ID_SEL_FIND_PREV,
    ID_SEL_FOLDER, ID_SEL_OPEN, ID_SEL_SEARCH, ID_SELALL, ID_SHORTCUTS, ID_STYLE_ALL_BASE, ID_STYLE_CFG, ID_SUMMARY, ID_SYM_ALL, ID_SYM_CTRL, ID_SYM_EOL,
    ID_SYM_INDENT, ID_SYM_NONPRINT, ID_SYM_WRAP, ID_SYM_WS, ID_SYNC_H, ID_SYNC_V, ID_TAB_CMP_CLIP, ID_TAB_CMP_FILE, ID_TAB_COLOR_CUSTOM, ID_TAB_COLOR_NONE,
    ID_TAB_PIN, ID_TAB_REVEAL, ID_THEME, ID_UNCMT_BLOCK, ID_UNCMT_LINE, ID_UNDO, ID_UNFOLD_ALL, ID_UNFOLD_CUR, ID_UPDATE_AUTO, ID_UPDATE_CHECK, ID_URL_DEC,
    ID_URL_ENC, ID_VOL_NEXT, ID_VOL_PREV, ID_WINDOWS, ID_WORKSPACE_TOGGLE, ID_WRAP, ID_XML_MIN, ID_XML_PRETTY, ID_ZOOMIN, ID_ZOOMOUT, ID_ZOOMRESET,
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
}

impl CommandState {
    pub fn of<S: Send + Copy>(ws: &Workspace<S>) -> CommandState {
        let sci = ws.sci;
        CommandState {
            has_selection: sci.send(sci::SCI_GETSELECTIONSTART, 0, 0) != sci.send(sci::SCI_GETSELECTIONEND, 0, 0),
            can_undo: sci.send(sci::SCI_CANUNDO, 0, 0) != 0,
            can_redo: sci.send(sci::SCI_CANREDO, 0, 0) != 0,
            has_path: ws.tabs.get(ws.active).is_some_and(|t| t.path.is_some()),
        }
    }
}

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

/// Every fixed command, in the palette's order.
pub static SPECS: &[CommandSpec] = &[
    row(ID_NEW, "&New", "File: New", "Ctrl+N", ""),
    row(ID_NEW_WB, "New &Whiteboard", "File: New Whiteboard", "", ""),
    row(ID_OPEN, "&Open...", "File: Open...", "Ctrl+O", ""),
    row(ID_OPEN_FOLDER_EXPLORER, "&Explorer", "File: Open Containing Folder in Explorer", "", ""),
    row(ID_OPEN_FOLDER_CMD, "&cmd", "File: Open Containing Folder in Command Prompt", "", ""),
    row(ID_OPEN_VIEWER, "Open in &Default Viewer", "File: Open in Default Viewer", "", ""),
    row(ID_OPEN_WORKSPACE, "Open Folder as &Workspace...", "File: Open Folder as Workspace...", "", ""),
    gated(ID_RELOAD, "Reload from Dis&k", "File: Reload from Disk", "Ctrl+R", "", has_path),
    row(ID_SAVE, "&Save", "File: Save", "Ctrl+S", ""),
    row(ID_SAVEAS, "Save &As...", "File: Save As...", "Ctrl+Alt+S", ""),
    row(ID_SAVE_COPY, "Save a Cop&y As...", "File: Save a Copy As...", "", ""),
    row(ID_SAVEALL, "Save A&ll", "File: Save All", "Ctrl+Shift+S", ""),
    row(ID_CLOSE, "&Close", "File: Close", "Ctrl+W", ""),
    row(ID_CLOSE_ALL, "Close All", "File: Close All", "Ctrl+Shift+W", ""),
    row(ID_CLOSE_OTHERS, "Close All but Active Document", "File: Close All but Active Document", "", ""),
    row(ID_CLOSE_LEFT, "Close All to the Left", "File: Close All to the Left", "", ""),
    row(ID_CLOSE_RIGHT, "Close All to the Right", "File: Close All to the Right", "", ""),
    row(ID_CLOSE_UNCHANGED, "Close All Unchanged", "File: Close All Unchanged", "", ""),
    row(ID_RECYCLE, "Move to Recycle &Bin", "File: Move to Recycle Bin", "", ""),
    row(ID_PRINT, "Print &Preview / Print...", "File: Print...", "", ""),
    row(ID_PRINT_NOW, "Print No&w", "File: Print Now", "", ""),
    row(ID_PAGE_SETUP, "Page Se&tup...", "File: Page Setup...", "", ""),
    row(ID_ENCRYPT, "&Encrypt This File", "File: Encrypt This File", "", ""),
    row(ID_DECRYPT, "Remove Encr&yption From This File", "File: Remove Encryption From This File", "", ""),
    row(ID_ENC_SETTINGS, "Encryption...", "Settings: Encryption...", "", ""),
    row(ID_PASSWORDS, "Passwords (Bitwarden)...", "Tools: Passwords (Bitwarden)...", "", ""),
    row(ID_PW_LOCK, "", "Tools: Lock Passwords (Bitwarden)", "", ""),
    row(ID_LOG_FOLLOW, "Log Follow (.log / .out / .err)", "View: Log Follow (.log / .out / .err)", "", ""),
    row(ID_TAB_PIN, "", "Tab: Pin / Unpin Current Tab", "", ""),
    row(ID_TAB_COLOR_NONE, "", "Tab: Clear Colour Mark", "", ""),
    row(ID_TAB_COLOR_CUSTOM, "", "Tab: Custom Colour...", "", ""),
    row(ID_TAB_CMP_FILE, "", "Tab: Compare with File...", "", ""),
    row(ID_TAB_CMP_CLIP, "", "Tab: Compare with Clipboard", "", ""),
    row(ID_TAB_REVEAL, "", "Tab: Reveal in File Tree", "", ""),
    row(ID_ENC_LOCK, "", "Settings: Lock Encryption", "", ""),
    row(ID_HTTP_SEND, "Send HTTP Request", "Tools: Send HTTP Request (Ctrl+Enter in a .http file)", "", "Ctrl+Enter (.http / .rest files)"),
    row(ID_RESTORE_CLOSED, "Restore Recent Closed File", "File: Restore Recent Closed File", "Ctrl+Shift+T", ""),
    row(ID_OPEN_ALL_RECENT, "", "File: Open All Recent Files", "", ""),
    row(ID_RECENT_CLEAR, "", "File: Empty Recent Files List", "", ""),
    row(ID_WORKSPACE_TOGGLE, "Folder as Workspace", "View: Toggle Folder Workspace", "", ""),
    row(ID_QUICKOPEN, "Quick Open...", "File: Quick Open...", "Ctrl+P", ""),
    gated(ID_UNDO, "&Undo", "Edit: Undo", "Ctrl+Z", "Ctrl+Z or Alt+Backspace", can_undo),
    gated(ID_CUT, "Cu&t", "Edit: Cut", "Ctrl+X", "Ctrl+X or Shift+DEL", has_selection),
    gated(ID_COPY, "&Copy", "Edit: Copy", "Ctrl+C", "Ctrl+C or Ctrl+INS", has_selection),
    row(ID_PASTE, "&Paste", "Edit: Paste", "Ctrl+V", "Ctrl+V or Shift+INS"),
    row(ID_PREFS, "Preferences...", "Settings: Preferences...", "", ""),
    row(ID_STYLE_CFG, "Style Configurator...", "Settings: Style Configurator...", "", ""),
    row(ID_SHORTCUTS, "Shortcut Mapper...", "Settings: Shortcut Mapper...", "", ""),
    row(ID_IMPORT_THEME, "Import style theme(s)...", "Settings: Import Style Theme...", "", ""),
    row(ID_EXPORT_THEME, "Export style theme...", "Settings: Export Style Theme...", "", ""),
    row(ID_EDIT_CONTEXT, "Edit Popup ContextMenu", "Settings: Edit Popup ContextMenu", "", ""),
    row(ID_B64_ENC, "Base64 Encode", "Tools: Base64 Encode", "", ""),
    row(ID_B64_DEC, "Base64 Decode", "Tools: Base64 Decode", "", ""),
    row(ID_URL_ENC, "URL Encode", "Tools: URL Encode", "", ""),
    row(ID_URL_DEC, "URL Decode", "Tools: URL Decode", "", ""),
    row(ID_JSON_PRETTY, "JSON: Pretty-print", "Tools: JSON Pretty-print", "", ""),
    row(ID_JSON_MIN, "JSON: Minify", "Tools: JSON Minify", "", ""),
    row(ID_XML_PRETTY, "XML: Pretty-print", "Tools: XML Pretty-print", "", ""),
    row(ID_XML_MIN, "XML: Minify", "Tools: XML Minify", "", ""),
    row(ID_HASH_BASE, "", "Tools: MD5 Generate...", "", ""),
    row(ID_WINDOWS, "", "Window: Windows...", "", ""),
    gated(ID_REDO, "&Redo", "Edit: Redo", "Ctrl+Y", "Ctrl+Y or Ctrl+Shift+Z", can_redo),
    row(ID_SELALL, "Select &All", "Edit: Select All", "Ctrl+A", ""),
    row(ID_L_DUP, "&Duplicate Current Line", "Edit: Duplicate Line", "Ctrl+D", ""),
    row(ID_L_DEL, "Delete Current Line", "Edit: Delete Line", "Ctrl+Shift+L", ""),
    row(ID_L_UP, "Move Up Current Line", "Edit: Move Line Up", "Ctrl+Shift+Up", ""),
    row(ID_L_DOWN, "Move Down Current Line", "Edit: Move Line Down", "Ctrl+Shift+Down", ""),
    row(ID_L_SORT_ASC, "Sort Lines Lexicographically Ascending", "Edit: Sort Lines Ascending", "", ""),
    row(ID_L_SORT_DESC, "Sort Lines Lexicographically Descending", "Edit: Sort Lines Descending", "", ""),
    row(ID_L_DEDUP, "Remove Duplicate Lines", "Edit: Remove Duplicate Lines", "", ""),
    row(ID_L_NOEMPTY, "Remove Empty Lines", "Edit: Remove Empty Lines", "", ""),
    row(ID_L_TRIM, "Trim Trailing Space", "Edit: Trim Trailing Space", "", ""),
    row(ID_C_UPPER, "&UPPERCASE", "Edit: UPPERCASE", "Ctrl+Shift+U", ""),
    row(ID_C_LOWER, "&lowercase", "Edit: lowercase", "Ctrl+U", ""),
    row(ID_COMMENT, "&Toggle Single Line Comment", "Edit: Toggle Line Comment", "Ctrl+Q", ""),
    row(ID_CMT_LINE, "Single Line &Comment", "Edit: Single Line Comment", "Ctrl+K", ""),
    row(ID_UNCMT_LINE, "Single Line &Uncomment", "Edit: Single Line Uncomment", "Ctrl+Shift+K", ""),
    row(ID_CMT_BLOCK, "&Block Comment", "Edit: Block Comment", "Ctrl+Shift+Q", ""),
    row(ID_UNCMT_BLOCK, "Block Uncomment", "Edit: Block Uncomment", "Ctrl+Shift+R", ""),
    gated(ID_DELETE, "&Delete", "Edit: Delete", "Del", "DEL", has_selection),
    row(ID_BEGIN_END_SEL, "Begin/End &Select", "Edit: Begin/End Select", "Ctrl+Shift+B", ""),
    row(ID_BEGIN_END_COL, "Begin/End Select in Column Mode", "Edit: Begin/End Select in Column Mode", "Alt+Shift+B", ""),
    row(ID_INS_DATE_SHORT, "Date Time (short version)", "Edit: Insert Date Time (short)", "", ""),
    row(ID_INS_DATE_LONG, "Date Time (long version)", "Edit: Insert Date Time (long)", "", ""),
    row(ID_INS_DATE_CUSTOM, "Date Time (customized)", "Edit: Insert Date Time (customized)", "", ""),
    row(ID_CP_FULLPATH, "Current Full File path to Clipboard", "Edit: Copy Current Full File Path", "", ""),
    row(ID_CP_NAME, "Current Filename to Clipboard", "Edit: Copy Current Filename", "", ""),
    row(ID_CP_DIR, "Current Dir. Path to Clipboard", "Edit: Copy Current Dir. Path", "", ""),
    row(ID_CP_ALLNAMES, "Copy All Filenames", "Edit: Copy All Filenames", "", ""),
    row(ID_CP_ALLPATHS, "Copy All File Paths", "Edit: Copy All File Paths", "", ""),
    row(ID_INDENT_INC, "&Increase Line Indent", "Edit: Increase Line Indent", "Tab", ""),
    row(ID_INDENT_DEC, "&Decrease Line Indent", "Edit: Decrease Line Indent", "Shift+Tab", ""),
    row(ID_CASE_PROPER, "&Proper Case", "Edit: Proper Case", "Alt+U", ""),
    row(ID_CASE_PROPER_BLEND, "Proper Case (blend)", "Edit: Proper Case (blend)", "Alt+Shift+U", ""),
    row(ID_CASE_SENTENCE, "&Sentence case", "Edit: Sentence case", "Ctrl+Alt+U", ""),
    row(ID_CASE_SENTENCE_BLEND, "Sentence case (blend)", "Edit: Sentence case (blend)", "Ctrl+Alt+Shift+U", ""),
    row(ID_CASE_INVERT, "i&NVERT cASE", "Edit: iNVERT cASE", "", ""),
    row(ID_CASE_RANDOM, "&ranDOm CasE", "Edit: ranDOm CasE", "", ""),
    row(ID_L_RM_CONSEC, "Remove Consecutive Duplicate Lines", "Edit: Remove Consecutive Duplicate Lines", "", ""),
    row(ID_L_RM_BLANK, "Remove Empty Lines (Containing Blank characters)", "Edit: Remove Empty Lines (Containing Blank characters)", "", ""),
    row(ID_L_SPLIT, "S&plit Lines", "Edit: Split Lines", "Ctrl+I", ""),
    row(ID_L_JOIN, "&Join Lines", "Edit: Join Lines", "Ctrl+J", ""),
    row(ID_L_TRANSPOSE, "&Transpose Current Line", "Edit: Transpose Current Line", "Ctrl+T", ""),
    row(ID_L_INS_ABOVE, "Insert Blank Line Above Current", "Edit: Insert Blank Line Above Current", "Ctrl+Alt+Enter", ""),
    row(ID_L_INS_BELOW, "Insert Blank Line Below Current", "Edit: Insert Blank Line Below Current", "Ctrl+Alt+Shift+Enter", ""),
    row(ID_L_REVERSE, "Reverse Line Order", "Edit: Reverse Line Order", "", ""),
    row(ID_L_RANDOM, "Randomize Line Order", "Edit: Randomize Line Order", "", ""),
    row(ID_L_CUT, "Cut Current Line", "Edit: Cut Current Line", "Ctrl+L", ""),
    row(ID_L_COPY, "Copy Current Line", "Edit: Copy Current Line", "Ctrl+Shift+X", ""),
    row(ID_L_SORT_NOCASE_ASC, "Sort Lines Lexicographically Ascending Ignoring Case", "Edit: Sort Lines Ignoring Case Ascending", "", ""),
    row(ID_L_SORT_NOCASE_DESC, "Sort Lines Lexicographically Descending Ignoring Case", "Edit: Sort Lines Ignoring Case Descending", "", ""),
    row(ID_L_SORT_INT_ASC, "Sort Lines As Integers Ascending", "Edit: Sort Lines As Integers Ascending", "", ""),
    row(ID_L_SORT_INT_DESC, "Sort Lines As Integers Descending", "Edit: Sort Lines As Integers Descending", "", ""),
    row(ID_L_SORT_DOT_ASC, "Sort Lines As Decimals (Dot) Ascending", "Edit: Sort Lines As Decimals (Dot) Ascending", "", ""),
    row(ID_L_SORT_DOT_DESC, "Sort Lines As Decimals (Dot) Descending", "Edit: Sort Lines As Decimals (Dot) Descending", "", ""),
    row(ID_L_SORT_COMMA_ASC, "Sort Lines As Decimals (Comma) Ascending", "Edit: Sort Lines As Decimals (Comma) Ascending", "", ""),
    row(ID_L_SORT_COMMA_DESC, "Sort Lines As Decimals (Comma) Descending", "Edit: Sort Lines As Decimals (Comma) Descending", "", ""),
    row(ID_AC_FUNC, "Function Completion", "Edit: Function Completion", "Ctrl+Space", ""),
    row(ID_AC_WORD, "Word Completion", "Edit: Word Completion", "Ctrl+Enter", ""),
    row(ID_AC_HINT, "Function Parameters Hint", "Edit: Function Parameters Hint", "Ctrl+Shift+Space", ""),
    row(ID_AC_PATH, "Path Completion", "Edit: Path Completion", "Ctrl+Alt+Space", ""),
    row(ID_B_TRIM_LEAD, "Trim Leading Space", "Edit: Trim Leading Space", "", ""),
    row(ID_B_TRIM_BOTH, "Trim Leading and Trailing Space", "Edit: Trim Leading and Trailing Space", "", ""),
    row(ID_B_EOL_SPACE, "EOL to Space", "Edit: EOL to Space", "", ""),
    row(ID_B_UNNECESSARY, "Remove Unnecessary Blank and EOL", "Edit: Remove Unnecessary Blank and EOL", "", ""),
    row(ID_B_TAB_SPACE, "TAB to Space", "Edit: TAB to Space", "", ""),
    row(ID_B_SPACE_TAB_ALL, "Space to TAB (All)", "Edit: Space to TAB (All)", "", ""),
    row(ID_B_SPACE_TAB_LEAD, "Space to TAB (Leading)", "Edit: Space to TAB (Leading)", "", ""),
    row(ID_PS_HTML, "Paste HTML Content", "Edit: Paste HTML Content", "", ""),
    row(ID_PS_RTF, "Paste RTF Content", "Edit: Paste RTF Content", "", ""),
    row(ID_PS_COPY_BIN, "Copy Binary Content", "Edit: Copy Binary Content", "", ""),
    row(ID_PS_CUT_BIN, "Cut Binary Content", "Edit: Cut Binary Content", "", ""),
    row(ID_PS_PASTE_BIN, "Paste Binary Content", "Edit: Paste Binary Content", "", ""),
    row(ID_SEL_OPEN, "Open File", "Edit: Open Selected File", "", ""),
    row(ID_SEL_FOLDER, "Open Containing Folder", "Edit: Open Selected Path in Explorer", "", ""),
    row(ID_SEL_SEARCH, "Search on Internet", "Edit: Search Selection on Internet", "", ""),
    row(ID_COLUMN_EDITOR, "Column &Editor...", "Edit: Column Editor...", "Alt+C", ""),
    row(ID_CHAR_PANEL, "Character Panel", "Edit: Character Panel", "", ""),
    row(ID_CLIP_HISTORY, "Clipboard History", "Edit: Clipboard History", "", ""),
    row(ID_READONLY, "Set Read-Only", "Edit: Set Read-Only", "", ""),
    row(ID_CLEAR_RO, "Clear Read-Only Flag", "Edit: Clear Read-Only Flag", "", ""),
    row(ID_EOL_CRLF, "Windows (CR LF)", "Edit: EOL to Windows (CR LF)", "", ""),
    row(ID_EOL_LF, "Unix (LF)", "Edit: EOL to Unix (LF)", "", ""),
    row(ID_EOL_CR, "Macintosh (CR)", "Edit: EOL to Macintosh (CR)", "", ""),
    row(ID_FIND, "&Find...", "Search: Find...", "Ctrl+F", ""),
    row(ID_REPLACE, "&Replace...", "Search: Replace...", "Ctrl+H", ""),
    row(ID_FIF, "Find in Files...", "Search: Find in Files...", "Ctrl+Shift+F", ""),
    row(ID_MARK, "&Mark...", "Search: Mark...", "Ctrl+M", ""),
    row(ID_FINDNEXT, "Find &Next", "Search: Find Next", "F3", ""),
    row(ID_SEL_FIND_NEXT, "Select and Find Next", "Search: Select and Find Next", "Ctrl+F3", ""),
    row(ID_SEL_FIND_PREV, "Select and Find Previous", "Search: Select and Find Previous", "Ctrl+Shift+F3", ""),
    row(ID_VOL_NEXT, "Find (Volatile) Next", "Search: Find (Volatile) Next", "Ctrl+Alt+F3", ""),
    row(ID_VOL_PREV, "Find (Volatile) Previous", "Search: Find (Volatile) Previous", "Ctrl+Alt+Shift+F3", ""),
    row(ID_INCR, "Incremental Search", "Search: Incremental Search", "Ctrl+Alt+I", ""),
    row(ID_RESULTS, "Search Results Window", "Search: Search Results Window", "F7", ""),
    row(ID_NEXT_RESULT, "Next Search Result", "Search: Next Search Result", "F4", ""),
    row(ID_PREV_RESULT, "Previous Search Result", "Search: Previous Search Result", "Shift+F4", ""),
    row(ID_BRACE_GOTO, "Go to Matching Brace", "Search: Go to Matching Brace", "Ctrl+B", ""),
    row(ID_BRACE_SEL, "Select All Between Matching Braces", "Search: Select All Between Matching Braces", "Ctrl+Alt+B", ""),
    row(ID_STYLE_ALL_BASE, "", "Search: Style All Occurrences of Token (1st)", "", ""),
    row(ID_FIND_CHARS, "Find characters in range...", "Search: Find characters in range...", "", ""),
    row(ID_BM_CUT, "Cut Bookmarked Lines", "Bookmark: Cut Bookmarked Lines", "", ""),
    row(ID_BM_COPY, "Copy Bookmarked Lines", "Bookmark: Copy Bookmarked Lines", "", ""),
    row(ID_BM_PASTE, "Paste to (Replace) Bookmarked Lines", "Bookmark: Paste to (Replace) Bookmarked Lines", "", ""),
    row(ID_BM_DELETE, "Remove Bookmarked Lines", "Bookmark: Remove Bookmarked Lines", "", ""),
    row(ID_BM_DELETE_UNMARKED, "Remove Non-Bookmarked Lines", "Bookmark: Remove Non-Bookmarked Lines", "", ""),
    row(ID_BM_INVERT, "Inverse Bookmark", "Bookmark: Inverse", "", ""),
    row(ID_FINDPREV, "Find &Previous", "Search: Find Previous", "Shift+F3", ""),
    row(ID_GOTO, "&Go to...", "Search: Go to Line...", "Ctrl+G", ""),
    row(ID_BM_TOGGLE, "&Toggle Bookmark", "Bookmark: Toggle", "Ctrl+F2", ""),
    row(ID_BM_NEXT, "&Next Bookmark", "Bookmark: Next", "F2", ""),
    row(ID_BM_PREV, "&Previous Bookmark", "Bookmark: Previous", "Shift+F2", ""),
    row(ID_BM_CLEAR, "&Clear All Bookmarks", "Bookmark: Clear All", "", ""),
    row(ID_WRAP, "Word wrap", "View: Toggle Word Wrap", "", ""),
    row(ID_ON_TOP, "Always on Top", "View: Always on Top", "", ""),
    row(ID_FULLSCREEN, "Toggle Full Screen Mode", "View: Toggle Full Screen Mode", "F11", ""),
    row(ID_POSTIT, "Post-It", "View: Post-It", "F12", ""),
    row(ID_DISTRACTION, "Distraction Free Mode", "View: Distraction Free Mode", "", ""),
    row(ID_SYM_WS, "Show Space and Tab", "View: Show Space and Tab", "", ""),
    row(ID_SYM_EOL, "Show End of Line", "View: Show End of Line", "", ""),
    row(ID_SYM_ALL, "Show All Characters", "View: Show All Characters", "", ""),
    row(ID_SYM_INDENT, "Show Indent Guide", "View: Show Indent Guide", "", ""),
    row(ID_SYM_WRAP, "Show Wrap Symbol", "View: Show Wrap Symbol", "", ""),
    row(ID_HIDE_LINES, "Hide Lines", "View: Hide Lines", "Alt+H", ""),
    row(ID_FOLD_ALL, "Fold All", "View: Fold All", "Alt+0", ""),
    row(ID_UNFOLD_ALL, "Unfold All", "View: Unfold All", "Alt+Shift+0", ""),
    row(ID_FOLD_CUR, "Fold Current Level", "View: Fold Current Level", "Ctrl+Alt+F", ""),
    row(ID_UNFOLD_CUR, "Unfold Current Level", "View: Unfold Current Level", "Ctrl+Alt+Shift+F", ""),
    row(ID_SUMMARY, "Summary...", "View: Summary...", "", ""),
    row(ID_DOC_LIST, "Document List", "View: Document List", "", ""),
    row(ID_FUNC_LIST, "Function List", "View: Function List", "", ""),
    row(ID_RTL, "Text Direction RTL", "View: Text Direction RTL", "Ctrl+Alt+R", ""),
    row(ID_LTR, "Text Direction LTR", "View: Text Direction LTR", "Ctrl+Alt+L", ""),
    row(ID_MONITOR, "Monitoring (tail -f)", "View: Monitoring (tail -f)", "", ""),
    row(ID_MOVE_TAB_FWD, "Move Tab Forward", "View: Move Tab Forward", "Ctrl+Shift+PgDn", ""),
    row(ID_MOVE_TAB_BACK, "Move Tab Backward", "View: Move Tab Backward", "Ctrl+Shift+PgUp", ""),
    row(ID_OPEN_NEW_INSTANCE, "Open in New Instance", "View: Open in New Instance", "", ""),
    row(ID_MOVE_NEW_INSTANCE, "Move to New Instance", "View: Move to New Instance", "", ""),
    row(ID_PREVIEW, "Live Preview", "View: Toggle Live Preview", "Ctrl+Shift+V", ""),
    row(ID_MD_TOOLBAR, "Markdown Toolbar", "View: Markdown Toolbar", "", ""),
    row(ID_MD_BOLD, "", "Markdown: Bold", "", ""),
    row(ID_MD_ITALIC, "", "Markdown: Italic", "", ""),
    row(ID_MD_STRIKE, "", "Markdown: Strikethrough", "", ""),
    row(ID_MD_CODE, "", "Markdown: Inline Code", "", ""),
    row(ID_MD_H1, "", "Markdown: Heading 1", "", ""),
    row(ID_MD_H2, "", "Markdown: Heading 2", "", ""),
    row(ID_MD_H3, "", "Markdown: Heading 3", "", ""),
    row(ID_MD_H4, "", "Markdown: Heading 4", "", ""),
    row(ID_MD_H5, "", "Markdown: Heading 5", "", ""),
    row(ID_MD_H6, "", "Markdown: Heading 6", "", ""),
    row(ID_MD_QUOTE, "", "Markdown: Quote", "", ""),
    row(ID_MD_BULLET, "", "Markdown: Bulleted List", "", ""),
    row(ID_MD_NUMBERED, "", "Markdown: Numbered List", "", ""),
    row(ID_MD_TASK, "", "Markdown: Task List", "", ""),
    row(ID_MD_LINK, "", "Markdown: Link", "", ""),
    row(ID_MD_IMAGE, "", "Markdown: Image", "", ""),
    row(ID_MD_TABLE, "", "Markdown: Table", "", ""),
    row(ID_MD_CODEBLOCK, "", "Markdown: Code Block", "", ""),
    row(ID_MD_RULE, "", "Markdown: Horizontal Rule", "", ""),
    row(ID_THEME, "Dark mode", "View: Toggle Dark Mode", "", ""),
    row(ID_MINIMAP, "Document Map", "View: Toggle Minimap", "", ""),
    row(ID_ZOOMIN, "Zoom In", "View: Zoom In", "Ctrl+Num +", ""),
    row(ID_ZOOMOUT, "Zoom Out", "View: Zoom Out", "Ctrl+Num -", ""),
    row(ID_ZOOMRESET, "Restore Default Zoom", "View: Reset Zoom", "Ctrl+Num /", ""),
    row(ID_NEXT_TAB, "Next Tab", "View: Next Tab", "Ctrl+Tab", "Ctrl+PgDn"),
    row(ID_PREV_TAB, "Previous Tab", "View: Previous Tab", "Ctrl+Shift+Tab", "Ctrl+PgUp"),
    row(ID_ABOUT, "&About Note++", "Help: About Note++", "", ""),
    row(ID_UPDATE_CHECK, "Check for Updates &Now", "Help: Check for Updates Now", "", ""),
    row(ID_UPDATE_AUTO, "Check for Updates &Automatically", "Help: Check for Updates Automatically", "", ""),
    row(ID_EXIT, "E&xit", "", "Alt+F4", ""),
    row(ID_OPEN_FOLDER_WS, "Folder as &Workspace", "", "", ""),
    row(ID_COLUMN_MODE, "Column Mode...", "", "", ""),
    row(ID_SYM_NONPRINT, "Show Non-Printing Characters", "", "", ""),
    row(ID_SYM_CTRL, "Show Control Characters && Unicode EOL", "", "", ""),
    row(ID_MOVE_OTHER_VIEW, "Move to Other View", "", "", ""),
    row(ID_CLONE_OTHER_VIEW, "Clone to Other View", "", "", ""),
    row(ID_FOCUS_OTHER, "Focus on Another View", "", "F8", ""),
    row(ID_SYNC_V, "Synchronize Vertical Scrolling", "", "", ""),
    row(ID_SYNC_H, "Synchronize Horizontal Scrolling", "", "", ""),
    row(ID_PALETTE, "Command Palette...", "", "Ctrl+Shift+P", ""),
    row(ID_PRESET_BASE, "Default (light)", "", "", ""),
];

fn index() -> &'static HashMap<usize, &'static CommandSpec> {
    static INDEX: OnceLock<HashMap<usize, &'static CommandSpec>> = OnceLock::new();
    INDEX.get_or_init(|| SPECS.iter().map(|s| (s.id, s)).collect())
}

/// The spec of command `id`, if it is a fixed command (not a member of a generated family).
pub fn spec(id: usize) -> Option<&'static CommandSpec> {
    index().get(&id).copied()
}

/// Is command `id` allowed to run now? Commands without a spec are always allowed.
pub fn is_enabled(id: usize, state: CommandState) -> bool {
    spec(id).is_none_or(|s| (s.enabled)(state))
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
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for s in SPECS {
            assert!(seen.insert(s.id), "command id {} is registered twice", s.id);
        }
    }

    #[test]
    fn every_default_shortcut_parses() {
        for s in SPECS.iter().filter(|s| !s.shortcut.is_empty()) {
            assert!(crate::shortcuts::parse(s.shortcut).is_some(), "{}: bad shortcut {:?}", s.palette, s.shortcut);
        }
    }

    #[test]
    fn every_typed_command_has_a_spec_and_round_trips() {
        for c in Command::FIXED {
            assert!(c.spec().is_some(), "{c:?} is not registered");
            assert_eq!(Command::from_id(c.id()), Some(c));
        }
    }

    #[test]
    fn tooltip_names_the_command_and_its_shortcut() {
        assert_eq!(Command::FileSave.spec().map(CommandSpec::tooltip).unwrap_or_default(), "Save (Ctrl+S)");
        assert_eq!(Command::FileOpen.spec().map(CommandSpec::tooltip).unwrap_or_default(), "Open (Ctrl+O)");
    }

    #[test]
    fn menu_text_uses_the_override_when_given() {
        let s = Command::FileNew.spec().expect("registered");
        assert_eq!(s.menu_text(None), "&New\tCtrl+N");
        assert_eq!(s.menu_text(Some("Ctrl+Alt+N")), "&New\tCtrl+Alt+N");
        assert_eq!(s.menu_text(Some("")), "&New");
    }

    #[test]
    fn selection_commands_wait_for_a_selection() {
        let none = CommandState::default();
        let some = CommandState { has_selection: true, ..CommandState::default() };
        assert!(!is_enabled(ID_COPY, none));
        assert!(is_enabled(ID_COPY, some));
        assert!(is_enabled(ID_NEW, none));
    }

    #[test]
    fn undo_and_redo_follow_their_stacks() {
        let s = CommandState { can_undo: true, ..CommandState::default() };
        assert!(is_enabled(ID_UNDO, s));
        assert!(!is_enabled(ID_REDO, s));
    }

    #[test]
    fn family_members_decode_to_typed_commands_and_back() {
        for id in [
            ID_RECENT_BASE,
            ID_RECENT_BASE + 7,
            ID_ENCODE_BASE + 4,
            ID_CONVERT_BASE + 2,
            ID_CHARSET_BASE + crate::doc::CHARSETS.len(),
            ID_LANG_BASE,
            ID_LANG_BASE + 5,
        ] {
            let c = Command::from_id(id).expect("a family member");
            assert_eq!(c.id(), id);
        }
        assert_eq!(Command::from_id(ID_RECENT_BASE + 3), Some(Command::OpenRecent(3)));
        assert_eq!(Command::from_id(ID_LANG_BASE + 1), Some(Command::SetLanguage(1)));
    }

    #[test]
    fn ids_outside_every_family_are_not_commands() {
        assert_eq!(Command::from_id(ID_RECENT_BASE + RECENT_SLOTS), None);
        assert_eq!(Command::from_id(ID_ENCODE_BASE + crate::tables::ENCODE_IN.len()), None);
        assert_eq!(Command::from_id(ID_LANG_BASE + crate::lang::LANGUAGES.len()), None);
    }

    #[test]
    fn palette_lists_only_commands_with_a_label() {
        assert!(palette().all(|s| !s.palette.is_empty()));
        assert!(palette().count() > 100);
    }
}
