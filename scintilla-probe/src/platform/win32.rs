//! Win32 shell: the host window, layout, menus, commands and message loop.
//!
//! The ONLY place that wires Windows UI to the portable `core`. Drawing of the
//! menu bar / toolbar / tabs / status bar lives in `chrome.rs`, the Find panel
//! in `findpanel.rs`. All editor behaviour is in `core` (`Workspace`, `search`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, PWSTR, w};

use super::chrome::{self, Chrome, StatusText, ToolItem};
use super::clip;
use super::findpanel::{self, Mode};
use super::form;
use super::palette::{self, Item, Kind};
use super::preview as pview;
use super::print;
use super::sidepanel;
use super::tree;
use super::whiteboard as wb;
use crate::core::edit::{self, ColumnNumbers, NumFormat, SortKind};
use crate::core::preview as pcore;
use crate::core::doc::{Enc, Eol};
use crate::core::files;
use crate::core::lang::LANGUAGES;
use crate::core::sci::{self, Send, Theme};
use crate::core::search::{self, Hit};
use crate::core::session::{Session, config_dir, session_path};
use crate::core::whiteboard;
use crate::core::workspace::{CaseOp, CloseOp, Completion, Confirm, LineOp, PathText, Ui, Workspace};

const WM_REFRESH: u32 = WM_APP + 1;
const WM_MINIMAP: u32 = WM_APP + 2;
const WM_SPLIT: u32 = WM_APP + 60;
const TIMER_PREVIEW: usize = 1;
const MINI_W: i32 = 110;
const SPLIT_W: i32 = 5;
const ID_PREVIEW: usize = 5301;
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
const ID_ENC_BASE: usize = 7000; // + index into ENCODINGS
const ID_LANG_BASE: usize = 8000; // + index into LANGUAGES

const ENCODINGS: [Enc; 5] = [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi];

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
    ("Search: Find Previous", ID_FINDPREV, "Shift+F3"),
    ("Search: Go to Line...", ID_GOTO, "Ctrl+G"),
    ("Bookmark: Toggle", ID_BM_TOGGLE, "Ctrl+F2"),
    ("Bookmark: Next", ID_BM_NEXT, "F2"),
    ("Bookmark: Previous", ID_BM_PREV, "Shift+F2"),
    ("Bookmark: Clear All", ID_BM_CLEAR, ""),
    ("View: Toggle Word Wrap", ID_WRAP, ""),
    ("View: Toggle Live Preview", ID_PREVIEW, "Ctrl+Shift+V"),
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
    for (i, e) in ENCODINGS.iter().enumerate() {
        v.push(Item { label: format!("Encoding: {}", e.label()), hint: String::new(), id: ID_ENC_BASE + i });
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
}

#[derive(Clone, Copy)]
pub struct Menus {
    file: HMENU,
    open_folder: HMENU,
    close_multi: HMENU,
    edit: HMENU,
    on_sel: HMENU,
    engine: HMENU,
    view: HMENU,
    enc: HMENU,
    eol: HMENU,
    lang: HMENU,
}

struct App {
    ws: Workspace<Win32Sci>,
    ui: Win32Ui,
    menus: Menus,
    hits: Vec<Hit>,
    hits_root: PathBuf,
    /// One-shot message shown in place of the cursor position.
    flash: Option<String>,
    /// What the File menu was last built from: (recent list, can restore a closed file).
    file_menu_key: (Vec<PathBuf>, bool),
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
            | ID_SAVE_SESSION | ID_RESTORE_CLOSED | ID_OPEN_ALL_RECENT | ID_WORKSPACE_TOGGLE
    ) || (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id)
}

thread_local! {
    static HANDLES: Cell<Option<Handles>> = const { Cell::new(None) };
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static MINI_ON: Cell<bool> = const { Cell::new(true) };
    static PREVIEW_ON: Cell<bool> = const { Cell::new(false) };
    static PREVIEW_MAX: Cell<bool> = const { Cell::new(false) };
    /// The active tab is a whiteboard, so the whiteboard view replaces the editor.
    static WB_ON: Cell<bool> = const { Cell::new(false) };
    /// The workspace tree is shown (only when a folder is open).
    static TREE_ON: Cell<bool> = const { Cell::new(false) };
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
        let text = wide(msg);
        unsafe { MessageBoxW(Some(self.host), PCWSTR(text.as_ptr()), w!("Note++"), MB_YESNO | MB_ICONQUESTION) == IDYES }
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
        let text = wide(&format!("Save changes to \"{name}\"?"));
        let r = unsafe { MessageBoxW(Some(self.host), PCWSTR(text.as_ptr()), w!("Note++"), MB_YESNOCANCEL | MB_ICONWARNING) };
        match r {
            IDYES => Confirm::Save,
            IDNO => Confirm::Discard,
            _ => Confirm::Cancel,
        }
    }

    fn error(&self, msg: &str) {
        let text = wide(msg);
        unsafe {
            MessageBoxW(Some(self.host), PCWSTR(text.as_ptr()), w!("Note++"), MB_OK | MB_ICONERROR);
        }
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
    fn react(f: &form::Form) {
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
    };
    let v = form::run(host, spec, |f| {
        f.set_checked(TEXT_R, true);
        f.set_checked(DEC, true);
        react(f);
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
                    self.flash = Some("No folder open: use File > Open Folder as Workspace".into());
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
            ID_COLUMN_MODE => unsafe {
                let t = wide(
                    "Column Mode selects a rectangle of text.\n\n\
                     \u{2022} Hold Alt and drag with the mouse, or\n\
                     \u{2022} press Alt+Shift with the arrow keys, or\n\
                     \u{2022} press Alt+Shift+B at one corner, move, and press it again.\n\n\
                     Then type to edit every line at once, or use Column Editor (Alt+C) to insert text or numbers.",
                );
                MessageBoxW(Some(ui.host), PCWSTR(t.as_ptr()), w!("Column Mode"), MB_OK | MB_ICONINFORMATION);
            },
            ID_COLUMN_EDITOR => self.flash = column_editor(ui.host, ws),
            ID_CHAR_PANEL | ID_CLIP_HISTORY => {
                let want = if id == ID_CHAR_PANEL { sidepanel::Mode::Characters } else { sidepanel::Mode::Clipboard };
                if sidepanel::mode() == Some(want) {
                    sidepanel::show_mode(None);
                } else {
                    if want == sidepanel::Mode::Clipboard {
                        if let Some(t) = clip::get_text(ui.host) {
                            sidepanel::record_clipboard(&t);
                        }
                    }
                    sidepanel::show_mode(Some(want));
                }
                layout();
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
                let t = wide(&format!(
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
                ));
                unsafe {
                    MessageBoxW(Some(ui.host), PCWSTR(t.as_ptr()), w!("About Note++"), MB_OK | MB_ICONINFORMATION);
                }
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
            _ if (ID_ENC_BASE..ID_ENC_BASE + ENCODINGS.len()).contains(&id) => ws.set_encoding(ENCODINGS[id - ID_ENC_BASE]),
            _ if (ID_LANG_BASE..ID_LANG_BASE + LANGUAGES.len()).contains(&id) => ws.set_language(LANGUAGES[id - ID_LANG_BASE].1),
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
                Ok(hits) => {
                    let lines: Vec<String> = hits.iter().map(|h| format!("Line {}:  {}", h.line, h.text)).collect();
                    note = Some(format!("{} hits in the current document", hits.len()));
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
                if let Some(h) = self.hits.get(arg as usize).cloned() {
                    let App { ws, ui, .. } = self;
                    if let Some(p) = &h.path {
                        ws.open_path(ui, p);
                    }
                    let a = ws.active;
                    if ws.is_dirty(a) && h.path.is_some() {
                        search::goto_line(&sci, h.line);
                    } else {
                        search::select_range(&sci, h.start, h.end);
                    }
                    unsafe {
                        let _ = SetFocus(Some(sci.0));
                    }
                }
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
        chrome::set_checked(&h.chrome, ID_WRAP, self.ws.wrap);
        chrome::set_checked(&h.chrome, ID_THEME, self.ws.theme.dark);
        chrome::set_theme(&h.chrome, self.ws.theme.dark, self.ws.theme.bg, self.ws.theme.fg);

        // Mermaid files open their live preview automatically; the preview follows the theme.
        let doc = self.ws.tabs[self.ws.active].doc;
        if doc != self.preview_doc {
            self.preview_doc = doc;
            if !PREVIEW_ON.with(|c| c.get()) && self.ws.preview_kind() == Some(pcore::Kind::Mermaid) {
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

        let title = wide(&self.ws.window_title());
        unsafe {
            let _ = SetWindowTextW(h.host, PCWSTR(title.as_ptr()));
            let on = |b: bool| if b { MF_CHECKED } else { MF_UNCHECKED };
            let m = self.menus;
            CheckMenuItem(m.view, ID_WRAP as u32, (MF_BYCOMMAND | on(self.ws.wrap)).0);
            CheckMenuItem(m.view, ID_MINIMAP as u32, (MF_BYCOMMAND | on(self.ws.minimap)).0);
            CheckMenuItem(m.view, ID_THEME as u32, (MF_BYCOMMAND | on(self.ws.theme.dark)).0);
            CheckMenuItem(m.view, ID_PREVIEW as u32, (MF_BYCOMMAND | on(preview_on)).0);

            let t = &self.ws.tabs[self.ws.active];
            let enc = ENCODINGS.iter().position(|e| *e == t.enc).unwrap_or(0);
            let _ = CheckMenuRadioItem(m.enc, ID_ENC_BASE as u32, (ID_ENC_BASE + ENCODINGS.len() - 1) as u32, (ID_ENC_BASE + enc) as u32, MF_BYCOMMAND.0);
            let lang = LANGUAGES.iter().position(|l| l.1 == t.lang).unwrap_or(0);
            let _ = CheckMenuRadioItem(m.lang, ID_LANG_BASE as u32, (ID_LANG_BASE + LANGUAGES.len() - 1) as u32, (ID_LANG_BASE + lang) as u32, MF_BYCOMMAND.0);
            let eol = match t.eol {
                Eol::Crlf => ID_EOL_CRLF,
                Eol::Lf => ID_EOL_LF,
                Eol::Cr => ID_EOL_CR,
            };
            let _ = CheckMenuRadioItem(m.eol, ID_EOL_CRLF as u32, ID_EOL_CR as u32, eol as u32, MF_BYCOMMAND.0);
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
            if let Err(e) = pview::ensure(h.host, &config_dir().join("webview")) {
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
            pcore::Kind::Markdown => pview::render("markdown", &pcore::markdown_to_html(&text)),
            pcore::Kind::Html => pview::render("html", &text),
            pcore::Kind::Mermaid => pview::render("mermaid", &text),
        }
    }

    fn preview_message(&mut self, code: usize) {
        match code {
            pview::PV_CLOSE => self.set_preview(false),
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

    /// Grey out and tick Edit menu items to match the editor. Runs as the menu opens.
    fn sync_edit_menu(&mut self) {
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
        mv(c.menu, 0, 0, w, chrome::MENU_H);
        mv(c.tools, 0, chrome::MENU_H, w, chrome::TOOL_H);
        let tabs_y = chrome::MENU_H + chrome::TOOL_H;
        mv(c.tabs, 0, tabs_y, w, chrome::TAB_H);
        mv(c.status, 0, total_h - chrome::STATUS_H, w, chrome::STATUS_H);

        let top = tabs_y + chrome::TAB_H;
        let panel_h = findpanel::height();
        let panel_y = total_h - chrome::STATUS_H - panel_h;
        mv(h.panel, 0, panel_y, w, panel_h);
        findpanel::arrange(w);

        let area_h = panel_y - top;
        // Folder workspace tree on the far left; everything below uses the space to its right.
        let tree_w = if TREE_ON.with(|c| c.get()) { tree::WIDTH.min(w / 2) } else { 0 };
        let _ = ShowWindow(h.tree, if tree_w > 0 { SW_SHOW } else { SW_HIDE });
        mv(h.tree, 0, top, tree_w, area_h);
        let (x0, w) = (tree_w, w - tree_w);
        let side_w = if sidepanel::mode().is_some() { sidepanel::WIDTH.min(w / 3) } else { 0 };
        let _ = ShowWindow(h.side, if side_w > 0 { SW_SHOW } else { SW_HIDE });
        mv(h.side, x0 + w - side_w, top, side_w, area_h);
        let w = w - side_w;
        let mini_on = MINI_ON.with(|c| c.get());
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
        WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_SETFOCUS | WM_CONTEXTMENU => LRESULT(0),
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
            with_app(|a| a.command(id));
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
                let tree_w = if TREE_ON.with(|c| c.get()) { tree::WIDTH.min(rc.right / 2) } else { 0 };
                let side_w = if sidepanel::mode().is_some() { sidepanel::WIDTH.min((rc.right - tree_w) / 3) } else { 0 };
                let content = (rc.right - tree_w - side_w).max(1) as f32;
                let ratio = (((rc.right - side_w) as f32 - l.0 as f32) / content).clamp(0.2, 0.8);
                PREVIEW_RATIO.with(|c| c.set(ratio));
                layout();
            }
            LRESULT(0)
        }
        pview::WM_PREVIEW => {
            let code = w.0;
            with_app(|a| a.preview_message(code));
            LRESULT(0)
        }
        wb::WM_WB => {
            with_app(|a| a.wb_events());
            LRESULT(0)
        }
        WM_INITMENUPOPUP => {
            with_app(|a| a.sync_edit_menu());
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
            if let Some(text) = sidepanel::notify(l) {
                with_app(|a| {
                    a.ws.insert_text(&text);
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
                        with_app(|a| a.ws.toggle_bookmark_at(pos));
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
    B::add(search, ID_FINDNEXT, "Find &Next\tF3");
    B::add(search, ID_FINDPREV, "Find &Previous\tShift+F3");
    B::add(search, ID_REPLACE, "&Replace...\tCtrl+H");
    B::add(search, ID_FIF, "Find in &Files...\tCtrl+Shift+F");
    B::add(search, ID_MARK, "&Mark...\tCtrl+M");
    B::sep(search);
    B::add(search, ID_GOTO, "&Go to...\tCtrl+G");
    let bm = B::popup();
    B::add(bm, ID_BM_TOGGLE, "&Toggle Bookmark\tCtrl+F2");
    B::add(bm, ID_BM_NEXT, "&Next Bookmark\tF2");
    B::add(bm, ID_BM_PREV, "&Previous Bookmark\tShift+F2");
    B::add(bm, ID_BM_CLEAR, "&Clear All Bookmarks");
    B::submenu(search, "&Bookmark", bm);

    let view = B::popup();
    B::add(view, ID_WRAP, "&Word wrap");
    B::add(view, ID_PREVIEW, "&Live Preview\tCtrl+Shift+V");
    B::add(view, ID_MINIMAP, "&Minimap");
    B::add(view, ID_THEME, "&Dark mode");
    B::sep(view);
    B::add(view, ID_PALETTE, "&Command Palette...\tCtrl+Shift+P");
    B::add(view, ID_QUICKOPEN, "&Quick Open...\tCtrl+P");
    B::sep(view);
    B::add(view, ID_ZOOMIN, "Zoom &in\tCtrl+Num +");
    B::add(view, ID_ZOOMOUT, "Zoom &out\tCtrl+Num -");
    B::add(view, ID_ZOOMRESET, "&Restore default zoom");
    B::sep(view);
    B::add(view, ID_NEXT_TAB, "&Next tab\tCtrl+Tab");
    B::add(view, ID_PREV_TAB, "&Previous tab\tCtrl+Shift+Tab");

    let enc = B::popup();
    for (i, e) in ENCODINGS.iter().enumerate() {
        B::add(enc, ID_ENC_BASE + i, e.label());
    }

    let lang = B::popup();
    for (i, (label, _)) in LANGUAGES.iter().enumerate() {
        B::add(lang, ID_LANG_BASE + i, label);
    }

    let help = B::popup();
    B::add(help, ID_ABOUT, "&About Note++");

    let bar = vec![
        ("File".to_string(), file),
        ("Edit".to_string(), edit),
        ("Search".to_string(), search),
        ("View".to_string(), view),
        ("Encoding".to_string(), enc),
        ("Language".to_string(), lang),
        ("?".to_string(), help),
    ];
    let menus = Menus { file, open_folder, close_multi, edit, on_sel: on_selection, engine, view, enc, eol: eol_sub, lang };
    fill_file_menu(&menus, &[], false);
    (bar, menus)
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

fn accelerators() -> HACCEL {
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
        ACCEL { fVirt: CTRL, key: 0x09, cmd: ID_NEXT_TAB as u16 },
        ACCEL { fVirt: CTRL_SHIFT, key: 0x09, cmd: ID_PREV_TAB as u16 },
    ];
    unsafe { CreateAcceleratorTableW(&table).unwrap() }
}

// ---- startup -----------------------------------------------------------------------

/// Create the host window, chrome, Scintilla editor + minimap and the Find panel.
pub fn create_window(title: &str, w: i32, h: i32) -> windows::core::Result<(Handles, Menus)> {
    unsafe {
        // Loading the DLL is what registers Scintilla's window class.
        LoadLibraryW(w!("Scintilla.dll"))?;
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
        let hs = Handles { host, edit, mini, split, tree: tree_view, side, chrome, panel };
        HANDLES.with(|c| c.set(Some(hs)));
        Ok((hs, menus))
    }
}

/// Build the workspace, restore the last session, open `files`, and run the message loop.
pub fn run(hs: Handles, menus: Menus, files: Vec<PathBuf>) {
    let ui = Win32Ui { host: hs.host };
    let mut ws = Workspace::new(Win32Sci(hs.edit));
    ws.attach_minimap(Win32Sci(hs.mini));
    if let Some(s) = Session::load(&session_path()) {
        ws.restore(&s);
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
        *c.borrow_mut() = Some(App { ws, ui, menus, hits: Vec::new(), hits_root: PathBuf::new(), flash: None, file_menu_key: (vec![PathBuf::from("\u{0}")], false), quick_files: Vec::new(), preview_key: None, preview_doc: 0, preview_dark: false, wb_ready: false, wb_shown: None, wb_dark: false, wb_pending: None })
    });

    unsafe {
        layout();
        with_app(|a| a.refresh());
        let _ = ShowWindow(hs.host, SW_SHOW);
        let _ = SetFocus(Some(hs.edit));

        let accel = accelerators();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            if TranslateAcceleratorW(hs.host, accel, &msg) != 0 {
                continue;
            }
            // Tab / Enter / Esc navigation inside the Find panel and Go To dialog.
            let in_dialog = [findpanel::hwnd(), findpanel::goto_hwnd()]
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
