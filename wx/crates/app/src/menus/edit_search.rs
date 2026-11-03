//! The Edit and Search menus.

use super::*;

/// The Edit menu: clipboard, insert, indent and case operations.
pub(super) fn menu_edit_basic() -> M {
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
pub(super) fn menu_edit_lines(edit: M) -> M {
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
pub(super) fn menu_edit_rest(edit: M) {
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
pub(super) fn menu_search() -> M {
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
