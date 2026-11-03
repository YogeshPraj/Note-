//! The command handler for the long tail of editor commands (clipboard, dates, folders, dialogs, passwords, hashes).

use super::*;

/// The long tail of editor commands: clipboard, dates, folders, dialogs, encryption, passwords.
pub struct ExtraCommands;

impl CommandHandler<App> for ExtraCommands {
    fn name(&self) -> &'static str {
        "extra"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        handle_1(app, id) || handle_2(app, id) || handle_3(app, id) || handle_4(app, id) || handle_5(app, id) || handle_6(app, id) || handle_7(app, id)
    }
}

pub(super) fn handle_1(app: &mut App, id: usize) -> bool {
    match id {
        c::ID_OPEN_FOLDER_EXPLORER | c::ID_OPEN_FOLDER_CMD | c::ID_OPEN_FOLDER_WS | c::ID_OPEN_VIEWER => match app.ws.tabs[app.ws.active].path.clone() {
            None => app.flash = Some("Save the file first".into()),
            Some(p) => match id {
                c::ID_OPEN_FOLDER_EXPLORER => crate::platform::current().reveal_in_file_manager(&p),
                c::ID_OPEN_FOLDER_CMD => crate::platform::current().open_terminal(p.parent()),
                c::ID_OPEN_FOLDER_WS => {
                    if let Some(d) = p.parent() {
                        app.open_workspace(d.to_path_buf());
                    }
                }
                _ if runs_code(&p) && !app.ui.ask_yes_no(&format!("{} is a program or script. Opening it will run it.\n\nRun it?", p.display())) => {}
                _ => crate::platform::current().open_default(&p.display().to_string()),
            },
        },
        c::ID_INS_DATE_SHORT | c::ID_INS_DATE_LONG | c::ID_INS_DATE_CUSTOM => {
            let text = if id == c::ID_INS_DATE_CUSTOM {
                let current = app.ws.date_format.clone();
                ask_date_format(app, &current).map(|f| {
                    app.ws.date_format.clone_from(&f);
                    edit::format_date_time(&f, &crate::platform::current().local_time())
                })
            } else {
                Some(crate::platform::current().date_time_text(id == c::ID_INS_DATE_LONG))
            };
            if let Some(t) = text {
                app.ws.insert_text(&t);
            }
        }
        c::ID_CP_FULLPATH | c::ID_CP_NAME | c::ID_CP_DIR | c::ID_CP_ALLNAMES | c::ID_CP_ALLPATHS => {
            let kind = match id {
                c::ID_CP_FULLPATH => PathText::FullPath,
                c::ID_CP_NAME => PathText::FileName,
                c::ID_CP_DIR => PathText::Directory,
                c::ID_CP_ALLNAMES => PathText::AllNames,
                _ => PathText::AllPaths,
            };
            app.flash = Some(match app.ws.path_text(kind) {
                Some(t) if crate::services::clipboard::copy(&t) => "Copied to the clipboard".into(),
                Some(_) => "The clipboard is busy".into(),
                None => "Save the file first".into(),
            });
        }
        _ => return false,
    }
    true
}

pub(super) fn handle_2(app: &mut App, id: usize) -> bool {
    let sci = app.ws.sci;
    match id {
        c::ID_SEL_OPEN | c::ID_SEL_FOLDER => {
            let sel = search::selected_text(&sci);
            let tab = &app.ws.tabs[app.ws.active];
            let found = selected_path(&sel, &[tab.path.as_deref().and_then(|p| p.parent()), app.ws.workspace.as_deref()]);
            match found {
                None => app.flash = Some("No file or folder with that name".into()),
                Some(p) if id == c::ID_SEL_FOLDER => crate::platform::current().reveal_in_file_manager(&p),
                Some(p) if p.is_file() => {
                    let App { ws, ui, .. } = app;
                    ws.open_path(ui, &p);
                }
                Some(p) => app.open_workspace(p),
            }
        }
        c::ID_SEL_SEARCH => {
            let sel = search::selected_text(&sci);
            if sel.trim().is_empty() {
                app.flash = Some("Select some text first".into());
            } else {
                crate::platform::current().open_default(&edit::search_url(app.ws.search_engine, &sel));
            }
        }
        c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV | c::ID_VOL_NEXT | c::ID_VOL_PREV => {
            let forward = matches!(id, c::ID_SEL_FIND_NEXT | c::ID_VOL_NEXT);
            search::select_word_at_caret(&sci);
            let token = search::selected_text(&sci);
            if token.is_empty() || token.contains(['\r', '\n']) {
                app.flash = Some("Select a word to search for".into());
            } else {
                let o = search::Opts { find: token, match_case: app.ws.search.match_case, wrap: true, ..Default::default() };
                // "Volatile" searches leave the Find box and F3 untouched.
                if matches!(id, c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV) {
                    app.ws.search = o.clone();
                }
                if search::find(&sci, &o, forward) == search::Found::NotFound {
                    app.flash = Some("Not found".into());
                }
            }
        }
        _ => return false,
    }
    true
}

pub(super) fn handle_3(app: &mut App, id: usize) -> bool {
    let sci = app.ws.sci;
    match id {
        c::ID_BM_CUT | c::ID_BM_COPY => {
            use notepp_core::doc::Eol;
            let eol = match app.ws.tabs[app.ws.active].eol {
                Eol::Crlf => "\r\n",
                Eol::Lf => "\n",
                Eol::Cr => "\r",
            };
            let text = bookmarks::marked_text(&sci, eol);
            if text.is_empty() && bookmarks::lines(&sci).is_empty() {
                app.flash = Some("No bookmarks".into());
            } else if crate::services::clipboard::copy(&text) {
                if id == c::ID_BM_CUT {
                    bookmarks::delete_lines(&sci, true);
                }
                app.flash = Some("Bookmarked lines copied".into());
            }
        }
        c::ID_BM_PASTE => match crate::services::clipboard::paste() {
            Some(t) => {
                let t = t.trim_end_matches(['\r', '\n']).to_string();
                app.flash = Some(format!("{} line(s) replaced", bookmarks::replace_lines(&sci, &t)));
            }
            None => app.flash = Some("Nothing to paste".into()),
        },
        _ if (c::ID_COPY_STYLED_BASE..c::ID_COPY_STYLED_BASE + 6).contains(&id) => {
            let slot = id - c::ID_COPY_STYLED_BASE;
            let slots: Vec<usize> = if slot < 5 { vec![slot] } else { (0..5).collect() };
            let text = search::styled_text(&sci, &slots);
            app.flash = Some(if text.is_empty() {
                "Nothing is styled".into()
            } else if crate::services::clipboard::copy(&text) {
                "Styled text copied".into()
            } else {
                "The clipboard is busy".into()
            });
        }
        c::ID_RTL | c::ID_LTR => {
            crate::widgets::native::current().set_rtl(app.stc.get_handle() as isize, id == c::ID_RTL);
            // Scintilla's own bidirectional mode: 1 = right-to-left, 0 = off.
            notepp_core::sci::Send::send(&app.ws.sci, 2709, usize::from(id == c::ID_RTL), 0);
        }
        _ => return false,
    }
    true
}

pub(super) fn handle_4(app: &mut App, id: usize) -> bool {
    match id {
        c::ID_ON_TOP => {
            let p = crate::widgets::native::current();
            let h = app.frame.get_handle() as isize;
            let on = !p.is_topmost(h);
            p.set_topmost(h, on);
        }
        c::ID_POSTIT | c::ID_DISTRACTION => {
            if id == c::ID_POSTIT {
                app.postit = !app.postit;
                app.distraction = false;
            } else {
                app.distraction = !app.distraction;
                app.postit = false;
            }
            app.ws.set_margins_visible(!app.distraction);
            // Distraction free mode also goes full screen.
            app.frame.show_full_screen(app.distraction);
        }
        c::ID_COLUMN_MODE => {
            let t = "Column Mode selects a rectangle of text.\n\n\u{2022} Hold Alt and drag with the mouse, or\n\u{2022} press Alt+Shift with the arrow keys, or\n\u{2022} press Alt+Shift+B at one corner, move, and press it again.\n\nThen type to edit every line at once, or use Column Editor (Alt+C) to insert text or numbers.";
            info(app, "Column Mode", t);
        }
        c::ID_COLUMN_EDITOR => app.flash = column_editor(app),
        c::ID_SUMMARY => summary(app),
        c::ID_FIND_CHARS => find_chars_dialog(app),
        c::ID_MD_TOOLBAR => {
            let mut s = app.ws.settings.clone();
            s.md_toolbar = !s.md_toolbar;
            crate::settings_ui::apply_settings(app, s);
        }
        c::ID_RESULTS => {
            if crate::findpanel::visible() {
                crate::findpanel::close();
            } else if app.results.hits.is_empty() {
                app.flash = Some("No search results yet".into());
            } else {
                let mode = if app.results.root.as_os_str().is_empty() { crate::findpanel::Mode::Find } else { crate::findpanel::Mode::InFiles };
                let (last, dir) = (app.ws.search.find.clone(), app.results.root.display().to_string());
                crate::findpanel::open(mode, "", &last, &dir);
            }
        }
        _ => return false,
    }
    true
}

pub(super) fn handle_5(app: &mut App, id: usize) -> bool {
    match id {
        _ if (c::ID_BROWSER_BASE..c::ID_BROWSER_BASE + notepp_core::tables::BROWSERS.len()).contains(&id) => match app.ws.tabs[app.ws.active].path.clone() {
            Some(p) => crate::platform::current().open_with(notepp_core::tables::BROWSERS[id - c::ID_BROWSER_BASE].1, &p),
            None => app.flash = Some("Save the file first".into()),
        },
        c::ID_MONITOR => {
            let (tab_id, path) = (app.ws.tabs[app.ws.active].id, app.ws.tabs[app.ws.active].path.clone());
            match (path, app.streams.monitoring.contains_key(&tab_id)) {
                (None, _) => app.flash = Some("Save the file first".into()),
                (Some(_), true) => {
                    app.streams.monitoring.remove(&tab_id);
                    app.ws.set_read_only(false);
                }
                (Some(p), false) => {
                    let len = std::fs::metadata(&p).map_or(0, |m| m.len());
                    app.streams.monitoring.insert(tab_id, len);
                    app.ws.set_read_only(true);
                    app.flash = Some("Monitoring: new lines are shown as the file grows".into());
                }
            }
        }
        c::ID_PW_LOCK => {
            notepp_core::bitwarden::lock();
            app.flash = Some("Passwords locked".into());
        }
        c::ID_PASSWORDS | c::ID_PW_TOOLBAR => app.passwords(),
        _ => return false,
    }
    true
}

pub(super) fn handle_6(app: &mut App, id: usize) -> bool {
    match id {
        c::ID_WINDOWS => {
            let mut sel = app.ws.active;
            loop {
                let rows = app.ws.window_rows();
                match dialogs::windows_dialog(&app.frame, &rows, sel.min(rows.len().saturating_sub(1))) {
                    None => break,
                    Some(WinAction::Activate(i)) => {
                        app.ws.activate(i);
                        break;
                    }
                    Some(WinAction::Save(rows)) => {
                        let App { ws, ui, .. } = app;
                        for &i in &rows {
                            // Saving goes through the active tab; whiteboards save from their own tab.
                            if i < ws.tabs.len() && ws.tabs[i].kind == notepp_core::workspace::TabKind::Text && ws.is_dirty(i) {
                                ws.activate(i);
                                ws.save(ui);
                            }
                        }
                        sel = ws.active;
                    }
                    Some(WinAction::Close(rows)) => {
                        let App { ws, ui, .. } = app;
                        // Highest first so the indices stay valid.
                        for &i in rows.iter().rev() {
                            if i < ws.tabs.len() && !ws.close(ui, i) {
                                break;
                            }
                        }
                        sel = ws.active;
                    }
                    Some(WinAction::Sort(field, descending)) => {
                        app.ws.sort_tabs(field, descending);
                        sel = app.ws.active;
                    }
                }
            }
        }
        _ if (c::ID_WINDOW_SORT_BASE..c::ID_WINDOW_SORT_BASE + dialogs::SORTS.len()).contains(&id) => {
            let (_, field, descending) = dialogs::SORTS[id - c::ID_WINDOW_SORT_BASE];
            app.ws.sort_tabs(field, descending);
        }
        _ => return false,
    }
    true
}

pub(super) fn handle_7(app: &mut App, id: usize) -> bool {
    let sci = app.ws.sci;
    match id {
        _ if (c::ID_HASH_BASE..c::ID_HASH_BASE + notepp_core::hash::ALGOS.len() * 10).contains(&id) => {
            let algo = notepp_core::hash::ALGOS[(id - c::ID_HASH_BASE) / 10];
            match (id - c::ID_HASH_BASE) % 10 {
                0 => dialogs::hash_text(&app.frame, algo),
                1 => {
                    let files = app.ui.pick_open();
                    let count = files.len();
                    // Big files take a while: hashed on a worker thread, with a progress window and Cancel.
                    let job = move |p: &crate::widgets::progress::Progress| {
                        notepp_core::hash::hash_files(algo, &files, &mut |done, total| {
                            p.set(crate::platform::wx::to_u32(done.saturating_mul(100).checked_div(total).unwrap_or(100)), "Reading the files...");
                            !p.cancelled()
                        })
                    };
                    if count > 0
                        && let Some(Some(lines)) = crate::widgets::progress::run(&app.frame, algo.name(), "Reading the files...", job)
                    {
                        dialogs::text_result(&app.frame, &format!("{} of {count} file(s)", algo.name()), &lines.join("\r\n"));
                    }
                }
                _ => {
                    let sel = search::selected_text(&sci);
                    if sel.is_empty() {
                        app.flash = Some("Select some text first".into());
                    } else {
                        let digest = algo.hash_bytes(sel.as_bytes());
                        app.flash = Some(if crate::services::clipboard::copy(&digest) {
                            format!("{} copied: {digest}", algo.name())
                        } else {
                            "The clipboard is busy".into()
                        });
                    }
                }
            }
        }
        _ => return false,
    }
    true
}
