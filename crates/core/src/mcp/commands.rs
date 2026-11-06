//! Which of the registry's commands an agent may run, by the name the Command Palette shows.
//!
//! An allow list: a command added to the registry stays out of an agent's reach until someone has looked at it. Being here
//! means it opens no dialog, as a dialog would stop the window until the user answers. Left out on purpose: anything that
//! reads or fills the clipboard (it can hold a password), starts another program or reaches the network, touches files,
//! passwords or encryption, prints, ends the program, or opens a window of its own. Saving has its own tool.

use super::access::Level;
use crate::commands::*;
use crate::registry::{CommandSpec, SPECS};

/// What changes a document's text.
const TEXT: &[usize] = &[
    ID_UNDO,
    ID_REDO,
    ID_DELETE,
    ID_L_DUP,
    ID_L_DEL,
    ID_L_UP,
    ID_L_DOWN,
    ID_L_SORT_ASC,
    ID_L_SORT_DESC,
    ID_L_SORT_NOCASE_ASC,
    ID_L_SORT_NOCASE_DESC,
    ID_L_SORT_INT_ASC,
    ID_L_SORT_INT_DESC,
    ID_L_SORT_DOT_ASC,
    ID_L_SORT_DOT_DESC,
    ID_L_SORT_COMMA_ASC,
    ID_L_SORT_COMMA_DESC,
    ID_L_DEDUP,
    ID_L_RM_CONSEC,
    ID_L_NOEMPTY,
    ID_L_RM_BLANK,
    ID_L_TRIM,
    ID_L_SPLIT,
    ID_L_JOIN,
    ID_L_TRANSPOSE,
    ID_L_INS_ABOVE,
    ID_L_INS_BELOW,
    ID_L_REVERSE,
    ID_L_RANDOM,
    ID_C_UPPER,
    ID_C_LOWER,
    ID_CASE_PROPER,
    ID_CASE_PROPER_BLEND,
    ID_CASE_SENTENCE,
    ID_CASE_SENTENCE_BLEND,
    ID_CASE_INVERT,
    ID_CASE_RANDOM,
    ID_COMMENT,
    ID_CMT_LINE,
    ID_UNCMT_LINE,
    ID_CMT_BLOCK,
    ID_UNCMT_BLOCK,
    ID_INDENT_INC,
    ID_INDENT_DEC,
    ID_INS_DATE_SHORT,
    ID_INS_DATE_LONG,
    ID_B_TRIM_LEAD,
    ID_B_TRIM_BOTH,
    ID_B_EOL_SPACE,
    ID_B_UNNECESSARY,
    ID_B_TAB_SPACE,
    ID_B_SPACE_TAB_ALL,
    ID_B_SPACE_TAB_LEAD,
    ID_EOL_CRLF,
    ID_EOL_LF,
    ID_EOL_CR,
    ID_BM_DELETE,
    ID_BM_DELETE_UNMARKED,
    ID_B64_ENC,
    ID_B64_DEC,
    ID_URL_ENC,
    ID_URL_DEC,
    ID_JSON_PRETTY,
    ID_JSON_MIN,
    ID_XML_PRETTY,
    ID_XML_MIN,
    ID_MD_BOLD,
    ID_MD_ITALIC,
    ID_MD_STRIKE,
    ID_MD_CODE,
    ID_MD_H1,
    ID_MD_H2,
    ID_MD_H3,
    ID_MD_H4,
    ID_MD_H5,
    ID_MD_H6,
    ID_MD_QUOTE,
    ID_MD_BULLET,
    ID_MD_NUMBERED,
    ID_MD_TASK,
    ID_MD_LINK,
    ID_MD_IMAGE,
    ID_MD_TABLE,
    ID_MD_CODEBLOCK,
    ID_MD_RULE,
];

/// What goes with changing text: the selection and the bookmarks those commands work from.
const MARKS: &[usize] = &[ID_SELALL, ID_BRACE_GOTO, ID_BRACE_SEL, ID_BM_TOGGLE, ID_BM_NEXT, ID_BM_PREV, ID_BM_CLEAR, ID_BM_INVERT];

/// What changes the window rather than a document.
const WINDOW: &[usize] = &[
    ID_WRAP,
    ID_ON_TOP,
    ID_FULLSCREEN,
    ID_POSTIT,
    ID_DISTRACTION,
    ID_SYM_WS,
    ID_SYM_EOL,
    ID_SYM_ALL,
    ID_SYM_INDENT,
    ID_SYM_WRAP,
    ID_HIDE_LINES,
    ID_FOLD_ALL,
    ID_UNFOLD_ALL,
    ID_FOLD_CUR,
    ID_UNFOLD_CUR,
    ID_DOC_LIST,
    ID_FUNC_LIST,
    ID_RTL,
    ID_LTR,
    ID_MOVE_TAB_FWD,
    ID_MOVE_TAB_BACK,
    ID_NEXT_TAB,
    ID_PREV_TAB,
    ID_TAB_PIN,
    ID_TAB_COLOR_NONE,
    ID_PREVIEW,
    ID_MD_TOOLBAR,
    ID_THEME,
    ID_MINIMAP,
    ID_ZOOMIN,
    ID_ZOOMOUT,
    ID_ZOOMRESET,
    ID_NEW_WB,
    ID_CLOSE,
    ID_CLOSE_UNCHANGED,
];

/// The level `id` needs, or None when agents cannot run it.
pub fn level_of(id: usize) -> Option<Level> {
    if TEXT.contains(&id) || MARKS.contains(&id) {
        Some(Level::Edit)
    } else if WINDOW.contains(&id) {
        Some(Level::Full)
    } else {
        None
    }
}

/// Does `id` change the active document's text? Those need a text tab that an agent may change.
pub fn changes_text(id: usize) -> bool {
    TEXT.contains(&id)
}

/// Should `id` run as one undo step? All text commands but the two that walk the undo history itself.
pub fn one_undo_step(id: usize) -> bool {
    changes_text(id) && id != ID_UNDO && id != ID_REDO
}

/// The commands agents can run, in the palette's order.
pub fn allowed() -> impl Iterator<Item = (&'static CommandSpec, Level)> {
    SPECS.iter().filter_map(|spec| level_of(spec.id).map(|level| (spec, level)))
}

/// The command agents know as `name`. Case does not matter.
pub fn find(name: &str) -> Option<(&'static CommandSpec, Level)> {
    allowed().find(|(spec, _)| spec.palette.eq_ignore_ascii_case(name.trim()))
}

/// What `list_commands` answers: each command and the level it needs.
pub fn listing() -> serde_json::Value {
    let rows: Vec<serde_json::Value> = allowed().map(|(spec, level)| serde_json::json!({ "name": spec.palette, "needs": level.key() })).collect();
    serde_json::json!({ "commands": rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn every_id() -> Vec<usize> {
        [TEXT, MARKS, WINDOW].concat()
    }

    #[test]
    fn every_listed_command_exists_once_with_a_name() {
        let ids = every_id();
        for id in &ids {
            let found: Vec<_> = SPECS.iter().filter(|s| s.id == *id).collect();
            assert_eq!(found.len(), 1, "id {id}");
            assert!(!found[0].palette.is_empty(), "id {id} has no name an agent could use");
            assert_eq!(ids.iter().filter(|i| *i == id).count(), 1, "id {id} is listed twice");
        }
        assert_eq!(allowed().count(), ids.len());
    }

    #[test]
    fn nothing_that_announces_a_dialog_is_listed() {
        for (spec, _) in allowed() {
            assert!(!spec.palette.ends_with("...") && !spec.menu.ends_with("..."), "{}", spec.palette);
        }
    }

    #[test]
    fn what_must_stay_out_is_out() {
        let never = [
            ID_CUT,
            ID_COPY,
            ID_PASTE,
            ID_L_CUT,
            ID_L_COPY,
            ID_BM_CUT,
            ID_BM_COPY,
            ID_BM_PASTE,
            ID_PS_PASTE_BIN,
            ID_CP_FULLPATH,
            ID_TAB_CMP_CLIP,
            ID_CLIP_HISTORY,
            ID_OPEN_FOLDER_EXPLORER,
            ID_OPEN_FOLDER_CMD,
            ID_OPEN_VIEWER,
            ID_SEL_OPEN,
            ID_SEL_FOLDER,
            ID_SEL_SEARCH,
            ID_OPEN_NEW_INSTANCE,
            ID_MOVE_NEW_INSTANCE,
            ID_HTTP_SEND,
            ID_REPORT_ISSUE,
            ID_UPDATE_CHECK,
            ID_UPDATE_AUTO,
            ID_RECYCLE,
            ID_RELOAD,
            ID_SAVE,
            ID_SAVEALL,
            ID_CLOSE_ALL,
            ID_CLOSE_OTHERS,
            ID_RESTORE_CLOSED,
            ID_RECENT_CLEAR,
            ID_READONLY,
            ID_CLEAR_RO,
            ID_ENCRYPT,
            ID_DECRYPT,
            ID_ENC_LOCK,
            ID_ENC_SETTINGS,
            ID_PASSWORDS,
            ID_PW_LOCK,
            ID_PRINT_NOW,
            ID_EXIT,
            ID_PREFS,
            ID_ABOUT,
            ID_MCP_STOP,
            ID_MCP_LOG,
        ];
        for id in never {
            assert_eq!(level_of(id), None, "id {id}");
        }
    }

    #[test]
    fn commands_are_found_by_their_palette_name() {
        assert_eq!(find("Edit: Sort Lines Ascending").map(|(s, l)| (s.id, l)), Some((ID_L_SORT_ASC, Level::Edit)));
        assert_eq!(find("  edit: undo ").map(|(s, _)| s.id), Some(ID_UNDO));
        assert_eq!(find("View: Toggle Word Wrap").map(|(_, l)| l), Some(Level::Full));
        assert!(find("Edit: Paste").is_none());
        assert!(find("").is_none(), "commands with no palette name must not match");
    }

    #[test]
    fn undo_is_not_wrapped_in_an_undo_step() {
        assert!(one_undo_step(ID_L_SORT_ASC));
        assert!(!one_undo_step(ID_UNDO) && !one_undo_step(ID_REDO) && !one_undo_step(ID_SELALL));
    }
}
