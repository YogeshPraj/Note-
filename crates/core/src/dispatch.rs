//! Commands that only need the workspace: ported from the Win32 shell by a script, arm by arm.
//! Commands that touch windows, dialogs or panes are handled in app.rs.

use crate::bookmarks;
use crate::commands::{
    ID_AC_FUNC, ID_AC_HINT, ID_AC_PATH, ID_AC_WORD, ID_B_EOL_SPACE, ID_B_SPACE_TAB_ALL, ID_B_SPACE_TAB_LEAD, ID_B_TAB_SPACE, ID_B_TRIM_BOTH, ID_B_TRIM_LEAD,
    ID_B_UNNECESSARY, ID_B64_DEC, ID_B64_ENC, ID_BEGIN_END_COL, ID_BEGIN_END_SEL, ID_BM_CLEAR, ID_BM_DELETE, ID_BM_DELETE_UNMARKED, ID_BM_INVERT, ID_BM_NEXT,
    ID_BM_PREV, ID_BM_TOGGLE, ID_BRACE_GOTO, ID_BRACE_SEL, ID_C_LOWER, ID_C_UPPER, ID_CASE_INVERT, ID_CASE_PROPER, ID_CASE_PROPER_BLEND, ID_CASE_RANDOM,
    ID_CASE_SENTENCE, ID_CASE_SENTENCE_BLEND, ID_CLEAR_RO, ID_CLEAR_STYLE_BASE, ID_CLOSE, ID_CLOSE_ALL, ID_CLOSE_LEFT, ID_CLOSE_OTHERS, ID_CLOSE_RIGHT,
    ID_CLOSE_UNCHANGED, ID_CMT_BLOCK, ID_CMT_LINE, ID_COMMENT, ID_COPY, ID_CUT, ID_DECRYPT, ID_DELETE, ID_ENC_LOCK, ID_ENC_TOGGLE, ID_ENCRYPT, ID_ENGINE_BASE,
    ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_EXPORT_THEME, ID_FAV_TOGGLE, ID_FOLD_ALL, ID_FOLD_CUR, ID_FOLD_LEVEL_BASE, ID_HIDE_LINES, ID_INDENT_DEC,
    ID_INDENT_INC, ID_JSON_MIN, ID_JSON_PRETTY, ID_JUMP_DOWN_BASE, ID_JUMP_UP_BASE, ID_L_COPY, ID_L_CUT, ID_L_DEDUP, ID_L_DEL, ID_L_DOWN, ID_L_DUP,
    ID_L_INS_ABOVE, ID_L_INS_BELOW, ID_L_JOIN, ID_L_NOEMPTY, ID_L_RANDOM, ID_L_REVERSE, ID_L_RM_BLANK, ID_L_RM_CONSEC, ID_L_SORT_ASC, ID_L_SORT_COMMA_ASC,
    ID_L_SORT_COMMA_DESC, ID_L_SORT_DESC, ID_L_SORT_DOT_ASC, ID_L_SORT_DOT_DESC, ID_L_SORT_INT_ASC, ID_L_SORT_INT_DESC, ID_L_SORT_NOCASE_ASC,
    ID_L_SORT_NOCASE_DESC, ID_L_SPLIT, ID_L_TRANSPOSE, ID_L_TRIM, ID_L_UP, ID_MOVE_NEW_INSTANCE, ID_MOVE_TAB_BACK, ID_MOVE_TAB_FWD, ID_NEW, ID_NEW_WB,
    ID_NEXT_TAB, ID_OPEN, ID_OPEN_ALL_RECENT, ID_OPEN_NEW_INSTANCE, ID_PASTE, ID_PREV_TAB, ID_PW_LOCK, ID_READONLY, ID_RECENT_CLEAR, ID_RECYCLE, ID_REDO,
    ID_RENAME, ID_RESTORE_CLOSED, ID_SAVE, ID_SAVE_COPY, ID_SAVEALL, ID_SAVEAS, ID_SELALL, ID_STYLE_ALL_BASE, ID_STYLE_ONE_BASE, ID_SYM_ALL, ID_SYM_CTRL,
    ID_SYM_EOL, ID_SYM_INDENT, ID_SYM_NONPRINT, ID_SYM_WRAP, ID_SYM_WS, ID_TAB_BASE, ID_TAB_COLOR_BASE, ID_TAB_COLOR_NONE, ID_TAB_PIN, ID_THEME,
    ID_UNCMT_BLOCK, ID_UNCMT_LINE, ID_UNDO, ID_UNFOLD_ALL, ID_UNFOLD_CUR, ID_UNFOLD_LEVEL_BASE, ID_URL_DEC, ID_URL_ENC, ID_WINDOW_DOC_BASE, ID_WRAP,
    ID_XML_MIN, ID_XML_PRETTY, ID_ZOOMIN, ID_ZOOMOUT, ID_ZOOMRESET,
};
use crate::doc::{CHARSETS, Enc, Eol};
use crate::edit::{self, SortKind};
use crate::lang::LANGUAGES;
use crate::registry::Command;
use crate::sci::{self, Theme};
use crate::search::{self};

use crate::view;
use crate::workspace::{CaseOp, CloseOp, Completion, LineOp, TextTool, Ui, Workspace};

use crate::tables::{ENCODE_IN, SLOT_NAMES, WINDOW_DOCS_SHOWN, md_fmt, theme_json};

/// Run command `id` if it needs nothing but the workspace. False when the shell must handle it.
pub fn command_core<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
    if let Some(cmd) = Command::from_id(id)
        && run_family(ws, ui, flash, cmd)
    {
        return true;
    }
    command_core_1(ws, ui, flash, id)
        || command_core_2(ws, ui, flash, id)
        || command_core_3(ws, flash, id)
        || command_core_4(ws, flash, id)
        || command_core_5(ws, ui, flash, id)
        || command_core_6(ws, flash, id)
        || command_core_7(ws, flash, id)
        || command_core_8(ws, ui, flash, id)
}

fn command_core_1<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
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
                *flash = Some(format!("Saved a copy to {}", p.display()));
            }
        }
        ID_RENAME => {
            if ws.rename(ui) {
                *flash = Some("Renamed".into());
            }
        }
        ID_RECYCLE => drop(ws.move_to_recycle_bin(ui)),
        _ => return false,
    }
    true
}

fn command_core_2<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    match id {
        ID_FAV_TOGGLE => {
            let was = ws.active_is_favourite();
            *flash = Some(
                if !ws.toggle_favourite() {
                    "Save the file first to make it a favourite"
                } else if was {
                    "Removed from Favourites"
                } else {
                    "Added to Favourites"
                }
                .into(),
            );
        }
        ID_TAB_PIN => {
            let i = ws.active;
            ws.toggle_pin(i);
        }
        ID_TAB_COLOR_NONE => {
            let i = ws.active;
            ws.set_tab_color(i, None);
        }
        _ if (ID_TAB_COLOR_BASE..ID_TAB_COLOR_BASE + crate::workspace::TAB_COLORS.len()).contains(&id) => {
            let i = ws.active;
            ws.set_tab_color(i, Some(crate::workspace::TAB_COLORS[id - ID_TAB_COLOR_BASE].1));
        }
        ID_PW_LOCK => {
            crate::bitwarden::lock();
            *flash = Some("Passwords locked".into());
        }
        ID_ENC_LOCK => *flash = Some(ws.lock_encryption().map_or_else(|e| e.to_string(), |()| "Encryption locked".into())),
        ID_ENCRYPT => *flash = ws.encrypt_active(ui),
        ID_DECRYPT => *flash = ws.decrypt_active(ui),
        ID_ENC_TOGGLE => *flash = if ws.is_encrypted() { ws.decrypt_active(ui) } else { ws.encrypt_active(ui) },
        ID_RESTORE_CLOSED => {
            if !ws.restore_closed(ui) {
                *flash = Some("No recently closed files".into());
            }
        }
        ID_OPEN_ALL_RECENT => ws.open_all_recent(ui),
        ID_UNDO => drop(sci.send(sci::SCI_UNDO, 0, 0)),
        ID_REDO => drop(sci.send(sci::SCI_REDO, 0, 0)),
        ID_CUT => drop(sci.send(sci::SCI_CUT, 0, 0)),
        ID_COPY => drop(sci.send(sci::SCI_COPY, 0, 0)),
        ID_PASTE => drop(sci.send(sci::SCI_PASTE, 0, 0)),
        ID_SELALL => drop(sci.send(sci::SCI_SELECTALL, 0, 0)),
        ID_EOL_CRLF => *flash = ws.set_eol(Eol::Crlf),
        ID_EOL_LF => *flash = ws.set_eol(Eol::Lf),
        _ => return false,
    }
    true
}

mod editing;

use editing::*;
mod view_cmds;

use view_cmds::*;
/// The family commands carry their index, so nothing here does id arithmetic.
fn run_family<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, cmd: Command) -> bool {
    match cmd {
        Command::OpenFavourite(i) => {
            let mut gone = None;
            ws.open_favourite(ui, i, &mut gone);
            if let Some(p) = gone {
                *flash = Some(format!("{} no longer exists and was removed from Favourites", p.display()));
            }
        }
        Command::OpenRecent(i) => {
            if let Some(p) = ws.recent.get(i).cloned() {
                if p.exists() {
                    ws.open_path(ui, &p);
                } else {
                    ws.prune_recent();
                    *flash = Some(format!("{} no longer exists", p.display()));
                }
            }
        }
        Command::EncodeIn(i) => {
            if let Some(enc) = ENCODE_IN.get(i) {
                ws.reload_as(ui, *enc);
            }
        }
        Command::ConvertTo(i) => {
            if let Some(enc) = ENCODE_IN.get(i) {
                *flash = ws.convert_to(ui, *enc);
            }
        }
        // The last charset index is Windows-1252, which is the app's ANSI.
        Command::SetCharset(i) => {
            ws.reload_as(ui, if i == CHARSETS.len() { Enc::Ansi } else { Enc::Cs(crate::num::to_u8(i)) });
        }
        Command::SetLanguage(i) => {
            if let Some((name, _)) = LANGUAGES.get(i) {
                ws.set_language(name);
            }
        }
        _ => return false,
    }
    true
}
