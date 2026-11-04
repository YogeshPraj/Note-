//! Event wiring: the minimap, menu keys, commands, the whiteboard and preview pages, the timers.

use super::*;

/// The minimap is a second view of the document: clicking or dragging in it scrolls the editor.
pub(super) fn wire_mini(app: &Shared, mini: StyledTextCtrl, stc: StyledTextCtrl) {
    use notepp_core::sci::Send;
    let jump = |app: &Shared, mini: StyledTextCtrl, e: &wxdragon::event::WindowEventData| {
        let p = match e {
            wxdragon::event::WindowEventData::MouseButton(m) => m.get_position(),
            wxdragon::event::WindowEventData::MouseMotion(m) => m.get_position(),
            _ => None,
        };
        let Some(p) = p else { return };
        let sci = WxSci::of(&mini);
        let pos = sci.send(sci::SCI_POSITIONFROMPOINT, crate::platform::wx::to_usize(p.x), p.y as isize).max(0);
        let line = sci.send(sci::SCI_LINEFROMPOSITION, crate::platform::wx::to_usize(pos), 0).max(0);
        if let Ok(mut a) = app.try_borrow_mut() {
            a.ws.scroll_to_line(crate::platform::wx::to_usize(line));
            a.refresh();
        }
    };
    let down = Rc::new(std::cell::Cell::new(false));
    let (a, d) = (app.clone(), down.clone());
    mini.on_mouse_left_down(move |e| {
        d.set(true);
        jump(&a, mini, &e);
    });
    let d = down.clone();
    mini.on_mouse_left_up(move |_| d.set(false));
    let a = app.clone();
    mini.on_mouse_motion(move |e| {
        if down.get() {
            jump(&a, mini, &e);
        }
    });
    let a = app.clone();
    mini.on_mouse_right_up(move |e| {
        if let wxdragon::event::WindowEventData::MouseButton(m) = &e
            && let Some(p) = m.get_position()
        {
            popups::minimap_menu(&a, mini.client_to_screen(p));
        }
    });
    mini.on_set_focus(move |_| stc.set_focus());
}

/// Clicking a bar hands it the focus once the click has been handled; give it back to the editor (unless a command has put
/// it somewhere deliberate, such as the find box).
pub(crate) fn refocus_editor(app: &Shared) {
    let Ok(a) = app.try_borrow() else { return };
    let (frame, stc, wbv, bars) = (a.frame, a.stc, a.wbv.clone(), [a.chrome.menu, a.chrome.tools, a.chrome.tabs, a.chrome.status]);
    let whiteboard = a.ws.is_whiteboard();
    let took = a.menu_took_focus.replace(false);
    crate::widgets::defer::later(&frame, move || {
        if took && whiteboard {
            wbv.set_focus();
        } else if took || bars.iter().any(WxWidget::has_focus) {
            stc.set_focus();
        }
    });
}

/// A page in a web view (the whiteboard, the preview) keeps the keyboard while a menu is open, and its keys never reach
/// the menu (Esc, arrows). So the window takes the focus for as long as the menu shows; `refocus_editor` gives it back.
pub(crate) fn take_focus_for_menu(app: &Shared) {
    let Ok(a) = app.try_borrow() else { return };
    if a.ws.is_whiteboard() || a.preview.split {
        a.menu_took_focus.set(true);
        crate::widgets::native::current().focus_window(a.frame.get_handle() as isize);
    }
}

/// Open menu `i` of the menu bar; from the keyboard its first item is highlighted.
pub(super) fn open_menu(a: &Shared, i: usize, keyboard: bool) {
    popups::bar_menu(a, i, keyboard);
}

/// Open the menu whose title underlines key (Alt+letter); an empty key is F10 and opens the first.
pub(crate) fn open_menu_for_key(a: &Shared, key: &str) {
    let Some(ch) = key.chars().next() else { return open_menu(a, 0, true) };
    let Some(titles) = a.try_borrow().ok().map(|app| app.chrome.menu_titles()) else { return };
    if let Some(i) = crate::widgets::menu_bar::menu_for_key(&titles, ch) {
        open_menu(a, i, true);
    }
}

/// Alt+letter opens the menu whose title underlines that letter; F10 opens the first.
pub(super) fn wire_menu_keys(app: &Shared, frame: Frame) {
    let a = app.clone();
    frame.on_char_hook(move |e| {
        let wxdragon::event::WindowEventData::Keyboard(k) = &e else {
            e.skip(true);
            return;
        };
        let code = k.get_key_code().unwrap_or(0);
        // Alt+letter opens the menu with that letter, also while another menu is open (the open one is replaced).
        let alt_menu = (k.alt_down() && !k.control_down()).then(|| u8::try_from(code).ok().filter(u8::is_ascii_graphic)).flatten().and_then(|b| {
            let titles = a.try_borrow().ok()?.chrome.menu_titles();
            crate::widgets::menu_bar::menu_for_key(&titles, char::from(b))
        });
        if let Some(i) = alt_menu {
            open_menu(&a, i, true);
            return;
        }
        // An open menu takes every other key.
        if crate::widgets::popup_menu::key(code, !k.control_down() && !k.alt_down()) {
            return;
        }
        // Ctrl+Tab and Ctrl+Shift+Tab switch tabs. Tab is a navigation key, which the toolkit and the editor take
        // before any menu shortcut sees it, so it is handled here.
        if code == KEY_TAB && k.control_down() && !k.alt_down() {
            switch_tab(&a, if k.shift_down() { -1 } else { 1 });
            return;
        }
        if code == 344 && !k.alt_down() && !k.control_down() {
            open_menu(&a, 0, true); // WXK_F10
        } else {
            e.skip(true);
        }
    });
    // Switching to another program closes an open menu; coming back looks for files other programs changed or deleted.
    let a = app.clone();
    frame.on_activate(move |e| {
        if let wxdragon::event::WindowEventData::Activate(act) = &e {
            if act.is_active() {
                // After the activation: the check may ask questions, which must not open inside this event.
                let a = a.clone();
                crate::widgets::defer::later(&frame, move || {
                    if let Ok(mut app) = a.try_borrow_mut() {
                        app.check_files_on_disk();
                    }
                });
            } else {
                crate::widgets::popup_menu::deactivated();
            }
        }
        e.skip(true);
    });
    // Moving, resizing or minimising the window would leave the menu hanging where it was.
    frame.on_size(|e| {
        crate::widgets::popup_menu::close_all();
        e.skip(true);
    });
    frame.on_move_event(|e| {
        crate::widgets::popup_menu::close_all();
        e.skip(true);
    });
}

/// One entry for menu items and toolbar buttons. Modeless windows need the shared handle, so they
/// are opened here, with nothing borrowed.
pub(crate) fn run_command(a: &Shared, id: usize) {
    trace(id, "event");
    // The enabled rule first: some commands below open their window directly instead of going through `execute`.
    if a.try_borrow().is_ok_and(|app| !notepp_core::registry::is_enabled(id, notepp_core::registry::CommandState::of(&app.ws))) {
        return;
    }
    match id {
        notepp_core::commands::ID_PREFS => return crate::settings_window::open(a, "general"),
        notepp_core::commands::ID_STYLE_CFG => return crate::settings_window::open(a, "colours"),
        notepp_core::commands::ID_SHORTCUTS => return crate::settings_window::open(a, "shortcuts"),
        notepp_core::commands::ID_EDIT_CONTEXT => return crate::settings_window::open(a, "context"),
        notepp_core::commands::ID_ENC_SETTINGS => return crate::settings_window::open(a, "security"),
        notepp_core::commands::ID_TAB_COLOR_CUSTOM => return crate::popups::custom_tab_colour(a),
        ID_FIND => return crate::find::open_panel(a, Mode::Find),
        ID_REPLACE => return crate::find::open_panel(a, Mode::Replace),
        ID_PALETTE => return crate::palette::open_commands(a),
        ID_QUICKOPEN => return crate::palette::open_files(a),
        ID_INCR => return crate::incr::open(a),
        ID_FIF => return crate::find::open_panel(a, Mode::InFiles),
        ID_MARK => return crate::find::open_panel(a, Mode::Mark),
        ID_FINDNEXT | ID_FINDPREV => return crate::find::step(a, id == ID_FINDNEXT),
        ID_GOTO => return crate::find::goto_line(a),
        _ => {}
    }
    if let Ok(mut app) = a.try_borrow_mut() {
        app.command(id);
    }
}
/// Messages from the Excalidraw page: readiness, scene changes, save requests.
pub(super) fn wire_whiteboard(app: &Shared, wbv: &WebPane) {
    let a = app.clone();
    wbv.on_script_message_received(move |e| {
        let msg = e.get_string().unwrap_or_default();
        let Some(ev) = wbm::parse(&msg) else { return };
        match ev {
            wbm::Event::Tab(delta) => return switch_tab(&a, delta),
            wbm::Event::Menu(key) => return open_menu_for_key(&a, &key),
            _ => {}
        }
        if let Ok(mut app) = a.try_borrow_mut() {
            app.wb_event(ev);
        }
    });
}

/// Go to the next (1) or previous (-1) tab and give the keyboard to what that tab shows: a page keeps it otherwise.
pub(crate) fn switch_tab(app: &Shared, delta: i32) {
    run_command(app, if delta < 0 { notepp_core::commands::ID_PREV_TAB } else { notepp_core::commands::ID_NEXT_TAB });
    focus_shown_tab(app);
}

/// Give the keyboard to what the active tab shows, just after the event in hand (it may have been in the preview page).
fn focus_shown_tab(app: &Shared) {
    let a = app.clone();
    let frame = app.borrow().frame;
    crate::widgets::defer::later(&frame, move || {
        let Ok(app) = a.try_borrow() else { return };
        if app.ws.is_whiteboard() {
            app.wbv.set_focus();
        } else {
            app.stc.set_focus();
        }
    });
}

pub(super) fn wire(app: &Shared, frame: Frame, stc: &StyledTextCtrl, view: &WebPane) {
    wire_events(app, frame, stc, view);
    wire_timers(app, frame);
}

#[allow(clippy::too_many_lines, reason = "wires each window event to its action, one block per event")]
pub(super) fn wire_events(app: &Shared, frame: Frame, stc: &StyledTextCtrl, view: &WebPane) {
    // The preview page finished loading: draw the current document in it.
    let a = app.clone();
    view.on_loaded(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.view.mark_ready();
            app.preview_update(true);
        }
    });
    // Messages from the page (close, refresh, lazy-loading of Mermaid / the JSON editor).
    let a = app.clone();
    let pv = view.clone();
    view.on_script_message_received(move |e| {
        let msg = notepp_core::jsonpath::page_text(&e.get_string().unwrap_or_default());
        match msg.as_str() {
            // The libraries go in from the main loop: scripts run from inside the page's own message callback get dropped.
            "need-mermaid" => {
                let pv = pv.clone();
                crate::widgets::defer::later(&frame, move || preview::load_mermaid(&pv));
            }
            "tab:next" | "tab:prev" => switch_tab(&a, if msg == "tab:prev" { -1 } else { 1 }),
            m if m.starts_with("menu:") => open_menu_for_key(&a, &m[5..]),
            "need-jsoneditor" => {
                let pv = pv.clone();
                crate::widgets::defer::later(&frame, move || preview::load_json_editor(&pv));
            }
            "max" => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    let on = !app.preview.maximized;
                    app.set_preview_maximized(on);
                }
            }
            "close" => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    app.set_preview(false);
                    app.refresh();
                }
            }
            "refresh" => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    app.preview_update(true);
                }
            }
            other => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    app.page_message(other);
                }
            }
        }
    });
    // Menu commands.
    let a = app.clone();
    frame.on_menu(move |e| run_command(&a, crate::platform::wx::to_usize(e.get_id())));

    // Every change to the text counts, for the session autosave (an edit can keep the length the same).
    stc.on_stc_change(|_| {
        EDITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    // Caret moves, edits and selection changes update the status bar.
    let a = app.clone();
    stc.on_stc_update_ui(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.refresh();
        }
    });

    // Toolbar buttons and the tab strip.
    let a = app.clone();
    let handler: Rc<dyn Fn(Act)> = Rc::new(move |act| {
        refocus_editor(&a);
        if let Act::Menu(i) = act {
            return open_menu(&a, i, false);
        }
        // Popup menus are modal: nothing may be borrowed while they are open (their commands re-enter the app).
        match act {
            Act::Context(i, x, y) => {
                let at = {
                    let Ok(mut app) = a.try_borrow_mut() else { return };
                    app.ws.activate(i);
                    app.refresh();
                    app.chrome.tabs.client_to_screen(Point::new(x, y))
                };
                return popups::tab_menu(&a, at);
            }
            Act::Status(seg, x, y) => {
                let at = match a.try_borrow() {
                    Ok(app) => app.chrome.status.client_to_screen(Point::new(x, y)),
                    Err(_) => return,
                };
                return popups::status_menu(&a, seg, at);
            }
            _ => {}
        }
        let a2 = a.clone();
        let Ok(mut app) = a.try_borrow_mut() else { return };
        match act {
            Act::Cmd(id) => {
                drop(app);
                return run_command(&a2, id);
            }
            Act::Select(i) => {
                app.ws.activate(i);
                app.refresh();
                drop(app);
                return focus_shown_tab(&a2);
            }
            Act::Close(i) => {
                let App { ws, ui, .. } = &mut *app;
                ws.close(ui, i);
            }
            Act::New => {
                app.ws.new_tab();
            }
            Act::Move(from, to) => app.ws.move_tab_to(from, to),
            Act::Menu(..) | Act::Context(..) | Act::Status(..) => {}
        }
        app.refresh();
    });
    app.borrow().chrome.set_handler(handler);
    // Resting on another tab shows the start of its text. Nothing is read while the app is busy (a dialog, a menu).
    let a = app.clone();
    app.borrow().chrome.set_tab_preview(move |i| a.try_borrow().map(|app| app.ws.tab_preview(i, TIP_LINES, TIP_WIDTH)).unwrap_or_default());
}

/// One timer for every periodic job, and saving the session when the window closes.
pub(super) fn wire_timers(app: &Shared, frame: Frame) {
    // One timer drives every periodic job; the schedule says how often each runs.
    #[derive(Clone, Copy, PartialEq)]
    enum Job {
        /// Followed logs, HTTP responses, the update check, files dropped by a second launch.
        Stream,
        /// Clipboard History records what is copied, whichever program copies it; a copied password is cleared.
        Clipboard,
        /// Keep the session file current (a crash or power cut then loses at most a few seconds).
        Autosave,
    }
    let schedule = Schedule::new(500).every(Job::Stream, 500).every(Job::Clipboard, 1000).every(Job::Autosave, 10_000);
    let ticker = Timer::new(&frame);
    let a = app.clone();
    let beat = std::cell::Cell::new(0u64);
    let base = crate::platform::wx::to_i32(schedule.base_ms());
    ticker.on_tick(move |_| {
        beat.set(beat.get() + 1);
        let Ok(mut app) = a.try_borrow_mut() else { return };
        for job in schedule.due(beat.get()) {
            match job {
                Job::Stream => {
                    app.update_poll();
                    app.check_inbox();
                    app.monitor_tick();
                    app.stream_tick();
                    app.in_files_poll();
                    app.file_index.poll();
                    crate::tree_reader::poll(&app.panels.tree);
                }
                Job::Clipboard => {
                    app.clip_tick();
                    app.poll_clipboard();
                }
                Job::Autosave => app.autosave_session(),
            }
        }
    });
    ticker.start(base, false);
    std::mem::forget(ticker);
    // Closing the window saves the session; unsaved text comes back next time.
    let a = app.clone();
    frame.on_close(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.save_session();
        }
        e.skip(true);
    });
}
