//! Commands that only need the workspace: ported from the Win32 shell by a script, arm by arm.
//! Commands that touch windows, dialogs or panes are handled in app.rs.

#![allow(unused_imports, unused_variables, unused_mut)]
use crate::bookmarks;
use crate::registry::Command;
use crate::commands::{ID_NEW_WB, ID_NEW, ID_OPEN, ID_SAVE, ID_SAVEAS, ID_SAVEALL, ID_CLOSE, ID_CLOSE_ALL, ID_CLOSE_OTHERS, ID_CLOSE_LEFT, ID_CLOSE_RIGHT, ID_CLOSE_UNCHANGED, ID_SAVE_COPY, ID_RENAME, ID_RECYCLE, ID_LOAD_SESSION, ID_SAVE_SESSION, ID_TAB_PIN, ID_TAB_COLOR_NONE, ID_TAB_COLOR_BASE, ID_PW_LOCK, ID_ENC_LOCK, ID_ENCRYPT, ID_DECRYPT, ID_ENC_TOGGLE, ID_RESTORE_CLOSED, ID_OPEN_ALL_RECENT, ID_UNDO, ID_REDO, ID_CUT, ID_COPY, ID_PASTE, ID_SELALL, ID_EOL_CRLF, ID_EOL_LF, ID_EOL_CR, ID_WRAP, ID_THEME, ID_ZOOMIN, ID_ZOOMOUT, ID_ZOOMRESET, ID_L_DUP, ID_L_DEL, ID_L_UP, ID_L_DOWN, ID_C_UPPER, ID_C_LOWER, ID_L_SORT_ASC, ID_L_SORT_DESC, ID_L_SORT_NOCASE_ASC, ID_L_SORT_NOCASE_DESC, ID_L_SORT_INT_ASC, ID_L_SORT_INT_DESC, ID_L_SORT_DOT_ASC, ID_L_SORT_DOT_DESC, ID_L_SORT_COMMA_ASC, ID_L_SORT_COMMA_DESC, ID_L_DEDUP, ID_L_RM_CONSEC, ID_L_NOEMPTY, ID_L_RM_BLANK, ID_L_REVERSE, ID_L_RANDOM, ID_L_JOIN, ID_L_SPLIT, ID_L_TRIM, ID_B_TRIM_LEAD, ID_B_TRIM_BOTH, ID_B_EOL_SPACE, ID_B_UNNECESSARY, ID_B_TAB_SPACE, ID_B_SPACE_TAB_ALL, ID_B_SPACE_TAB_LEAD, ID_COMMENT, ID_CMT_LINE, ID_UNCMT_LINE, ID_CMT_BLOCK, ID_UNCMT_BLOCK, ID_L_INS_ABOVE, ID_L_INS_BELOW, ID_L_CUT, ID_L_COPY, ID_L_TRANSPOSE, ID_CASE_PROPER, ID_CASE_PROPER_BLEND, ID_CASE_SENTENCE, ID_CASE_SENTENCE_BLEND, ID_CASE_INVERT, ID_CASE_RANDOM, ID_DELETE, ID_BEGIN_END_SEL, ID_BEGIN_END_COL, ID_INDENT_INC, ID_INDENT_DEC, ID_AC_FUNC, ID_AC_WORD, ID_AC_HINT, ID_AC_PATH, ID_ENGINE_BASE, ID_SYM_WS, ID_SYM_EOL, ID_SYM_NONPRINT, ID_SYM_CTRL, ID_SYM_ALL, ID_SYM_INDENT, ID_SYM_WRAP, ID_OPEN_NEW_INSTANCE, ID_MOVE_NEW_INSTANCE, ID_TAB_BASE, ID_MOVE_TAB_FWD, ID_MOVE_TAB_BACK, ID_HIDE_LINES, ID_FOLD_ALL, ID_UNFOLD_ALL, ID_FOLD_CUR, ID_UNFOLD_CUR, ID_FOLD_LEVEL_BASE, ID_UNFOLD_LEVEL_BASE, ID_READONLY, ID_CLEAR_RO, ID_BM_TOGGLE, ID_BM_NEXT, ID_BM_PREV, ID_BM_CLEAR, ID_RECENT_CLEAR, ID_RECENT_BASE, ID_NEXT_TAB, ID_PREV_TAB, ID_BRACE_GOTO, ID_BRACE_SEL, ID_STYLE_ALL_BASE, ID_STYLE_ONE_BASE, ID_CLEAR_STYLE_BASE, ID_JUMP_UP_BASE, ID_JUMP_DOWN_BASE, ID_BM_DELETE, ID_BM_DELETE_UNMARKED, ID_BM_INVERT, ID_EXPORT_THEME, ID_B64_ENC, ID_B64_DEC, ID_URL_ENC, ID_URL_DEC, ID_JSON_PRETTY, ID_JSON_MIN, ID_XML_PRETTY, ID_XML_MIN, ID_WINDOW_DOC_BASE, ID_ENCODE_BASE, ID_CHARSET_BASE, ID_CONVERT_BASE, ID_LANG_BASE};
use crate::doc::{CHARSETS, Enc, Eol};
use crate::edit::{self, ColumnNumbers, NumFormat, SortKind};
use crate::files;
use crate::functions;
use crate::hash::{ALGOS, Algo};
use crate::lang::LANGUAGES;
use crate::sci::{self, Send as _, Theme};
use crate::search::{self, Hit};
use crate::session::{Session, config_dir, session_path};
use crate::settings::{self, Settings};
use crate::workspace::{CaseOp, CloseOp, Completion, Confirm, LineOp, PathText, TextTool, Ui, Workspace};
use crate::{jsonpath, mdformat, shortcuts, textools, update, view, whiteboard};

use crate::tables::{md_fmt, SLOT_NAMES, theme_json, WINDOW_DOCS_SHOWN, ENCODE_IN};

/// Run command `id` if it needs nothing but the workspace. False when the shell must handle it.
pub fn command_core<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    if let Some(cmd) = Command::from_id(id)
        && run_family(ws, ui, flash, cmd)
    {
        return true;
    }
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
            ID_LOAD_SESSION => {
                if let Some(p) = ui.pick_session(false) {
                    match Session::load(&p) {
                        Some(s) => {
                            let n = ws.open_session(ui, &s);
                            *flash = Some(format!("Opened {n} tab(s) from the session"));
                        }
                        None => ui.error("That file is not a valid Note++ session."),
                    }
                }
            }
            ID_SAVE_SESSION => {
                if let Some(p) = ui.pick_session(true) {
                    match ws.snapshot().save(&p) {
                        Ok(()) => *flash = Some(format!("Session saved to {}", p.display())),
                        Err(e) => ui.error(&format!("Cannot save the session\n\n{e}")),
                    }
                }
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
                ws.set_tab_color(i, Some(crate::num::to_u8(id - ID_TAB_COLOR_BASE)));
            }
            ID_PW_LOCK => {
                crate::bitwarden::lock();
                *flash = Some("Passwords locked".into());
            }
            ID_ENC_LOCK => {
                ws.vault.lock();
                *flash = Some("Encryption locked".into());
            }
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
            _ if md_fmt(id).is_some() => {
                if let Some(f) = md_fmt(id) {
                    *flash = ws.md_format(f);
                }
            }
            ID_ZOOMIN => drop(sci.send(sci::SCI_ZOOMIN, 0, 0)),
            ID_ZOOMOUT => drop(sci.send(sci::SCI_ZOOMOUT, 0, 0)),
            ID_ZOOMRESET => drop(sci.send(sci::SCI_SETZOOM, 0, 0)),
            // With a selection this repeats the selected text; with none, the whole line.
            ID_L_DUP => drop(sci.send(sci::SCI_SELECTIONDUPLICATE, 0, 0)),
            ID_L_DEL => drop(sci.send(sci::SCI_LINEDELETE, 0, 0)),
            ID_L_UP => drop(sci.send(sci::SCI_MOVESELECTEDLINESUP, 0, 0)),
            ID_L_DOWN => drop(sci.send(sci::SCI_MOVESELECTEDLINESDOWN, 0, 0)),
            ID_C_UPPER => drop(sci.send(sci::SCI_UPPERCASE, 0, 0)),
            ID_C_LOWER => drop(sci.send(sci::SCI_LOWERCASE, 0, 0)),
            ID_L_SORT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, false)),
            ID_L_SORT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::Lexicographic, true)),
            ID_L_SORT_NOCASE_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, false)),
            ID_L_SORT_NOCASE_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::IgnoreCase, true)),
            ID_L_SORT_INT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::Integer, false)),
            ID_L_SORT_INT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::Integer, true)),
            ID_L_SORT_DOT_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, false)),
            ID_L_SORT_DOT_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalDot, true)),
            ID_L_SORT_COMMA_ASC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, false)),
            ID_L_SORT_COMMA_DESC => *flash = ws.line_op(LineOp::Sort(SortKind::DecimalComma, true)),
            ID_L_DEDUP => *flash = ws.line_op(LineOp::RemoveDuplicates),
            ID_L_RM_CONSEC => *flash = ws.line_op(LineOp::RemoveConsecutiveDuplicates),
            ID_L_NOEMPTY => *flash = ws.line_op(LineOp::RemoveEmpty),
            ID_L_RM_BLANK => *flash = ws.line_op(LineOp::RemoveBlank),
            ID_L_REVERSE => *flash = ws.line_op(LineOp::Reverse),
            ID_L_RANDOM => *flash = ws.line_op(LineOp::Randomize),
            ID_L_JOIN => *flash = ws.line_op(LineOp::Join),
            ID_L_SPLIT => *flash = ws.line_op(LineOp::Split),
            ID_L_TRIM => *flash = ws.line_op(LineOp::TrimTrailing),
            ID_B_TRIM_LEAD => *flash = ws.line_op(LineOp::TrimLeading),
            ID_B_TRIM_BOTH => *flash = ws.line_op(LineOp::TrimBoth),
            ID_B_EOL_SPACE => *flash = ws.line_op(LineOp::EolToSpace),
            ID_B_UNNECESSARY => *flash = ws.line_op(LineOp::RemoveUnnecessaryBlanks),
            ID_B_TAB_SPACE => *flash = ws.line_op(LineOp::TabsToSpaces),
            ID_B_SPACE_TAB_ALL => *flash = ws.line_op(LineOp::SpacesToTabsAll),
            ID_B_SPACE_TAB_LEAD => *flash = ws.line_op(LineOp::SpacesToTabsLeading),
            ID_COMMENT => *flash = ws.line_op(LineOp::ToggleComment),
            ID_CMT_LINE => *flash = ws.line_op(LineOp::Comment),
            ID_UNCMT_LINE => *flash = ws.line_op(LineOp::Uncomment),
            ID_CMT_BLOCK => *flash = ws.line_op(LineOp::BlockComment),
            ID_UNCMT_BLOCK => *flash = ws.line_op(LineOp::BlockUncomment),
            ID_L_INS_ABOVE | ID_L_INS_BELOW => ws.insert_blank_line(id == ID_L_INS_ABOVE),
            ID_L_CUT => drop(sci.send(sci::SCI_LINECUT, 0, 0)),
            ID_L_COPY => drop(sci.send(sci::SCI_LINECOPY, 0, 0)),
            ID_L_TRANSPOSE => drop(sci.send(sci::SCI_LINETRANSPOSE, 0, 0)),
            ID_CASE_PROPER => *flash = ws.case_op(CaseOp::Proper),
            ID_CASE_PROPER_BLEND => *flash = ws.case_op(CaseOp::ProperBlend),
            ID_CASE_SENTENCE => *flash = ws.case_op(CaseOp::Sentence),
            ID_CASE_SENTENCE_BLEND => *flash = ws.case_op(CaseOp::SentenceBlend),
            ID_CASE_INVERT => *flash = ws.case_op(CaseOp::Invert),
            ID_CASE_RANDOM => *flash = ws.case_op(CaseOp::Random),
            ID_DELETE => drop(sci.send(sci::SCI_CLEAR, 0, 0)),
            ID_BEGIN_END_SEL => *flash = ws.begin_end_select(false),
            ID_BEGIN_END_COL => *flash = ws.begin_end_select(true),
            ID_INDENT_INC | ID_INDENT_DEC => ws.indent(id == ID_INDENT_INC),
            ID_AC_FUNC => *flash = ws.complete(Completion::Functions),
            ID_AC_WORD => *flash = ws.complete(Completion::Words),
            ID_AC_HINT => *flash = ws.complete(Completion::Hint),
            ID_AC_PATH => *flash = ws.complete(Completion::Path),
            _ if (ID_ENGINE_BASE..ID_ENGINE_BASE + edit::SEARCH_ENGINES.len()).contains(&id) => {
                ws.search_engine = id - ID_ENGINE_BASE;
                *flash = Some(format!("Search engine: {}", edit::SEARCH_ENGINES[ws.search_engine].0));
            }
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
                None => *flash = Some("Save the file first".into()),
                Some(_) if id == ID_MOVE_NEW_INSTANCE && ws.is_dirty(ws.active) => *flash = Some("Save the file before moving it".into()),
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
                Ok(n) => *flash = Some(format!("{n} line(s) hidden: click the + in the margin to show them")),
                Err(e) => *flash = Some(e.to_string()),
            },
            ID_FOLD_ALL | ID_UNFOLD_ALL => view::fold_all(&sci, id == ID_UNFOLD_ALL),
            ID_FOLD_CUR | ID_UNFOLD_CUR => {
                if !view::fold_current_level(&sci, id == ID_UNFOLD_CUR) {
                    *flash = Some("No fold block at the caret".into());
                }
            }
            _ if (ID_FOLD_LEVEL_BASE..ID_FOLD_LEVEL_BASE + 8).contains(&id) => drop(view::fold_level(&sci, id - ID_FOLD_LEVEL_BASE + 1, false)),
            _ if (ID_UNFOLD_LEVEL_BASE..ID_UNFOLD_LEVEL_BASE + 8).contains(&id) => drop(view::fold_level(&sci, id - ID_UNFOLD_LEVEL_BASE + 1, true)),
            ID_READONLY => {
                let on = !ws.is_read_only();
                ws.set_read_only(on);
                *flash = Some(if on { "Read-only" } else { "Editable" }.into());
            }
            ID_CLEAR_RO => {
                if let Some(p) = ws.tabs[ws.active].path.clone() {
                    let cleared = std::fs::metadata(&p).map(|m| {
                        let mut perm = m.permissions();
                        perm.set_readonly(false);
                        std::fs::set_permissions(&p, perm)
                    });
                    *flash = Some(match cleared {
                        Ok(Ok(())) => "Read-only flag cleared".into(),
                        _ => "Could not clear the read-only flag".into(),
                    });
                }
            }
            ID_BM_TOGGLE => ws.toggle_bookmark(),
            ID_BM_NEXT | ID_BM_PREV => {
                if !ws.goto_bookmark(id == ID_BM_NEXT) {
                    *flash = Some("No bookmarks".into());
                }
            }
            ID_BM_CLEAR => ws.clear_bookmarks(),
            ID_RECENT_CLEAR => ws.recent.clear(),
            ID_NEXT_TAB => ws.next_tab(1),
            ID_PREV_TAB => ws.next_tab(-1),
            ID_BRACE_GOTO => {
                if !search::goto_matching_brace(&sci) {
                    *flash = Some("No matching brace".into());
                }
            }
            ID_BRACE_SEL => {
                if !search::select_between_braces(&sci) {
                    *flash = Some("No matching brace".into());
                }
            }
            _ if (ID_STYLE_ALL_BASE..ID_STYLE_ALL_BASE + 5).contains(&id) => {
                let token = search::token_at_caret(&sci);
                *flash = Some(if token.is_empty() || token.contains(['\r', '\n']) {
                    "Select a word to style".into()
                } else {
                    format!("{} occurrence(s) styled", search::style_all(&sci, id - ID_STYLE_ALL_BASE, &token))
                });
            }
            _ if (ID_STYLE_ONE_BASE..ID_STYLE_ONE_BASE + 5).contains(&id) => {
                search::select_word_at_caret(&sci);
                if !search::style_selection(&sci, id - ID_STYLE_ONE_BASE) {
                    *flash = Some("Select some text to style".into());
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
                    *flash = Some(format!("Nothing is styled with the {} style", SLOT_NAMES[slot]));
                }
            }
            ID_BM_DELETE | ID_BM_DELETE_UNMARKED => {
                *flash = Some(format!("{} line(s) removed", bookmarks::delete_lines(&sci, id == ID_BM_DELETE)));
            }
            ID_BM_INVERT => bookmarks::invert(&sci),
            ID_EXPORT_THEME => {
                if let Some(p) = ui.pick_save("note-theme.json") {
                    match std::fs::write(&p, theme_json(&ws.settings)) {
                        Ok(()) => *flash = Some(format!("Theme saved to {}", p.display())),
                        Err(e) => ui.error(&format!("Cannot save the theme\n\n{e}")),
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
                *flash = ws.text_tool(op);
            }
            _ if (ID_WINDOW_DOC_BASE..ID_WINDOW_DOC_BASE + WINDOW_DOCS_SHOWN).contains(&id) => ws.activate(id - ID_WINDOW_DOC_BASE),
        _ => return false,
    }
    true
}

/// The family commands carry their index, so nothing here does id arithmetic.
fn run_family<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, cmd: Command) -> bool {
    match cmd {
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