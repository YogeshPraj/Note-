//! Win32 shell: the host window, layout, menus, commands and message loop.
//!
//! The ONLY place that wires Windows UI to the portable `core`. Drawing of the
//! menu bar / toolbar / tabs / status bar lives in `chrome.rs`, the Find panel
//! in `findpanel.rs`. All editor behaviour is in `core` (`Workspace`, `search`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, GetCapture, ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, PWSTR, w};

use super::chrome::{self, Chrome, StatusText, ToolItem};
use super::clip;
use super::findpanel::{self, Mode};
use super::form;
use super::incsearch;
use super::mdbar;
use super::palette::{self, Item, Kind};
use super::preview as pview;
use super::print;
use super::sidepanel;
use super::tree;
use super::whiteboard as wb;
use crate::core::edit::{self, ColumnNumbers, NumFormat, SortKind};
use crate::core::bookmarks;
use crate::core::hash::{ALGOS, Algo};
use crate::core::settings::{self, Settings};
use crate::core::shortcuts;
use super::dialogs;
use crate::core::functions;
use crate::core::mdformat;
use crate::core::view;
use crate::core::preview as pcore;
use crate::core::doc::{CHARSETS, Enc, Eol};
use crate::core::files;
use crate::core::lang::LANGUAGES;
use crate::core::sci::{self, Send, Theme};
use crate::core::search::{self, Hit};
use crate::core::session::{Session, config_dir, session_path};
use crate::core::whiteboard;
use crate::core::workspace::{CaseOp, CloseOp, Completion, Confirm, LineOp, PathText, TextTool, Ui, Workspace};

const WM_REFRESH: u32 = WM_APP + 1;
const WM_MINIMAP: u32 = WM_APP + 2;
const WM_SPLIT: u32 = WM_APP + 60;
const TIMER_PREVIEW: usize = 1;
const MINI_W: i32 = 110;
const SPLIT_W: i32 = 5;
const ID_PREVIEW: usize = 5301;
// Markdown toolbar and commands (the toolbar buttons use these ids directly)
const ID_MD_BOLD: usize = 5400;
const ID_MD_ITALIC: usize = 5401;
const ID_MD_STRIKE: usize = 5402;
const ID_MD_CODE: usize = 5403;
const ID_MD_H1: usize = 5404;
const ID_MD_H2: usize = 5405;
const ID_MD_H3: usize = 5406;
const ID_MD_QUOTE: usize = 5407;
const ID_MD_BULLET: usize = 5408;
const ID_MD_NUMBERED: usize = 5409;
const ID_MD_TASK: usize = 5410;
const ID_MD_LINK: usize = 5411;
const ID_MD_IMAGE: usize = 5412;
const ID_MD_TABLE: usize = 5413;
const ID_MD_CODEBLOCK: usize = 5414;
const ID_MD_RULE: usize = 5415;
const ID_MD_TOOLBAR: usize = 5417;
const ID_MD_H4: usize = 5418;
const ID_MD_H5: usize = 5419;
const ID_MD_H6: usize = 5420;

/// The format a Markdown command id stands for.
fn md_fmt(id: usize) -> Option<mdformat::Fmt> {
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

fn md_ids() -> mdbar::Ids {
    mdbar::Ids {
        bold: ID_MD_BOLD,
        italic: ID_MD_ITALIC,
        strike: ID_MD_STRIKE,
        code: ID_MD_CODE,
        h1: ID_MD_H1,
        h2: ID_MD_H2,
        h3: ID_MD_H3,
        quote: ID_MD_QUOTE,
        bullet: ID_MD_BULLET,
        numbered: ID_MD_NUMBERED,
        task: ID_MD_TASK,
        link: ID_MD_LINK,
        image: ID_MD_IMAGE,
        table: ID_MD_TABLE,
        code_block: ID_MD_CODEBLOCK,
        rule: ID_MD_RULE,
    }
}
const ID_NEW_WB: usize = 1008;

const ID_NEW: usize = 1001;
const ID_OPEN: usize = 1002;
const ID_SAVE: usize = 1003;
const ID_SAVEAS: usize = 1004;
const ID_SAVEALL: usize = 1005;
const ID_CLOSE: usize = 1006;
const ID_EXIT: usize = 1007;
const ID_UNDO: usize = 2001;
const ID_REDO: usize = 2002;
const ID_CUT: usize = 2003;
const ID_COPY: usize = 2004;
const ID_PASTE: usize = 2005;
const ID_SELALL: usize = 2006;
const ID_EOL_CRLF: usize = 2010;
const ID_EOL_LF: usize = 2011;
const ID_EOL_CR: usize = 2012;
const ID_WRAP: usize = 3001;
const ID_ZOOMIN: usize = 3002;
const ID_ZOOMOUT: usize = 3003;
const ID_ZOOMRESET: usize = 3004;
const ID_THEME: usize = 3005;
const ID_MINIMAP: usize = 3006;
const ID_NEXT_TAB: usize = 4001;
const ID_PREV_TAB: usize = 4002;
const ID_FIND: usize = 5001;
const ID_REPLACE: usize = 5002;
const ID_FINDNEXT: usize = 5003;
const ID_FINDPREV: usize = 5004;
const ID_GOTO: usize = 5005;
const ID_FIF: usize = 5006;
const ID_MARK: usize = 5007;
const ID_PALETTE: usize = 5201;
const ID_QUICKOPEN: usize = 5202;
const ID_BM_TOGGLE: usize = 5101;
const ID_BM_NEXT: usize = 5102;
const ID_BM_PREV: usize = 5103;
const ID_BM_CLEAR: usize = 5104;
const ID_BM_CUT: usize = 5105;
const ID_BM_COPY: usize = 5106;
const ID_BM_PASTE: usize = 5107;
const ID_BM_DELETE: usize = 5108;
const ID_BM_DELETE_UNMARKED: usize = 5109;
const ID_BM_INVERT: usize = 5110;
// Search menu (Notepad++ layout)
const ID_SEL_FIND_NEXT: usize = 5011;
const ID_SEL_FIND_PREV: usize = 5012;
const ID_VOL_NEXT: usize = 5013;
const ID_VOL_PREV: usize = 5014;
const ID_INCR: usize = 5015;
const ID_RESULTS: usize = 5016;
const ID_NEXT_RESULT: usize = 5017;
const ID_PREV_RESULT: usize = 5018;
const ID_BRACE_GOTO: usize = 5019;
const ID_BRACE_SEL: usize = 5020;
const ID_FIND_CHARS: usize = 5021;
const ID_STYLE_ALL_BASE: usize = 5030; // + style slot 0..4
const ID_STYLE_ONE_BASE: usize = 5040;
const ID_CLEAR_STYLE_BASE: usize = 5050; // + 0..4, + 5 = all styles
const ID_JUMP_UP_BASE: usize = 5060;
const ID_JUMP_DOWN_BASE: usize = 5070;
const ID_COPY_STYLED_BASE: usize = 5080; // + 0..4, + 5 = all styles
const SLOT_NAMES: [&str; 5] = ["1st", "2nd", "3rd", "4th", "5th"];
// View menu (Notepad++ layout)
const ID_ON_TOP: usize = 3101;
const ID_FULLSCREEN: usize = 3102;
const ID_POSTIT: usize = 3103;
const ID_DISTRACTION: usize = 3104;
const ID_BROWSER_BASE: usize = 3110; // + index into BROWSERS
const ID_SYM_WS: usize = 3120;
const ID_SYM_EOL: usize = 3121;
const ID_SYM_NONPRINT: usize = 3122;
const ID_SYM_CTRL: usize = 3123;
const ID_SYM_ALL: usize = 3124;
const ID_SYM_INDENT: usize = 3125;
const ID_SYM_WRAP: usize = 3126;
const ID_MOVE_NEW_INSTANCE: usize = 3130;
const ID_OPEN_NEW_INSTANCE: usize = 3131;
const ID_MOVE_OTHER_VIEW: usize = 3132;
const ID_CLONE_OTHER_VIEW: usize = 3133;
const ID_TAB_BASE: usize = 3140; // + 0..8 = Tab 1..9
const ID_MOVE_TAB_FWD: usize = 3150;
const ID_MOVE_TAB_BACK: usize = 3151;
const ID_FOCUS_OTHER: usize = 3160;
const ID_HIDE_LINES: usize = 3161;
const ID_FOLD_ALL: usize = 3170;
const ID_UNFOLD_ALL: usize = 3171;
const ID_FOLD_CUR: usize = 3172;
const ID_UNFOLD_CUR: usize = 3173;
const ID_FOLD_LEVEL_BASE: usize = 3180; // + 0..7
const ID_UNFOLD_LEVEL_BASE: usize = 3190; // + 0..7
const ID_SUMMARY: usize = 3200;
const ID_PROJECT_BASE: usize = 3201; // + 0..2 (not available)
const ID_DOC_LIST: usize = 3206;
const ID_FUNC_LIST: usize = 3207;
const ID_SYNC_V: usize = 3208;
const ID_SYNC_H: usize = 3209;
const ID_RTL: usize = 3210;
const ID_LTR: usize = 3211;
const ID_MONITOR: usize = 3212;
const TIMER_MONITOR: usize = 2;
const TIMER_EXTERNAL: usize = 3;
// Settings and Tools menus
const ID_PREFS: usize = 3301;
const ID_STYLE_CFG: usize = 3302;
const ID_SHORTCUTS: usize = 3303;
const ID_IMPORT_THEME: usize = 3305;
const ID_EXPORT_THEME: usize = 3306;
const ID_EDIT_CONTEXT: usize = 3307;
const ID_HASH_BASE: usize = 3400; // + algorithm index * 10 + (0 generate, 1 from files, 2 from selection)
const ID_B64_ENC: usize = 3450;
const ID_B64_DEC: usize = 3451;
const ID_URL_ENC: usize = 3452;
const ID_URL_DEC: usize = 3453;
const ID_JSON_PRETTY: usize = 3454;
const ID_JSON_MIN: usize = 3455;
const ID_XML_PRETTY: usize = 3456;
const ID_XML_MIN: usize = 3457;
const ID_WINDOWS: usize = 3600;
const ID_WINDOW_SORT_BASE: usize = 3610; // + index into dialogs::SORTS
const ID_WINDOW_DOC_BASE: usize = 11000; // + 0..9: the first nine tabs
const WINDOW_DOCS_SHOWN: usize = 9;
/// (name, executable) for View > View Current File in.
const BROWSERS: [(&str, &str); 3] = [("Firefox", "firefox.exe"), ("Chrome", "chrome.exe"), ("Edge", "msedge.exe")];
const ID_L_DUP: usize = 2101;
const ID_L_DEL: usize = 2102;
const ID_L_UP: usize = 2103;
const ID_L_DOWN: usize = 2104;
const ID_L_SORT_ASC: usize = 2105;
const ID_L_SORT_DESC: usize = 2106;
const ID_L_DEDUP: usize = 2107;
const ID_L_NOEMPTY: usize = 2108;
const ID_L_TRIM: usize = 2109;
const ID_C_UPPER: usize = 2110;
const ID_C_LOWER: usize = 2111;
const ID_COMMENT: usize = 2112;
// Edit menu (Notepad++ layout)
const ID_DELETE: usize = 2201;
const ID_BEGIN_END_SEL: usize = 2202;
const ID_BEGIN_END_COL: usize = 2203;
const ID_INS_DATE_SHORT: usize = 2210;
const ID_INS_DATE_LONG: usize = 2211;
const ID_INS_DATE_CUSTOM: usize = 2212;
const ID_CP_FULLPATH: usize = 2220;
const ID_CP_NAME: usize = 2221;
const ID_CP_DIR: usize = 2222;
const ID_CP_ALLNAMES: usize = 2223;
const ID_CP_ALLPATHS: usize = 2224;
const ID_INDENT_INC: usize = 2230;
const ID_INDENT_DEC: usize = 2231;
const ID_CASE_PROPER: usize = 2240;
const ID_CASE_PROPER_BLEND: usize = 2241;
const ID_CASE_SENTENCE: usize = 2242;
const ID_CASE_SENTENCE_BLEND: usize = 2243;
const ID_CASE_INVERT: usize = 2244;
const ID_CASE_RANDOM: usize = 2245;
const ID_L_RM_CONSEC: usize = 2250;
const ID_L_SPLIT: usize = 2251;
const ID_L_JOIN: usize = 2252;
const ID_L_RM_BLANK: usize = 2253;
const ID_L_INS_ABOVE: usize = 2254;
const ID_L_INS_BELOW: usize = 2255;
const ID_L_REVERSE: usize = 2256;
const ID_L_RANDOM: usize = 2257;
const ID_L_SORT_NOCASE_ASC: usize = 2258;
const ID_L_SORT_NOCASE_DESC: usize = 2259;
const ID_L_SORT_INT_ASC: usize = 2260;
const ID_L_SORT_INT_DESC: usize = 2261;
const ID_L_SORT_DOT_ASC: usize = 2262;
const ID_L_SORT_DOT_DESC: usize = 2263;
const ID_L_SORT_COMMA_ASC: usize = 2264;
const ID_L_SORT_COMMA_DESC: usize = 2265;
const ID_L_CUT: usize = 2266;
const ID_L_COPY: usize = 2267;
const ID_L_TRANSPOSE: usize = 2268;
const ID_CMT_LINE: usize = 2271;
const ID_UNCMT_LINE: usize = 2272;
const ID_CMT_BLOCK: usize = 2273;
const ID_UNCMT_BLOCK: usize = 2274;
const ID_AC_FUNC: usize = 2280;
const ID_AC_WORD: usize = 2281;
const ID_AC_HINT: usize = 2282;
const ID_AC_PATH: usize = 2283;
const ID_B_TRIM_LEAD: usize = 2291;
const ID_B_TRIM_BOTH: usize = 2292;
const ID_B_EOL_SPACE: usize = 2293;
const ID_B_UNNECESSARY: usize = 2294;
const ID_B_TAB_SPACE: usize = 2295;
const ID_B_SPACE_TAB_ALL: usize = 2296;
const ID_B_SPACE_TAB_LEAD: usize = 2297;
const ID_PS_HTML: usize = 2300;
const ID_PS_RTF: usize = 2301;
const ID_PS_COPY_BIN: usize = 2302;
const ID_PS_CUT_BIN: usize = 2303;
const ID_PS_PASTE_BIN: usize = 2304;
const ID_SEL_OPEN: usize = 2310;
const ID_SEL_FOLDER: usize = 2311;
const ID_SEL_SEARCH: usize = 2312;
const ID_ENGINE_BASE: usize = 2320; // + index into SEARCH_ENGINES
const ID_COLUMN_MODE: usize = 2330;
const ID_COLUMN_EDITOR: usize = 2331;
const ID_CHAR_PANEL: usize = 2332;
const ID_CLIP_HISTORY: usize = 2333;
const ID_READONLY: usize = 2334;
const ID_CLEAR_RO: usize = 2335;
const BINARY_FORMAT: &str = "Note++ Binary Text";
const ID_RECENT_BASE: usize = 10000; // + index into the recent list
const ID_RECENT_CLEAR: usize = 10100;
// File menu (Notepad++ layout)
const ID_OPEN_FOLDER_EXPLORER: usize = 1101;
const ID_OPEN_FOLDER_CMD: usize = 1102;
const ID_OPEN_FOLDER_WS: usize = 1103;
const ID_OPEN_VIEWER: usize = 1104;
const ID_OPEN_WORKSPACE: usize = 1105;
const ID_RELOAD: usize = 1106;
const ID_SAVE_COPY: usize = 1107;
const ID_RENAME: usize = 1108;
const ID_CLOSE_ALL: usize = 1109;
const ID_CLOSE_OTHERS: usize = 1110;
const ID_CLOSE_LEFT: usize = 1111;
const ID_CLOSE_RIGHT: usize = 1112;
const ID_CLOSE_UNCHANGED: usize = 1113;
const ID_RECYCLE: usize = 1114;
const ID_LOAD_SESSION: usize = 1115;
const ID_SAVE_SESSION: usize = 1116;
const ID_PRINT: usize = 1117;
const ID_PRINT_NOW: usize = 1118;
const ID_RESTORE_CLOSED: usize = 1119;
const ID_OPEN_ALL_RECENT: usize = 1120;
const ID_WORKSPACE_TOGGLE: usize = 3007;
/// Position of "Open Containing Folder" in the File menu (for greying the submenu).
const FILE_OPEN_FOLDER_POS: u32 = 3;
const ID_ABOUT: usize = 9001;
const ID_ENCODE_BASE: usize = 7000; // + index into ENCODE_IN: "Encode in ..."
const ID_CHARSET_BASE: usize = 7100; // + index into CHARSETS (CHARSETS.len() = Windows-1252)
const ID_CONVERT_BASE: usize = 7200; // + index into ENCODE_IN: "Convert to ..."
const ID_LANG_BASE: usize = 8000; // + index into LANGUAGES

/// The encodings offered by "Encode in" and "Convert to", in Notepad++'s order.
const ENCODE_IN: [Enc; 5] = [Enc::Ansi, Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Be, Enc::Utf16Le];

/// Everything the Command Palette offers: (label, command id, shortcut).
const PALETTE_COMMANDS: &[(&str, usize, &str)] = &[
    ("File: New", ID_NEW, "Ctrl+N"),
    ("File: New Whiteboard", ID_NEW_WB, ""),
    ("File: Open...", ID_OPEN, "Ctrl+O"),
    ("File: Open Containing Folder in Explorer", ID_OPEN_FOLDER_EXPLORER, ""),
    ("File: Open Containing Folder in Command Prompt", ID_OPEN_FOLDER_CMD, ""),
    ("File: Open in Default Viewer", ID_OPEN_VIEWER, ""),
    ("File: Open Folder as Workspace...", ID_OPEN_WORKSPACE, ""),
    ("File: Reload from Disk", ID_RELOAD, "Ctrl+R"),
    ("File: Save", ID_SAVE, "Ctrl+S"),
    ("File: Save As...", ID_SAVEAS, "Ctrl+Alt+S"),
    ("File: Save a Copy As...", ID_SAVE_COPY, ""),
    ("File: Save All", ID_SAVEALL, "Ctrl+Shift+S"),
    ("File: Rename...", ID_RENAME, ""),
    ("File: Close", ID_CLOSE, "Ctrl+W"),
    ("File: Close All", ID_CLOSE_ALL, "Ctrl+Shift+W"),
    ("File: Close All but Active Document", ID_CLOSE_OTHERS, ""),
    ("File: Close All to the Left", ID_CLOSE_LEFT, ""),
    ("File: Close All to the Right", ID_CLOSE_RIGHT, ""),
    ("File: Close All Unchanged", ID_CLOSE_UNCHANGED, ""),
    ("File: Move to Recycle Bin", ID_RECYCLE, ""),
    ("File: Load Session...", ID_LOAD_SESSION, ""),
    ("File: Save Session...", ID_SAVE_SESSION, ""),
    ("File: Print...", ID_PRINT, ""),
    ("File: Print Now", ID_PRINT_NOW, ""),
    ("File: Restore Recent Closed File", ID_RESTORE_CLOSED, "Ctrl+Shift+T"),
    ("File: Open All Recent Files", ID_OPEN_ALL_RECENT, ""),
    ("File: Empty Recent Files List", ID_RECENT_CLEAR, ""),
    ("View: Toggle Folder Workspace", ID_WORKSPACE_TOGGLE, ""),
    ("File: Quick Open...", ID_QUICKOPEN, "Ctrl+P"),
    ("Edit: Undo", ID_UNDO, "Ctrl+Z"),
    ("Edit: Cut", ID_CUT, "Ctrl+X"),
    ("Edit: Copy", ID_COPY, "Ctrl+C"),
    ("Edit: Paste", ID_PASTE, "Ctrl+V"),
    ("Settings: Preferences...", ID_PREFS, ""),
    ("Settings: Style Configurator...", ID_STYLE_CFG, ""),
    ("Settings: Shortcut Mapper...", ID_SHORTCUTS, ""),
    ("Settings: Import Style Theme...", ID_IMPORT_THEME, ""),
    ("Settings: Export Style Theme...", ID_EXPORT_THEME, ""),
    ("Settings: Edit Popup ContextMenu", ID_EDIT_CONTEXT, ""),
    ("Tools: Base64 Encode", ID_B64_ENC, ""),
    ("Tools: Base64 Decode", ID_B64_DEC, ""),
    ("Tools: URL Encode", ID_URL_ENC, ""),
    ("Tools: URL Decode", ID_URL_DEC, ""),
    ("Tools: JSON Pretty-print", ID_JSON_PRETTY, ""),
    ("Tools: JSON Minify", ID_JSON_MIN, ""),
    ("Tools: XML Pretty-print", ID_XML_PRETTY, ""),
    ("Tools: XML Minify", ID_XML_MIN, ""),
    ("Tools: MD5 Generate...", ID_HASH_BASE, ""),
    ("Tools: MD5 from Selection into Clipboard", ID_HASH_BASE + 2, ""),
    ("Tools: SHA-1 Generate...", ID_HASH_BASE + 10, ""),
    ("Tools: SHA-1 from Selection into Clipboard", ID_HASH_BASE + 12, ""),
    ("Tools: SHA-256 Generate...", ID_HASH_BASE + 20, ""),
    ("Tools: SHA-256 from Selection into Clipboard", ID_HASH_BASE + 22, ""),
    ("Tools: SHA-512 Generate...", ID_HASH_BASE + 30, ""),
    ("Tools: SHA-512 from Selection into Clipboard", ID_HASH_BASE + 32, ""),
    ("Window: Windows...", ID_WINDOWS, ""),
    ("Edit: Redo", ID_REDO, "Ctrl+Y"),
    ("Edit: Select All", ID_SELALL, "Ctrl+A"),
    ("Edit: Duplicate Line", ID_L_DUP, "Ctrl+D"),
    ("Edit: Delete Line", ID_L_DEL, "Ctrl+L"),
    ("Edit: Move Line Up", ID_L_UP, "Ctrl+Shift+Up"),
    ("Edit: Move Line Down", ID_L_DOWN, "Ctrl+Shift+Down"),
    ("Edit: Sort Lines Ascending", ID_L_SORT_ASC, ""),
    ("Edit: Sort Lines Descending", ID_L_SORT_DESC, ""),
    ("Edit: Remove Duplicate Lines", ID_L_DEDUP, ""),
    ("Edit: Remove Empty Lines", ID_L_NOEMPTY, ""),
    ("Edit: Trim Trailing Space", ID_L_TRIM, ""),
    ("Edit: UPPERCASE", ID_C_UPPER, "Ctrl+Shift+U"),
    ("Edit: lowercase", ID_C_LOWER, "Ctrl+U"),
    ("Edit: Toggle Line Comment", ID_COMMENT, "Ctrl+Q"),
    ("Edit: Single Line Comment", ID_CMT_LINE, "Ctrl+K"),
    ("Edit: Single Line Uncomment", ID_UNCMT_LINE, "Ctrl+Shift+K"),
    ("Edit: Block Comment", ID_CMT_BLOCK, "Ctrl+Shift+Q"),
    ("Edit: Block Uncomment", ID_UNCMT_BLOCK, "Ctrl+Shift+R"),
    ("Edit: Delete", ID_DELETE, "Del"),
    ("Edit: Begin/End Select", ID_BEGIN_END_SEL, "Ctrl+Shift+B"),
    ("Edit: Begin/End Select in Column Mode", ID_BEGIN_END_COL, "Alt+Shift+B"),
    ("Edit: Insert Date Time (short)", ID_INS_DATE_SHORT, ""),
    ("Edit: Insert Date Time (long)", ID_INS_DATE_LONG, ""),
    ("Edit: Insert Date Time (customized)", ID_INS_DATE_CUSTOM, ""),
    ("Edit: Copy Current Full File Path", ID_CP_FULLPATH, ""),
    ("Edit: Copy Current Filename", ID_CP_NAME, ""),
    ("Edit: Copy Current Dir. Path", ID_CP_DIR, ""),
    ("Edit: Copy All Filenames", ID_CP_ALLNAMES, ""),
    ("Edit: Copy All File Paths", ID_CP_ALLPATHS, ""),
    ("Edit: Increase Line Indent", ID_INDENT_INC, "Tab"),
    ("Edit: Decrease Line Indent", ID_INDENT_DEC, "Shift+Tab"),
    ("Edit: Proper Case", ID_CASE_PROPER, ""),
    ("Edit: Proper Case (blend)", ID_CASE_PROPER_BLEND, ""),
    ("Edit: Sentence case", ID_CASE_SENTENCE, ""),
    ("Edit: Sentence case (blend)", ID_CASE_SENTENCE_BLEND, ""),
    ("Edit: iNVERT cASE", ID_CASE_INVERT, ""),
    ("Edit: ranDOm CasE", ID_CASE_RANDOM, ""),
    ("Edit: Remove Consecutive Duplicate Lines", ID_L_RM_CONSEC, ""),
    ("Edit: Remove Empty Lines (Containing Blank characters)", ID_L_RM_BLANK, ""),
    ("Edit: Split Lines", ID_L_SPLIT, ""),
    ("Edit: Join Lines", ID_L_JOIN, ""),
    ("Edit: Transpose Current Line", ID_L_TRANSPOSE, "Ctrl+T"),
    ("Edit: Insert Blank Line Above Current", ID_L_INS_ABOVE, "Ctrl+Alt+Shift+Enter"),
    ("Edit: Insert Blank Line Below Current", ID_L_INS_BELOW, "Ctrl+Alt+Enter"),
    ("Edit: Reverse Line Order", ID_L_REVERSE, ""),
    ("Edit: Randomize Line Order", ID_L_RANDOM, ""),
    ("Edit: Cut Current Line", ID_L_CUT, ""),
    ("Edit: Copy Current Line", ID_L_COPY, ""),
    ("Edit: Sort Lines Ignoring Case Ascending", ID_L_SORT_NOCASE_ASC, ""),
    ("Edit: Sort Lines Ignoring Case Descending", ID_L_SORT_NOCASE_DESC, ""),
    ("Edit: Sort Lines As Integers Ascending", ID_L_SORT_INT_ASC, ""),
    ("Edit: Sort Lines As Integers Descending", ID_L_SORT_INT_DESC, ""),
    ("Edit: Sort Lines As Decimals (Dot) Ascending", ID_L_SORT_DOT_ASC, ""),
    ("Edit: Sort Lines As Decimals (Dot) Descending", ID_L_SORT_DOT_DESC, ""),
    ("Edit: Sort Lines As Decimals (Comma) Ascending", ID_L_SORT_COMMA_ASC, ""),
    ("Edit: Sort Lines As Decimals (Comma) Descending", ID_L_SORT_COMMA_DESC, ""),
    ("Edit: Function Completion", ID_AC_FUNC, "Ctrl+Space"),
    ("Edit: Word Completion", ID_AC_WORD, "Ctrl+Enter"),
    ("Edit: Function Parameters Hint", ID_AC_HINT, "Ctrl+Shift+Space"),
    ("Edit: Path Completion", ID_AC_PATH, "Ctrl+Alt+Space"),
    ("Edit: Trim Leading Space", ID_B_TRIM_LEAD, ""),
    ("Edit: Trim Leading and Trailing Space", ID_B_TRIM_BOTH, ""),
    ("Edit: EOL to Space", ID_B_EOL_SPACE, ""),
    ("Edit: Remove Unnecessary Blank and EOL", ID_B_UNNECESSARY, ""),
    ("Edit: TAB to Space", ID_B_TAB_SPACE, ""),
    ("Edit: Space to TAB (All)", ID_B_SPACE_TAB_ALL, ""),
    ("Edit: Space to TAB (Leading)", ID_B_SPACE_TAB_LEAD, ""),
    ("Edit: Paste HTML Content", ID_PS_HTML, ""),
    ("Edit: Paste RTF Content", ID_PS_RTF, ""),
    ("Edit: Copy Binary Content", ID_PS_COPY_BIN, ""),
    ("Edit: Cut Binary Content", ID_PS_CUT_BIN, ""),
    ("Edit: Paste Binary Content", ID_PS_PASTE_BIN, ""),
    ("Edit: Open Selected File", ID_SEL_OPEN, ""),
    ("Edit: Open Selected Path in Explorer", ID_SEL_FOLDER, ""),
    ("Edit: Search Selection on Internet", ID_SEL_SEARCH, ""),
    ("Edit: Column Editor...", ID_COLUMN_EDITOR, "Alt+C"),
    ("Edit: Character Panel", ID_CHAR_PANEL, ""),
    ("Edit: Clipboard History", ID_CLIP_HISTORY, ""),
    ("Edit: Set Read-Only", ID_READONLY, ""),
    ("Edit: Clear Read-Only Flag", ID_CLEAR_RO, ""),
    ("Edit: EOL to Windows (CR LF)", ID_EOL_CRLF, ""),
    ("Edit: EOL to Unix (LF)", ID_EOL_LF, ""),
    ("Edit: EOL to Macintosh (CR)", ID_EOL_CR, ""),
    ("Search: Find...", ID_FIND, "Ctrl+F"),
    ("Search: Replace...", ID_REPLACE, "Ctrl+H"),
    ("Search: Find in Files...", ID_FIF, "Ctrl+Shift+F"),
    ("Search: Mark...", ID_MARK, "Ctrl+M"),
    ("Search: Find Next", ID_FINDNEXT, "F3"),
    ("Search: Select and Find Next", ID_SEL_FIND_NEXT, "Ctrl+F3"),
    ("Search: Select and Find Previous", ID_SEL_FIND_PREV, "Ctrl+Shift+F3"),
    ("Search: Find (Volatile) Next", ID_VOL_NEXT, "Ctrl+Alt+F3"),
    ("Search: Find (Volatile) Previous", ID_VOL_PREV, "Ctrl+Alt+Shift+F3"),
    ("Search: Incremental Search", ID_INCR, "Ctrl+Alt+I"),
    ("Search: Search Results Window", ID_RESULTS, "F7"),
    ("Search: Next Search Result", ID_NEXT_RESULT, "F4"),
    ("Search: Previous Search Result", ID_PREV_RESULT, "Shift+F4"),
    ("Search: Go to Matching Brace", ID_BRACE_GOTO, "Ctrl+B"),
    ("Search: Select All Between Matching Braces", ID_BRACE_SEL, "Ctrl+Alt+B"),
    ("Search: Style All Occurrences of Token (1st)", ID_STYLE_ALL_BASE, ""),
    ("Search: Clear all Styles", ID_CLEAR_STYLE_BASE + 5, ""),
    ("Search: Find characters in range...", ID_FIND_CHARS, ""),
    ("Bookmark: Cut Bookmarked Lines", ID_BM_CUT, ""),
    ("Bookmark: Copy Bookmarked Lines", ID_BM_COPY, ""),
    ("Bookmark: Paste to (Replace) Bookmarked Lines", ID_BM_PASTE, ""),
    ("Bookmark: Remove Bookmarked Lines", ID_BM_DELETE, ""),
    ("Bookmark: Remove Non-Bookmarked Lines", ID_BM_DELETE_UNMARKED, ""),
    ("Bookmark: Inverse", ID_BM_INVERT, ""),
    ("Search: Find Previous", ID_FINDPREV, "Shift+F3"),
    ("Search: Go to Line...", ID_GOTO, "Ctrl+G"),
    ("Bookmark: Toggle", ID_BM_TOGGLE, "Ctrl+F2"),
    ("Bookmark: Next", ID_BM_NEXT, "F2"),
    ("Bookmark: Previous", ID_BM_PREV, "Shift+F2"),
    ("Bookmark: Clear All", ID_BM_CLEAR, ""),
    ("View: Toggle Word Wrap", ID_WRAP, ""),
    ("View: Always on Top", ID_ON_TOP, ""),
    ("View: Toggle Full Screen Mode", ID_FULLSCREEN, "F11"),
    ("View: Post-It", ID_POSTIT, "F12"),
    ("View: Distraction Free Mode", ID_DISTRACTION, ""),
    ("View: Show Space and Tab", ID_SYM_WS, ""),
    ("View: Show End of Line", ID_SYM_EOL, ""),
    ("View: Show All Characters", ID_SYM_ALL, ""),
    ("View: Show Indent Guide", ID_SYM_INDENT, ""),
    ("View: Show Wrap Symbol", ID_SYM_WRAP, ""),
    ("View: Hide Lines", ID_HIDE_LINES, "Alt+H"),
    ("View: Fold All", ID_FOLD_ALL, "Alt+0"),
    ("View: Unfold All", ID_UNFOLD_ALL, "Alt+Shift+0"),
    ("View: Fold Current Level", ID_FOLD_CUR, "Ctrl+Alt+F"),
    ("View: Unfold Current Level", ID_UNFOLD_CUR, "Ctrl+Alt+Shift+F"),
    ("View: Summary...", ID_SUMMARY, ""),
    ("View: Document List", ID_DOC_LIST, ""),
    ("View: Function List", ID_FUNC_LIST, ""),
    ("View: Text Direction RTL", ID_RTL, "Ctrl+Alt+R"),
    ("View: Text Direction LTR", ID_LTR, "Ctrl+Alt+L"),
    ("View: Monitoring (tail -f)", ID_MONITOR, ""),
    ("View: Move Tab Forward", ID_MOVE_TAB_FWD, "Ctrl+Shift+PgDn"),
    ("View: Move Tab Backward", ID_MOVE_TAB_BACK, "Ctrl+Shift+PgUp"),
    ("View: Open in New Instance", ID_OPEN_NEW_INSTANCE, ""),
    ("View: Move to New Instance", ID_MOVE_NEW_INSTANCE, ""),
    ("View: Toggle Live Preview", ID_PREVIEW, "Ctrl+Shift+V"),
    ("View: Markdown Toolbar", ID_MD_TOOLBAR, ""),
    ("Markdown: Bold", ID_MD_BOLD, ""),
    ("Markdown: Italic", ID_MD_ITALIC, ""),
    ("Markdown: Strikethrough", ID_MD_STRIKE, ""),
    ("Markdown: Inline Code", ID_MD_CODE, ""),
    ("Markdown: Heading 1", ID_MD_H1, ""),
    ("Markdown: Heading 2", ID_MD_H2, ""),
    ("Markdown: Heading 3", ID_MD_H3, ""),
    ("Markdown: Heading 4", ID_MD_H4, ""),
    ("Markdown: Heading 5", ID_MD_H5, ""),
    ("Markdown: Heading 6", ID_MD_H6, ""),
    ("Markdown: Quote", ID_MD_QUOTE, ""),
    ("Markdown: Bulleted List", ID_MD_BULLET, ""),
    ("Markdown: Numbered List", ID_MD_NUMBERED, ""),
    ("Markdown: Task List", ID_MD_TASK, ""),
    ("Markdown: Link", ID_MD_LINK, ""),
    ("Markdown: Image", ID_MD_IMAGE, ""),
    ("Markdown: Table", ID_MD_TABLE, ""),
    ("Markdown: Code Block", ID_MD_CODEBLOCK, ""),
    ("Markdown: Horizontal Rule", ID_MD_RULE, ""),
    ("View: Toggle Dark Mode", ID_THEME, ""),
    ("View: Toggle Minimap", ID_MINIMAP, ""),
    ("View: Zoom In", ID_ZOOMIN, "Ctrl+Num +"),
    ("View: Zoom Out", ID_ZOOMOUT, "Ctrl+Num -"),
    ("View: Reset Zoom", ID_ZOOMRESET, ""),
    ("View: Next Tab", ID_NEXT_TAB, "Ctrl+Tab"),
    ("View: Previous Tab", ID_PREV_TAB, "Ctrl+Shift+Tab"),
    ("Help: About Note++", ID_ABOUT, ""),
];

fn command_items() -> Vec<Item> {
    let mut v: Vec<Item> = PALETTE_COMMANDS.iter().map(|(l, id, k)| Item { label: l.to_string(), hint: k.to_string(), id: *id }).collect();
    for (i, e) in ENCODE_IN.iter().enumerate() {
        v.push(Item { label: format!("Encoding: Encode in {}", e.label()), hint: String::new(), id: ID_ENCODE_BASE + i });
        v.push(Item { label: format!("Encoding: Convert to {}", e.label()), hint: String::new(), id: ID_CONVERT_BASE + i });
    }
    for (i, c) in CHARSETS.iter().enumerate() {
        v.push(Item { label: format!("Encoding: Character set {} ({})", c.label, c.group), hint: String::new(), id: ID_CHARSET_BASE + i });
    }
    for (i, (label, _)) in LANGUAGES.iter().enumerate() {
        v.push(Item { label: format!("Language: {label}"), hint: String::new(), id: ID_LANG_BASE + i });
    }
    v
}

/// Win32 delivers Scintilla messages with SendMessage. GTK would use
/// `scintilla_send_message`, Cocoa `[ScintillaView message:]`.
#[derive(Clone, Copy)]
pub struct Win32Sci(pub HWND);

impl Send for Win32Sci {
    fn send(&self, msg: u32, w: usize, l: isize) -> isize {
        unsafe { SendMessageW(self.0, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
    }
}

#[derive(Clone, Copy)]
pub struct Handles {
    host: HWND,
    edit: HWND,
    mini: HWND,
    split: HWND,
    tree: HWND,
    side: HWND,
    chrome: Chrome,
    panel: HWND,
    mdbar: HWND,
}

#[derive(Clone, Copy)]
pub struct Menus {
    file: HMENU,
    open_folder: HMENU,
    close_multi: HMENU,
    edit: HMENU,
    sym: HMENU,
    search: HMENU,
    on_sel: HMENU,
    engine: HMENU,
    view: HMENU,
    enc: HMENU,
    eol: HMENU,
    lang: HMENU,
    window: HMENU,
}

struct App {
    ws: Workspace<Win32Sci>,
    ui: Win32Ui,
    menus: Menus,
    hits: Vec<Hit>,
    /// Which result F4 / Shift+F4 last opened.
    hit_cursor: usize,
    /// Tabs followed like `tail -f` (tab id -> file length last seen).
    monitoring: HashMap<u64, u64>,
    /// What the Function List was last built from (document, length).
    side_key: Option<(isize, isize)>,
    hits_root: PathBuf,
    /// One-shot message shown in place of the cursor position.
    flash: Option<String>,
    /// What the File menu was last built from: (recent list, can restore a closed file).
    file_menu_key: (Vec<PathBuf>, bool),
    /// What the Window menu was last built from: (title and modified flag per tab, active tab).
    window_key: (Vec<(String, bool)>, usize),
    /// Files offered by Quick Open (index = palette item id).
    quick_files: Vec<PathBuf>,
    /// Content identity last sent to the preview, to skip redundant renders.
    preview_key: Option<(isize, usize, u64)>,
    /// Document the auto-open check last looked at.
    preview_doc: isize,
    preview_dark: bool,
    /// Excalidraw page has loaded and can receive a scene.
    wb_ready: bool,
    /// Tab id whose scene is currently loaded in the whiteboard view.
    wb_shown: Option<u64>,
    wb_dark: bool,
    /// A Save / Save As waiting for the page to hand over its latest scene.
    wb_pending: Option<WbPending>,
}

#[derive(Clone, Copy, PartialEq)]
enum WbPending {
    Save,
    SaveAs,
    SaveCopy,
}

/// Commands that make sense on a whiteboard tab (everything else targets the text editor).
fn wb_allows(id: usize) -> bool {
    matches!(
        id,
        ID_NEW | ID_NEW_WB | ID_OPEN | ID_SAVE | ID_SAVEAS | ID_SAVEALL | ID_CLOSE | ID_EXIT | ID_THEME | ID_NEXT_TAB | ID_PREV_TAB
            | ID_ABOUT | ID_PALETTE | ID_QUICKOPEN | ID_RECENT_CLEAR | ID_OPEN_FOLDER_EXPLORER | ID_OPEN_FOLDER_CMD
            | ID_OPEN_FOLDER_WS | ID_OPEN_VIEWER | ID_OPEN_WORKSPACE | ID_RELOAD | ID_SAVE_COPY | ID_RENAME | ID_CLOSE_ALL
            | ID_CLOSE_OTHERS | ID_CLOSE_LEFT | ID_CLOSE_RIGHT | ID_CLOSE_UNCHANGED | ID_RECYCLE | ID_LOAD_SESSION
            | ID_SAVE_SESSION | ID_RESTORE_CLOSED | ID_OPEN_ALL_RECENT | ID_WORKSPACE_TOGGLE | ID_PREFS | ID_STYLE_CFG | ID_SHORTCUTS
            | ID_IMPORT_THEME | ID_EXPORT_THEME | ID_EDIT_CONTEXT | ID_WINDOWS
    ) || (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id)
        || (ID_WINDOW_DOC_BASE..ID_WINDOW_DOC_BASE + WINDOW_DOCS_SHOWN).contains(&id)
        || (ID_WINDOW_SORT_BASE..ID_WINDOW_SORT_BASE + dialogs::SORTS.len()).contains(&id)
}

thread_local! {
    static HANDLES: Cell<Option<Handles>> = const { Cell::new(None) };
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static MINI_ON: Cell<bool> = const { Cell::new(true) };
    static PREVIEW_ON: Cell<bool> = const { Cell::new(false) };
    /// The Markdown formatting bar is shown (a Markdown tab is active and the bar is enabled).
    static MD_BAR_ON: Cell<bool> = const { Cell::new(false) };
    /// Editor top line last pushed to / received from the preview (usize::MAX = send again).
    static SYNC_LINE: Cell<usize> = const { Cell::new(usize::MAX) };
    static PREVIEW_MAX: Cell<bool> = const { Cell::new(false) };
    /// The active tab is a whiteboard, so the whiteboard view replaces the editor.
    static WB_ON: Cell<bool> = const { Cell::new(false) };
    /// The workspace tree is shown (only when a folder is open).
    static TREE_ON: Cell<bool> = const { Cell::new(false) };
    /// Where the editor's selection started when Incremental Search opened.
    static INCR_START: Cell<usize> = const { Cell::new(0) };
    /// Chrome parts hidden by Post-It / Distraction Free Mode: 1 menu, 2 toolbar, 4 tabs, 8 status bar.
    static HIDE: Cell<u8> = const { Cell::new(0) };
    /// Parts of the chrome turned off in Preferences (same bits as `HIDE`).
    static USER_HIDE: Cell<u8> = const { Cell::new(0) };
    /// The live accelerator table (a HACCEL handle).
    static ACCEL_TABLE: Cell<isize> = const { Cell::new(0) };
    static DISTRACTION: Cell<bool> = const { Cell::new(false) };
    static POSTIT: Cell<bool> = const { Cell::new(false) };
    static FULLSCREEN: Cell<bool> = const { Cell::new(false) };
    /// Window placement and style to return to when leaving full screen.
    static WINDOWED: Cell<Option<(WINDOWPLACEMENT, isize)>> = const { Cell::new(None) };
    /// Fraction of the editor row given to the preview.
    static PREVIEW_RATIO: Cell<f32> = const { Cell::new(0.5) };
}

static MINI_PREV: AtomicIsize = AtomicIsize::new(0);

/// Run `f` on the app unless it is already borrowed (e.g. a message arrives
/// while a modal dialog is open inside a command), in which case skip.
fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|c| {
        if let Ok(mut b) = c.try_borrow_mut() {
            if let Some(a) = b.as_mut() {
                f(a);
            }
        }
    });
}

fn handles() -> Option<Handles> {
    HANDLES.with(|c| c.get())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Build a double-NUL terminated "label\0pattern\0...\0\0" filter string.
fn wide_multi(parts: &[&str]) -> Vec<u16> {
    let mut v = Vec::new();
    for p in parts {
        v.extend(p.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

// ---- file dialogs ----------------------------------------------------------

#[derive(Clone, Copy)]
struct Win32Ui {
    host: HWND,
}

impl Ui for Win32Ui {
    fn pick_open(&self) -> Vec<PathBuf> {
        let mut buf = vec![0u16; 65536];
        let filter = wide_multi(&["All files (*.*)", "*.*", "Text (*.txt)", "*.txt"]);
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.host,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_EXPLORER | OFN_ALLOWMULTISELECT | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY,
            ..Default::default()
        };
        if !unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
            return Vec::new();
        }
        // Result: "dir\0file1\0file2\0\0", or just "fullpath\0\0" for one file.
        let parts: Vec<String> =
            buf.split(|c| *c == 0).take_while(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
        match parts.len() {
            0 => Vec::new(),
            1 => vec![PathBuf::from(&parts[0])],
            _ => parts[1..].iter().map(|f| PathBuf::from(&parts[0]).join(f)).collect(),
        }
    }

    fn pick_save(&self, suggested: &str) -> Option<PathBuf> {
        let mut buf = vec![0u16; 32768];
        for (i, c) in suggested.encode_utf16().take(buf.len() - 1).enumerate() {
            buf[i] = c;
        }
        let filter = wide_multi(&["All files (*.*)", "*.*", "Text (*.txt)", "*.txt"]);
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.host,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
            ..Default::default()
        };
        if !unsafe { GetSaveFileNameW(&mut ofn) }.as_bool() {
            return None;
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
    }

    fn pick_session(&self, save: bool) -> Option<PathBuf> {
        let mut buf = vec![0u16; 32768];
        if save {
            for (i, c) in "session.json".encode_utf16().enumerate() {
                buf[i] = c;
            }
        }
        let filter = wide_multi(&["Note++ session (*.json)", "*.json", "All files (*.*)", "*.*"]);
        let ext = wide("json");
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.host,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            lpstrDefExt: PCWSTR(ext.as_ptr()),
            Flags: if save { OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST } else { OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST },
            ..Default::default()
        };
        let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
        if !ok.as_bool() {
            return None;
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
    }

    fn ask_yes_no(&self, msg: &str) -> bool {
        dialogs::message(self.host, "Note++", msg, dialogs::Tone::Question, &[("&Yes", dialogs::BTN_YES), ("&No", dialogs::BTN_NO)], dialogs::BTN_YES, dialogs::BTN_NO) == dialogs::BTN_YES
    }

    fn recycle(&self, path: &std::path::Path) -> Result<(), String> {
        use windows::Win32::UI::Shell::{FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, SHFILEOPSTRUCTW, SHFileOperationW};
        // pFrom is a list of paths, each NUL-terminated, ended by an extra NUL.
        let mut from: Vec<u16> = path.as_os_str().to_string_lossy().encode_utf16().collect();
        from.extend([0, 0]);
        let mut op = SHFILEOPSTRUCTW {
            hwnd: self.host,
            wFunc: FO_DELETE,
            pFrom: PCWSTR(from.as_ptr()),
            fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT).0 as u16,
            ..Default::default()
        };
        let rc = unsafe { SHFileOperationW(&mut op) };
        if rc == 0 && !op.fAnyOperationsAborted.as_bool() {
            Ok(())
        } else {
            Err(format!("error code {rc:#x}"))
        }
    }

    fn confirm_save(&self, name: &str) -> Confirm {
        let buttons = [("&Save", dialogs::BTN_SAVE), ("Do&n't Save", dialogs::BTN_DONT_SAVE), ("Cancel", dialogs::BTN_CANCEL)];
        match dialogs::message(self.host, "Note++", &format!("Save changes to \"{name}\"?"), dialogs::Tone::Warning, &buttons, dialogs::BTN_SAVE, dialogs::BTN_CANCEL) {
            dialogs::BTN_SAVE => Confirm::Save,
            dialogs::BTN_DONT_SAVE => Confirm::Discard,
            _ => Confirm::Cancel,
        }
    }

    fn error(&self, msg: &str) {
        dialogs::message(self.host, "Note++", msg, dialogs::Tone::Error, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
    }
}

// ---- app behaviour -----------------------------------------------------------

/// Open Explorer with `path` selected.
fn explorer_select(path: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    // raw_arg: Explorer wants `/select,"path"` as one argument with its own quotes.
    let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

/// A new Command Prompt window in `dir`.
fn open_command_prompt(dir: Option<&std::path::Path>) {
    use std::os::windows::process::CommandExt;
    let mut c = std::process::Command::new("cmd");
    c.arg("/K");
    if let Some(d) = dir {
        c.current_dir(d);
    }
    let _ = c.creation_flags(0x10).spawn(); // CREATE_NEW_CONSOLE
}

/// Open a file with whatever program Windows associates with it.
fn shell_open(host: HWND, path: &std::path::Path) {
    let p = wide(&path.display().to_string());
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(Some(host), w!("open"), PCWSTR(p.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

/// Show `dir` in the workspace tree and make it the root for Quick Open / Find in Files.
fn open_workspace(ws: &mut Workspace<Win32Sci>, dir: PathBuf) {
    tree::set_root(&dir);
    ws.workspace = Some(dir);
    TREE_ON.with(|c| c.set(true));
    layout();
}

/// The file or folder named by selected text (quotes and extra lines ignored),
/// looked up as given and then relative to each of `bases`.
fn selected_path(selection: &str, bases: &[Option<&std::path::Path>]) -> Option<PathBuf> {
    let first = selection.lines().next()?.trim().trim_matches(['"', '\'']).trim();
    if first.is_empty() {
        return None;
    }
    let direct = PathBuf::from(first);
    if direct.exists() {
        return Some(direct);
    }
    bases.iter().flatten().map(|b| b.join(first)).find(|p| p.exists())
}

fn local_time() -> edit::DateTime {
    use windows::Win32::System::SystemInformation::GetLocalTime;
    let t = unsafe { GetLocalTime() };
    edit::DateTime {
        year: t.wYear as i32,
        month: t.wMonth as u32,
        day: t.wDay as u32,
        weekday: t.wDayOfWeek as u32,
        hour: t.wHour as u32,
        minute: t.wMinute as u32,
        second: t.wSecond as u32,
    }
}

/// Time then date, in the user's regional format.
fn system_date_time(long: bool) -> String {
    use windows::Win32::Globalization::{DATE_LONGDATE, DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_FORMAT_FLAGS};
    use windows::Win32::System::SystemInformation::GetLocalTime;
    unsafe {
        let now = GetLocalTime();
        let mut date = [0u16; 128];
        let mut time = [0u16; 64];
        let d = GetDateFormatEx(PCWSTR::null(), if long { DATE_LONGDATE } else { DATE_SHORTDATE }, Some(&now), PCWSTR::null(), Some(&mut date), PCWSTR::null());
        let t = GetTimeFormatEx(PCWSTR::null(), TIME_FORMAT_FLAGS(0), Some(&now), PCWSTR::null(), Some(&mut time));
        let part = |buf: &[u16], n: i32| String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())]);
        format!("{} {}", part(&time, t), part(&date, d))
    }
}

/// Search > Find characters in range: stays open so Find can be pressed repeatedly.
fn find_chars_dialog(host: HWND, sci: Win32Sci) {
    use form::{Kind, ctl};
    const NON_ASCII: usize = 100;
    const ASCII: usize = 101;
    const MY_RANGE: usize = 102;
    const FROM: usize = 103;
    const TO: usize = 104;
    const UP: usize = 121;
    const DOWN: usize = 122;
    const WRAP: usize = 123;
    const STATUS: usize = 130;
    fn react(f: &form::Form, _id: usize) {
        let mine = f.checked(MY_RANGE);
        f.enable(FROM, mine);
        f.enable(TO, mine);
    }
    // Decimal, or hex with a 0x prefix.
    fn number(s: &str) -> Option<u32> {
        let s = s.trim();
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16).ok(),
            None => s.parse().ok(),
        }
    }
    let apply = move |v: &form::Values, f: &form::Form| {
        let (lo, hi) = if v.checked(NON_ASCII) {
            (0x80, u32::MAX)
        } else if v.checked(ASCII) {
            (0x00, 0x7F)
        } else {
            match (number(v.text(FROM)), number(v.text(TO))) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => {
                    f.set_text(STATUS, "Enter a valid range, e.g. 0 to 255");
                    return true;
                }
            }
        };
        let found = search::find_char(&sci, lo, hi, v.checked(DOWN), v.checked(WRAP));
        f.set_text(STATUS, if found { "Found" } else { "No character in that range" });
        true
    };
    let spec = form::Spec {
        title: "Find characters in range",
        size: (360, 270),
        ctls: vec![
            ctl(110, Kind::Group, "Search mode", 12, 8, 336, 118),
            ctl(NON_ASCII, Kind::Radio(true), "&Non-ASCII characters (128 - 255 and above)", 26, 28, 310, 20),
            ctl(ASCII, Kind::Radio(false), "&ASCII characters (0 - 127)", 26, 52, 310, 20),
            ctl(MY_RANGE, Kind::Radio(false), "&My range:", 26, 80, 90, 20),
            ctl(FROM, Kind::Edit, "0", 120, 78, 60, 22),
            ctl(111, Kind::Label, "to", 188, 81, 16, 18),
            ctl(TO, Kind::Edit, "255", 208, 78, 60, 22),
            ctl(112, Kind::Label, "(decimal, or 0x for hex)", 26, 102, 250, 18),
            ctl(120, Kind::Group, "Direction", 12, 132, 336, 48),
            ctl(UP, Kind::Radio(true), "&Up", 26, 150, 80, 20),
            ctl(DOWN, Kind::Radio(false), "&Down", 120, 150, 80, 20),
            ctl(WRAP, Kind::Check, "&Wrap around", 26, 188, 200, 20),
            ctl(STATUS, Kind::Label, "", 12, 214, 336, 18),
        ],
        ok: "Find",
        on_change: Some(react),
        on_apply: Some(&apply),
    };
    let _ = form::run(host, spec, |f| {
        f.set_checked(NON_ASCII, true);
        f.set_checked(DOWN, true);
        f.set_checked(WRAP, true);
        react(f, 0);
    });
    unsafe {
        let _ = SetFocus(Some(sci.0));
    }
}

// ---- settings, shortcuts and the editor's context menu --------------------------

/// Chrome parts the user turned off in Preferences (same bits as `HIDE`).
fn user_hide(s: &Settings) -> u8 {
    (!s.show_toolbar as u8) << 1 | (!s.show_tabbar as u8) << 2 | (!s.show_statusbar as u8) << 3
}

/// Make `s` current: restyle the editor, save it, and update the chrome.
fn apply_settings(ws: &mut Workspace<Win32Sci>, s: Settings) {
    ws.set_settings(s);
    let _ = ws.settings.save();
    USER_HIDE.with(|c| c.set(user_hide(&ws.settings)));
    ws.set_margins_visible(!DISTRACTION.with(|c| c.get()));
    layout();
}

/// The command shortcuts the Shortcut Mapper offers: everything in the command
/// list except the ones Scintilla handles itself.
fn mapper_rows(s: &Settings) -> Vec<dialogs::MapRow> {
    let native = [ID_UNDO, ID_REDO, ID_SELALL, ID_CUT, ID_COPY, ID_PASTE];
    let mut seen = std::collections::HashSet::new();
    PALETTE_COMMANDS
        .iter()
        .filter(|(_, id, _)| !native.contains(id) && seen.insert(*id))
        .map(|(name, id, default)| {
            let current = s.shortcuts.get(&id.to_string()).cloned().unwrap_or_else(|| default.to_string());
            dialogs::MapRow { id: *id, name: name.to_string(), default: default.to_string(), current }
        })
        .collect()
}

/// Rewrite the shortcut shown after the tab in every menu item for `id`.
fn retext_deep(menu: HMENU, id: usize, shortcut: &str) {
    unsafe {
        for i in 0..GetMenuItemCount(Some(menu)).max(0) {
            let sub = GetSubMenu(menu, i);
            if !sub.0.is_null() {
                retext_deep(sub, id, shortcut);
            }
        }
        let mut buf = [0u16; 256];
        let n = GetMenuStringW(menu, id as u32, Some(&mut buf), MF_BYCOMMAND);
        if n <= 0 {
            return;
        }
        let text = String::from_utf16_lossy(&buf[..n as usize]);
        let base = text.split('\t').next().unwrap_or("");
        let new = if shortcut.is_empty() { base.to_string() } else { format!("{base}\t{shortcut}") };
        if new != text {
            let grayed = GetMenuState(menu, id as u32, MF_BYCOMMAND) & MF_GRAYED.0 != 0;
            let t = wide(&new);
            let _ = ModifyMenuW(menu, id as u32, MF_BYCOMMAND | MF_STRING | if grayed { MF_GRAYED } else { MF_ENABLED }, id, PCWSTR(t.as_ptr()));
        }
    }
}

/// Rebuild the accelerator table and menu texts from the saved shortcut overrides.
fn install_shortcuts(menus: &Menus, s: &Settings, previous: Option<&Settings>) {
    let table = accelerators(&s.shortcuts);
    let old = ACCEL_TABLE.with(|c| c.replace(table.0 as isize));
    if old != 0 {
        unsafe {
            let _ = DestroyAcceleratorTable(HACCEL(old as *mut core::ffi::c_void));
        }
    }
    // Only items whose shortcut changed need new menu text.
    let before = previous.map(|p| &p.shortcuts);
    for (id_text, text) in &s.shortcuts {
        if before.is_none_or(|b| b.get(id_text) != Some(text)) {
            if let Ok(id) = id_text.parse::<usize>() {
                for m in [menus.file, menus.edit, menus.search, menus.view] {
                    retext_deep(m, id, text);
                }
            }
        }
    }
    if let Some(p) = previous {
        for id_text in p.shortcuts.keys().filter(|k| !s.shortcuts.contains_key(*k)) {
            if let Ok(id) = id_text.parse::<usize>() {
                let default = PALETTE_COMMANDS.iter().find(|(_, i, _)| *i == id).map(|(_, _, d)| *d).unwrap_or("");
                for m in [menus.file, menus.edit, menus.search, menus.view] {
                    retext_deep(m, id, default);
                }
            }
        }
    }
}

const DEFAULT_CONTEXT_MENU: &str = "# Right-click menu of the editor. One command per line, as named in the Command Palette;\n\
# a line with just - makes a separator. Save the file and the next right-click uses it.\n\
Edit: Cut\nEdit: Copy\nEdit: Paste\nEdit: Delete\nEdit: Select All\n-\n\
Edit: Open Selected File\nEdit: Search Selection on Internet\n-\n\
Edit: Toggle Line Comment\nEdit: Block Comment\nEdit: Block Uncomment\n-\n\
Edit: UPPERCASE\nEdit: lowercase\nEdit: Proper Case\n-\n\
Search: Style All Occurrences of Token (1st)\nSearch: Clear all Styles\n-\n\
View: Fold All\nView: Unfold All\n";

fn context_menu_path() -> PathBuf {
    config_dir().join("contextmenu.txt")
}

fn ensure_context_menu_file() -> PathBuf {
    let p = context_menu_path();
    if !p.exists() {
        let _ = std::fs::create_dir_all(config_dir());
        let _ = std::fs::write(&p, DEFAULT_CONTEXT_MENU);
    }
    p
}

/// Entries of the context menu file: Some(command) or None for a separator.
fn context_entries(text: &str) -> Vec<Option<(&'static str, usize, &'static str)>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            if l == "-" {
                Some(None)
            } else {
                PALETTE_COMMANDS.iter().find(|(name, _, _)| name.eq_ignore_ascii_case(l)).map(|(n, id, k)| Some((*n, *id, *k)))
            }
        })
        .collect()
}

/// The theme colours as JSON ({"light": {...}, "dark": {...}}) for export.
fn theme_json(s: &Settings) -> String {
    let side = |base: crate::core::sci::Theme, over: &std::collections::BTreeMap<String, String>| {
        let t = settings::themed(base, over);
        let map: serde_json::Map<String, serde_json::Value> =
            settings::ROLES.iter().filter_map(|(_, k)| Some((k.to_string(), serde_json::Value::String(settings::to_hex(settings::color_of(&t, k)?))))).collect();
        serde_json::Value::Object(map)
    };
    let v = serde_json::json!({ "light": side(crate::core::sci::Theme::light(), &s.light_colors), "dark": side(crate::core::sci::Theme::dark(), &s.dark_colors) });
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

/// Read a theme file into colour overrides; returns how many colours were taken.
fn import_theme(s: &mut Settings, json: &str) -> Result<usize, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut taken = 0;
    for (name, dark) in [("light", false), ("dark", true)] {
        let Some(obj) = v.get(name).and_then(|x| x.as_object()) else { continue };
        let base = if dark { crate::core::sci::Theme::dark() } else { crate::core::sci::Theme::light() };
        let entered: Vec<(&str, String)> =
            settings::ROLES.iter().filter_map(|(_, k)| obj.get(*k).and_then(|x| x.as_str()).map(|h| (*k, h.to_string()))).collect();
        let diff = settings::diff_from(&base, &entered);
        taken += entered.iter().filter(|(_, h)| settings::parse_hex(h).is_some()).count();
        if dark {
            s.dark_colors = diff;
        } else {
            s.light_colors = diff;
        }
    }
    if taken == 0 { Err("The file has no \"light\" or \"dark\" colours".into()) } else { Ok(taken) }
}

/// Enter or leave full screen on the monitor the window is on.
fn set_fullscreen(host: HWND, on: bool) {
    use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow};
    if on == FULLSCREEN.with(|c| c.get()) {
        return;
    }
    unsafe {
        if on {
            let mut wp = WINDOWPLACEMENT { length: size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
            let _ = GetWindowPlacement(host, &mut wp);
            let style = GetWindowLongPtrW(host, GWL_STYLE);
            WINDOWED.with(|c| c.set(Some((wp, style))));
            SetWindowLongPtrW(host, GWL_STYLE, style & !((WS_CAPTION.0 | WS_THICKFRAME.0) as isize));
            let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
            let _ = GetMonitorInfoW(MonitorFromWindow(host, MONITOR_DEFAULTTONEAREST), &mut mi);
            let r = mi.rcMonitor;
            let _ = SetWindowPos(host, Some(HWND_TOP), r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_FRAMECHANGED | SWP_NOOWNERZORDER);
        } else if let Some((wp, style)) = WINDOWED.with(|c| c.take()) {
            SetWindowLongPtrW(host, GWL_STYLE, style);
            let _ = SetWindowPlacement(host, &wp);
            let _ = SetWindowPos(host, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
        }
    }
    FULLSCREEN.with(|c| c.set(on));
}

/// Open `file` with a specific program (found through App Paths, e.g. `chrome.exe`).
/// The working directory is System32, so a planted `chrome.exe` in the current
/// directory is never picked up.
fn shell_open_with(host: HWND, exe: &str, file: &std::path::Path) {
    let (e, f) = (wide(exe), wide(&format!("\"{}\"", file.display())));
    let mut sys = [0u16; 260];
    let n = unsafe { windows::Win32::System::SystemInformation::GetSystemDirectoryW(Some(&mut sys)) } as usize;
    let dir = if n > 0 && n < sys.len() { PCWSTR(sys.as_ptr()) } else { PCWSTR::null() };
    unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(Some(host), w!("open"), PCWSTR(e.as_ptr()), PCWSTR(f.as_ptr()), dir, SW_SHOWNORMAL);
    }
}

/// Files that run code when "opened" (programs, scripts, shortcuts, installers).
fn runs_code(path: &std::path::Path) -> bool {
    const RUNNABLE: [&str; 28] = [
        "exe", "com", "scr", "pif", "bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "lnk", "msi", "msp", "reg", "cpl", "jar", "msc", "appx", "msix", "application", "gadget", "url", "dll",
    ];
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| RUNNABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// Right-to-left or left-to-right reading order for the editor.
fn set_text_direction(edit: HWND, rtl: bool) {
    unsafe {
        let ex = GetWindowLongPtrW(edit, GWL_EXSTYLE);
        let bits = (WS_EX_RIGHT.0 | WS_EX_RTLREADING.0 | WS_EX_LEFTSCROLLBAR.0) as isize;
        SetWindowLongPtrW(edit, GWL_EXSTYLE, if rtl { ex | bits } else { ex & !bits });
        let _ = SetWindowPos(edit, None, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED);
        // Scintilla's own bidirectional mode: 1 = right-to-left, 0 = off.
        SendMessageW(edit, 2709, Some(WPARAM(rtl as usize)), None);
    }
}

fn file_time_text(t: std::time::SystemTime) -> String {
    use windows::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) else { return String::new() };
    let ticks = (d.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(d.subsec_nanos()) / 100;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let (mut utc, mut local) = (SYSTEMTIME::default(), SYSTEMTIME::default());
    unsafe {
        if FileTimeToSystemTime(&ft, &mut utc).is_err() || SystemTimeToTzSpecificLocalTime(None, &utc, &mut local).is_err() {
            return String::new();
        }
    }
    let dt = edit::DateTime {
        year: local.wYear as i32,
        month: local.wMonth as u32,
        day: local.wDay as u32,
        weekday: local.wDayOfWeek as u32,
        hour: local.wHour as u32,
        minute: local.wMinute as u32,
        second: local.wSecond as u32,
    };
    edit::format_date_time("yyyy-MM-dd HH:mm:ss", &dt)
}

/// View > Summary: file facts and text statistics in a message box.
fn show_summary(host: HWND, ws: &Workspace<Win32Sci>) {
    let sci = ws.sci;
    let c = view::count_text(&sci::get_text(&sci));
    let t = &ws.tabs[ws.active];
    let mut s = String::new();
    match &t.path {
        Some(p) => {
            s += &format!("Full file path: {}\n", p.display());
            if let Ok(md) = std::fs::metadata(p) {
                if let Ok(created) = md.created() {
                    s += &format!("Created: {}\n", file_time_text(created));
                }
                if let Ok(modified) = md.modified() {
                    s += &format!("Modified: {}\n", file_time_text(modified));
                }
                s += &format!("File size on disk: {} bytes\n", md.len());
            }
        }
        None => s += &format!("{} (not saved yet)\n", t.title),
    }
    s += &format!(
        "\nCharacters (without line endings): {}\nWords: {}\nLines: {}\nDocument length: {} bytes\nEncoding: {}\n",
        c.chars,
        c.words,
        c.lines,
        c.bytes,
        t.enc.label()
    );
    let sel = search::selected_text(&sci);
    if !sel.is_empty() {
        let v = view::count_text(&sel);
        s += &format!("\nSelection: {} characters, {} words, {} lines", v.chars, v.words, v.lines);
    }
    dialogs::message(host, "Summary", &s, dialogs::Tone::Info, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
}

/// Ask for the picture used by Insert > Date Time (customized).
fn ask_date_format(host: HWND, current: &str) -> Option<String> {
    use form::{Kind, ctl};
    const FMT: usize = 101;
    let spec = form::Spec {
        title: "Date Time (customized)",
        size: (420, 140),
        ctls: vec![
            ctl(100, Kind::Label, "Format: yyyy MM dd HH mm ss tt dddd MMMM  ('quoted text' is kept as is)", 14, 12, 392, 34),
            ctl(FMT, Kind::Edit, current, 14, 52, 392, 24),
        ],
        ok: "Insert",
        on_change: None,
        on_apply: None,
    };
    let v = form::run(host, spec, |_| {})?;
    let f = v.text(FMT).trim().to_string();
    (!f.is_empty()).then_some(f)
}

/// The Column Editor: insert the same text, or a counting sequence, on every selected line.
fn column_editor(host: HWND, ws: &mut Workspace<Win32Sci>) -> Option<String> {
    use form::{Kind, ctl};
    const TEXT_R: usize = 100;
    const TEXT: usize = 101;
    const NUM_R: usize = 102;
    const INIT: usize = 104;
    const STEP: usize = 106;
    const REPEAT: usize = 108;
    const ZEROS: usize = 109;
    const DEC: usize = 111;
    const HEX: usize = 112;
    const OCT: usize = 113;
    const BIN: usize = 114;
    fn react(f: &form::Form, _id: usize) {
        let text = f.checked(TEXT_R);
        f.enable(TEXT, text);
        for id in [103, INIT, 105, STEP, 107, REPEAT, ZEROS, DEC, HEX, OCT, BIN] {
            f.enable(id, !text);
        }
    }
    let spec = form::Spec {
        title: "Column / Multi-Selection Editor",
        size: (380, 262),
        ctls: vec![
            ctl(TEXT_R, Kind::Radio(true), "&Text to Insert", 14, 12, 200, 20),
            ctl(TEXT, Kind::Edit, "", 30, 36, 330, 24),
            ctl(NUM_R, Kind::Radio(false), "&Number to Insert", 14, 72, 200, 20),
            ctl(103, Kind::Label, "Initial number:", 30, 98, 110, 20),
            ctl(INIT, Kind::Edit, "1", 150, 95, 100, 24),
            ctl(105, Kind::Label, "Increase by:", 30, 128, 110, 20),
            ctl(STEP, Kind::Edit, "1", 150, 125, 100, 24),
            ctl(107, Kind::Label, "Repeat:", 30, 158, 110, 20),
            ctl(REPEAT, Kind::Edit, "1", 150, 155, 100, 24),
            ctl(ZEROS, Kind::Check, "Leading &zeros", 30, 188, 200, 20),
            ctl(110, Kind::Group, "Format", 268, 86, 92, 122),
            ctl(DEC, Kind::Radio(true), "&Dec", 280, 106, 70, 20),
            ctl(HEX, Kind::Radio(false), "&Hex", 280, 130, 70, 20),
            ctl(OCT, Kind::Radio(false), "&Oct", 280, 154, 70, 20),
            ctl(BIN, Kind::Radio(false), "&Bin", 280, 178, 70, 20),
        ],
        ok: "OK",
        on_change: Some(react),
        on_apply: None,
    };
    let v = form::run(host, spec, |f| {
        f.set_checked(TEXT_R, true);
        f.set_checked(DEC, true);
        react(f, 0);
    })?;
    if v.checked(TEXT_R) {
        return ws.column_edit(Some(v.text(TEXT)), None);
    }
    let int = |id| v.text(id).trim().parse::<i64>().ok();
    let (Some(initial), Some(step), Some(repeat)) = (int(INIT), int(STEP), int(REPEAT)) else {
        return Some("The initial number, step and repeat must be whole numbers".into());
    };
    let format = if v.checked(HEX) {
        NumFormat::Hex
    } else if v.checked(OCT) {
        NumFormat::Oct
    } else if v.checked(BIN) {
        NumFormat::Bin
    } else {
        NumFormat::Dec
    };
    let numbers = ColumnNumbers { initial, step, repeat: repeat.max(1) as usize, format, leading_zeros: v.checked(ZEROS) };
    ws.column_edit(None, Some(&numbers))
}

fn popup_at_cursor(menu: HMENU, host: HWND) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Show above the status bar so the menu stays on screen.
        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), host, None);
    }
}

impl App {
    fn command(&mut self, id: usize) {
        if self.ws.is_whiteboard() {
            if !wb_allows(id) {
                self.flash = Some("Not available on a whiteboard".into());
                return self.refresh();
            }
            // Saving needs the page's latest scene first (its change events are debounced).
            if matches!(id, ID_SAVE | ID_SAVEAS | ID_SAVE_COPY) && self.wb_ready {
                self.wb_pending = Some(match id {
                    ID_SAVE => WbPending::Save,
                    ID_SAVEAS => WbPending::SaveAs,
                    _ => WbPending::SaveCopy,
                });
                return wb::request_data();
            }
        }
        if matches!(id, ID_NEXT_RESULT | ID_PREV_RESULT) {
            return self.step_result(id == ID_NEXT_RESULT);
        }
        let App { ws, ui, .. } = self;
        let sci = ws.sci;
        match id {
            ID_NEW_WB => {
                ws.new_whiteboard();
            }
            ID_NEW => {
                ws.new_tab();
            }
            ID_OPEN => ws.open_dialog(ui),
            ID_SAVE => {
                ws.save(ui);
            }
            ID_SAVEAS => {
                ws.save_as(ui);
            }
            ID_SAVEALL => ws.save_all(ui),
            ID_CLOSE => {
                let a = ws.active;
                ws.close(ui, a);
            }
            ID_CLOSE_ALL => drop(ws.close_many(ui, CloseOp::All)),
            ID_CLOSE_OTHERS => drop(ws.close_many(ui, CloseOp::AllButActive)),
            ID_CLOSE_LEFT => drop(ws.close_many(ui, CloseOp::ToTheLeft)),
            ID_CLOSE_RIGHT => drop(ws.close_many(ui, CloseOp::ToTheRight)),
            ID_CLOSE_UNCHANGED => drop(ws.close_many(ui, CloseOp::Unchanged)),
            ID_SAVE_COPY => {
                if let Some(p) = ws.save_copy_as(ui) {
                    self.flash = Some(format!("Saved a copy to {}", p.display()));
                }
            }
            ID_RENAME => {
                if ws.rename(ui) {
                    self.flash = Some("Renamed".into());
                }
            }
            ID_RELOAD => {
                if ws.reload_from_disk(ui) {
                    self.wb_shown = None; // a whiteboard must push the reloaded scene to its page
                    self.flash = Some("Reloaded from disk".into());
                }
            }
            ID_RECYCLE => drop(ws.move_to_recycle_bin(ui)),
            ID_OPEN_FOLDER_EXPLORER | ID_OPEN_FOLDER_CMD | ID_OPEN_FOLDER_WS | ID_OPEN_VIEWER => match ws.tabs[ws.active].path.clone() {
                None => self.flash = Some("Save the file first".into()),
                Some(p) => match id {
                    ID_OPEN_FOLDER_EXPLORER => explorer_select(&p),
                    ID_OPEN_FOLDER_CMD => open_command_prompt(p.parent()),
                    ID_OPEN_FOLDER_WS => {
                        if let Some(d) = p.parent() {
                            open_workspace(ws, d.to_path_buf());
                        }
                    }
                    _ if runs_code(&p) && !ui.ask_yes_no(&format!("{} is a program or script. Opening it will run it.\n\nRun it?", p.display())) => {}
                    _ => shell_open(ui.host, &p),
                },
            },
            ID_OPEN_WORKSPACE => {
                if let Some(d) = findpanel::browse_folder(ui.host) {
                    open_workspace(ws, d);
                }
            }
            ID_WORKSPACE_TOGGLE => {
                if ws.workspace.is_some() {
                    TREE_ON.with(|c| c.set(!c.get()));
                    layout();
                } else {
                    // No folder yet: pick one, which opens it in the tree.
                    if let Some(d) = findpanel::browse_folder(ui.host) {
                        open_workspace(ws, d);
                    }
                }
            }
            ID_LOAD_SESSION => {
                if let Some(p) = ui.pick_session(false) {
                    match Session::load(&p) {
                        Some(s) => {
                            let n = ws.open_session(&s);
                            self.flash = Some(format!("Opened {n} tab(s) from the session"));
                        }
                        None => ui.error("That file is not a valid Note++ session."),
                    }
                }
            }
            ID_SAVE_SESSION => {
                if let Some(p) = ui.pick_session(true) {
                    match ws.snapshot().save(&p) {
                        Ok(()) => self.flash = Some(format!("Session saved to {}", p.display())),
                        Err(e) => ui.error(&format!("Cannot save the session\n\n{e}")),
                    }
                }
            }
            ID_PRINT | ID_PRINT_NOW => {
                let title = ws.tabs[ws.active].title.clone();
                match print::print(ui.host, &sci, &title, id == ID_PRINT) {
                    Ok(print::Outcome::Printed { pages }) => self.flash = Some(format!("Printed {pages} page(s)")),
                    Ok(print::Outcome::Cancelled) => {}
                    Err(e) => ui.error(&e),
                }
            }
            ID_RESTORE_CLOSED => {
                if !ws.restore_closed(ui) {
                    self.flash = Some("No recently closed files".into());
                }
            }
            ID_OPEN_ALL_RECENT => ws.open_all_recent(ui),
            ID_EXIT => unsafe {
                let _ = PostMessageW(Some(ui.host), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            ID_UNDO => drop(sci.send(sci::SCI_UNDO, 0, 0)),
            ID_REDO => drop(sci.send(sci::SCI_REDO, 0, 0)),
            ID_CUT => drop(sci.send(sci::SCI_CUT, 0, 0)),
            ID_COPY => drop(sci.send(sci::SCI_COPY, 0, 0)),
            ID_PASTE => drop(sci.send(sci::SCI_PASTE, 0, 0)),
            ID_SELALL => drop(sci.send(sci::SCI_SELECTALL, 0, 0)),
            ID_EOL_CRLF => ws.set_eol(Eol::Crlf),
            ID_EOL_LF => ws.set_eol(Eol::Lf),
            ID_EOL_CR => ws.set_eol(Eol::Cr),
            ID_WRAP => {
                let on = !ws.wrap;
                ws.set_wrap(on);
            }
            ID_THEME => {
                let next = if ws.theme.dark { Theme::light() } else { Theme::dark() };
                ws.set_theme(next);
            }
            ID_MINIMAP => {
                ws.minimap = !ws.minimap;
                MINI_ON.with(|c| c.set(ws.minimap));
                layout();
            }
            ID_PREVIEW => {
                let on = !PREVIEW_ON.with(|c| c.get());
                self.set_preview(on);
                self.remember_md_preview(on);
            }
            _ if md_fmt(id).is_some() => {
                self.flash = ws.md_format(md_fmt(id).unwrap());
            }
            ID_MD_TOOLBAR => {
                let mut s = ws.settings.clone();
                s.md_toolbar = !s.md_toolbar;
                apply_settings(ws, s);
            }
            ID_ZOOMIN => drop(sci.send(sci::SCI_ZOOMIN, 0, 0)),
            ID_ZOOMOUT => drop(sci.send(sci::SCI_ZOOMOUT, 0, 0)),
            ID_ZOOMRESET => drop(sci.send(sci::SCI_SETZOOM, 0, 0)),
            ID_L_DUP => drop(sci.send(sci::SCI_LINEDUPLICATE, 0, 0)),
            ID_L_DEL => drop(sci.send(sci::SCI_LINEDELETE, 0, 0)),
            ID_L_UP => drop(sci.send(sci::SCI_MOVESELECTEDLINESUP, 0, 0)),
            ID_L_DOWN => drop(sci.send(sci::SCI_MOVESELECTEDLINESDOWN, 0, 0)),
            ID_C_UPPER => drop(sci.send(sci::SCI_UPPERCASE, 0, 0)),
            ID_C_LOWER => drop(sci.send(sci::SCI_LOWERCASE, 0, 0)),
            ID_L_SORT_ASC => self.flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, false)),
            ID_L_SORT_DESC => self.flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, true)),
            ID_L_SORT_NOCASE_ASC => self.flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, false)),
            ID_L_SORT_NOCASE_DESC => self.flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, true)),
            ID_L_SORT_INT_ASC => self.flash = ws.line_op(LineOp::Sort(SortKind::Integer, false)),
            ID_L_SORT_INT_DESC => self.flash = ws.line_op(LineOp::Sort(SortKind::Integer, true)),
            ID_L_SORT_DOT_ASC => self.flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, false)),
            ID_L_SORT_DOT_DESC => self.flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, true)),
            ID_L_SORT_COMMA_ASC => self.flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, false)),
            ID_L_SORT_COMMA_DESC => self.flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, true)),
            ID_L_DEDUP => self.flash = ws.line_op(LineOp::RemoveDuplicates),
            ID_L_RM_CONSEC => self.flash = ws.line_op(LineOp::RemoveConsecutiveDuplicates),
            ID_L_NOEMPTY => self.flash = ws.line_op(LineOp::RemoveEmpty),
            ID_L_RM_BLANK => self.flash = ws.line_op(LineOp::RemoveBlank),
            ID_L_REVERSE => self.flash = ws.line_op(LineOp::Reverse),
            ID_L_RANDOM => self.flash = ws.line_op(LineOp::Randomize),
            ID_L_JOIN => self.flash = ws.line_op(LineOp::Join),
            ID_L_SPLIT => self.flash = ws.line_op(LineOp::Split),
            ID_L_TRIM => self.flash = ws.line_op(LineOp::TrimTrailing),
            ID_B_TRIM_LEAD => self.flash = ws.line_op(LineOp::TrimLeading),
            ID_B_TRIM_BOTH => self.flash = ws.line_op(LineOp::TrimBoth),
            ID_B_EOL_SPACE => self.flash = ws.line_op(LineOp::EolToSpace),
            ID_B_UNNECESSARY => self.flash = ws.line_op(LineOp::RemoveUnnecessaryBlanks),
            ID_B_TAB_SPACE => self.flash = ws.line_op(LineOp::TabsToSpaces),
            ID_B_SPACE_TAB_ALL => self.flash = ws.line_op(LineOp::SpacesToTabsAll),
            ID_B_SPACE_TAB_LEAD => self.flash = ws.line_op(LineOp::SpacesToTabsLeading),
            ID_COMMENT => self.flash = ws.line_op(LineOp::ToggleComment),
            ID_CMT_LINE => self.flash = ws.line_op(LineOp::Comment),
            ID_UNCMT_LINE => self.flash = ws.line_op(LineOp::Uncomment),
            ID_CMT_BLOCK => self.flash = ws.line_op(LineOp::BlockComment),
            ID_UNCMT_BLOCK => self.flash = ws.line_op(LineOp::BlockUncomment),
            ID_L_INS_ABOVE | ID_L_INS_BELOW => ws.insert_blank_line(id == ID_L_INS_ABOVE),
            ID_L_CUT => drop(sci.send(sci::SCI_LINECUT, 0, 0)),
            ID_L_COPY => drop(sci.send(sci::SCI_LINECOPY, 0, 0)),
            ID_L_TRANSPOSE => drop(sci.send(sci::SCI_LINETRANSPOSE, 0, 0)),
            ID_CASE_PROPER => self.flash = ws.case_op(CaseOp::Proper),
            ID_CASE_PROPER_BLEND => self.flash = ws.case_op(CaseOp::ProperBlend),
            ID_CASE_SENTENCE => self.flash = ws.case_op(CaseOp::Sentence),
            ID_CASE_SENTENCE_BLEND => self.flash = ws.case_op(CaseOp::SentenceBlend),
            ID_CASE_INVERT => self.flash = ws.case_op(CaseOp::Invert),
            ID_CASE_RANDOM => self.flash = ws.case_op(CaseOp::Random),
            ID_DELETE => drop(sci.send(sci::SCI_CLEAR, 0, 0)),
            ID_BEGIN_END_SEL => self.flash = ws.begin_end_select(false),
            ID_BEGIN_END_COL => self.flash = ws.begin_end_select(true),
            ID_INDENT_INC | ID_INDENT_DEC => ws.indent(id == ID_INDENT_INC),
            ID_AC_FUNC => self.flash = ws.complete(Completion::Functions),
            ID_AC_WORD => self.flash = ws.complete(Completion::Words),
            ID_AC_HINT => self.flash = ws.complete(Completion::Hint),
            ID_AC_PATH => self.flash = ws.complete(Completion::Path),
            ID_INS_DATE_SHORT | ID_INS_DATE_LONG | ID_INS_DATE_CUSTOM => {
                let text = if id == ID_INS_DATE_CUSTOM {
                    ask_date_format(ui.host, &ws.date_format).map(|f| {
                        ws.date_format = f.clone();
                        edit::format_date_time(&f, &local_time())
                    })
                } else {
                    Some(system_date_time(id == ID_INS_DATE_LONG))
                };
                if let Some(t) = text {
                    ws.insert_text(&t);
                }
            }
            ID_CP_FULLPATH | ID_CP_NAME | ID_CP_DIR | ID_CP_ALLNAMES | ID_CP_ALLPATHS => {
                let kind = match id {
                    ID_CP_FULLPATH => PathText::FullPath,
                    ID_CP_NAME => PathText::FileName,
                    ID_CP_DIR => PathText::Directory,
                    ID_CP_ALLNAMES => PathText::AllNames,
                    _ => PathText::AllPaths,
                };
                self.flash = Some(match ws.path_text(kind) {
                    Some(t) if clip::set_text(ui.host, &t) => "Copied to the clipboard".into(),
                    Some(_) => "The clipboard is busy".into(),
                    None => "Save the file first".into(),
                });
            }
            ID_PS_HTML | ID_PS_RTF => {
                let (format, what) = if id == ID_PS_HTML { ("HTML Format", "HTML") } else { ("Rich Text Format", "RTF") };
                match clip::get_bytes(ui.host, format) {
                    Some(b) if id == ID_PS_HTML => ws.insert_text(&clip::html_fragment(&b)),
                    Some(b) => ws.insert_text(String::from_utf8_lossy(&b).trim_end_matches('\0')),
                    None => self.flash = Some(format!("No {what} content on the clipboard")),
                }
            }
            ID_PS_COPY_BIN | ID_PS_CUT_BIN => {
                let text = search::selected_text(&sci);
                if text.is_empty() {
                    self.flash = Some("Select some text first".into());
                } else if clip::set_binary(ui.host, BINARY_FORMAT, text.as_bytes()) {
                    if id == ID_PS_CUT_BIN {
                        sci.send(sci::SCI_CLEAR, 0, 0);
                    }
                    self.flash = Some(format!("{} byte(s) on the clipboard", text.len()));
                }
            }
            ID_PS_PASTE_BIN => match clip::get_bytes(ui.host, BINARY_FORMAT) {
                Some(b) => ws.insert_bytes(&b),
                None => self.flash = Some("No binary content on the clipboard".into()),
            },
            ID_SEL_OPEN | ID_SEL_FOLDER => {
                let sel = search::selected_text(&sci);
                let tab = &ws.tabs[ws.active];
                let found = selected_path(&sel, &[tab.path.as_deref().and_then(|p| p.parent()), ws.workspace.as_deref()]);
                match found {
                    None => self.flash = Some("No file or folder with that name".into()),
                    Some(p) if id == ID_SEL_FOLDER => explorer_select(&p),
                    Some(p) if p.is_file() => ws.open_path(ui, &p),
                    Some(p) => open_workspace(ws, p),
                }
            }
            ID_SEL_SEARCH => {
                let sel = search::selected_text(&sci);
                if sel.trim().is_empty() {
                    self.flash = Some("Select some text first".into());
                } else {
                    shell_open(ui.host, std::path::Path::new(&edit::search_url(ws.search_engine, &sel)));
                }
            }
            _ if (ID_ENGINE_BASE..ID_ENGINE_BASE + edit::SEARCH_ENGINES.len()).contains(&id) => {
                ws.search_engine = id - ID_ENGINE_BASE;
                self.flash = Some(format!("Search engine: {}", edit::SEARCH_ENGINES[ws.search_engine].0));
            }
            ID_COLUMN_MODE => {
                let t = "Column Mode selects a rectangle of text.\n\n\
                     \u{2022} Hold Alt and drag with the mouse, or\n\
                     \u{2022} press Alt+Shift with the arrow keys, or\n\
                     \u{2022} press Alt+Shift+B at one corner, move, and press it again.\n\n\
                     Then type to edit every line at once, or use Column Editor (Alt+C) to insert text or numbers.";
                dialogs::message(ui.host, "Column Mode", t, dialogs::Tone::Info, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
            }
            ID_COLUMN_EDITOR => self.flash = column_editor(ui.host, ws),
            ID_CHAR_PANEL | ID_CLIP_HISTORY | ID_DOC_LIST | ID_FUNC_LIST => {
                let want = match id {
                    ID_CHAR_PANEL => sidepanel::Mode::Characters,
                    ID_CLIP_HISTORY => sidepanel::Mode::Clipboard,
                    ID_DOC_LIST => sidepanel::Mode::Documents,
                    _ => sidepanel::Mode::Functions,
                };
                if sidepanel::mode() == Some(want) {
                    sidepanel::show_mode(None);
                } else {
                    if want == sidepanel::Mode::Clipboard {
                        if let Some(t) = clip::get_text(ui.host) {
                            sidepanel::record_clipboard(&t);
                        }
                    }
                    self.side_key = None;
                    sidepanel::show_mode(Some(want));
                }
                layout();
            }
            ID_ON_TOP => unsafe {
                let on = GetWindowLongPtrW(ui.host, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize == 0;
                let _ = SetWindowPos(ui.host, Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            },
            ID_FULLSCREEN => set_fullscreen(ui.host, !FULLSCREEN.with(|c| c.get())),
            ID_POSTIT | ID_DISTRACTION => {
                if id == ID_POSTIT {
                    POSTIT.with(|c| c.set(!c.get()));
                    DISTRACTION.with(|c| c.set(false));
                } else {
                    DISTRACTION.with(|c| c.set(!c.get()));
                    POSTIT.with(|c| c.set(false));
                }
                let (postit, distraction) = (POSTIT.with(|c| c.get()), DISTRACTION.with(|c| c.get()));
                HIDE.with(|c| c.set(if postit { 0b1111 } else if distraction { 0b1110 } else { 0 }));
                ws.set_margins_visible(!distraction);
                // Distraction free mode also goes full screen, as in Notepad++.
                set_fullscreen(ui.host, distraction);
                layout();
            }
            _ if (ID_BROWSER_BASE..ID_BROWSER_BASE + BROWSERS.len()).contains(&id) => match ws.tabs[ws.active].path.clone() {
                Some(p) => shell_open_with(ui.host, BROWSERS[id - ID_BROWSER_BASE].1, &p),
                None => self.flash = Some("Save the file first".into()),
            },
            ID_SYM_WS | ID_SYM_EOL | ID_SYM_NONPRINT | ID_SYM_CTRL | ID_SYM_ALL | ID_SYM_INDENT | ID_SYM_WRAP => {
                let mut v = ws.symbols;
                match id {
                    ID_SYM_WS => v.whitespace = !v.whitespace,
                    ID_SYM_EOL => v.eol = !v.eol,
                    ID_SYM_NONPRINT => {
                        let on = !v.non_printing();
                        v.whitespace = on;
                        v.eol = on;
                    }
                    ID_SYM_CTRL => v.control = !v.control,
                    ID_SYM_ALL => {
                        let on = !v.all();
                        v.whitespace = on;
                        v.eol = on;
                        v.control = on;
                    }
                    ID_SYM_INDENT => v.indent_guides = !v.indent_guides,
                    _ => v.wrap_symbol = !v.wrap_symbol,
                }
                ws.set_symbols(v);
            }
            ID_OPEN_NEW_INSTANCE | ID_MOVE_NEW_INSTANCE => match ws.tabs[ws.active].path.clone() {
                None => self.flash = Some("Save the file first".into()),
                Some(_) if id == ID_MOVE_NEW_INSTANCE && ws.is_dirty(ws.active) => self.flash = Some("Save the file before moving it".into()),
                Some(p) => {
                    let started = std::env::current_exe().and_then(|exe| std::process::Command::new(exe).arg("--new-instance").arg(&p).spawn());
                    match started {
                        Ok(_) if id == ID_MOVE_NEW_INSTANCE => {
                            let a = ws.active;
                            ws.close(ui, a);
                        }
                        Ok(_) => {}
                        Err(e) => ui.error(&format!("Cannot start a new instance\n\n{e}")),
                    }
                }
            },
            _ if (ID_TAB_BASE..ID_TAB_BASE + 9).contains(&id) => {
                let i = id - ID_TAB_BASE;
                if i < ws.tabs.len() {
                    ws.activate(i);
                }
            }
            ID_MOVE_TAB_FWD | ID_MOVE_TAB_BACK => {
                ws.move_tab(if id == ID_MOVE_TAB_FWD { 1 } else { -1 });
            }
            ID_HIDE_LINES => match view::hide_selected_lines(&sci) {
                Ok(n) => self.flash = Some(format!("{n} line(s) hidden: click the + in the margin to show them")),
                Err(e) => self.flash = Some(e),
            },
            ID_FOLD_ALL | ID_UNFOLD_ALL => view::fold_all(&sci, id == ID_UNFOLD_ALL),
            ID_FOLD_CUR | ID_UNFOLD_CUR => {
                if !view::fold_current_level(&sci, id == ID_UNFOLD_CUR) {
                    self.flash = Some("No fold block at the caret".into());
                }
            }
            _ if (ID_FOLD_LEVEL_BASE..ID_FOLD_LEVEL_BASE + 8).contains(&id) => drop(view::fold_level(&sci, id - ID_FOLD_LEVEL_BASE + 1, false)),
            _ if (ID_UNFOLD_LEVEL_BASE..ID_UNFOLD_LEVEL_BASE + 8).contains(&id) => drop(view::fold_level(&sci, id - ID_UNFOLD_LEVEL_BASE + 1, true)),
            ID_SUMMARY => show_summary(ui.host, ws),
            ID_RTL | ID_LTR => set_text_direction(sci.0, id == ID_RTL),
            ID_MONITOR => {
                let (tab_id, path) = (ws.tabs[ws.active].id, ws.tabs[ws.active].path.clone());
                match (path, self.monitoring.contains_key(&tab_id)) {
                    (None, _) => self.flash = Some("Save the file first".into()),
                    (Some(_), true) => {
                        self.monitoring.remove(&tab_id);
                        ws.set_read_only(false);
                    }
                    (Some(p), false) => {
                        let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                        self.monitoring.insert(tab_id, len);
                        ws.set_read_only(true);
                        unsafe {
                            SetTimer(Some(ui.host), TIMER_MONITOR, 700, None);
                        }
                        self.flash = Some("Monitoring: new lines are shown as the file grows".into());
                    }
                }
            }
            ID_READONLY => {
                let on = !ws.is_read_only();
                ws.set_read_only(on);
                self.flash = Some(if on { "Read-only" } else { "Editable" }.into());
            }
            ID_CLEAR_RO => {
                if let Some(p) = ws.tabs[ws.active].path.clone() {
                    let cleared = std::fs::metadata(&p).map(|m| {
                        let mut perm = m.permissions();
                        perm.set_readonly(false);
                        std::fs::set_permissions(&p, perm)
                    });
                    self.flash = Some(match cleared {
                        Ok(Ok(())) => "Read-only flag cleared".into(),
                        _ => "Could not clear the read-only flag".into(),
                    });
                }
            }
            ID_BM_TOGGLE => ws.toggle_bookmark(),
            ID_BM_NEXT | ID_BM_PREV => {
                if !ws.goto_bookmark(id == ID_BM_NEXT) {
                    self.flash = Some("No bookmarks".into());
                }
            }
            ID_BM_CLEAR => ws.clear_bookmarks(),
            ID_RECENT_CLEAR => ws.recent.clear(),
            _ if (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id) => {
                if let Some(p) = ws.recent.get(id - ID_RECENT_BASE).cloned() {
                    if p.exists() {
                        ws.open_path(ui, &p);
                    } else {
                        ws.prune_recent();
                        self.flash = Some(format!("{} no longer exists", p.display()));
                    }
                }
            }
            ID_NEXT_TAB => ws.next_tab(1),
            ID_PREV_TAB => ws.next_tab(-1),
            ID_ABOUT => {
                let build = if cfg!(debug_assertions) { "debug" } else { "release" };
                let t = format!(
                    "Note++ (native)\n\
                     Version {} ({build} build)\n\n\
                     A developer-focused text editor in Rust, built on the Scintilla editing component \
                     and Lexilla lexers. It is the native counterpart of the Note++ Electron app (v1.9.0).\n\n\
                     Author: Yogesh Prajapati\n\
                     https://github.com/YogeshPraj/Note-\n\n\
                     Components:\n\
                     \u{2022} Scintilla 5.5.2 and Lexilla 5.5.4 \u{2014} Neil Hodgson (HPND license)\n\
                     \u{2022} fancy-regex \u{2014} regular-expression search\n\
                     \u{2022} resvg \u{2014} toolbar icon rendering\n\
                     \u{2022} windows-rs \u{2014} Win32 bindings",
                    env!("CARGO_PKG_VERSION")
                );
                dialogs::message(ui.host, "About Note++", &t, dialogs::Tone::Info, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
            }
            // The panel and dialogs take focus themselves, so skip the refocus below.
            ID_PALETTE => {
                palette::open(ui.host, Kind::Commands, command_items());
                return;
            }
            ID_QUICKOPEN => {
                // Open tabs, then recent files, then everything under the project root.
                let mut files: Vec<PathBuf> = Vec::new();
                let push = |p: &PathBuf, files: &mut Vec<PathBuf>| {
                    if !files.contains(p) {
                        files.push(p.clone());
                    }
                };
                for t in &ws.tabs {
                    if let Some(p) = &t.path {
                        push(p, &mut files);
                    }
                }
                for p in &ws.recent {
                    push(p, &mut files);
                }
                // The open workspace folder, if any, otherwise the active file's project.
                let start = ws.tabs[ws.active].path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
                let root = ws.workspace.clone().or_else(|| start.as_ref().map(|d| files::project_root(d)));
                if let Some(root) = &root {
                    for p in files::scan_files(root, 8000) {
                        push(&p, &mut files);
                    }
                }
                let items: Vec<Item> = files
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let dir = p.parent().map(|d| root.as_ref().and_then(|r| d.strip_prefix(r).ok()).unwrap_or(d));
                        Item {
                            label: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            hint: dir.map(|d| d.display().to_string()).unwrap_or_default(),
                            id: i,
                        }
                    })
                    .collect();
                self.quick_files = files;
                palette::open(ui.host, Kind::Files, items);
                return;
            }
            // The panel and dialogs take focus themselves, so skip the refocus below.
            ID_FIND | ID_REPLACE | ID_FIF | ID_MARK => {
                let mode = match id {
                    ID_FIND => Mode::Find,
                    ID_REPLACE => Mode::Replace,
                    ID_FIF => Mode::InFiles,
                    _ => Mode::Mark,
                };
                let dir = ws
                    .workspace
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .or_else(|| ws.tabs[ws.active].path.as_ref().and_then(|p| p.parent()).map(|p| p.display().to_string()))
                    .unwrap_or_default();
                findpanel::open(mode, &search::selected_text(&sci), &ws.search.find, &dir);
                self.refresh();
                return;
            }
            ID_RESULTS => {
                if findpanel::visible() {
                    findpanel::close();
                    layout();
                } else if self.hits.is_empty() {
                    self.flash = Some("No search results yet".into());
                } else {
                    let mode = if self.hits_root.as_os_str().is_empty() { Mode::Find } else { Mode::InFiles };
                    findpanel::open(mode, "", &ws.search.find, &self.hits_root.display().to_string());
                }
                self.refresh();
                return;
            }
            ID_INCR => {
                INCR_START.with(|c| c.set(sci.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize));
                incsearch::open(ui.host);
                return;
            }
            ID_SEL_FIND_NEXT | ID_SEL_FIND_PREV | ID_VOL_NEXT | ID_VOL_PREV => {
                let forward = matches!(id, ID_SEL_FIND_NEXT | ID_VOL_NEXT);
                search::select_word_at_caret(&sci);
                let token = search::selected_text(&sci);
                if token.is_empty() || token.contains(['\r', '\n']) {
                    self.flash = Some("Select a word to search for".into());
                } else {
                    let o = search::Opts { find: token, match_case: ws.search.match_case, wrap: true, ..Default::default() };
                    // "Volatile" searches leave the Find box and F3 untouched.
                    if matches!(id, ID_SEL_FIND_NEXT | ID_SEL_FIND_PREV) {
                        ws.search = o.clone();
                    }
                    if search::find(&sci, &o, forward) == search::Found::NotFound {
                        self.flash = Some("Not found".into());
                    }
                }
            }
            ID_BRACE_GOTO => {
                if !search::goto_matching_brace(&sci) {
                    self.flash = Some("No matching brace".into());
                }
            }
            ID_BRACE_SEL => {
                if !search::select_between_braces(&sci) {
                    self.flash = Some("No matching brace".into());
                }
            }
            _ if (ID_STYLE_ALL_BASE..ID_STYLE_ALL_BASE + 5).contains(&id) => {
                let token = search::token_at_caret(&sci);
                self.flash = Some(if token.is_empty() || token.contains(['\r', '\n']) {
                    "Select a word to style".into()
                } else {
                    format!("{} occurrence(s) styled", search::style_all(&sci, id - ID_STYLE_ALL_BASE, &token))
                });
            }
            _ if (ID_STYLE_ONE_BASE..ID_STYLE_ONE_BASE + 5).contains(&id) => {
                search::select_word_at_caret(&sci);
                if !search::style_selection(&sci, id - ID_STYLE_ONE_BASE) {
                    self.flash = Some("Select some text to style".into());
                }
            }
            _ if (ID_CLEAR_STYLE_BASE..ID_CLEAR_STYLE_BASE + 6).contains(&id) => {
                let slot = id - ID_CLEAR_STYLE_BASE;
                search::clear_style(&sci, (slot < 5).then_some(slot));
            }
            _ if (ID_JUMP_UP_BASE..ID_JUMP_UP_BASE + 5).contains(&id) || (ID_JUMP_DOWN_BASE..ID_JUMP_DOWN_BASE + 5).contains(&id) => {
                let down = id >= ID_JUMP_DOWN_BASE;
                let slot = id - if down { ID_JUMP_DOWN_BASE } else { ID_JUMP_UP_BASE };
                if !search::jump(&sci, slot, down) {
                    self.flash = Some(format!("Nothing is styled with the {} style", SLOT_NAMES[slot]));
                }
            }
            _ if (ID_COPY_STYLED_BASE..ID_COPY_STYLED_BASE + 6).contains(&id) => {
                let slot = id - ID_COPY_STYLED_BASE;
                let slots: Vec<usize> = if slot < 5 { vec![slot] } else { (0..5).collect() };
                let text = search::styled_text(&sci, &slots);
                self.flash = Some(if text.is_empty() {
                    "Nothing is styled".into()
                } else if clip::set_text(ui.host, &text) {
                    "Styled text copied".into()
                } else {
                    "The clipboard is busy".into()
                });
            }
            ID_BM_CUT | ID_BM_COPY => {
                let eol = match ws.tabs[ws.active].eol {
                    Eol::Crlf => "\r\n",
                    Eol::Lf => "\n",
                    Eol::Cr => "\r",
                };
                let text = bookmarks::marked_text(&sci, eol);
                if text.is_empty() && bookmarks::lines(&sci).is_empty() {
                    self.flash = Some("No bookmarks".into());
                } else if clip::set_text(ui.host, &text) {
                    if id == ID_BM_CUT {
                        bookmarks::delete_lines(&sci, true);
                    }
                    self.flash = Some("Bookmarked lines copied".into());
                }
            }
            ID_BM_PASTE => match clip::get_text(ui.host) {
                Some(t) => {
                    let t = t.trim_end_matches(['\r', '\n']).to_string();
                    self.flash = Some(format!("{} line(s) replaced", bookmarks::replace_lines(&sci, &t)));
                }
                None => self.flash = Some("Nothing to paste".into()),
            },
            ID_BM_DELETE | ID_BM_DELETE_UNMARKED => {
                self.flash = Some(format!("{} line(s) removed", bookmarks::delete_lines(&sci, id == ID_BM_DELETE)));
            }
            ID_BM_INVERT => bookmarks::invert(&sci),
            ID_PREFS => {
                if let Some(s) = dialogs::preferences(ui.host, &ws.settings) {
                    apply_settings(ws, s);
                }
            }
            ID_STYLE_CFG => {
                if let Some(e) = dialogs::style_configurator(ui.host, &ws.theme) {
                    let dark = ws.theme.dark;
                    let base = if dark { Theme::dark() } else { Theme::light() };
                    let diff = if e.reset { Default::default() } else { settings::diff_from(&base, &e.entered) };
                    let bad = e.entered.iter().filter(|(_, h)| settings::parse_hex(h).is_none()).count();
                    let mut s = ws.settings.clone();
                    if dark {
                        s.dark_colors = diff;
                    } else {
                        s.light_colors = diff;
                    }
                    apply_settings(ws, s);
                    if bad > 0 {
                        self.flash = Some(format!("{bad} colour(s) were not valid #RRGGBB and were ignored"));
                    }
                }
            }
            ID_SHORTCUTS => {
                if let Some(done) = dialogs::shortcut_mapper(ui.host, mapper_rows(&ws.settings)) {
                    let mut s = ws.settings.clone();
                    s.shortcuts.clear();
                    for r in done {
                        if r.current != r.default {
                            s.shortcuts.insert(r.id.to_string(), r.current);
                        }
                    }
                    let previous = std::mem::replace(&mut ws.settings, s.clone());
                    let _ = s.save();
                    install_shortcuts(&self.menus, &s, Some(&previous));
                    self.flash = Some("Shortcuts updated".into());
                }
            }
            ID_IMPORT_THEME => {
                if let Some(p) = ui.pick_open().into_iter().next() {
                    let imported = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| {
                        let mut s = ws.settings.clone();
                        import_theme(&mut s, &t).map(|n| (s, n))
                    });
                    match imported {
                        Ok((s, n)) => {
                            apply_settings(ws, s);
                            self.flash = Some(format!("Imported {n} colour(s)"));
                        }
                        Err(e) => ui.error(&format!("Cannot import the theme\n\n{e}")),
                    }
                }
            }
            ID_EXPORT_THEME => {
                if let Some(p) = ui.pick_save("note-theme.json") {
                    match std::fs::write(&p, theme_json(&ws.settings)) {
                        Ok(()) => self.flash = Some(format!("Theme saved to {}", p.display())),
                        Err(e) => ui.error(&format!("Cannot save the theme\n\n{e}")),
                    }
                }
            }
            ID_EDIT_CONTEXT => {
                let p = ensure_context_menu_file();
                ws.open_path(ui, &p);
            }
            _ if (ID_HASH_BASE..ID_HASH_BASE + ALGOS.len() * 10).contains(&id) => {
                let algo: Algo = ALGOS[(id - ID_HASH_BASE) / 10];
                match (id - ID_HASH_BASE) % 10 {
                    0 => dialogs::hash_text(ui.host, algo),
                    1 => {
                        let files = ui.pick_open();
                        if !files.is_empty() {
                            let lines: Vec<String> = files
                                .iter()
                                .map(|p| match std::fs::File::open(p).and_then(|f| algo.hash_reader(f)) {
                                    Ok(h) => format!("{h}  {}", p.display()),
                                    Err(e) => format!("(error)  {}: {e}", p.display()),
                                })
                                .collect();
                            dialogs::text_result(ui.host, &format!("{} of {} file(s)", algo.name(), files.len()), &lines.join("\r\n"));
                        }
                    }
                    _ => {
                        let sel = search::selected_text(&sci);
                        if sel.is_empty() {
                            self.flash = Some("Select some text first".into());
                        } else {
                            let digest = algo.hash_bytes(sel.as_bytes());
                            self.flash = Some(if clip::set_text(ui.host, &digest) { format!("{} copied: {digest}", algo.name()) } else { "The clipboard is busy".into() });
                        }
                    }
                }
            }
            ID_B64_ENC | ID_B64_DEC | ID_URL_ENC | ID_URL_DEC | ID_JSON_PRETTY | ID_JSON_MIN | ID_XML_PRETTY | ID_XML_MIN => {
                let op = match id {
                    ID_B64_ENC => TextTool::Base64Encode,
                    ID_B64_DEC => TextTool::Base64Decode,
                    ID_URL_ENC => TextTool::UrlEncode,
                    ID_URL_DEC => TextTool::UrlDecode,
                    ID_JSON_PRETTY => TextTool::JsonPretty,
                    ID_JSON_MIN => TextTool::JsonMinify,
                    ID_XML_PRETTY => TextTool::XmlPretty,
                    _ => TextTool::XmlMinify,
                };
                self.flash = ws.text_tool(op);
            }
            ID_WINDOWS => {
                let mut sel = ws.active;
                loop {
                    let rows = ws.window_rows();
                    match dialogs::windows_dialog(ui.host, &rows, sel.min(rows.len().saturating_sub(1))) {
                        None => break,
                        Some(dialogs::WinAction::Activate(i)) => {
                            ws.activate(i);
                            break;
                        }
                        Some(dialogs::WinAction::Save(rows)) => {
                            for &i in &rows {
                                // Saving goes through the active tab; whiteboards save from their own tab.
                                if i < ws.tabs.len() && ws.tabs[i].kind == crate::core::workspace::TabKind::Text && ws.is_dirty(i) {
                                    ws.activate(i);
                                    ws.save(ui);
                                }
                            }
                            sel = ws.active;
                        }
                        Some(dialogs::WinAction::Close(rows)) => {
                            // Highest first so the indices stay valid.
                            for &i in rows.iter().rev() {
                                if i < ws.tabs.len() && !ws.close(ui, i) {
                                    break;
                                }
                            }
                            sel = ws.active;
                        }
                        Some(dialogs::WinAction::Sort(field, descending)) => {
                            ws.sort_tabs(field, descending);
                            sel = ws.active;
                        }
                    }
                }
            }
            _ if (ID_WINDOW_SORT_BASE..ID_WINDOW_SORT_BASE + dialogs::SORTS.len()).contains(&id) => {
                let (_, field, descending) = dialogs::SORTS[id - ID_WINDOW_SORT_BASE];
                ws.sort_tabs(field, descending);
            }
            _ if (ID_WINDOW_DOC_BASE..ID_WINDOW_DOC_BASE + WINDOW_DOCS_SHOWN).contains(&id) => ws.activate(id - ID_WINDOW_DOC_BASE),
            ID_FIND_CHARS => {
                find_chars_dialog(ui.host, sci);
            }
            ID_GOTO => {
                findpanel::open_goto(ui.host, sci::line_count(&sci));
                return;
            }
            ID_FINDNEXT | ID_FINDPREV => {
                if ws.search.find.is_empty() {
                    findpanel::open(Mode::Find, &search::selected_text(&sci), "", "");
                    self.refresh();
                    return;
                }
                let o = ws.search.clone();
                search::find(&sci, &o, id == ID_FINDNEXT);
            }
            _ if (ID_ENCODE_BASE..ID_ENCODE_BASE + ENCODE_IN.len()).contains(&id) => {
                ws.reload_as(ui, ENCODE_IN[id - ID_ENCODE_BASE]);
            }
            _ if (ID_CHARSET_BASE..=ID_CHARSET_BASE + CHARSETS.len()).contains(&id) => {
                let i = id - ID_CHARSET_BASE;
                // The last entry is Windows-1252, which is the app's ANSI.
                ws.reload_as(ui, if i == CHARSETS.len() { Enc::Ansi } else { Enc::Cs(i as u8) });
            }
            _ if (ID_CONVERT_BASE..ID_CONVERT_BASE + ENCODE_IN.len()).contains(&id) => {
                self.flash = ws.convert_to(ui, ENCODE_IN[id - ID_CONVERT_BASE]);
            }
            _ if (ID_LANG_BASE..ID_LANG_BASE + LANGUAGES.len()).contains(&id) => ws.set_language(LANGUAGES[id - ID_LANG_BASE].0),
            _ => {}
        }
        self.refresh();
        self.focus_active();
    }

    /// Give keyboard focus to whichever surface the active tab uses.
    fn focus_active(&self) {
        if self.ws.is_whiteboard() && wb::is_created() {
            wb::focus();
        } else {
            unsafe {
                let _ = SetFocus(Some(self.ws.sci.0));
            }
        }
    }

    /// Events from the Excalidraw page: scene changes, readiness, save requests.
    fn wb_events(&mut self) {
        for ev in wb::take_events() {
            match ev {
                wb::Event::Ready => {
                    self.wb_ready = true;
                    self.wb_shown = None;
                    self.wb_dark = !self.ws.theme.dark; // force a theme push
                }
                wb::Event::State(s) => self.wb_apply_scene(s),
                wb::Event::Data(s) => {
                    self.wb_apply_scene(s);
                    self.wb_finish_pending();
                }
                wb::Event::SaveRequest => {
                    self.wb_pending = Some(WbPending::Save);
                    wb::request_data();
                }
            }
        }
        self.refresh();
    }

    /// Store a scene reported by the page into the active whiteboard tab.
    fn wb_apply_scene(&mut self, s: String) {
        let current = self.ws.is_whiteboard().then(|| self.ws.tabs[self.ws.active].id);
        // Ignore echoes for a scene that is no longer the one on screen.
        if current.is_some() && current == self.wb_shown {
            let t = &mut self.ws.tabs[self.ws.active];
            if t.content != s {
                // Pan/zoom and Excalidraw's own re-serialisation are not edits.
                let edited = !whiteboard::same_drawing(&t.content, &s);
                t.content = s;
                if edited {
                    t.dirty = true;
                }
            }
        }
    }

    /// Finish a Save / Save As that was waiting for the latest scene.
    fn wb_finish_pending(&mut self) {
        let Some(p) = self.wb_pending.take() else { return };
        let App { ws, ui, .. } = self;
        if !ws.is_whiteboard() {
            return;
        }
        match p {
            WbPending::Save => ws.save(ui),
            WbPending::SaveAs => ws.save_as(ui),
            WbPending::SaveCopy => ws.save_copy_as(ui).is_some(),
        };
        self.refresh();
    }

    /// Actions posted by the Find panel and the Go To dialog.
    fn panel_command(&mut self, id: usize, arg: isize) {
        let sci = self.ws.sci;
        let mode = findpanel::mode();
        let opts = findpanel::read_opts();
        if matches!(id, findpanel::P_NEXT | findpanel::P_PREV | findpanel::P_REPLACE | findpanel::P_REPLACE_ALL | findpanel::P_COUNT | findpanel::P_FINDALL | findpanel::P_FIF | findpanel::P_MARK) {
            self.ws.search = opts.clone();
        }
        let found = |f: search::Found| match f {
            search::Found::At { wrapped: true } => "Found (wrapped around the document)".to_string(),
            search::Found::At { wrapped: false } => String::new(),
            search::Found::NotFound => "Not found".to_string(),
            search::Found::BadPattern(e) => format!("Invalid regular expression: {e}"),
        };
        let outcome = |r: Result<usize, String>, what: &str| match r {
            Ok(n) => format!("{n} {what}"),
            Err(e) => format!("Invalid regular expression: {e}"),
        };
        let mut note: Option<String> = None;
        match id {
            findpanel::P_NEXT => match mode {
                Mode::InFiles => return self.panel_command(findpanel::P_FIF, 0),
                Mode::Mark => return self.panel_command(findpanel::P_MARK, 0),
                _ => note = Some(found(search::find(&sci, &opts, true))),
            },
            findpanel::P_PREV => note = Some(found(search::find(&sci, &opts, false))),
            findpanel::P_REPLACE => {
                let (replaced, f) = search::replace_current(&sci, &opts);
                let n = found(f);
                note = Some(if replaced && n.is_empty() { "Replaced".into() } else { n });
            }
            findpanel::P_REPLACE_ALL => note = Some(outcome(search::replace_all(&sci, &opts), "replaced")),
            findpanel::P_COUNT => note = Some(outcome(search::count(&sci, &opts), "matches")),
            findpanel::P_MARK => note = Some(outcome(search::mark_all(&sci, &opts, search::INDIC_FIND), "matches marked")),
            findpanel::P_CLEAR => {
                search::clear_marks(&sci, search::INDIC_FIND);
                note = Some("Marks cleared".into());
            }
            findpanel::P_FINDALL => match search::find_all(&sci, &opts) {
                Ok(mut hits) => {
                    let total = hits.len();
                    hits.truncate(search::MAX_LIST);
                    let lines: Vec<String> = hits.iter().map(|h| format!("Line {}:  {}", h.line, h.text)).collect();
                    note = Some(if total > hits.len() { format!("{total} hits (listing the first {})", hits.len()) } else { format!("{total} hits in the current document") });
                    self.hits = hits;
                    findpanel::set_results(&lines);
                }
                Err(e) => note = Some(format!("Invalid regular expression: {e}")),
            },
            findpanel::P_FIF => {
                let (dir, filters) = findpanel::read_dir_filter();
                if dir.as_os_str().is_empty() || !dir.is_dir() {
                    note = Some("Choose an existing directory first".into());
                } else {
                    match search::find_in_files(&dir, &filters, &opts) {
                        Ok(hits) => {
                            let files: std::collections::HashSet<_> = hits.iter().filter_map(|h| h.path.as_ref()).collect();
                            note = Some(format!("{} hits in {} files{}", hits.len(), files.len(), if hits.len() >= search::MAX_HITS { " (limit reached)" } else { "" }));
                            let lines: Vec<String> = hits
                                .iter()
                                .map(|h| {
                                    let p = h.path.as_ref().unwrap();
                                    let rel = p.strip_prefix(&dir).unwrap_or(p);
                                    format!("{} ({}):  {}", rel.display(), h.line, h.text)
                                })
                                .collect();
                            self.hits = hits;
                            self.hits_root = dir;
                            findpanel::set_results(&lines);
                        }
                        Err(e) => note = Some(format!("Invalid regular expression: {e}")),
                    }
                }
            }
            findpanel::P_BROWSE => {
                if let Some(d) = findpanel::browse_folder(self.ui.host) {
                    findpanel::set_dir(&d.display().to_string());
                }
            }
            findpanel::P_LIST_OPEN => {
                self.hit_cursor = arg.max(0) as usize;
                self.open_hit(self.hit_cursor);
            }
            findpanel::P_GOTO => {
                search::goto_line(&sci, arg.max(1) as usize);
                unsafe {
                    let _ = SetFocus(Some(sci.0));
                }
            }
            findpanel::P_CLOSE => {
                findpanel::close();
                layout();
                unsafe {
                    let _ = SetFocus(Some(sci.0));
                }
            }
            findpanel::P_RELAYOUT => layout(),
            _ => {}
        }
        if let Some(n) = note {
            findpanel::set_status(&n);
        }
        self.refresh();
    }

    /// Open result `idx` of the last search in the editor.
    fn open_hit(&mut self, idx: usize) {
        let sci = self.ws.sci;
        if let Some(h) = self.hits.get(idx).cloned() {
            let App { ws, ui, .. } = self;
            if let Some(p) = &h.path {
                ws.open_path(ui, p);
            }
            let a = ws.active;
            let sci = ws.sci;
            if ws.is_dirty(a) && h.path.is_some() {
                search::goto_line(&sci, h.line);
            } else {
                search::select_range(&sci, h.start, h.end);
            }
        }
        unsafe {
            let _ = SetFocus(Some(sci.0));
        }
    }

    /// F4 / Shift+F4: step through the results of the last Find All / Find in Files.
    fn step_result(&mut self, forward: bool) {
        let n = self.hits.len();
        if n == 0 {
            self.flash = Some("No search results yet".into());
        } else {
            self.hit_cursor = if forward { (self.hit_cursor + 1) % n } else { (self.hit_cursor + n - 1) % n };
            self.open_hit(self.hit_cursor);
            self.flash = Some(format!("Result {} of {n}", self.hit_cursor + 1));
        }
        self.refresh();
    }

    /// Incremental Search window events.
    fn incr_event(&mut self, kind: usize) {
        let sci = self.ws.sci;
        if kind == incsearch::K_CLOSE {
            incsearch::close();
            self.focus_active();
            return;
        }
        let text = incsearch::text();
        let start = INCR_START.with(|c| c.get());
        if text.is_empty() {
            sci.send(sci::SCI_SETSEL, start, start as isize);
            incsearch::set_status("");
            return;
        }
        let o = search::Opts { find: text, wrap: true, ..Default::default() };
        let forward = kind != incsearch::K_PREV;
        if kind == incsearch::K_CHANGED {
            // Typing re-searches from where the search began.
            sci.send(sci::SCI_SETSEL, start, start as isize);
        }
        let before = (sci.send(sci::SCI_GETSELECTIONSTART, 0, 0), sci.send(sci::SCI_GETSELECTIONEND, 0, 0));
        match search::find(&sci, &o, forward) {
            search::Found::At { wrapped: true } => incsearch::set_status("Reached the end, continued from the other end"),
            search::Found::At { wrapped: false } => incsearch::set_status(""),
            _ => {
                sci.send(sci::SCI_SETSEL, before.0 as usize, before.1);
                incsearch::set_status("Phrase not found");
            }
        }
    }

    /// Right-click in the editor: the menu listed in contextmenu.txt.
    fn show_editor_context_menu(&mut self, x: i32, y: i32) {
        let file_text = std::fs::read_to_string(ensure_context_menu_file()).unwrap_or_else(|_| DEFAULT_CONTEXT_MENU.to_string());
        let entries = context_entries(&file_text);
        if entries.is_empty() {
            return;
        }
        unsafe {
            let (mut px, mut py) = (x, y);
            if x == -1 && y == -1 {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                (px, py) = (pt.x, pt.y);
            }
            let Ok(menu) = CreatePopupMenu() else { return };
            for e in entries {
                match e {
                    None => {
                        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    }
                    Some((name, id, default)) => {
                        let label = name.split_once(": ").map_or(name, |(_, l)| l);
                        let key = self.ws.settings.shortcuts.get(&id.to_string()).map(String::as_str).unwrap_or(default);
                        let text = wide(&if key.is_empty() { label.to_string() } else { format!("{label}\t{key}") });
                        let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(text.as_ptr()));
                    }
                }
            }
            let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_TOPALIGN, px, py, Some(0), self.ui.host, None);
            let _ = DestroyMenu(menu);
        }
    }

    fn status_click(&mut self, seg: isize) {
        let menu = match seg {
            chrome::SEG_ENC => self.menus.enc,
            chrome::SEG_EOL => self.menus.eol,
            chrome::SEG_LANG => self.menus.lang,
            _ => return,
        };
        popup_at_cursor(menu, self.ui.host);
    }

    /// Show the whiteboard view while a whiteboard tab is active, and push the
    /// tab's scene (and the theme) into it when needed.
    fn wb_sync(&mut self, wb_active: bool) {
        let Some(h) = handles() else { return };
        if wb_active != WB_ON.with(|c| c.get()) {
            if wb_active {
                if let Err(e) = wb::ensure(h.host, &config_dir().join("webview-wb")) {
                    self.flash = Some(format!("Whiteboard unavailable: {e}"));
                }
            }
            WB_ON.with(|c| c.set(wb_active));
            wb::set_visible(wb_active);
            layout();
            if wb_active {
                self.wb_shown = None;
            }
        }
        if wb_active && self.wb_ready {
            let dark = self.ws.theme.dark;
            if self.wb_dark != dark {
                self.wb_dark = dark;
                wb::set_theme(dark);
            }
            let t = &self.ws.tabs[self.ws.active];
            if self.wb_shown != Some(t.id) {
                self.wb_shown = Some(t.id);
                wb::load(&t.content);
                if wb::is_created() {
                    wb::focus();
                }
            }
        }
    }

    /// Bring chrome, title, menu check marks and minimap in line with the workspace.
    fn refresh(&mut self) {
        let Some(h) = handles() else { return };
        let wb_active = self.ws.is_whiteboard();
        if !wb_active {
            self.ws.update_smart();
            self.ws.sync_minimap();
        }
        self.wb_sync(wb_active);
        chrome::set_tabs(&h.chrome, self.ws.tab_infos(), self.ws.active);
        let s = self.ws.status();
        let position = self.flash.take().unwrap_or(s.position);
        chrome::set_status(
            &h.chrome,
            StatusText { position, lines: s.lines, length: s.length, enc: s.enc, eol: s.eol, lang: s.lang, ins: "INS".into() },
        );
        self.sync_file_menu();
        self.refresh_side();
        chrome::set_checked(&h.chrome, ID_WRAP, self.ws.wrap);
        chrome::set_checked(&h.chrome, ID_THEME, self.ws.theme.dark);
        chrome::set_theme(&h.chrome, self.ws.theme.dark, self.ws.theme.bg, self.ws.theme.fg);
        findpanel::retheme();
        thread_local! {
            static NATIVE_DARK: Cell<Option<bool>> = const { Cell::new(None) };
        }
        if NATIVE_DARK.with(|c| c.replace(Some(self.ws.theme.dark))) != Some(self.ws.theme.dark) {
            mdbar::retheme();
            sidepanel::restyle();
            tree::restyle();
        }

        // Mermaid files open their live preview automatically; the preview follows the theme.
        // Markdown files follow the user's last choice (preview on by default).
        let doc = self.ws.tabs[self.ws.active].doc;
        if doc != self.preview_doc {
            self.preview_doc = doc;
            let on = PREVIEW_ON.with(|c| c.get());
            if self.ws.is_markdown() {
                if self.ws.settings.md_preview != on {
                    self.set_preview(self.ws.settings.md_preview);
                }
            } else if !on && self.ws.preview_kind() == Some(pcore::Kind::Mermaid) {
                self.set_preview(true);
            }
        }
        let preview_on = PREVIEW_ON.with(|c| c.get());
        if preview_on {
            if self.preview_dark != self.ws.theme.dark {
                self.preview_dark = self.ws.theme.dark;
                pview::set_theme(self.preview_dark);
                self.preview_key = None;
            }
            // Debounce: re-render shortly after the last change.
            unsafe {
                SetTimer(Some(h.host), TIMER_PREVIEW, 350, None);
            }
        }
        chrome::set_checked(&h.chrome, ID_PREVIEW, preview_on);

        // The Markdown formatting bar follows the active tab; its toggles mirror the app state.
        let want_bar = self.ws.is_markdown() && self.ws.settings.md_toolbar && !DISTRACTION.with(|c| c.get());
        if MD_BAR_ON.with(|c| c.replace(want_bar)) != want_bar {
            layout();
        }
        self.sync_scroll_to_preview();
        let title = wide(&self.ws.window_title());
        unsafe {
            let _ = SetWindowTextW(h.host, PCWSTR(title.as_ptr()));
            let on = |b: bool| if b { MF_CHECKED } else { MF_UNCHECKED };
            let m = self.menus;
            CheckMenuItem(m.view, ID_WRAP as u32, (MF_BYCOMMAND | on(self.ws.wrap)).0);
            CheckMenuItem(m.view, ID_MINIMAP as u32, (MF_BYCOMMAND | on(self.ws.minimap)).0);
            CheckMenuItem(m.view, ID_THEME as u32, (MF_BYCOMMAND | on(self.ws.theme.dark)).0);
            CheckMenuItem(m.view, ID_PREVIEW as u32, (MF_BYCOMMAND | on(preview_on)).0);
            CheckMenuItem(m.view, ID_MD_TOOLBAR as u32, (MF_BYCOMMAND | on(self.ws.settings.md_toolbar)).0);

            let t = &self.ws.tabs[self.ws.active];
            let eol = match t.eol {
                Eol::Crlf => ID_EOL_CRLF,
                Eol::Lf => ID_EOL_LF,
                Eol::Cr => ID_EOL_CR,
            };
            let _ = CheckMenuRadioItem(m.eol, ID_EOL_CRLF as u32, ID_EOL_CR as u32, eol as u32, MF_BYCOMMAND.0);
        }
    }

    /// Remember whether the user wants Markdown files previewed (only while a Markdown tab is active).
    fn remember_md_preview(&mut self, on: bool) {
        if self.ws.is_markdown() && self.ws.settings.md_preview != on {
            let mut s = self.ws.settings.clone();
            s.md_preview = on;
            apply_settings(&mut self.ws, s);
        }
    }

    /// Show or hide the live-preview pane.
    fn set_preview(&mut self, on: bool) {
        let Some(h) = handles() else { return };
        if on {
            if self.ws.preview_kind().is_none() {
                self.flash = Some("Preview works for Markdown, HTML and Mermaid files".into());
                return;
            }
            if let Err(e) = pview::ensure(h.host, &config_dir().join("webview"), self.ws.settings.preview_remote_images) {
                self.flash = Some(format!("Preview unavailable (WebView2): {e}"));
                return;
            }
            pview::set_theme(self.ws.theme.dark);
            self.preview_dark = self.ws.theme.dark;
        }
        PREVIEW_ON.with(|c| c.set(on));
        if !on {
            PREVIEW_MAX.with(|c| c.set(false));
        }
        pview::set_visible(on);
        self.preview_key = None;
        layout();
        if on {
            pview::set_max(false);
            self.preview_update(true);
        }
    }

    /// Re-render the preview if the active document changed (or `force`).
    fn preview_update(&mut self, force: bool) {
        if !PREVIEW_ON.with(|c| c.get()) {
            return;
        }
        let key = self.ws.preview_key();
        if !force && self.preview_key == Some(key) {
            return;
        }
        self.preview_key = Some(key);
        let Some(kind) = self.ws.preview_kind() else {
            return pview::render("message", "Preview is available for Markdown, HTML and Mermaid files.");
        };
        let text = sci::get_text(&self.ws.sci);
        match kind {
            pcore::Kind::Markdown if pcore::is_bare_mermaid(&text) => pview::render("mermaid", &text),
            pcore::Kind::Markdown => {
                pview::render("markdown", &pcore::markdown_to_html(&text));
                // The page was rebuilt: line it up with the editor again.
                SYNC_LINE.with(|c| c.set(usize::MAX));
                self.sync_scroll_to_preview();
            }
            pcore::Kind::Html => pview::render("html", &text),
            pcore::Kind::Mermaid => pview::render("mermaid", &text),
        }
    }

    /// Scroll the preview to the editor's top line (when synchronized scrolling is on).
    fn sync_scroll_to_preview(&mut self) {
        if !PREVIEW_ON.with(|c| c.get()) || !self.ws.is_markdown() {
            return;
        }
        let line = self.ws.top_doc_line();
        if SYNC_LINE.with(|c| c.replace(line)) != line {
            pview::scroll_to_line(line as f64, self.ws.line_total());
        }
    }

    fn preview_message(&mut self, code: usize, arg: isize) {
        match code {
            pview::PV_SCROLL => {
                // The user scrolled the preview: bring the editor to the same source line.
                if self.ws.is_markdown() {
                    let line = ((arg as f64 / 100.0).floor().max(0.0) as usize).min(self.ws.line_total().saturating_sub(1));
                    if SYNC_LINE.with(|c| c.replace(line)) != line {
                        self.ws.set_top_doc_line(line);
                    }
                }
            }
            pview::PV_CLOSE => {
                self.set_preview(false);
                self.remember_md_preview(false);
            }
            pview::PV_MAX => {
                let on = !PREVIEW_MAX.with(|c| c.get());
                PREVIEW_MAX.with(|c| c.set(on));
                pview::set_max(on);
                layout();
            }
            pview::PV_NEED_MERMAID => pview::load_mermaid(),
            pview::PV_REFRESH => self.preview_update(true),
            _ => {}
        }
    }

    /// Rebuild the File menu when the recent list or "can restore a closed file" changed.
    fn sync_file_menu(&mut self) {
        let key = (self.ws.recent.clone(), !self.ws.closed.is_empty());
        if key != self.file_menu_key {
            fill_file_menu(&self.menus, &key.0, key.1);
            self.file_menu_key = key;
        }
        // Items that act on the file behind the active tab are greyed for unsaved tabs.
        let has_path = self.ws.tabs[self.ws.active].path.is_some();
        let state = if has_path { MF_ENABLED } else { MF_GRAYED };
        let m = self.menus;
        unsafe {
            for id in [ID_OPEN_VIEWER, ID_RELOAD, ID_RECYCLE] {
                let _ = EnableMenuItem(m.file, id as u32, MF_BYCOMMAND | state);
            }
            let _ = EnableMenuItem(m.file, FILE_OPEN_FOLDER_POS, MF_BYPOSITION | state);
            for id in [ID_OPEN_FOLDER_EXPLORER, ID_OPEN_FOLDER_CMD, ID_OPEN_FOLDER_WS] {
                let _ = EnableMenuItem(m.open_folder, id as u32, MF_BYCOMMAND | state);
            }
        }
    }

    fn refresh_side(&mut self) {
        match sidepanel::mode() {
            Some(sidepanel::Mode::Documents) => {
                let rows = self
                    .ws
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        let name = format!("{}{}", t.title, if self.ws.is_dirty(i) { " *" } else { "" });
                        (vec![name, t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default()], i)
                    })
                    .collect();
                sidepanel::set_rows(rows);
            }
            Some(sidepanel::Mode::Functions) => {
                if self.ws.is_whiteboard() {
                    sidepanel::set_rows(Vec::new());
                    self.side_key = None;
                    return;
                }
                let key = (self.ws.tabs[self.ws.active].doc, sci::length(&self.ws.sci));
                if self.side_key == Some(key) {
                    return;
                }
                self.side_key = Some(key);
                let text = sci::get_text(&self.ws.sci);
                let rows = functions::extract(&text, self.ws.tabs[self.ws.active].lang)
                    .into_iter()
                    .map(|e| (vec![e.name, (e.line + 1).to_string()], e.line))
                    .collect();
                sidepanel::set_rows(rows);
            }
            _ => {}
        }
    }

    fn check_external(&mut self) {
        let App { ws, ui, .. } = self;
        if ws.check_external(ui) {
            self.wb_shown = None; // a reloaded whiteboard must push its scene to the page
            self.refresh();
        }
    }

    /// Monitoring (tail -f): reload the active file when it has changed on disk.
    fn monitor_tick(&mut self) {
        let alive: Vec<u64> = self.ws.tabs.iter().map(|t| t.id).collect();
        self.monitoring.retain(|id, _| alive.contains(id));
        if self.monitoring.is_empty() {
            unsafe {
                let _ = KillTimer(Some(self.ui.host), TIMER_MONITOR);
            }
            return;
        }
        let t = &self.ws.tabs[self.ws.active];
        let (id, path) = (t.id, t.path.clone());
        let (Some(last), Some(path)) = (self.monitoring.get(&id).copied(), path) else { return };
        let Ok(md) = std::fs::metadata(&path) else { return };
        if md.len() == last || self.ws.is_dirty(self.ws.active) {
            return;
        }
        let App { ws, ui, .. } = self;
        if ws.reload_from_disk(ui) {
            let end = sci::length(&ws.sci).max(0) as usize;
            ws.sci.send(sci::SCI_GOTOPOS, end, 0);
            ws.sci.send(sci::SCI_SCROLLCARET, 0, 0);
        }
        self.monitoring.insert(id, md.len());
        self.refresh();
    }

    /// Grey out and tick Edit menu items to match the editor. Runs as the menu opens.
    fn sync_edit_menu(&mut self) {
        let docs: Vec<(String, bool)> = (0..self.ws.tabs.len())
            .map(|i| {
                let t = &self.ws.tabs[i];
                (t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t.title.clone()), self.ws.is_dirty(i))
            })
            .collect();
        let key = (docs, self.ws.active);
        if key != self.window_key {
            fill_window_menu(self.menus.window, &key.0, key.1);
            self.window_key = key;
        }
        let m = self.menus;
        let s = self.ws.sci;
        let has_selection = s.send(sci::SCI_GETSELECTIONSTART, 0, 0) != s.send(sci::SCI_GETSELECTIONEND, 0, 0);
        let state = |on: bool| MF_BYCOMMAND | if on { MF_ENABLED } else { MF_GRAYED };
        let checked = |on: bool| MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED };
        let read_only_file = self.ws.tabs[self.ws.active]
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .is_some_and(|md| md.permissions().readonly());
        unsafe {
            let _ = EnableMenuItem(m.edit, ID_UNDO as u32, state(s.send(sci::SCI_CANUNDO, 0, 0) != 0));
            let _ = EnableMenuItem(m.edit, ID_REDO as u32, state(s.send(sci::SCI_CANREDO, 0, 0) != 0));
            let _ = EnableMenuItem(m.edit, ID_PASTE as u32, state(s.send(sci::SCI_CANPASTE, 0, 0) != 0));
            // Encoding and Language menus: tick the entry in effect for this tab.
            let t = &self.ws.tabs[self.ws.active];
            let (cur, also) = match t.enc {
                Enc::Cs(i) => (ID_CHARSET_BASE + i as usize, usize::MAX),
                e => (
                    ID_ENCODE_BASE + ENCODE_IN.iter().position(|x| *x == e).unwrap_or(0),
                    if e == Enc::Ansi { ID_CHARSET_BASE + CHARSETS.len() } else { usize::MAX },
                ),
            };
            for id in (ID_ENCODE_BASE..ID_ENCODE_BASE + ENCODE_IN.len()).chain(ID_CHARSET_BASE..=ID_CHARSET_BASE + CHARSETS.len()) {
                check_deep(m.enc, id, id == cur || id == also);
            }
            let cur_lang = LANGUAGES.iter().position(|l| l.0 == t.lang_label).unwrap_or(0);
            for i in 0..LANGUAGES.len() {
                check_deep(m.lang, ID_LANG_BASE + i, i == cur_lang);
            }
            let v = self.ws.symbols;
            for (id, on) in [
                (ID_SYM_WS, v.whitespace),
                (ID_SYM_EOL, v.eol),
                (ID_SYM_NONPRINT, v.non_printing()),
                (ID_SYM_CTRL, v.control),
                (ID_SYM_ALL, v.all()),
                (ID_SYM_INDENT, v.indent_guides),
                (ID_SYM_WRAP, v.wrap_symbol),
            ] {
                let _ = CheckMenuItem(m.sym, id as u32, checked(on).0);
            }
            let top_most = GetWindowLongPtrW(self.ui.host, GWL_EXSTYLE) & WS_EX_TOPMOST.0 as isize != 0;
            let mode = sidepanel::mode();
            let tab = &self.ws.tabs[self.ws.active];
            let monitored = self.monitoring.contains_key(&tab.id);
            for (id, on) in [
                (ID_ON_TOP, top_most),
                (ID_FULLSCREEN, FULLSCREEN.with(|c| c.get())),
                (ID_POSTIT, POSTIT.with(|c| c.get())),
                (ID_DISTRACTION, DISTRACTION.with(|c| c.get())),
                (ID_WORKSPACE_TOGGLE, TREE_ON.with(|c| c.get())),
                (ID_DOC_LIST, mode == Some(sidepanel::Mode::Documents)),
                (ID_FUNC_LIST, mode == Some(sidepanel::Mode::Functions)),
                (ID_MONITOR, monitored),
            ] {
                let _ = CheckMenuItem(m.view, id as u32, checked(on).0);
            }
            let _ = EnableMenuItem(m.view, ID_MONITOR as u32, state(tab.path.is_some()));
            let _ = EnableMenuItem(m.search, ID_RESULTS as u32, state(!self.hits.is_empty()));
            let brace = search::matching_brace(&s).is_some();
            for id in [ID_BRACE_GOTO, ID_BRACE_SEL] {
                let _ = EnableMenuItem(m.search, id as u32, state(brace));
            }
            for id in [ID_SEL_OPEN, ID_SEL_FOLDER, ID_SEL_SEARCH] {
                let _ = EnableMenuItem(m.on_sel, id as u32, state(has_selection));
            }
            let _ = CheckMenuItem(m.edit, ID_READONLY as u32, checked(self.ws.is_read_only()).0);
            let _ = EnableMenuItem(m.edit, ID_CLEAR_RO as u32, state(read_only_file));
            let _ = CheckMenuItem(m.edit, ID_CHAR_PANEL as u32, checked(sidepanel::mode() == Some(sidepanel::Mode::Characters)).0);
            let _ = CheckMenuItem(m.edit, ID_CLIP_HISTORY as u32, checked(sidepanel::mode() == Some(sidepanel::Mode::Clipboard)).0);
            let engine = ID_ENGINE_BASE + self.ws.search_engine.min(edit::SEARCH_ENGINES.len() - 1);
            let _ = CheckMenuRadioItem(m.engine, ID_ENGINE_BASE as u32, (ID_ENGINE_BASE + edit::SEARCH_ENGINES.len() - 1) as u32, engine as u32, MF_BYCOMMAND.0);
        }
    }

    fn save_session(&mut self) {
        if crate::core::session::standalone() {
            return;
        }
        let s = self.ws.snapshot();
        let _ = s.save(&session_path());
    }
}

// ---- layout ------------------------------------------------------------------

fn layout() {
    let Some(h) = handles() else { return };
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(h.host, &mut rc);
        let (w, total_h) = (rc.right, rc.bottom);
        let mv = |win: HWND, x: i32, y: i32, ww: i32, hh: i32| {
            let _ = MoveWindow(win, x, y, ww.max(0), hh.max(0), true);
        };
        let c = h.chrome;
        let hide = HIDE.with(|c| c.get()) | USER_HIDE.with(|c| c.get());
        let part = |bit: u8, h: i32| if hide & bit != 0 { 0 } else { h };
        let (mh, th, tabh, sh) = (part(1, chrome::MENU_H), part(2, chrome::TOOL_H), part(4, chrome::TAB_H), part(8, chrome::STATUS_H));
        for (win, bit) in [(c.menu, 1u8), (c.tools, 2), (c.tabs, 4), (c.status, 8)] {
            let _ = ShowWindow(win, if hide & bit != 0 { SW_HIDE } else { SW_SHOW });
        }
        mv(c.menu, 0, 0, w, mh);
        mv(c.tools, 0, mh, w, th);
        let tabs_y = mh + th;
        mv(c.tabs, 0, tabs_y, w, tabh);
        mv(c.status, 0, total_h - sh, w, sh);

        // The Markdown formatting bar sits between the tab strip and the editor.
        let bar_y = tabs_y + tabh;
        let md_h = if MD_BAR_ON.with(|c| c.get()) && !DISTRACTION.with(|c| c.get()) && !WB_ON.with(|c| c.get()) { mdbar::HEIGHT } else { 0 };
        let _ = ShowWindow(h.mdbar, if md_h > 0 { SW_SHOW } else { SW_HIDE });
        mv(h.mdbar, 0, bar_y, w, md_h);
        if md_h > 0 {
            mdbar::arrange(&md_ids(), w);
        }

        let top = bar_y + md_h;
        let panel_h = findpanel::height();
        let panel_y = total_h - sh - panel_h;
        mv(h.panel, 0, panel_y, w, panel_h);
        findpanel::arrange(w);

        let area_h = panel_y - top;
        // Folder workspace tree on the far left; everything below uses the space to its right.
        let distraction = DISTRACTION.with(|c| c.get());
        let tree_w = if TREE_ON.with(|c| c.get()) && !distraction { tree::WIDTH.min(w / 2) } else { 0 };
        let _ = ShowWindow(h.tree, if tree_w > 0 { SW_SHOW } else { SW_HIDE });
        mv(h.tree, 0, top, tree_w, area_h);
        let (x0, w) = (tree_w, w - tree_w);
        let side_w = if sidepanel::mode().is_some() && !distraction { sidepanel::WIDTH.min(w / 3) } else { 0 };
        let _ = ShowWindow(h.side, if side_w > 0 { SW_SHOW } else { SW_HIDE });
        mv(h.side, x0 + w - side_w, top, side_w, area_h);
        let w = w - side_w;
        let mini_on = MINI_ON.with(|c| c.get()) && !distraction;
        let preview_on = PREVIEW_ON.with(|c| c.get());
        let maximised = preview_on && PREVIEW_MAX.with(|c| c.get());
        let show = |win: HWND, on: bool| {
            let _ = ShowWindow(win, if on { SW_SHOW } else { SW_HIDE });
        };

        let wb_on = WB_ON.with(|c| c.get());

        if wb_on {
            // A whiteboard tab: the Excalidraw view replaces the editor row.
            show(h.edit, false);
            show(h.mini, false);
            show(h.split, false);
            pview::set_visible(false);
            wb::set_bounds(x0, top, w, area_h);
        } else if maximised {
            pview::set_visible(true);
            // The preview takes the whole editor row.
            show(h.edit, false);
            show(h.mini, false);
            show(h.split, false);
            pview::set_bounds(x0, top, w, area_h);
        } else {
            // Editor (with its minimap) on the left, optional splitter + preview on the right.
            let preview_w = if preview_on {
                ((w as f32) * PREVIEW_RATIO.with(|c| c.get())) as i32
            } else {
                0
            }
            .clamp(0, (w - 320).max(0));
            let preview_w = if preview_on { preview_w.max(240.min(w / 2)) } else { 0 };
            let split_w = if preview_on { SPLIT_W } else { 0 };
            let editor_w = w - preview_w - split_w;
            let mini_w = if mini_on && editor_w > 520 { MINI_W } else { 0 };

            show(h.edit, true);
            pview::set_visible(preview_on);
            mv(h.edit, x0, top, editor_w - mini_w, area_h);
            mv(h.mini, x0 + editor_w - mini_w, top, mini_w, area_h);
            show(h.mini, mini_w > 0);
            show(h.split, preview_on);
            if preview_on {
                mv(h.split, x0 + editor_w, top, split_w, area_h);
                pview::set_bounds(x0 + editor_w + split_w, top, preview_w, area_h);
            }
        }
    }
    with_app(|a| a.ws.sync_minimap());
}

/// The draggable bar between editor and preview: reports the cursor's x in
/// host client coordinates while dragging.
unsafe extern "system" fn split_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            unsafe { SetCursor(LoadCursorW(None, IDC_SIZEWE).ok()) };
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            unsafe { SetCapture(hwnd) };
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = unsafe { ReleaseCapture() };
            LRESULT(0)
        }
        WM_MOUSEMOVE if unsafe { GetCapture() } == hwnd => {
            if let Some(h) = handles() {
                let mut pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let _ = ScreenToClient(h.host, &mut pt);
                    let _ = PostMessageW(Some(h.host), WM_SPLIT, WPARAM(0), LPARAM(pt.x as isize));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

/// Minimap subclass: clicks and drags scroll the main view; the minimap itself
/// is never focused or edited.
unsafe extern "system" fn mini_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let prev: WNDPROC = unsafe { std::mem::transmute(MINI_PREV.load(Ordering::Relaxed)) };
    match msg {
        WM_LBUTTONDOWN | WM_MOUSEMOVE if msg == WM_LBUTTONDOWN || w.0 & 0x0001 != 0 => {
            let (x, y) = ((l.0 & 0xFFFF) as i16 as usize, ((l.0 >> 16) & 0xFFFF) as i16 as isize);
            let pos = unsafe { SendMessageW(hwnd, sci::SCI_POSITIONFROMPOINT, Some(WPARAM(x)), Some(LPARAM(y))).0 };
            let line = unsafe { SendMessageW(hwnd, sci::SCI_LINEFROMPOSITION, Some(WPARAM(pos.max(0) as usize)), None).0 };
            if let Some(h) = handles() {
                unsafe {
                    let _ = PostMessageW(Some(h.host), WM_MINIMAP, WPARAM(0), LPARAM(line));
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if let Some(h) = handles() {
                let menu = MenuBuilder::popup();
                MenuBuilder::add(menu, ID_MINIMAP, "Disable Minimap");
                unsafe {
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_TOPALIGN, pt.x, pt.y, Some(0), h.host, None);
                    let _ = DestroyMenu(menu);
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_SETFOCUS | WM_CONTEXTMENU => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_SETCURSOR => {
            unsafe { SetCursor(LoadCursorW(None, IDC_HAND).ok()) };
            LRESULT(1)
        }
        WM_MOUSEWHEEL => {
            if let Some(h) = handles() {
                return unsafe { SendMessageW(h.edit, msg, Some(w), Some(l)) };
            }
            LRESULT(0)
        }
        _ => unsafe { CallWindowProcW(prev, hwnd, msg, w, l) },
    }
}

/// Leading part of Scintilla's `SCNotification` (64-bit layout), enough to read
/// the position and margin of a margin click.
#[repr(C)]
struct SciNotify {
    hwnd_from: HWND,
    id_from: usize,
    code: u32,
    position: isize,
    ch: i32,
    modifiers: i32,
    modification_type: i32,
    text: *const u8,
    length: isize,
    lines_added: isize,
    message: i32,
    wparam: usize,
    lparam: isize,
    line: isize,
    fold_level_now: i32,
    fold_level_prev: i32,
    margin: i32,
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            layout();
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = unsafe { &mut *(l.0 as *mut MINMAXINFO) };
            mmi.ptMinTrackSize = POINT { x: 760, y: 460 };
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if let Some(h) = handles() {
                if WB_ON.with(|c| c.get()) && wb::is_created() {
                    wb::focus();
                } else {
                    unsafe {
                        let _ = SetFocus(Some(h.edit));
                    }
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = w.0 & 0xFFFF;
            // Scintilla's focus notifications arrive as id 0; handling them as a command would
            // pull focus back to the editor in the middle of a toolbar button click.
            if id != 0 {
                with_app(|a| a.command(id));
            }
            LRESULT(0)
        }
        chrome::WM_BAR => {
            let arg = l.0;
            with_app(|a| {
                match w.0 {
                    chrome::BAR_TAB_SELECT => a.ws.activate(arg as usize),
                    chrome::BAR_TAB_CLOSE => {
                        let App { ws, ui, .. } = a;
                        ws.close(ui, arg as usize);
                    }
                    chrome::BAR_TAB_NEW => {
                        a.ws.new_tab();
                    }
                    chrome::BAR_TAB_MOVE => a.ws.move_tab_to(arg as usize & 0xFFFF, (arg as usize >> 16) & 0xFFFF),
                    chrome::BAR_STATUS => a.status_click(arg),
                    _ => {}
                }
                a.refresh();
                unsafe {
                    let _ = SetFocus(Some(a.ws.sci.0));
                }
            });
            LRESULT(0)
        }
        findpanel::WM_PANEL => {
            let (id, arg) = (w.0, l.0);
            with_app(|a| a.panel_command(id, arg));
            LRESULT(0)
        }
        palette::WM_PALETTE => {
            let (kind, id) = (w.0, l.0 as usize);
            with_app(|a| {
                if kind == Kind::Files as usize {
                    if let Some(p) = a.quick_files.get(id).cloned() {
                        let App { ws, ui, .. } = a;
                        ws.open_path(ui, &p);
                    }
                    a.refresh();
                    unsafe {
                        let _ = SetFocus(Some(a.ws.sci.0));
                    }
                } else {
                    a.command(id);
                }
            });
            LRESULT(0)
        }
        WM_MINIMAP => {
            with_app(|a| {
                a.ws.scroll_to_line(l.0.max(0) as usize);
            });
            LRESULT(0)
        }
        WM_TIMER if w.0 == TIMER_EXTERNAL => {
            // Only while this window is the one in use: a prompt of ours must not stack on another.
            if unsafe { GetActiveWindow() } == hwnd {
                with_app(|a| a.check_external());
            }
            LRESULT(0)
        }
        0x001C => {
            // WM_ACTIVATEAPP: coming back to the app is when changes made elsewhere are noticed.
            if w.0 != 0 {
                with_app(|a| a.check_external());
            }
            LRESULT(0)
        }
        WM_TIMER if w.0 == TIMER_MONITOR => {
            with_app(|a| a.monitor_tick());
            LRESULT(0)
        }
        WM_TIMER if w.0 == TIMER_PREVIEW => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_PREVIEW);
            }
            with_app(|a| a.preview_update(false));
            LRESULT(0)
        }
        WM_SPLIT => {
            let mut rc = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
            }
            if rc.right > 0 {
                let tree_w = if TREE_ON.with(|c| c.get()) && !DISTRACTION.with(|c| c.get()) { tree::WIDTH.min(rc.right / 2) } else { 0 };
                let side_w = if sidepanel::mode().is_some() && !DISTRACTION.with(|c| c.get()) { sidepanel::WIDTH.min((rc.right - tree_w) / 3) } else { 0 };
                let content = (rc.right - tree_w - side_w).max(1) as f32;
                let ratio = (((rc.right - side_w) as f32 - l.0 as f32) / content).clamp(0.2, 0.8);
                PREVIEW_RATIO.with(|c| c.set(ratio));
                layout();
            }
            LRESULT(0)
        }
        pview::WM_PREVIEW => {
            let (code, arg) = (w.0, l.0);
            with_app(|a| a.preview_message(code, arg));
            LRESULT(0)
        }
        wb::WM_WB => {
            with_app(|a| a.wb_events());
            LRESULT(0)
        }
        WM_CONTEXTMENU => {
            let from = HWND(w.0 as *mut core::ffi::c_void);
            if handles().is_some_and(|h| h.edit == from) {
                let (x, y) = ((l.0 & 0xFFFF) as i16 as i32, ((l.0 >> 16) & 0xFFFF) as i16 as i32);
                with_app(|a| a.show_editor_context_menu(x, y));
                return LRESULT(0);
            }
            unsafe { DefWindowProcW(hwnd, msg, w, l) }
        }
        WM_INITMENUPOPUP => {
            with_app(|a| a.sync_edit_menu());
            LRESULT(0)
        }
        incsearch::WM_INCR => {
            let kind = w.0;
            with_app(|a| a.incr_event(kind));
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
            if let Some(text) = clip::get_text(hwnd) {
                sidepanel::record_clipboard(&text);
            }
            LRESULT(0)
        }
        WM_NOTIFY => {
            let hdr = unsafe { &*(l.0 as *const windows::Win32::UI::Controls::NMHDR) };
            // Character Panel / Clipboard History: a double-click inserts the entry.
            if let Some(action) = sidepanel::notify(l) {
                with_app(|a| {
                    match action {
                        sidepanel::Action::Insert(text) => a.ws.insert_text(&text),
                        sidepanel::Action::Activate(i) => a.ws.activate(i),
                        sidepanel::Action::Goto(line) => drop(search::goto_line(&a.ws.sci, line + 1)),
                    }
                    a.refresh();
                    a.focus_active();
                });
                return LRESULT(0);
            }
            // The workspace tree: expanding folders is handled inside; a file asks to be opened.
            if let Some(path) = tree::notify(l) {
                with_app(|a| {
                    let App { ws, ui, .. } = &mut *a;
                    ws.open_path(ui, &path);
                    a.refresh();
                    a.focus_active();
                });
                return LRESULT(0);
            }
            if let Some(h) = handles() {
                if hdr.hwndFrom == h.edit && hdr.code == sci::SCN_MARGINCLICK {
                    let n = unsafe { &*(l.0 as *const SciNotify) };
                    if n.margin == 0 {
                        let pos = n.position.max(0) as usize;
                        with_app(|a| {
                            let sci = a.ws.sci;
                            let line = sci.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0) as usize;
                            // A "+" marker stands for lines hidden below it; otherwise this toggles a bookmark.
                            if view::has_hidden_marker(&sci, line) {
                                view::show_hidden_after(&sci, line);
                            } else {
                                a.ws.toggle_bookmark_at(pos);
                            }
                        });
                    }
                }
                if hdr.hwndFrom == h.edit && matches!(hdr.code, sci::SCN_SAVEPOINTLEFT | sci::SCN_SAVEPOINTREACHED | sci::SCN_UPDATEUI) {
                    // Deferred: this arrives inside a SendMessage the app itself made.
                    unsafe {
                        let _ = PostMessageW(Some(hwnd), WM_REFRESH, WPARAM(0), LPARAM(0));
                    }
                }
            }
            LRESULT(0)
        }
        WM_REFRESH => {
            with_app(|a| a.refresh());
            LRESULT(0)
        }
        WM_CLOSE => {
            with_app(|a| a.save_session());
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

// ---- menus, toolbar, accelerators ------------------------------------------------

struct MenuBuilder;

impl MenuBuilder {
    fn add(m: HMENU, id: usize, text: &str) {
        let t = wide(text);
        unsafe {
            let _ = AppendMenuW(m, MF_STRING, id, PCWSTR(t.as_ptr()));
        }
    }
    fn add_state(m: HMENU, id: usize, text: &str, enabled: bool) {
        let t = wide(text);
        unsafe {
            let _ = AppendMenuW(m, MF_STRING | if enabled { MF_ENABLED } else { MF_GRAYED }, id, PCWSTR(t.as_ptr()));
        }
    }
    fn sep(m: HMENU) {
        unsafe {
            let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
        }
    }
    fn popup() -> HMENU {
        unsafe { CreatePopupMenu().unwrap() }
    }
    fn submenu(parent: HMENU, label: &str, child: HMENU) {
        let t = wide(label);
        unsafe {
            let _ = AppendMenuW(parent, MF_POPUP, child.0 as usize, PCWSTR(t.as_ptr()));
        }
    }
}

fn eol_menu() -> HMENU {
    let m = MenuBuilder::popup();
    MenuBuilder::add(m, ID_EOL_CRLF, "Windows (CR LF)");
    MenuBuilder::add(m, ID_EOL_LF, "Unix (LF)");
    MenuBuilder::add(m, ID_EOL_CR, "Macintosh (CR)");
    m
}

/// Rebuild the Window menu: Sort By, Windows..., then the first nine tabs (the active one ticked).
/// `RemoveMenu` keeps the Sort By submenu alive between rebuilds.
fn fill_window_menu(menu: HMENU, docs: &[(String, bool)], active: usize) {
    use MenuBuilder as B;
    thread_local! {
        static SORT_MENU: Cell<isize> = const { Cell::new(0) };
    }
    unsafe {
        while GetMenuItemCount(Some(menu)) > 0 {
            let _ = RemoveMenu(menu, 0, MF_BYPOSITION);
        }
    }
    let sort = SORT_MENU.with(|c| {
        if c.get() == 0 {
            let s = B::popup();
            for (i, (label, _, _)) in dialogs::SORTS.iter().enumerate() {
                B::add(s, ID_WINDOW_SORT_BASE + i, label);
            }
            c.set(s.0 as isize);
        }
        HMENU(c.get() as *mut core::ffi::c_void)
    });
    B::submenu(menu, "Sort By", sort);
    B::add(menu, ID_WINDOWS, "&Windows...");
    B::sep(menu);
    for (i, (name, dirty)) in docs.iter().enumerate().take(WINDOW_DOCS_SHOWN) {
        let label = format!("&{} {}{}", i + 1, if *dirty { "*" } else { "" }, name.replace('&', "&&"));
        B::add(menu, ID_WINDOW_DOC_BASE + i, &label);
    }
    if active < WINDOW_DOCS_SHOWN && active < docs.len() {
        unsafe {
            let _ = CheckMenuItem(menu, (ID_WINDOW_DOC_BASE + active) as u32, (MF_BYCOMMAND | MF_CHECKED).0);
        }
    }
}

/// Fill the File menu in Notepad++'s order. Run again whenever the recent list
/// changes; `RemoveMenu` (not `DeleteMenu`) keeps the submenus alive for reuse.
fn fill_file_menu(m: &Menus, recent: &[PathBuf], can_restore: bool) {
    use MenuBuilder as B;
    unsafe {
        while GetMenuItemCount(Some(m.file)) > 0 {
            let _ = RemoveMenu(m.file, 0, MF_BYPOSITION);
        }
    }
    let f = m.file;
    B::add(f, ID_NEW, "&New\tCtrl+N");
    B::add(f, ID_NEW_WB, "New &Whiteboard");
    B::add(f, ID_OPEN, "&Open...\tCtrl+O");
    B::submenu(f, "Open Containing &Folder", m.open_folder); // keep FILE_OPEN_FOLDER_POS in step
    B::add(f, ID_OPEN_VIEWER, "Open in &Default Viewer");
    B::add(f, ID_OPEN_WORKSPACE, "Open Folder as &Workspace...");
    B::add(f, ID_RELOAD, "Reload from Dis&k\tCtrl+R");
    B::add(f, ID_SAVE, "&Save\tCtrl+S");
    B::add(f, ID_SAVEAS, "Save &As...\tCtrl+Alt+S");
    B::add(f, ID_SAVE_COPY, "Save a Cop&y As...");
    B::add(f, ID_SAVEALL, "Save A&ll\tCtrl+Shift+S");
    B::add(f, ID_RENAME, "Rena&me...");
    B::add(f, ID_CLOSE, "&Close\tCtrl+W");
    B::add(f, ID_CLOSE_ALL, "Close All\tCtrl+Shift+W");
    B::submenu(f, "Close M&ultiple Documents", m.close_multi);
    B::add(f, ID_RECYCLE, "Move to Recycle &Bin");
    B::sep(f);
    B::add(f, ID_LOAD_SESSION, "Load Session...");
    B::add(f, ID_SAVE_SESSION, "Save Session...");
    B::sep(f);
    // Ctrl+P is Quick Open in Note++, so Print has no shortcut.
    B::add(f, ID_PRINT, "&Print...");
    B::add(f, ID_PRINT_NOW, "Print No&w");
    B::sep(f);
    if !recent.is_empty() {
        for (i, p) in recent.iter().enumerate() {
            let mnemonic = if i < 9 { format!("&{}", i + 1) } else { "1&0".to_string() };
            B::add(f, ID_RECENT_BASE + i, &format!("{mnemonic}: {}", p.display().to_string().replace('&', "&&")));
        }
        B::sep(f);
    }
    B::add_state(f, ID_RESTORE_CLOSED, "Restore Recent Closed File\tCtrl+Shift+T", can_restore);
    B::add_state(f, ID_OPEN_ALL_RECENT, "Open All Recent Files", !recent.is_empty());
    B::add_state(f, ID_RECENT_CLEAR, "Empty Recent Files List", !recent.is_empty());
    B::sep(f);
    B::add(f, ID_EXIT, "E&xit\tAlt+F4");
}

fn build_menus() -> (Vec<(String, HMENU)>, Menus) {
    use MenuBuilder as B;
    let file = B::popup();
    let open_folder = B::popup();
    B::add(open_folder, ID_OPEN_FOLDER_EXPLORER, "&Explorer");
    B::add(open_folder, ID_OPEN_FOLDER_CMD, "&cmd");
    B::add(open_folder, ID_OPEN_FOLDER_WS, "Folder as &Workspace");
    let close_multi = B::popup();
    B::add(close_multi, ID_CLOSE_OTHERS, "Close All but Active Document");
    B::add(close_multi, ID_CLOSE_LEFT, "Close All to the Left");
    B::add(close_multi, ID_CLOSE_RIGHT, "Close All to the Right");
    B::add(close_multi, ID_CLOSE_UNCHANGED, "Close All Unchanged");

    let edit = B::popup();
    B::add(edit, ID_UNDO, "&Undo\tCtrl+Z or Alt+Backspace");
    B::add(edit, ID_REDO, "&Redo\tCtrl+Y or Ctrl+Shift+Z");
    B::sep(edit);
    B::add(edit, ID_CUT, "Cu&t\tCtrl+X or Shift+DEL");
    B::add(edit, ID_COPY, "&Copy\tCtrl+C or Ctrl+INS");
    B::add(edit, ID_PASTE, "&Paste\tCtrl+V or Shift+INS");
    B::add(edit, ID_DELETE, "&Delete\tDEL");
    B::add(edit, ID_SELALL, "Select &All\tCtrl+A");
    B::add(edit, ID_BEGIN_END_SEL, "Begin/End &Select\tCtrl+Shift+B");
    B::add(edit, ID_BEGIN_END_COL, "Begin/End Select in Column Mode\tAlt+Shift+B");
    B::sep(edit);

    let insert = B::popup();
    B::add(insert, ID_INS_DATE_SHORT, "Date Time (short version)");
    B::add(insert, ID_INS_DATE_LONG, "Date Time (long version)");
    B::add(insert, ID_INS_DATE_CUSTOM, "Date Time (customized)");
    B::submenu(edit, "In&sert", insert);

    let to_clip = B::popup();
    B::add(to_clip, ID_CP_FULLPATH, "Current Full File path to Clipboard");
    B::add(to_clip, ID_CP_NAME, "Current Filename to Clipboard");
    B::add(to_clip, ID_CP_DIR, "Current Dir. Path to Clipboard");
    B::sep(to_clip);
    B::add(to_clip, ID_CP_ALLNAMES, "Copy All Filenames");
    B::add(to_clip, ID_CP_ALLPATHS, "Copy All File Paths");
    B::submenu(edit, "Copy to Clip&board", to_clip);

    let indent = B::popup();
    B::add(indent, ID_INDENT_INC, "&Increase Line Indent\tTab");
    B::add(indent, ID_INDENT_DEC, "&Decrease Line Indent\tShift+Tab");
    B::submenu(edit, "&Indent", indent);

    let case = B::popup();
    B::add(case, ID_C_UPPER, "&UPPERCASE\tCtrl+Shift+U");
    B::add(case, ID_C_LOWER, "&lowercase\tCtrl+U");
    B::sep(case);
    B::add(case, ID_CASE_PROPER, "&Proper Case");
    B::add(case, ID_CASE_PROPER_BLEND, "Proper Case (blend)");
    B::add(case, ID_CASE_SENTENCE, "&Sentence case");
    B::add(case, ID_CASE_SENTENCE_BLEND, "Sentence case (blend)");
    B::add(case, ID_CASE_INVERT, "i&NVERT cASE");
    B::add(case, ID_CASE_RANDOM, "&ranDOm CasE");
    B::submenu(edit, "Convert Case &to", case);

    let lines = B::popup();
    B::add(lines, ID_L_DUP, "&Duplicate Current Line\tCtrl+D");
    B::add(lines, ID_L_DEDUP, "Remove Duplicate Lines");
    B::add(lines, ID_L_RM_CONSEC, "Remove Consecutive Duplicate Lines");
    B::add(lines, ID_L_SPLIT, "S&plit Lines");
    B::add(lines, ID_L_JOIN, "&Join Lines");
    B::add(lines, ID_L_UP, "Move Up Current Line\tCtrl+Shift+Up");
    B::add(lines, ID_L_DOWN, "Move Down Current Line\tCtrl+Shift+Down");
    B::add(lines, ID_L_TRANSPOSE, "&Transpose Current Line\tCtrl+T");
    B::add(lines, ID_L_NOEMPTY, "Remove Empty Lines");
    B::add(lines, ID_L_RM_BLANK, "Remove Empty Lines (Containing Blank characters)");
    B::add(lines, ID_L_INS_ABOVE, "Insert Blank Line Above Current\tCtrl+Alt+Shift+Enter");
    B::add(lines, ID_L_INS_BELOW, "Insert Blank Line Below Current\tCtrl+Alt+Enter");
    B::add(lines, ID_L_REVERSE, "Reverse Line Order");
    B::add(lines, ID_L_RANDOM, "Randomize Line Order");
    B::sep(lines);
    B::add(lines, ID_L_CUT, "Cut Current Line");
    B::add(lines, ID_L_COPY, "Copy Current Line");
    B::add(lines, ID_L_DEL, "Delete Current Line\tCtrl+L");
    B::sep(lines);
    B::add(lines, ID_L_SORT_ASC, "Sort Lines Lexicographically Ascending");
    B::add(lines, ID_L_SORT_DESC, "Sort Lines Lexicographically Descending");
    B::add(lines, ID_L_SORT_NOCASE_ASC, "Sort Lines Lexicographically Ascending Ignoring Case");
    B::add(lines, ID_L_SORT_NOCASE_DESC, "Sort Lines Lexicographically Descending Ignoring Case");
    B::add(lines, ID_L_SORT_INT_ASC, "Sort Lines As Integers Ascending");
    B::add(lines, ID_L_SORT_INT_DESC, "Sort Lines As Integers Descending");
    B::add(lines, ID_L_SORT_COMMA_ASC, "Sort Lines As Decimals (Comma) Ascending");
    B::add(lines, ID_L_SORT_COMMA_DESC, "Sort Lines As Decimals (Comma) Descending");
    B::add(lines, ID_L_SORT_DOT_ASC, "Sort Lines As Decimals (Dot) Ascending");
    B::add(lines, ID_L_SORT_DOT_DESC, "Sort Lines As Decimals (Dot) Descending");
    B::submenu(edit, "&Line Operations", lines);

    let comment = B::popup();
    B::add(comment, ID_COMMENT, "&Toggle Single Line Comment\tCtrl+Q");
    B::add(comment, ID_CMT_LINE, "Single Line &Comment\tCtrl+K");
    B::add(comment, ID_UNCMT_LINE, "Single Line &Uncomment\tCtrl+Shift+K");
    B::add(comment, ID_CMT_BLOCK, "&Block Comment\tCtrl+Shift+Q");
    B::add(comment, ID_UNCMT_BLOCK, "Block Uncomment\tCtrl+Shift+R");
    B::submenu(edit, "Comment/Uncomment", comment);

    let complete = B::popup();
    B::add(complete, ID_AC_FUNC, "Function Completion\tCtrl+Space");
    B::add(complete, ID_AC_WORD, "Word Completion\tCtrl+Enter");
    B::add(complete, ID_AC_HINT, "Function Parameters Hint\tCtrl+Shift+Space");
    B::add(complete, ID_AC_PATH, "Path Completion\tCtrl+Alt+Space");
    B::submenu(edit, "Auto-Completion", complete);

    let eol_sub = eol_menu();
    B::submenu(edit, "EOL Conversion", eol_sub);

    let blank = B::popup();
    B::add(blank, ID_L_TRIM, "Trim Trailing Space");
    B::add(blank, ID_B_TRIM_LEAD, "Trim Leading Space");
    B::add(blank, ID_B_TRIM_BOTH, "Trim Leading and Trailing Space");
    B::add(blank, ID_B_EOL_SPACE, "EOL to Space");
    B::add(blank, ID_B_UNNECESSARY, "Remove Unnecessary Blank and EOL");
    B::sep(blank);
    B::add(blank, ID_B_TAB_SPACE, "TAB to Space");
    B::add(blank, ID_B_SPACE_TAB_ALL, "Space to TAB (All)");
    B::add(blank, ID_B_SPACE_TAB_LEAD, "Space to TAB (Leading)");
    B::submenu(edit, "Blank Operations", blank);

    let paste_special = B::popup();
    B::add(paste_special, ID_PS_HTML, "Paste HTML Content");
    B::add(paste_special, ID_PS_RTF, "Paste RTF Content");
    B::sep(paste_special);
    B::add(paste_special, ID_PS_COPY_BIN, "Copy Binary Content");
    B::add(paste_special, ID_PS_CUT_BIN, "Cut Binary Content");
    B::add(paste_special, ID_PS_PASTE_BIN, "Paste Binary Content");
    B::submenu(edit, "Paste Special", paste_special);

    let on_selection = B::popup();
    B::add(on_selection, ID_SEL_OPEN, "Open File");
    B::add(on_selection, ID_SEL_FOLDER, "Open Containing Folder");
    B::add(on_selection, ID_SEL_SEARCH, "Search on Internet");
    let engine = B::popup();
    for (i, (name, _)) in edit::SEARCH_ENGINES.iter().enumerate() {
        B::add(engine, ID_ENGINE_BASE + i, name);
    }
    B::submenu(on_selection, "Change Search Engine", engine);
    B::submenu(edit, "On Selection", on_selection);
    B::sep(edit);

    B::add(edit, ID_COLUMN_MODE, "Column Mode...");
    B::add(edit, ID_COLUMN_EDITOR, "Column &Editor...\tAlt+C");
    B::add(edit, ID_CHAR_PANEL, "Character Panel");
    B::add(edit, ID_CLIP_HISTORY, "Clipboard History");
    B::sep(edit);
    B::add(edit, ID_READONLY, "Set Read-Only");
    B::add(edit, ID_CLEAR_RO, "Clear Read-Only Flag");

    let search = B::popup();
    B::add(search, ID_FIND, "&Find...\tCtrl+F");
    B::add(search, ID_FIF, "Find in Files...\tCtrl+Shift+F");
    B::add(search, ID_FINDNEXT, "Find &Next\tF3");
    B::add(search, ID_FINDPREV, "Find &Previous\tShift+F3");
    B::add(search, ID_SEL_FIND_NEXT, "Select and Find Next\tCtrl+F3");
    B::add(search, ID_SEL_FIND_PREV, "Select and Find Previous\tCtrl+Shift+F3");
    B::add(search, ID_VOL_NEXT, "Find (Volatile) Next\tCtrl+Alt+F3");
    B::add(search, ID_VOL_PREV, "Find (Volatile) Previous\tCtrl+Alt+Shift+F3");
    B::add(search, ID_REPLACE, "&Replace...\tCtrl+H");
    B::add(search, ID_INCR, "Incremental Search\tCtrl+Alt+I");
    B::add(search, ID_RESULTS, "Search Results Window\tF7");
    B::add(search, ID_NEXT_RESULT, "Next Search Result\tF4");
    B::add(search, ID_PREV_RESULT, "Previous Search Result\tShift+F4");
    B::add(search, ID_GOTO, "&Go to...\tCtrl+G");
    B::add(search, ID_BRACE_GOTO, "Go to Matching Brace\tCtrl+B");
    B::add(search, ID_BRACE_SEL, "Select All Between Matching Braces\tCtrl+Alt+B");
    B::add(search, ID_MARK, "&Mark...\tCtrl+M");
    B::sep(search);
    let slot_menu = |title: &str, base: usize, prefix: &str, suffix: &str, extra: Option<&str>| {
        let m = B::popup();
        for (i, n) in SLOT_NAMES.iter().enumerate() {
            B::add(m, base + i, &format!("{prefix}{n}{suffix}"));
        }
        if let Some(e) = extra {
            B::add(m, base + SLOT_NAMES.len(), e);
        }
        B::submenu(search, title, m);
    };
    slot_menu("Style All Occurrences of Token", ID_STYLE_ALL_BASE, "Using ", " Style", None);
    slot_menu("Style One Token", ID_STYLE_ONE_BASE, "Using ", " Style", None);
    slot_menu("Clear Style", ID_CLEAR_STYLE_BASE, "Clear ", " Style", Some("Clear all Styles"));
    slot_menu("Jump Up", ID_JUMP_UP_BASE, "", " Style", None);
    slot_menu("Jump Down", ID_JUMP_DOWN_BASE, "", " Style", None);
    slot_menu("Copy Styled Text", ID_COPY_STYLED_BASE, "", " Style", Some("All Styles"));
    B::sep(search);
    let bm = B::popup();
    B::add(bm, ID_BM_TOGGLE, "&Toggle Bookmark\tCtrl+F2");
    B::add(bm, ID_BM_NEXT, "&Next Bookmark\tF2");
    B::add(bm, ID_BM_PREV, "&Previous Bookmark\tShift+F2");
    B::add(bm, ID_BM_CLEAR, "&Clear All Bookmarks");
    B::add(bm, ID_BM_CUT, "Cut Bookmarked Lines");
    B::add(bm, ID_BM_COPY, "Copy Bookmarked Lines");
    B::add(bm, ID_BM_PASTE, "Paste to (Replace) Bookmarked Lines");
    B::add(bm, ID_BM_DELETE, "Remove Bookmarked Lines");
    B::add(bm, ID_BM_DELETE_UNMARKED, "Remove Non-Bookmarked Lines");
    B::add(bm, ID_BM_INVERT, "Inverse Bookmark");
    B::submenu(search, "&Bookmark", bm);
    B::sep(search);
    B::add(search, ID_FIND_CHARS, "Find characters in range...");

    let view = B::popup();
    B::add(view, ID_ON_TOP, "Always on Top");
    B::add(view, ID_FULLSCREEN, "Toggle Full Screen Mode\tF11");
    B::add(view, ID_POSTIT, "Post-It\tF12");
    B::add(view, ID_DISTRACTION, "Distraction Free Mode");
    B::sep(view);
    let browsers = B::popup();
    for (i, (name, _)) in BROWSERS.iter().enumerate() {
        B::add(browsers, ID_BROWSER_BASE + i, name);
    }
    B::submenu(view, "View Current File in", browsers);
    B::sep(view);
    let sym = B::popup();
    B::add(sym, ID_SYM_WS, "Show Space and Tab");
    B::add(sym, ID_SYM_EOL, "Show End of Line");
    B::add(sym, ID_SYM_NONPRINT, "Show Non-Printing Characters");
    B::add(sym, ID_SYM_CTRL, "Show Control Characters && Unicode EOL");
    B::add(sym, ID_SYM_ALL, "Show All Characters");
    B::sep(sym);
    B::add(sym, ID_SYM_INDENT, "Show Indent Guide");
    B::add(sym, ID_SYM_WRAP, "Show Wrap Symbol");
    B::submenu(view, "Show Symbol", sym);
    let zoom = B::popup();
    B::add(zoom, ID_ZOOMIN, "Zoom In\tCtrl+Num +");
    B::add(zoom, ID_ZOOMOUT, "Zoom Out\tCtrl+Num -");
    B::add(zoom, ID_ZOOMRESET, "Restore Default Zoom\tCtrl+Num /");
    B::submenu(view, "Zoom", zoom);
    let moveclone = B::popup();
    B::add_state(moveclone, ID_MOVE_OTHER_VIEW, "Move to Other View", false);
    B::add_state(moveclone, ID_CLONE_OTHER_VIEW, "Clone to Other View", false);
    B::add(moveclone, ID_MOVE_NEW_INSTANCE, "Move to New Instance");
    B::add(moveclone, ID_OPEN_NEW_INSTANCE, "Open in New Instance");
    B::submenu(view, "Move/Clone Current Document", moveclone);
    let tab = B::popup();
    for i in 0..9 {
        B::add(tab, ID_TAB_BASE + i, &format!("Tab {}", i + 1));
    }
    B::sep(tab);
    B::add(tab, ID_NEXT_TAB, "Next Tab\tCtrl+PgDn");
    B::add(tab, ID_PREV_TAB, "Previous Tab\tCtrl+PgUp");
    B::add(tab, ID_MOVE_TAB_FWD, "Move Tab Forward\tCtrl+Shift+PgDn");
    B::add(tab, ID_MOVE_TAB_BACK, "Move Tab Backward\tCtrl+Shift+PgUp");
    B::submenu(view, "Tab", tab);
    B::add(view, ID_WRAP, "Word wrap");
    B::add_state(view, ID_FOCUS_OTHER, "Focus on Another View\tF8", false);
    B::add(view, ID_HIDE_LINES, "Hide Lines\tAlt+H");
    B::sep(view);
    B::add(view, ID_FOLD_ALL, "Fold All\tAlt+0");
    B::add(view, ID_UNFOLD_ALL, "Unfold All\tAlt+Shift+0");
    B::add(view, ID_FOLD_CUR, "Fold Current Level\tCtrl+Alt+F");
    B::add(view, ID_UNFOLD_CUR, "Unfold Current Level\tCtrl+Alt+Shift+F");
    let fold_level = B::popup();
    let unfold_level = B::popup();
    for i in 0..8 {
        B::add(fold_level, ID_FOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+{}", i + 1, i + 1));
        B::add(unfold_level, ID_UNFOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+Shift+{}", i + 1, i + 1));
    }
    B::submenu(view, "Fold Level", fold_level);
    B::submenu(view, "Unfold Level", unfold_level);
    B::sep(view);
    B::add(view, ID_SUMMARY, "Summary...");
    B::sep(view);
    let projects = B::popup();
    for i in 0..3 {
        B::add_state(projects, ID_PROJECT_BASE + i, &format!("Project Panel {}", i + 1), false);
    }
    B::submenu(view, "Project Panels", projects);
    B::add(view, ID_WORKSPACE_TOGGLE, "Folder as Workspace");
    B::add(view, ID_MINIMAP, "Document Map");
    B::add(view, ID_DOC_LIST, "Document List");
    B::add(view, ID_FUNC_LIST, "Function List");
    B::sep(view);
    B::add_state(view, ID_SYNC_V, "Synchronize Vertical Scrolling", false);
    B::add_state(view, ID_SYNC_H, "Synchronize Horizontal Scrolling", false);
    B::sep(view);
    B::add(view, ID_RTL, "Text Direction RTL\tCtrl+Alt+R");
    B::add(view, ID_LTR, "Text Direction LTR\tCtrl+Alt+L");
    B::sep(view);
    B::add(view, ID_MONITOR, "Monitoring (tail -f)");
    B::sep(view);
    // Note++ extras that Notepad++ keeps elsewhere.
    B::add(view, ID_PREVIEW, "Live Preview\tCtrl+Shift+V");
    B::add(view, ID_MD_TOOLBAR, "Markdown Toolbar");
    B::add(view, ID_THEME, "Dark mode");
    B::add(view, ID_PALETTE, "Command Palette...\tCtrl+Shift+P");
    B::add(view, ID_QUICKOPEN, "Quick Open...\tCtrl+P");

    let enc = B::popup();
    for (i, e) in ENCODE_IN.iter().enumerate() {
        B::add(enc, ID_ENCODE_BASE + i, &format!("Encode in {}", e.label()));
    }
    // Character sets, one submenu per region.
    let charsets = B::popup();
    let mut regions: Vec<(&str, HMENU)> = Vec::new();
    for (i, c) in CHARSETS.iter().enumerate() {
        let sub = match regions.iter().find(|(g, _)| *g == c.group) {
            Some((_, m)) => *m,
            None => {
                let m = B::popup();
                if c.group == "Western European" {
                    B::add(m, ID_CHARSET_BASE + CHARSETS.len(), "Windows-1252");
                }
                regions.push((c.group, m));
                B::submenu(charsets, c.group, m);
                m
            }
        };
        B::add(sub, ID_CHARSET_BASE + i, c.label);
    }
    B::submenu(enc, "Character sets", charsets);
    B::sep(enc);
    for (i, e) in ENCODE_IN.iter().enumerate() {
        B::add(enc, ID_CONVERT_BASE + i, &format!("Convert to {}", e.label()));
    }

    // Languages: "Normal Text", then one submenu per initial letter.
    let lang = B::popup();
    B::add(lang, ID_LANG_BASE, "Normal Text");
    let mut by_letter: std::collections::BTreeMap<char, Vec<usize>> = std::collections::BTreeMap::new();
    for (i, (label, _)) in LANGUAGES.iter().enumerate().skip(1) {
        by_letter.entry(label.chars().next().unwrap_or('#').to_ascii_uppercase()).or_default().push(i);
    }
    for (letter, mut items) in by_letter {
        items.sort_by_key(|i| LANGUAGES[*i].0.to_lowercase());
        let sub = B::popup();
        for i in items {
            B::add(sub, ID_LANG_BASE + i, &LANGUAGES[i].0.replace('&', "&&"));
        }
        B::submenu(lang, &letter.to_string(), sub);
    }

    let help = B::popup();
    B::add(help, ID_ABOUT, "&About Note++");

    // Settings
    let settings_menu = B::popup();
    B::add(settings_menu, ID_PREFS, "Preferences...");
    B::add(settings_menu, ID_STYLE_CFG, "Style Configurator...");
    B::add(settings_menu, ID_SHORTCUTS, "Shortcut Mapper...");
    B::sep(settings_menu);
    let import = B::popup();
    B::add(import, ID_IMPORT_THEME, "Import style theme(s)...");
    B::add(import, ID_EXPORT_THEME, "Export style theme...");
    B::submenu(settings_menu, "Import", import);
    B::sep(settings_menu);
    B::add(settings_menu, ID_EDIT_CONTEXT, "Edit Popup ContextMenu");

    // Tools: one submenu per digest, then the text tools.
    let tools = B::popup();
    for (i, algo) in ALGOS.iter().enumerate() {
        let sub = B::popup();
        B::add(sub, ID_HASH_BASE + i * 10, "Generate...");
        B::add(sub, ID_HASH_BASE + i * 10 + 1, "Generate from files...");
        B::add(sub, ID_HASH_BASE + i * 10 + 2, "Generate from selection into clipboard");
        B::submenu(tools, algo.name(), sub);
    }
    B::sep(tools);
    let text_tools = B::popup();
    B::add(text_tools, ID_B64_ENC, "Base64 Encode");
    B::add(text_tools, ID_B64_DEC, "Base64 Decode");
    B::sep(text_tools);
    B::add(text_tools, ID_URL_ENC, "URL Encode");
    B::add(text_tools, ID_URL_DEC, "URL Decode");
    B::sep(text_tools);
    B::add(text_tools, ID_JSON_PRETTY, "JSON: Pretty-print");
    B::add(text_tools, ID_JSON_MIN, "JSON: Minify");
    B::add(text_tools, ID_XML_PRETTY, "XML: Pretty-print");
    B::add(text_tools, ID_XML_MIN, "XML: Minify");
    B::submenu(tools, "Text Tools", text_tools);

    // Window: Sort By, Windows..., then the open documents (filled as the menu opens).
    let window = B::popup();

    let bar = vec![
        ("File".to_string(), file),
        ("Edit".to_string(), edit),
        ("Search".to_string(), search),
        ("View".to_string(), view),
        ("Encoding".to_string(), enc),
        ("Language".to_string(), lang),
        ("Settings".to_string(), settings_menu),
        ("Tools".to_string(), tools),
        ("Window".to_string(), window),
        ("?".to_string(), help),
    ];
    let menus = Menus { file, open_folder, close_multi, edit, sym, search, on_sel: on_selection, engine, view, enc, eol: eol_sub, lang, window };
    fill_file_menu(&menus, &[], false);
    fill_window_menu(window, &[], 0);
    (bar, menus)
}

/// Set or clear the check mark of command `id` wherever it sits in `menu`'s submenus.
fn check_deep(menu: HMENU, id: usize, on: bool) {
    unsafe {
        for i in 0..GetMenuItemCount(Some(menu)).max(0) {
            let sub = GetSubMenu(menu, i);
            if !sub.0.is_null() {
                check_deep(sub, id, on);
            }
        }
        let _ = CheckMenuItem(menu, id as u32, (MF_BYCOMMAND | if on { MF_CHECKED } else { MF_UNCHECKED }).0);
    }
}

fn toolbar_items() -> Vec<ToolItem> {
    macro_rules! btn {
        ($cmd:expr, $file:literal, $tip:literal) => {
            ToolItem::Btn { cmd: $cmd, svg: include_bytes!(concat!("../../assets/", $file)), tip: $tip }
        };
    }
    vec![
        btn!(ID_NEW, "new.svg", "New (Ctrl+N)"),
        btn!(ID_NEW_WB, "whiteboard.svg", "New Whiteboard"),
        btn!(ID_OPEN, "open.svg", "Open (Ctrl+O)"),
        btn!(ID_SAVE, "save.svg", "Save (Ctrl+S)"),
        btn!(ID_SAVEALL, "saveall.svg", "Save All (Ctrl+Shift+S)"),
        btn!(ID_CLOSE, "close.svg", "Close (Ctrl+W)"),
        btn!(ID_PRINT, "print.svg", "Print"),
        ToolItem::Sep,
        btn!(ID_CUT, "cut.svg", "Cut (Ctrl+X)"),
        btn!(ID_COPY, "copy.svg", "Copy (Ctrl+C)"),
        btn!(ID_PASTE, "paste.svg", "Paste (Ctrl+V)"),
        ToolItem::Sep,
        btn!(ID_UNDO, "undo.svg", "Undo (Ctrl+Z)"),
        btn!(ID_REDO, "redo.svg", "Redo (Ctrl+Y)"),
        ToolItem::Sep,
        btn!(ID_FIND, "find.svg", "Find (Ctrl+F)"),
        btn!(ID_REPLACE, "replace.svg", "Replace (Ctrl+H)"),
        ToolItem::Sep,
        btn!(ID_ZOOMIN, "zoomin.svg", "Zoom In"),
        btn!(ID_ZOOMOUT, "zoomout.svg", "Zoom Out"),
        ToolItem::Sep,
        btn!(ID_WRAP, "wordwrap.svg", "Word Wrap"),
        btn!(ID_PREVIEW, "preview.svg", "Live Preview (Ctrl+Shift+V)"),
        btn!(ID_THEME, "darkmode.svg", "Dark Mode"),
    ]
}

fn accelerators(overrides: &std::collections::BTreeMap<String, String>) -> HACCEL {
    // FVIRTKEY 0x01, FSHIFT 0x04, FCONTROL 0x08, FALT 0x10
    const NONE: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x01);
    const SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x05);
    const CTRL: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x09);
    const CTRL_SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x0D);
    const CTRL_ALT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x19);
    const ALT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x11);
    const ALT_SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x15);
    const CTRL_ALT_SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x1D);
    let k = |f, key: char, cmd: usize| ACCEL { fVirt: f, key: key as u16, cmd: cmd as u16 };
    let table = [
        k(CTRL, 'N', ID_NEW),
        k(CTRL, 'O', ID_OPEN),
        k(CTRL, 'S', ID_SAVE),
        k(CTRL_ALT, 'S', ID_SAVEAS),
        k(CTRL_SHIFT, 'S', ID_SAVEALL),
        k(CTRL, 'W', ID_CLOSE),
        k(CTRL_SHIFT, 'W', ID_CLOSE_ALL),
        k(CTRL, 'R', ID_RELOAD),
        k(CTRL_SHIFT, 'T', ID_RESTORE_CLOSED),
        k(CTRL, 'F', ID_FIND),
        k(CTRL, 'H', ID_REPLACE),
        k(CTRL, 'G', ID_GOTO),
        k(CTRL, 'M', ID_MARK),
        k(CTRL_SHIFT, 'F', ID_FIF),
        ACCEL { fVirt: NONE, key: 0x72, cmd: ID_FINDNEXT as u16 }, // F3
        ACCEL { fVirt: SHIFT, key: 0x72, cmd: ID_FINDPREV as u16 }, // Shift+F3
        ACCEL { fVirt: CTRL, key: 0x72, cmd: ID_SEL_FIND_NEXT as u16 },
        ACCEL { fVirt: CTRL_SHIFT, key: 0x72, cmd: ID_SEL_FIND_PREV as u16 },
        ACCEL { fVirt: CTRL_ALT, key: 0x72, cmd: ID_VOL_NEXT as u16 },
        ACCEL { fVirt: CTRL_ALT_SHIFT, key: 0x72, cmd: ID_VOL_PREV as u16 },
        ACCEL { fVirt: NONE, key: 0x76, cmd: ID_RESULTS as u16 },     // F7
        ACCEL { fVirt: NONE, key: 0x73, cmd: ID_NEXT_RESULT as u16 }, // F4
        ACCEL { fVirt: SHIFT, key: 0x73, cmd: ID_PREV_RESULT as u16 },
        k(CTRL_ALT, 'I', ID_INCR),
        k(CTRL, 'B', ID_BRACE_GOTO),
        k(CTRL_ALT, 'B', ID_BRACE_SEL),
        ACCEL { fVirt: NONE, key: 0x71, cmd: ID_BM_NEXT as u16 },   // F2
        ACCEL { fVirt: SHIFT, key: 0x71, cmd: ID_BM_PREV as u16 },  // Shift+F2
        ACCEL { fVirt: CTRL, key: 0x71, cmd: ID_BM_TOGGLE as u16 }, // Ctrl+F2
        k(CTRL, 'D', ID_L_DUP),
        k(CTRL, 'P', ID_QUICKOPEN),
        k(CTRL_SHIFT, 'V', ID_PREVIEW),
        k(CTRL_SHIFT, 'P', ID_PALETTE),
        k(CTRL, 'L', ID_L_DEL),
        k(CTRL, 'U', ID_C_LOWER),
        k(CTRL_SHIFT, 'U', ID_C_UPPER),
        ACCEL { fVirt: CTRL_SHIFT, key: 0x26, cmd: ID_L_UP as u16 },   // Ctrl+Shift+Up
        ACCEL { fVirt: CTRL_SHIFT, key: 0x28, cmd: ID_L_DOWN as u16 }, // Ctrl+Shift+Down
        ACCEL { fVirt: CTRL, key: 0xBF, cmd: ID_COMMENT as u16 },      // Ctrl+/
        k(CTRL, 'Q', ID_COMMENT),
        k(CTRL, 'K', ID_CMT_LINE),
        k(CTRL_SHIFT, 'K', ID_UNCMT_LINE),
        k(CTRL_SHIFT, 'Q', ID_CMT_BLOCK),
        k(CTRL_SHIFT, 'R', ID_UNCMT_BLOCK),
        k(CTRL, 'T', ID_L_TRANSPOSE),
        k(CTRL_SHIFT, 'B', ID_BEGIN_END_SEL),
        k(ALT_SHIFT, 'B', ID_BEGIN_END_COL),
        k(ALT, 'C', ID_COLUMN_EDITOR),
        k(CTRL_SHIFT, 'Z', ID_REDO),
        ACCEL { fVirt: ALT, key: 0x08, cmd: ID_UNDO as u16 },            // Alt+Backspace
        ACCEL { fVirt: CTRL, key: 0x20, cmd: ID_AC_FUNC as u16 },        // Ctrl+Space
        ACCEL { fVirt: CTRL, key: 0x0D, cmd: ID_AC_WORD as u16 },        // Ctrl+Enter
        ACCEL { fVirt: CTRL_SHIFT, key: 0x20, cmd: ID_AC_HINT as u16 },  // Ctrl+Shift+Space
        ACCEL { fVirt: CTRL_ALT, key: 0x20, cmd: ID_AC_PATH as u16 },    // Ctrl+Alt+Space
        ACCEL { fVirt: CTRL_ALT, key: 0x0D, cmd: ID_L_INS_BELOW as u16 }, // Ctrl+Alt+Enter
        ACCEL { fVirt: CTRL_ALT_SHIFT, key: 0x0D, cmd: ID_L_INS_ABOVE as u16 },
        ACCEL { fVirt: NONE, key: 0x7A, cmd: ID_FULLSCREEN as u16 }, // F11
        ACCEL { fVirt: NONE, key: 0x7B, cmd: ID_POSTIT as u16 },     // F12
        k(ALT, 'H', ID_HIDE_LINES),
        k(ALT, '0', ID_FOLD_ALL),
        k(ALT_SHIFT, '0', ID_UNFOLD_ALL),
        k(CTRL_ALT, 'F', ID_FOLD_CUR),
        k(CTRL_ALT_SHIFT, 'F', ID_UNFOLD_CUR),
        k(CTRL_ALT, 'R', ID_RTL),
        k(CTRL_ALT, 'L', ID_LTR),
        ACCEL { fVirt: CTRL, key: 0x22, cmd: ID_NEXT_TAB as u16 },        // Ctrl+PgDn
        ACCEL { fVirt: CTRL, key: 0x21, cmd: ID_PREV_TAB as u16 },        // Ctrl+PgUp
        ACCEL { fVirt: CTRL_SHIFT, key: 0x22, cmd: ID_MOVE_TAB_FWD as u16 },
        ACCEL { fVirt: CTRL_SHIFT, key: 0x21, cmd: ID_MOVE_TAB_BACK as u16 },
        ACCEL { fVirt: CTRL, key: 0x6F, cmd: ID_ZOOMRESET as u16 },       // Ctrl+Num /
        k(ALT, '1', ID_FOLD_LEVEL_BASE),
        k(ALT, '2', ID_FOLD_LEVEL_BASE + 1),
        k(ALT, '3', ID_FOLD_LEVEL_BASE + 2),
        k(ALT, '4', ID_FOLD_LEVEL_BASE + 3),
        k(ALT, '5', ID_FOLD_LEVEL_BASE + 4),
        k(ALT, '6', ID_FOLD_LEVEL_BASE + 5),
        k(ALT, '7', ID_FOLD_LEVEL_BASE + 6),
        k(ALT, '8', ID_FOLD_LEVEL_BASE + 7),
        k(ALT_SHIFT, '1', ID_UNFOLD_LEVEL_BASE),
        k(ALT_SHIFT, '2', ID_UNFOLD_LEVEL_BASE + 1),
        k(ALT_SHIFT, '3', ID_UNFOLD_LEVEL_BASE + 2),
        k(ALT_SHIFT, '4', ID_UNFOLD_LEVEL_BASE + 3),
        k(ALT_SHIFT, '5', ID_UNFOLD_LEVEL_BASE + 4),
        k(ALT_SHIFT, '6', ID_UNFOLD_LEVEL_BASE + 5),
        k(ALT_SHIFT, '7', ID_UNFOLD_LEVEL_BASE + 6),
        k(ALT_SHIFT, '8', ID_UNFOLD_LEVEL_BASE + 7),
        ACCEL { fVirt: CTRL, key: 0x09, cmd: ID_NEXT_TAB as u16 },
        ACCEL { fVirt: CTRL_SHIFT, key: 0x09, cmd: ID_PREV_TAB as u16 },
    ];
    unsafe {
        let mut accels = table.to_vec();
        // A remapped command loses its default keys and gets the chosen one.
        for (id_text, text) in overrides {
            let Ok(id) = id_text.parse::<u16>() else { continue };
            accels.retain(|a| a.cmd != id);
            if let Some(chord) = shortcuts::parse(text) {
                accels.push(ACCEL { fVirt: ACCEL_VIRT_FLAGS(shortcuts::accel_flags(&chord)), key: chord.vk, cmd: id });
            }
        }
        CreateAcceleratorTableW(&accels).unwrap()
    }
}

// ---- startup -----------------------------------------------------------------------

/// Stop Windows looking for DLLs in the current directory or on PATH (DLL planting).
/// Call first thing in `main`; libraries then come from System32 or the exe's folder only.
pub fn harden_dll_search() {
    use windows::Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_APPLICATION_DIR, LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories, SetDllDirectoryW};
    unsafe {
        let _ = SetDllDirectoryW(w!(""));
        let _ = SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR);
    }
}

/// Create the host window, chrome, Scintilla editor + minimap and the Find panel.
pub fn create_window(title: &str, w: i32, h: i32) -> windows::core::Result<(Handles, Menus)> {
    unsafe {
        // Loading the DLL is what registers Scintilla's window class. Use the copy beside the
        // exe by full path, never one found through the search order.
        let scintilla_dll = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("Scintilla.dll")));
        let path = wide(&scintilla_dll.unwrap_or_else(|| PathBuf::from("Scintilla.dll")).to_string_lossy());
        LoadLibraryW(PCWSTR(path.as_ptr()))?;
        let hinstance = GetModuleHandleW(None)?;
        let class = w!("NoteppHost");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });

        let title_w = wide(title);
        let host = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            w,
            h,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        // chrome::create also sets up the Mica backdrop (title bar, menu, toolbar, tabs).
        let (menu_bar, menus) = build_menus();
        let chrome = chrome::create(host, menu_bar, &toolbar_items());

        let scintilla = |extra: WINDOW_STYLE| {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("Scintilla"),
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | extra,
                0,
                0,
                10,
                10,
                Some(host),
                None,
                Some(hinstance.into()),
                None,
            )
        };
        let edit = scintilla(WS_TABSTOP)?;
        let mini = scintilla(WINDOW_STYLE(0))?;
        MINI_PREV.store(SetWindowLongPtrW(mini, GWLP_WNDPROC, mini_proc as *const () as isize), Ordering::Relaxed);

        findpanel::create(host);
        let panel = findpanel::hwnd().unwrap();
        mdbar::create(host, &md_ids());
        let mdbar_hwnd = mdbar::hwnd().unwrap();

        // Splitter between editor and preview (hidden until the preview opens).
        let split_class = w!("NoteppSplit");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(split_proc),
            hInstance: hinstance.into(),
            lpszClassName: split_class,
            hCursor: LoadCursorW(None, IDC_SIZEWE)?,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(17 as *mut core::ffi::c_void), // COLOR_BTNSHADOW + 1
            ..Default::default()
        });
        let split = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            split_class,
            PCWSTR::null(),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            SPLIT_W,
            10,
            Some(host),
            None,
            Some(hinstance.into()),
            None,
        )?;

        let tree_view = tree::create(host);
        let side = sidepanel::create(host);
        clip::listen(host, true);
        let hs = Handles { host, edit, mini, split, tree: tree_view, side, chrome, panel, mdbar: mdbar_hwnd };
        HANDLES.with(|c| c.set(Some(hs)));
        Ok((hs, menus))
    }
}

/// Build the workspace, restore the last session, open `files`, and run the message loop.
pub fn run(hs: Handles, menus: Menus, files: Vec<PathBuf>) {
    let ui = Win32Ui { host: hs.host };
    let mut ws = Workspace::new(Win32Sci(hs.edit));
    ws.attach_minimap(Win32Sci(hs.mini));
    // The editor's own right-click menu is replaced by the one in contextmenu.txt.
    unsafe {
        SendMessageW(hs.edit, sci::SCI_USEPOPUP, Some(WPARAM(0)), None);
    }
    let prefs = Settings::load();
    USER_HIDE.with(|c| c.set(user_hide(&prefs)));
    install_shortcuts(&menus, &prefs, None);
    ws.set_settings(prefs);
    if !crate::core::session::standalone() {
        if let Some(s) = Session::load(&session_path()) {
            ws.restore(&s);
        }
    }
    for f in &files {
        ws.open_path(&ui, f);
    }
    if ws.tabs.is_empty() {
        ws.new_tab();
    }
    MINI_ON.with(|c| c.set(ws.minimap));
    if let Some(root) = ws.workspace.clone() {
        tree::set_root(&root);
        TREE_ON.with(|c| c.set(true));
    }
    APP.with(|c| {
        *c.borrow_mut() = Some(App { ws, ui, menus, hits: Vec::new(), hit_cursor: 0, monitoring: HashMap::new(), side_key: None, hits_root: PathBuf::new(), flash: None, file_menu_key: (vec![PathBuf::from("\u{0}")], false), window_key: (vec![("\u{0}".into(), false)], usize::MAX), quick_files: Vec::new(), preview_key: None, preview_doc: 0, preview_dark: false, wb_ready: false, wb_shown: None, wb_dark: false, wb_pending: None })
    });

    unsafe {
        layout();
        with_app(|a| a.refresh());
        SetTimer(Some(hs.host), TIMER_EXTERNAL, 2000, None);
        let _ = ShowWindow(hs.host, SW_SHOW);
        let _ = SetFocus(Some(hs.edit));

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let accel = HACCEL(ACCEL_TABLE.with(|c| c.get()) as *mut core::ffi::c_void);
            if TranslateAcceleratorW(hs.host, accel, &msg) != 0 {
                continue;
            }
            // Tab / Enter / Esc navigation inside the Find panel and Go To dialog.
            let in_dialog = [findpanel::hwnd(), findpanel::goto_hwnd(), incsearch::hwnd()]
                .into_iter()
                .flatten()
                .any(|d| IsDialogMessageW(d, &msg).as_bool());
            if !in_dialog {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

#[cfg(test)]
mod accelerator_tests {
    use super::*;

    #[test]
    fn executable_files_are_recognised() {
        for yes in ["a.exe", "B.BAT", "x.Ps1", "s.lnk", "t.hta", "i.msi", "w.url"] {
            assert!(runs_code(std::path::Path::new(yes)), "{yes}");
        }
        for no in ["a.txt", "readme.md", "noext", "exe", "x.exe.txt", "p.py"] {
            assert!(!runs_code(std::path::Path::new(no)), "{no}");
        }
    }

    fn entries(table: HACCEL) -> Vec<ACCEL> {
        unsafe {
            let n = CopyAcceleratorTableW(table, None);
            let mut v = vec![ACCEL::default(); n as usize];
            CopyAcceleratorTableW(table, Some(&mut v));
            v
        }
    }

    #[test]
    fn overrides_replace_the_default_keys() {
        let plain = entries(accelerators(&Default::default()));
        // Ctrl+D duplicates a line by default.
        assert!(plain.iter().any(|a| a.cmd as usize == ID_L_DUP && a.key == b'D' as u16));

        let mut o = std::collections::BTreeMap::new();
        o.insert(ID_L_DUP.to_string(), "Ctrl+Alt+Shift+9".to_string());
        let changed = entries(accelerators(&o));
        let dup: Vec<&ACCEL> = changed.iter().filter(|a| a.cmd as usize == ID_L_DUP).collect();
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].key, b'9' as u16);
        assert_eq!(dup[0].fVirt.0, 1 | 4 | 8 | 0x10);
        // Everything else is untouched.
        assert_eq!(changed.len(), plain.len());

        // An empty shortcut removes the key entirely.
        o.insert(ID_L_DUP.to_string(), String::new());
        assert!(!entries(accelerators(&o)).iter().any(|a| a.cmd as usize == ID_L_DUP));
    }

    #[test]
    fn context_menu_lines_resolve_to_commands() {
        let e = context_entries("# comment\nEdit: Cut\n-\nTools: Base64 Encode\nNo Such Command\n");
        assert_eq!(e.len(), 3);
        assert_eq!(e[0].map(|x| x.1), Some(ID_CUT));
        assert!(e[1].is_none());
        assert_eq!(e[2].map(|x| x.1), Some(ID_B64_ENC));
        assert!(context_entries(DEFAULT_CONTEXT_MENU).len() >= 15);
    }

    #[test]
    fn theme_export_round_trips() {
        let mut s = Settings::default();
        s.light_colors.insert("keyword".into(), "#112233".into());
        let json = theme_json(&s);
        let mut back = Settings::default();
        let n = import_theme(&mut back, &json).unwrap();
        assert!(n >= 28);
        assert_eq!(back.light_colors.get("keyword").map(String::as_str), Some("#112233"));
        assert_eq!(back.light_colors.len(), 1);
        assert!(back.dark_colors.is_empty());
        assert!(import_theme(&mut back, "{}").is_err());
    }
}
