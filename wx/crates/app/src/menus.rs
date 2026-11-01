//! Menus. The structure is defined once as a tree of plain data (`B::popup` / `B::add` / ...), the
//! same calls the Win32 shell uses, and then turned into wxWidgets menus by `realize`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use notepp_core::commands::*;
use notepp_core::doc::{CHARSETS, Enc};
use notepp_core::hash::ALGOS;
use notepp_core::lang::LANGUAGES;
use notepp_core::{edit, settings, shortcuts};
use wxdragon::prelude::Menu;

use crate::menu_spec::{Entry as MenuEntry, MenuSpec};

use crate::tables::*;

/// A handle to a menu under construction (like an HMENU: cheap to copy and reuse).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct M(usize);

enum Entry {
    Item { id: usize, label: String, enabled: bool },
    Sep,
    Sub { label: String, menu: M },
}

thread_local! {
    static ARENA: RefCell<Vec<Vec<Entry>>> = const { RefCell::new(Vec::new()) };
    /// Shortcut text the user chose for a command id, shown instead of the default.
    static OVERRIDES: RefCell<HashMap<usize, String>> = RefCell::new(HashMap::new());
}

/// Show the user's own shortcuts in the menus (call before building them).
pub fn set_shortcut_overrides(map: HashMap<usize, String>) {
    OVERRIDES.with(|o| *o.borrow_mut() = map);
}

pub struct B;

impl B {
    pub fn popup() -> M {
        ARENA.with(|a| {
            let mut a = a.borrow_mut();
            a.push(Vec::new());
            M(a.len() - 1)
        })
    }
    pub fn add(m: M, id: usize, text: &str) {
        Self::add_state(m, id, text, true);
    }
    pub fn add_state(m: M, id: usize, text: &str, enabled: bool) {
        let label = with_override(id, text);
        ARENA.with(|a| a.borrow_mut()[m.0].push(Entry::Item { id, label, enabled }));
    }
    pub fn sep(m: M) {
        ARENA.with(|a| a.borrow_mut()[m.0].push(Entry::Sep));
    }
    pub fn submenu(parent: M, label: &str, child: M) {
        ARENA.with(|a| a.borrow_mut()[parent.0].push(Entry::Sub { label: label.to_string(), menu: child }));
    }
    pub fn clear(m: M) {
        ARENA.with(|a| a.borrow_mut()[m.0].clear());
    }
}

/// Replace the shortcut part of a label ("Name\tCtrl+N") with the user's choice, if any.
fn with_override(id: usize, text: &str) -> String {
    let name = text.split('\t').next().unwrap_or(text);
    OVERRIDES.with(|o| match o.borrow().get(&id) {
        Some(k) if k.is_empty() => name.to_string(),
        Some(k) => format!("{name}\t{k}"),
        None => text.to_string(),
    })
}

/// Commands that are switches: they get a tick box.
fn checkable(id: usize) -> bool {
    matches!(
        id,
        ID_WRAP | ID_THEME | ID_MINIMAP | ID_PREVIEW | ID_LOG_FOLLOW | ID_MD_TOOLBAR | ID_UPDATE_AUTO | ID_READONLY | ID_CHAR_PANEL
            | ID_CLIP_HISTORY | ID_DOC_LIST | ID_FUNC_LIST | ID_FULLSCREEN | ID_ON_TOP | ID_POSTIT | ID_DISTRACTION | ID_MONITOR | ID_WORKSPACE_TOGGLE
            | ID_EOL_CRLF | ID_EOL_LF | ID_EOL_CR
    ) || PALETTE_COMMANDS.iter().any(|(name, i, _)| *i == id && name.starts_with("View: Show "))
        || (ID_ENGINE_BASE..ID_ENGINE_BASE + edit::SEARCH_ENGINES.len()).contains(&id)
}

/// The menu under construction as plain data (submenus included).
pub fn spec(m: M) -> MenuSpec {
    ARENA.with(|a| spec_inner(&a.borrow(), m))
}

fn spec_inner(a: &[Vec<Entry>], m: M) -> MenuSpec {
    let mut s = MenuSpec::new();
    for e in &a[m.0] {
        s.push(match e {
            Entry::Item { id, label, enabled } => MenuEntry::Item { id: *id, label: label.clone(), enabled: *enabled, check: checkable(*id).then_some(false) },
            Entry::Sep => MenuEntry::Sep,
            Entry::Sub { label, menu } => MenuEntry::Sub { label: label.clone(), menu: spec_inner(a, *menu) },
        });
    }
    s
}

/// Build the wxWidgets menu for a menu under construction.
pub fn realize(m: M) -> Menu {
    spec(m).realize()
}
// ---- generated from the Win32 shell's menu definitions -----------------------------------------

#[derive(Clone, Copy)]
pub struct Menus {
    pub file: M,
    pub open_folder: M,
    pub close_multi: M,
    pub print: M,
    pub recent: M,
    pub edit: M,
    pub sym: M,
    pub search: M,
    pub on_sel: M,
    pub engine: M,
    pub view: M,
    pub enc: M,
    pub eol: M,
    pub lang: M,
    pub window: M,
    pub help: M,
}

pub fn eol_menu() -> M {
    let m = B::popup();
    B::add(m, ID_EOL_CRLF, "Windows (CR LF)");
    B::add(m, ID_EOL_LF, "Unix (LF)");
    B::add(m, ID_EOL_CR, "Macintosh (CR)");
    m
}

/// Fill the File menu in Notepad++'s order. Run again whenever the recent list
/// changes; `RemoveMenu` (not `DeleteMenu`) keeps the submenus alive for reuse.
pub fn fill_file_menu(m: &Menus, recent: &[PathBuf], can_restore: bool) {
    
    B::clear(m.file);
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
    B::add(f, ID_CLOSE, "&Close\tCtrl+W");
    B::add(f, ID_CLOSE_ALL, "Close All\tCtrl+Shift+W");
    B::submenu(f, "Close M&ultiple Documents", m.close_multi);
    B::add(f, ID_RECYCLE, "Move to Recycle &Bin");
    B::sep(f);
    B::add(f, ID_LOAD_SESSION, "Load Session...");
    B::add(f, ID_SAVE_SESSION, "Save Session...");
    B::sep(f);
    // Ctrl+P is Quick Open in Note++, so Print has no shortcut.
    B::submenu(f, "&Print", m.print);
    B::sep(f);
    B::add(f, ID_ENCRYPT, "&Encrypt This File");
    B::add(f, ID_DECRYPT, "Remove Encr&yption From This File");
    B::sep(f);
    B::clear(m.recent);
    let r = m.recent;
    if recent.is_empty() {
        B::add_state(r, 0, "(no recent files)", false);
    }
    for (i, p) in recent.iter().enumerate() {
        let mnemonic = if i < 9 { format!("&{}", i + 1) } else { "1&0".to_string() };
        B::add(r, ID_RECENT_BASE + i, &format!("{mnemonic}: {}", p.display().to_string().replace('&', "&&")));
    }
    B::sep(r);
    B::add_state(r, ID_OPEN_ALL_RECENT, "Open All Recent Files", !recent.is_empty());
    B::add_state(r, ID_RECENT_CLEAR, "Empty Recent Files List", !recent.is_empty());
    B::submenu(f, "&Recent Files", m.recent);
    B::add_state(f, ID_RESTORE_CLOSED, "Restore Recent Closed File\tCtrl+Shift+T", can_restore);
    B::sep(f);
    B::add(f, ID_EXIT, "E&xit\tAlt+F4");
}

pub fn build_menus() -> (Vec<(String, M)>, Menus) {
    
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
    let print = B::popup();
    B::add(print, ID_PRINT, "Print &Preview / Print...");
    B::add(print, ID_PRINT_NOW, "Print No&w");
    B::add(print, ID_PAGE_SETUP, "Page Se&tup...");
    let recent = B::popup();

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
    B::add(case, ID_CASE_PROPER, "&Proper Case\tAlt+U");
    B::add(case, ID_CASE_PROPER_BLEND, "Proper Case (blend)\tAlt+Shift+U");
    B::add(case, ID_CASE_SENTENCE, "&Sentence case\tCtrl+Alt+U");
    B::add(case, ID_CASE_SENTENCE_BLEND, "Sentence case (blend)\tCtrl+Alt+Shift+U");
    B::add(case, ID_CASE_INVERT, "i&NVERT cASE");
    B::add(case, ID_CASE_RANDOM, "&ranDOm CasE");
    B::submenu(edit, "Convert Case &to", case);

    let lines = B::popup();
    B::add(lines, ID_L_DUP, "&Duplicate Current Line\tCtrl+D");
    B::add(lines, ID_L_DEDUP, "Remove Duplicate Lines");
    B::add(lines, ID_L_RM_CONSEC, "Remove Consecutive Duplicate Lines");
    B::add(lines, ID_L_SPLIT, "S&plit Lines\tCtrl+I");
    B::add(lines, ID_L_JOIN, "&Join Lines\tCtrl+J");
    B::add(lines, ID_L_UP, "Move Up Current Line\tCtrl+Shift+Up");
    B::add(lines, ID_L_DOWN, "Move Down Current Line\tCtrl+Shift+Down");
    B::add(lines, ID_L_TRANSPOSE, "&Transpose Current Line\tCtrl+T");
    B::add(lines, ID_L_NOEMPTY, "Remove Empty Lines");
    B::add(lines, ID_L_RM_BLANK, "Remove Empty Lines (Containing Blank characters)");
    B::add(lines, ID_L_INS_ABOVE, "Insert Blank Line Above Current\tCtrl+Alt+Enter");
    B::add(lines, ID_L_INS_BELOW, "Insert Blank Line Below Current\tCtrl+Alt+Shift+Enter");
    B::add(lines, ID_L_REVERSE, "Reverse Line Order");
    B::add(lines, ID_L_RANDOM, "Randomize Line Order");
    B::sep(lines);
    B::add(lines, ID_L_CUT, "Cut Current Line\tCtrl+L");
    B::add(lines, ID_L_COPY, "Copy Current Line\tCtrl+Shift+X");
    B::add(lines, ID_L_DEL, "Delete Current Line\tCtrl+Shift+L");
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
    B::add(view, ID_LOG_FOLLOW, "Log Follow (.log / .out / .err)");
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
    let mut regions: Vec<(&str, M)> = Vec::new();
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
    B::sep(help);
    B::add(help, ID_UPDATE_CHECK, "Check for Updates &Now");
    B::add(help, ID_UPDATE_AUTO, "Check for Updates &Automatically");

    // Settings
    let settings_menu = B::popup();
    B::add(settings_menu, ID_PREFS, "Preferences...");
    B::add(settings_menu, ID_STYLE_CFG, "Style Configurator...");
    B::add(settings_menu, ID_SHORTCUTS, "Shortcut Mapper...");
    B::sep(settings_menu);
    let themes = B::popup();
    B::add(themes, ID_PRESET_BASE, "Default (light)");
    B::add(themes, ID_PRESET_BASE + 1, "Default (dark)");
    B::sep(themes);
    for (i, p) in settings::PRESETS.iter().enumerate() {
        B::add(themes, ID_PRESET_BASE + 2 + i, p.name);
    }
    B::submenu(settings_menu, "Themes", themes);
    let import = B::popup();
    B::add(import, ID_IMPORT_THEME, "Import style theme(s)...");
    B::add(import, ID_EXPORT_THEME, "Export style theme...");
    B::submenu(settings_menu, "Import", import);
    B::sep(settings_menu);
    B::add(settings_menu, ID_ENC_SETTINGS, "Encryption...");
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
    B::sep(tools);
    B::add(tools, ID_PASSWORDS, "Passwords (Bitwarden)...");
    B::add(tools, ID_HTTP_SEND, "Send HTTP Request\tCtrl+Enter (.http / .rest files)");

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
    let menus = Menus { file, open_folder, close_multi, print, recent, edit, sym, search, on_sel: on_selection, engine, view, enc, eol: eol_sub, lang, window, help };
    fill_file_menu(&menus, &[], false);
    (bar, menus)
}

