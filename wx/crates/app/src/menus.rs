//! Menus. The structure is defined once as a tree of plain data (`B::popup` / `B::add` / ...), the
//! same calls the Win32 shell uses, and then turned into wxWidgets menus by `realize`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;

use notepp_core::commands::{
    ID_ABOUT, ID_AC_FUNC, ID_AC_HINT, ID_AC_PATH, ID_AC_WORD, ID_B_EOL_SPACE, ID_B_SPACE_TAB_ALL, ID_B_SPACE_TAB_LEAD, ID_B_TAB_SPACE, ID_B_TRIM_BOTH,
    ID_B_TRIM_LEAD, ID_B_UNNECESSARY, ID_B64_DEC, ID_B64_ENC, ID_BEGIN_END_COL, ID_BEGIN_END_SEL, ID_BM_CLEAR, ID_BM_COPY, ID_BM_CUT, ID_BM_DELETE,
    ID_BM_DELETE_UNMARKED, ID_BM_INVERT, ID_BM_NEXT, ID_BM_PASTE, ID_BM_PREV, ID_BM_TOGGLE, ID_BRACE_GOTO, ID_BRACE_SEL, ID_BROWSER_BASE, ID_C_LOWER,
    ID_C_UPPER, ID_CASE_INVERT, ID_CASE_PROPER, ID_CASE_PROPER_BLEND, ID_CASE_RANDOM, ID_CASE_SENTENCE, ID_CASE_SENTENCE_BLEND, ID_CHAR_PANEL, ID_CLEAR_RO,
    ID_CLEAR_STYLE_BASE, ID_CLIP_HISTORY, ID_CLONE_OTHER_VIEW, ID_CLOSE, ID_CLOSE_ALL, ID_CLOSE_LEFT, ID_CLOSE_OTHERS, ID_CLOSE_RIGHT, ID_CLOSE_UNCHANGED,
    ID_CMT_BLOCK, ID_CMT_LINE, ID_COLUMN_EDITOR, ID_COLUMN_MODE, ID_COMMENT, ID_COPY, ID_COPY_STYLED_BASE, ID_CP_ALLNAMES, ID_CP_ALLPATHS, ID_CP_DIR,
    ID_CP_FULLPATH, ID_CP_NAME, ID_CUT, ID_DECRYPT, ID_DELETE, ID_DISTRACTION, ID_DOC_LIST, ID_EDIT_CONTEXT, ID_ENC_SETTINGS, ID_ENCRYPT, ID_ENGINE_BASE,
    ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_EXIT, ID_EXPORT_THEME, ID_FIF, ID_FIND, ID_FIND_CHARS, ID_FINDNEXT, ID_FINDPREV, ID_FOCUS_OTHER, ID_FOLD_ALL,
    ID_FOLD_CUR, ID_FOLD_LEVEL_BASE, ID_FULLSCREEN, ID_FUNC_LIST, ID_GOTO, ID_HASH_BASE, ID_HIDE_LINES, ID_HTTP_SEND, ID_IMPORT_THEME, ID_INCR, ID_INDENT_DEC,
    ID_INDENT_INC, ID_INS_DATE_CUSTOM, ID_INS_DATE_LONG, ID_INS_DATE_SHORT, ID_JSON_MIN, ID_JSON_PRETTY, ID_JUMP_DOWN_BASE, ID_JUMP_UP_BASE, ID_L_COPY,
    ID_L_CUT, ID_L_DEDUP, ID_L_DEL, ID_L_DOWN, ID_L_DUP, ID_L_INS_ABOVE, ID_L_INS_BELOW, ID_L_JOIN, ID_L_NOEMPTY, ID_L_RANDOM, ID_L_REVERSE, ID_L_RM_BLANK,
    ID_L_RM_CONSEC, ID_L_SORT_ASC, ID_L_SORT_COMMA_ASC, ID_L_SORT_COMMA_DESC, ID_L_SORT_DESC, ID_L_SORT_DOT_ASC, ID_L_SORT_DOT_DESC, ID_L_SORT_INT_ASC,
    ID_L_SORT_INT_DESC, ID_L_SORT_NOCASE_ASC, ID_L_SORT_NOCASE_DESC, ID_L_SPLIT, ID_L_TRANSPOSE, ID_L_TRIM, ID_L_UP, ID_LOG_FOLLOW, ID_LTR, ID_MARK,
    ID_MD_TOOLBAR, ID_MINIMAP, ID_MONITOR, ID_MOVE_NEW_INSTANCE, ID_MOVE_OTHER_VIEW, ID_MOVE_TAB_BACK, ID_MOVE_TAB_FWD, ID_NEW, ID_NEW_WB, ID_NEXT_RESULT,
    ID_NEXT_TAB, ID_ON_TOP, ID_OPEN, ID_OPEN_ALL_RECENT, ID_OPEN_FOLDER_CMD, ID_OPEN_FOLDER_EXPLORER, ID_OPEN_FOLDER_WS, ID_OPEN_NEW_INSTANCE, ID_OPEN_VIEWER,
    ID_OPEN_WORKSPACE, ID_PAGE_SETUP, ID_PALETTE, ID_PASSWORDS, ID_PASTE, ID_POSTIT, ID_PREFS, ID_PRESET_BASE, ID_PREV_RESULT, ID_PREV_TAB, ID_PREVIEW,
    ID_PRINT, ID_PRINT_NOW, ID_PROJECT_BASE, ID_PS_COPY_BIN, ID_PS_CUT_BIN, ID_PS_HTML, ID_PS_PASTE_BIN, ID_PS_RTF, ID_QUICKOPEN, ID_READONLY, ID_RECENT_CLEAR,
    ID_RECYCLE, ID_REDO, ID_RELOAD, ID_REPLACE, ID_RESTORE_CLOSED, ID_RESULTS, ID_RTL, ID_SAVE, ID_SAVE_COPY, ID_SAVEALL, ID_SAVEAS, ID_SEL_FIND_NEXT,
    ID_SEL_FIND_PREV, ID_SEL_FOLDER, ID_SEL_OPEN, ID_SEL_SEARCH, ID_SELALL, ID_SHORTCUTS, ID_STYLE_ALL_BASE, ID_STYLE_CFG, ID_STYLE_ONE_BASE, ID_SUMMARY,
    ID_SYM_ALL, ID_SYM_CTRL, ID_SYM_EOL, ID_SYM_INDENT, ID_SYM_NONPRINT, ID_SYM_WRAP, ID_SYM_WS, ID_SYNC_H, ID_SYNC_V, ID_TAB_BASE, ID_THEME, ID_UNCMT_BLOCK,
    ID_UNCMT_LINE, ID_UNDO, ID_UNFOLD_ALL, ID_UNFOLD_CUR, ID_UNFOLD_LEVEL_BASE, ID_UPDATE_AUTO, ID_UPDATE_CHECK, ID_URL_DEC, ID_URL_ENC, ID_VOL_NEXT,
    ID_VOL_PREV, ID_WINDOW_DOC_BASE, ID_WINDOW_SORT_BASE, ID_WINDOWS, ID_WORKSPACE_TOGGLE, ID_WRAP, ID_XML_MIN, ID_XML_PRETTY, ID_ZOOMIN, ID_ZOOMOUT,
    ID_ZOOMRESET,
};
use notepp_core::hash::ALGOS;
use notepp_core::{edit, settings};
use wxdragon::prelude::Menu;

use crate::menu_spec::{Entry as MenuEntry, MenuSpec};
use notepp_core::menu_provider::{CharsetGroups, ConvertToItems, EncodeInItems, GroupedMenuProvider, Languages, MenuProvider, RecentFiles};

use notepp_core::tables::{BROWSERS, SLOT_NAMES, WINDOW_DOCS_SHOWN};

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

/// Show the effective shortcuts of the keymap in the menus (call before building them).
pub fn set_keymap(keymap: &notepp_core::keymap::KeyMap) {
    let map = keymap.overrides().into_iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|id| (id, v))).collect();
    OVERRIDES.with(|o| *o.borrow_mut() = map);
}

/// Lay out a provider's items. Choosing one runs its command through `execute()`, like any other menu item.
fn add_items(m: M, provider: &dyn MenuProvider) {
    for item in provider.items() {
        B::add(m, item.command.id(), &item.label);
    }
}

/// Lay out a provider's groups as submenus.
fn add_groups(parent: M, provider: &dyn GroupedMenuProvider) {
    for group in provider.groups() {
        let sub = B::popup();
        for item in &group.items {
            B::add(sub, item.command.id(), &item.label);
        }
        B::submenu(parent, &group.title, sub);
    }
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
    /// Add a registered command: its text and shortcut come from the command registry.
    pub fn cmd(m: M, id: usize) {
        Self::cmd_state(m, id, true);
    }
    pub fn cmd_state(m: M, id: usize, enabled: bool) {
        let text = notepp_core::registry::spec(id).map_or_else(|| format!("Command {id}"), |s| s.menu_text(None));
        Self::add_state(m, id, &text, enabled);
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
        ID_WRAP
            | ID_THEME
            | ID_MINIMAP
            | ID_PREVIEW
            | ID_LOG_FOLLOW
            | ID_MD_TOOLBAR
            | ID_UPDATE_AUTO
            | ID_READONLY
            | ID_CHAR_PANEL
            | ID_CLIP_HISTORY
            | ID_DOC_LIST
            | ID_FUNC_LIST
            | ID_FULLSCREEN
            | ID_ON_TOP
            | ID_POSTIT
            | ID_DISTRACTION
            | ID_MONITOR
            | ID_WORKSPACE_TOGGLE
            | ID_EOL_CRLF
            | ID_EOL_LF
            | ID_EOL_CR
    ) || notepp_core::registry::spec(id).is_some_and(|s| s.palette.starts_with("View: Show "))
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
    pub enc: M,
    pub eol: M,
    pub lang: M,
    pub window: M,
}

/// The Window menu: Sort By, Windows..., then the first nine open documents (the active one is ticked).
pub fn fill_window_menu(m: M, titles: &[String], active: usize) {
    B::clear(m);
    let sort = B::popup();
    for (i, (name, ..)) in crate::dialogs::SORTS.iter().enumerate() {
        B::add(sort, ID_WINDOW_SORT_BASE + i, name);
    }
    B::submenu(m, "&Sort By", sort);
    B::add(m, ID_WINDOWS, "&Windows...");
    if titles.is_empty() {
        return;
    }
    B::sep(m);
    for (i, title) in titles.iter().take(WINDOW_DOCS_SHOWN).enumerate() {
        let tick = if i == active { "  \u{2713}" } else { "" };
        B::add(m, ID_WINDOW_DOC_BASE + i, &format!("&{} {}{tick}", i + 1, title.replace('&', "&&")));
    }
}

pub fn eol_menu() -> M {
    let m = B::popup();
    B::cmd(m, ID_EOL_CRLF);
    B::cmd(m, ID_EOL_LF);
    B::cmd(m, ID_EOL_CR);
    m
}

/// Fill the File menu in Notepad++'s order. Run again whenever the recent list
/// changes; `RemoveMenu` (not `DeleteMenu`) keeps the submenus alive for reuse.
pub fn fill_file_menu(m: &Menus, recent: &[PathBuf], can_restore: bool) {
    B::clear(m.file);
    let f = m.file;
    B::cmd(f, ID_NEW);
    B::cmd(f, ID_NEW_WB);
    B::cmd(f, ID_OPEN);
    B::submenu(f, "Open Containing &Folder", m.open_folder); // keep FILE_OPEN_FOLDER_POS in step
    B::cmd(f, ID_OPEN_VIEWER);
    B::cmd(f, ID_OPEN_WORKSPACE);
    B::cmd(f, ID_RELOAD);
    B::cmd(f, ID_SAVE);
    B::cmd(f, ID_SAVEAS);
    B::cmd(f, ID_SAVE_COPY);
    B::cmd(f, ID_SAVEALL);
    B::cmd(f, ID_CLOSE);
    B::cmd(f, ID_CLOSE_ALL);
    B::submenu(f, "Close M&ultiple Documents", m.close_multi);
    B::cmd(f, ID_RECYCLE);
    B::sep(f);
    // Ctrl+P is Quick Open in Note++, so Print has no shortcut.
    B::submenu(f, "&Print", m.print);
    B::sep(f);
    B::cmd(f, ID_ENCRYPT);
    B::cmd(f, ID_DECRYPT);
    B::sep(f);
    B::clear(m.recent);
    let r = m.recent;
    if recent.is_empty() {
        B::add_state(r, 0, "(no recent files)", false);
    }
    add_items(r, &RecentFiles { files: recent });
    B::sep(r);
    B::add_state(r, ID_OPEN_ALL_RECENT, "Open All Recent Files", !recent.is_empty());
    B::add_state(r, ID_RECENT_CLEAR, "Empty Recent Files List", !recent.is_empty());
    B::submenu(f, "&Recent Files", m.recent);
    B::cmd_state(f, ID_RESTORE_CLOSED, can_restore);
    B::sep(f);
    B::cmd(f, ID_EXIT);
}

/// The File menu and the submenus `fill_file_menu` fills later.
fn menu_file() -> (M, M, M, M, M) {
    let file = B::popup();
    let open_folder = B::popup();
    B::cmd(open_folder, ID_OPEN_FOLDER_EXPLORER);
    B::cmd(open_folder, ID_OPEN_FOLDER_CMD);
    B::cmd(open_folder, ID_OPEN_FOLDER_WS);
    let close_multi = B::popup();
    B::cmd(close_multi, ID_CLOSE_OTHERS);
    B::cmd(close_multi, ID_CLOSE_LEFT);
    B::cmd(close_multi, ID_CLOSE_RIGHT);
    B::cmd(close_multi, ID_CLOSE_UNCHANGED);
    let print = B::popup();
    B::cmd(print, ID_PRINT);
    B::cmd(print, ID_PRINT_NOW);
    B::cmd(print, ID_PAGE_SETUP);
    let recent = B::popup();
    (file, open_folder, close_multi, print, recent)
}

/// The Edit menu: clipboard, insert, indent and case operations.
fn menu_edit_basic() -> M {
    let edit = B::popup();
    B::cmd(edit, ID_UNDO);
    B::cmd(edit, ID_REDO);
    B::sep(edit);
    B::cmd(edit, ID_CUT);
    B::cmd(edit, ID_COPY);
    B::cmd(edit, ID_PASTE);
    B::cmd(edit, ID_DELETE);
    B::cmd(edit, ID_SELALL);
    B::cmd(edit, ID_BEGIN_END_SEL);
    B::cmd(edit, ID_BEGIN_END_COL);
    B::sep(edit);

    let insert = B::popup();
    B::cmd(insert, ID_INS_DATE_SHORT);
    B::cmd(insert, ID_INS_DATE_LONG);
    B::cmd(insert, ID_INS_DATE_CUSTOM);
    B::submenu(edit, "In&sert", insert);

    let to_clip = B::popup();
    B::cmd(to_clip, ID_CP_FULLPATH);
    B::cmd(to_clip, ID_CP_NAME);
    B::cmd(to_clip, ID_CP_DIR);
    B::sep(to_clip);
    B::cmd(to_clip, ID_CP_ALLNAMES);
    B::cmd(to_clip, ID_CP_ALLPATHS);
    B::submenu(edit, "Copy to Clip&board", to_clip);

    let indent = B::popup();
    B::cmd(indent, ID_INDENT_INC);
    B::cmd(indent, ID_INDENT_DEC);
    B::submenu(edit, "&Indent", indent);

    let case = B::popup();
    B::cmd(case, ID_C_UPPER);
    B::cmd(case, ID_C_LOWER);
    B::sep(case);
    B::cmd(case, ID_CASE_PROPER);
    B::cmd(case, ID_CASE_PROPER_BLEND);
    B::cmd(case, ID_CASE_SENTENCE);
    B::cmd(case, ID_CASE_SENTENCE_BLEND);
    B::cmd(case, ID_CASE_INVERT);
    B::cmd(case, ID_CASE_RANDOM);
    B::submenu(edit, "Convert Case &to", case);
    edit
}

/// Edit menu: line, comment, completion and EOL operations. Returns the EOL submenu.
fn menu_edit_lines(edit: M) -> M {
    let lines = B::popup();
    B::cmd(lines, ID_L_DUP);
    B::cmd(lines, ID_L_DEDUP);
    B::cmd(lines, ID_L_RM_CONSEC);
    B::cmd(lines, ID_L_SPLIT);
    B::cmd(lines, ID_L_JOIN);
    B::cmd(lines, ID_L_UP);
    B::cmd(lines, ID_L_DOWN);
    B::cmd(lines, ID_L_TRANSPOSE);
    B::cmd(lines, ID_L_NOEMPTY);
    B::cmd(lines, ID_L_RM_BLANK);
    B::cmd(lines, ID_L_INS_ABOVE);
    B::cmd(lines, ID_L_INS_BELOW);
    B::cmd(lines, ID_L_REVERSE);
    B::cmd(lines, ID_L_RANDOM);
    B::sep(lines);
    B::cmd(lines, ID_L_CUT);
    B::cmd(lines, ID_L_COPY);
    B::cmd(lines, ID_L_DEL);
    B::sep(lines);
    B::cmd(lines, ID_L_SORT_ASC);
    B::cmd(lines, ID_L_SORT_DESC);
    B::cmd(lines, ID_L_SORT_NOCASE_ASC);
    B::cmd(lines, ID_L_SORT_NOCASE_DESC);
    B::cmd(lines, ID_L_SORT_INT_ASC);
    B::cmd(lines, ID_L_SORT_INT_DESC);
    B::cmd(lines, ID_L_SORT_COMMA_ASC);
    B::cmd(lines, ID_L_SORT_COMMA_DESC);
    B::cmd(lines, ID_L_SORT_DOT_ASC);
    B::cmd(lines, ID_L_SORT_DOT_DESC);
    B::submenu(edit, "&Line Operations", lines);

    let comment = B::popup();
    B::cmd(comment, ID_COMMENT);
    B::cmd(comment, ID_CMT_LINE);
    B::cmd(comment, ID_UNCMT_LINE);
    B::cmd(comment, ID_CMT_BLOCK);
    B::cmd(comment, ID_UNCMT_BLOCK);
    B::submenu(edit, "Comment/Uncomment", comment);

    let complete = B::popup();
    B::cmd(complete, ID_AC_FUNC);
    B::cmd(complete, ID_AC_WORD);
    B::cmd(complete, ID_AC_HINT);
    B::cmd(complete, ID_AC_PATH);
    B::submenu(edit, "Auto-Completion", complete);

    let eol_sub = eol_menu();
    B::submenu(edit, "EOL Conversion", eol_sub);

    let blank = B::popup();
    B::cmd(blank, ID_L_TRIM);
    B::cmd(blank, ID_B_TRIM_LEAD);
    B::cmd(blank, ID_B_TRIM_BOTH);
    B::cmd(blank, ID_B_EOL_SPACE);
    B::cmd(blank, ID_B_UNNECESSARY);
    B::sep(blank);
    B::cmd(blank, ID_B_TAB_SPACE);
    B::cmd(blank, ID_B_SPACE_TAB_ALL);
    B::cmd(blank, ID_B_SPACE_TAB_LEAD);
    B::submenu(edit, "Blank Operations", blank);
    eol_sub
}

/// Edit menu: paste special, on-selection and read-only.
fn menu_edit_rest(edit: M) {
    let paste_special = B::popup();
    B::cmd(paste_special, ID_PS_HTML);
    B::cmd(paste_special, ID_PS_RTF);
    B::sep(paste_special);
    B::cmd(paste_special, ID_PS_COPY_BIN);
    B::cmd(paste_special, ID_PS_CUT_BIN);
    B::cmd(paste_special, ID_PS_PASTE_BIN);
    B::submenu(edit, "Paste Special", paste_special);

    let on_selection = B::popup();
    B::cmd(on_selection, ID_SEL_OPEN);
    B::cmd(on_selection, ID_SEL_FOLDER);
    B::cmd(on_selection, ID_SEL_SEARCH);
    let engine = B::popup();
    for (i, (name, _)) in edit::SEARCH_ENGINES.iter().enumerate() {
        B::add(engine, ID_ENGINE_BASE + i, name);
    }
    B::submenu(on_selection, "Change Search Engine", engine);
    B::submenu(edit, "On Selection", on_selection);
    B::sep(edit);

    B::cmd(edit, ID_COLUMN_MODE);
    B::cmd(edit, ID_COLUMN_EDITOR);
    B::cmd(edit, ID_CHAR_PANEL);
    B::cmd(edit, ID_CLIP_HISTORY);
    B::sep(edit);
    B::cmd(edit, ID_READONLY);
    B::cmd(edit, ID_CLEAR_RO);
}

/// The Search menu.
fn menu_search() -> M {
    let search = B::popup();
    B::cmd(search, ID_FIND);
    B::cmd(search, ID_FIF);
    B::cmd(search, ID_FINDNEXT);
    B::cmd(search, ID_FINDPREV);
    B::cmd(search, ID_SEL_FIND_NEXT);
    B::cmd(search, ID_SEL_FIND_PREV);
    B::cmd(search, ID_VOL_NEXT);
    B::cmd(search, ID_VOL_PREV);
    B::cmd(search, ID_REPLACE);
    B::cmd(search, ID_INCR);
    B::cmd(search, ID_RESULTS);
    B::cmd(search, ID_NEXT_RESULT);
    B::cmd(search, ID_PREV_RESULT);
    B::cmd(search, ID_GOTO);
    B::cmd(search, ID_BRACE_GOTO);
    B::cmd(search, ID_BRACE_SEL);
    B::cmd(search, ID_MARK);
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
    B::cmd(bm, ID_BM_TOGGLE);
    B::cmd(bm, ID_BM_NEXT);
    B::cmd(bm, ID_BM_PREV);
    B::cmd(bm, ID_BM_CLEAR);
    B::cmd(bm, ID_BM_CUT);
    B::cmd(bm, ID_BM_COPY);
    B::cmd(bm, ID_BM_PASTE);
    B::cmd(bm, ID_BM_DELETE);
    B::cmd(bm, ID_BM_DELETE_UNMARKED);
    B::cmd(bm, ID_BM_INVERT);
    B::submenu(search, "&Bookmark", bm);
    B::sep(search);
    B::cmd(search, ID_FIND_CHARS);
    search
}

/// The View menu: modes, symbols, zoom and the other-view commands.
fn menu_view_basic() -> M {
    let view = B::popup();
    B::cmd(view, ID_ON_TOP);
    B::cmd(view, ID_FULLSCREEN);
    B::cmd(view, ID_POSTIT);
    B::cmd(view, ID_DISTRACTION);
    B::sep(view);
    let browsers = B::popup();
    for (i, (name, _)) in BROWSERS.iter().enumerate() {
        B::add(browsers, ID_BROWSER_BASE + i, name);
    }
    B::submenu(view, "View Current File in", browsers);
    B::sep(view);
    let sym = B::popup();
    B::cmd(sym, ID_SYM_WS);
    B::cmd(sym, ID_SYM_EOL);
    B::cmd(sym, ID_SYM_NONPRINT);
    B::cmd(sym, ID_SYM_CTRL);
    B::cmd(sym, ID_SYM_ALL);
    B::sep(sym);
    B::cmd(sym, ID_SYM_INDENT);
    B::cmd(sym, ID_SYM_WRAP);
    B::submenu(view, "Show Symbol", sym);
    let zoom = B::popup();
    B::cmd(zoom, ID_ZOOMIN);
    B::cmd(zoom, ID_ZOOMOUT);
    B::cmd(zoom, ID_ZOOMRESET);
    B::submenu(view, "Zoom", zoom);
    let moveclone = B::popup();
    B::cmd_state(moveclone, ID_MOVE_OTHER_VIEW, false);
    B::cmd_state(moveclone, ID_CLONE_OTHER_VIEW, false);
    B::cmd(moveclone, ID_MOVE_NEW_INSTANCE);
    B::cmd(moveclone, ID_OPEN_NEW_INSTANCE);
    B::submenu(view, "Move/Clone Current Document", moveclone);
    view
}

/// View menu: tabs, folding, projects, panels and the Note++ extras.
fn menu_view_rest(view: M) {
    let tab = B::popup();
    for i in 0..9 {
        B::add(tab, ID_TAB_BASE + i, &format!("Tab {}", i + 1));
    }
    B::sep(tab);
    B::cmd(tab, ID_NEXT_TAB);
    B::cmd(tab, ID_PREV_TAB);
    B::cmd(tab, ID_MOVE_TAB_FWD);
    B::cmd(tab, ID_MOVE_TAB_BACK);
    B::submenu(view, "Tab", tab);
    B::cmd(view, ID_WRAP);
    B::cmd_state(view, ID_FOCUS_OTHER, false);
    B::cmd(view, ID_HIDE_LINES);
    B::sep(view);
    B::cmd(view, ID_FOLD_ALL);
    B::cmd(view, ID_UNFOLD_ALL);
    B::cmd(view, ID_FOLD_CUR);
    B::cmd(view, ID_UNFOLD_CUR);
    let fold_level = B::popup();
    let unfold_level = B::popup();
    for i in 0..8 {
        B::add(fold_level, ID_FOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+{}", i + 1, i + 1));
        B::add(unfold_level, ID_UNFOLD_LEVEL_BASE + i, &format!("Level {}\tAlt+Shift+{}", i + 1, i + 1));
    }
    B::submenu(view, "Fold Level", fold_level);
    B::submenu(view, "Unfold Level", unfold_level);
    B::sep(view);
    B::cmd(view, ID_SUMMARY);
    B::sep(view);
    let projects = B::popup();
    for i in 0..3 {
        B::add_state(projects, ID_PROJECT_BASE + i, &format!("Project Panel {}", i + 1), false);
    }
    B::submenu(view, "Project Panels", projects);
    B::cmd(view, ID_WORKSPACE_TOGGLE);
    B::cmd(view, ID_MINIMAP);
    B::cmd(view, ID_DOC_LIST);
    B::cmd(view, ID_FUNC_LIST);
    B::sep(view);
    B::cmd_state(view, ID_SYNC_V, false);
    B::cmd_state(view, ID_SYNC_H, false);
    B::sep(view);
    B::cmd(view, ID_RTL);
    B::cmd(view, ID_LTR);
    B::sep(view);
    B::cmd(view, ID_MONITOR);
    B::cmd(view, ID_LOG_FOLLOW);
    B::sep(view);
    // Note++ extras that Notepad++ keeps elsewhere.
    B::cmd(view, ID_PREVIEW);
    B::cmd(view, ID_MD_TOOLBAR);
    B::cmd(view, ID_THEME);
    B::cmd(view, ID_PALETTE);
    B::cmd(view, ID_QUICKOPEN);
}

/// The Encoding and Language menus.
fn menu_encoding_language() -> (M, M) {
    let enc = B::popup();
    add_items(enc, &EncodeInItems);
    let charsets = B::popup();
    add_groups(charsets, &CharsetGroups);
    B::submenu(enc, "Character sets", charsets);
    B::sep(enc);
    add_items(enc, &ConvertToItems);

    // Languages: "Normal Text", then one submenu per initial letter.
    let lang = B::popup();
    add_items(lang, &Languages);
    add_groups(lang, &Languages);
    (enc, lang)
}

/// The ? menu.
fn menu_help() -> M {
    let help = B::popup();
    B::cmd(help, ID_ABOUT);
    B::sep(help);
    B::cmd(help, ID_UPDATE_CHECK);
    B::cmd(help, ID_UPDATE_AUTO);
    help
}

/// The Settings menu.
fn menu_settings() -> M {
    let settings_menu = B::popup();
    B::cmd(settings_menu, ID_PREFS);
    B::cmd(settings_menu, ID_STYLE_CFG);
    B::cmd(settings_menu, ID_SHORTCUTS);
    B::sep(settings_menu);
    let themes = B::popup();
    B::cmd(themes, ID_PRESET_BASE);
    B::add(themes, ID_PRESET_BASE + 1, "Default (dark)");
    B::sep(themes);
    for (i, p) in settings::PRESETS.iter().enumerate() {
        B::add(themes, ID_PRESET_BASE + 2 + i, p.name);
    }
    B::submenu(settings_menu, "Themes", themes);
    let import = B::popup();
    B::cmd(import, ID_IMPORT_THEME);
    B::cmd(import, ID_EXPORT_THEME);
    B::submenu(settings_menu, "Import", import);
    B::sep(settings_menu);
    B::cmd(settings_menu, ID_ENC_SETTINGS);
    B::cmd(settings_menu, ID_EDIT_CONTEXT);
    settings_menu
}

/// The Tools menu: one submenu per digest, then the text tools.
fn menu_tools() -> M {
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
    B::cmd(text_tools, ID_B64_ENC);
    B::cmd(text_tools, ID_B64_DEC);
    B::sep(text_tools);
    B::cmd(text_tools, ID_URL_ENC);
    B::cmd(text_tools, ID_URL_DEC);
    B::sep(text_tools);
    B::cmd(text_tools, ID_JSON_PRETTY);
    B::cmd(text_tools, ID_JSON_MIN);
    B::cmd(text_tools, ID_XML_PRETTY);
    B::cmd(text_tools, ID_XML_MIN);
    B::submenu(tools, "Text Tools", text_tools);
    B::sep(tools);
    B::cmd(tools, ID_PASSWORDS);
    B::cmd(tools, ID_HTTP_SEND);
    tools
}

pub fn build_menus() -> (Vec<(String, M)>, Menus) {
    let (file, open_folder, close_multi, print, recent) = menu_file();
    let edit = menu_edit_basic();
    let eol_sub = menu_edit_lines(edit);
    menu_edit_rest(edit);
    let search = menu_search();
    let view = menu_view_basic();
    menu_view_rest(view);
    let (enc, lang) = menu_encoding_language();
    let help = menu_help();
    let settings_menu = menu_settings();
    let tools = menu_tools();
    // Window: Sort By, Windows..., then the open documents (see `fill_window_menu`).
    let window = B::popup();
    fill_window_menu(window, &[], 0);
    let bar = vec![
        ("&File".to_string(), file),
        ("&Edit".to_string(), edit),
        ("&Search".to_string(), search),
        ("&View".to_string(), view),
        ("E&ncoding".to_string(), enc),
        ("&Language".to_string(), lang),
        ("Set&tings".to_string(), settings_menu),
        ("T&ools".to_string(), tools),
        ("&Window".to_string(), window),
        ("&?".to_string(), help),
    ];
    let menus = Menus { file, open_folder, close_multi, print, recent, enc, eol: eol_sub, lang, window };
    fill_file_menu(&menus, &[], false);
    (bar, menus)
}

#[cfg(test)]
mod golden {
    use super::*;

    fn dump(spec: &MenuSpec, depth: usize, out: &mut String) {
        for e in spec.entries() {
            let pad = "  ".repeat(depth);
            match e {
                MenuEntry::Item { id, label, enabled, check } => out.push_str(&format!("{pad}{id}\t{label}\t{enabled}\t{check:?}\n")),
                MenuEntry::Sep => out.push_str(&format!("{pad}-\n")),
                MenuEntry::Sub { label, menu } => {
                    out.push_str(&format!("{pad}> {label}\n"));
                    dump(menu, depth + 1, out);
                }
            }
        }
    }

    /// The whole menu bar as text. Menus now come from the command registry; this keeps them from changing by accident.
    /// To accept an intended change, delete `tests/menus.golden` and run the tests once.
    #[test]
    fn the_menu_bar_matches_its_snapshot() {
        let (bar, _) = build_menus();
        let mut text = String::new();
        for (title, m) in &bar {
            text.push_str(&format!("# {title}\n"));
            dump(&spec(*m), 0, &mut text);
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("menus.golden");
        if let Ok(old) = std::fs::read_to_string(&path) {
            assert_eq!(old.replace("\r\n", "\n"), text, "menus changed; delete {} to accept", path.display());
        } else {
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            std::fs::write(&path, &text).unwrap();
        }
    }

    fn label_of(spec: &MenuSpec, wanted: usize) -> Option<String> {
        spec.entries().iter().find_map(|e| match e {
            MenuEntry::Item { id, label, .. } if *id == wanted => Some(label.clone()),
            MenuEntry::Sub { menu, .. } => label_of(menu, wanted),
            _ => None,
        })
    }

    fn built_label(keymap: &notepp_core::keymap::KeyMap, id: usize) -> String {
        set_keymap(keymap);
        let (bar, _) = build_menus();
        bar.iter().find_map(|(_, m)| label_of(&spec(*m), id)).unwrap_or_default()
    }

    fn keymap_of(pairs: &[(usize, &str)]) -> notepp_core::keymap::KeyMap {
        notepp_core::keymap::KeyMap::build(&pairs.iter().map(|(id, t)| (id.to_string(), (*t).to_string())).collect())
    }

    #[test]
    fn menus_show_the_effective_shortcut_not_the_default() {
        use notepp_core::commands::{ID_OPEN, ID_SAVE};
        assert_eq!(built_label(&keymap_of(&[]), ID_SAVE), "&Save\tCtrl+S");
        assert_eq!(built_label(&keymap_of(&[(ID_SAVE, "Ctrl+Alt+K")]), ID_SAVE), "&Save\tCtrl+Alt+K");
        assert_eq!(built_label(&keymap_of(&[(ID_SAVE, "")]), ID_SAVE), "&Save");
        // Ctrl+O belongs to Open, so Save's request is refused and Save keeps its default in the menu.
        assert_eq!(built_label(&keymap_of(&[(ID_SAVE, "Ctrl+O")]), ID_SAVE), "&Save\tCtrl+S");
        assert!(built_label(&keymap_of(&[(ID_SAVE, "Ctrl+O")]), ID_OPEN).ends_with("Ctrl+O"));
    }
}
