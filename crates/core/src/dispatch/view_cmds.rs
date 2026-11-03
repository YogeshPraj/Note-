//! View, folding, bookmarks, styles, tools and windows: the commands that only need the workspace.

use super::*;

pub(super) fn command_core_5<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
    match id {
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
        _ => return false,
    }
    true
}

pub(super) fn command_core_6<S: sci::Send + Copy>(ws: &mut Workspace<S>, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    match id {
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
                    #[allow(clippy::permissions_set_readonly_false, reason = "clears the DOS read-only attribute")]
                    perm.set_readonly(false);
                    std::fs::set_permissions(&p, perm)
                });
                *flash = Some(match cleared {
                    Ok(Ok(())) => "Read-only flag cleared".into(),
                    _ => "Could not clear the read-only flag".into(),
                });
            } else {
                *flash = Some("Save the file first".into());
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
        _ => return false,
    }
    true
}

pub(super) fn command_core_7<S: sci::Send + Copy>(ws: &mut Workspace<S>, flash: &mut Option<String>, id: usize) -> bool {
    let sci = ws.sci;
    match id {
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
        _ => return false,
    }
    true
}

pub(super) fn command_core_8<S: sci::Send + Copy>(ws: &mut Workspace<S>, ui: &dyn Ui, flash: &mut Option<String>, id: usize) -> bool {
    match id {
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
