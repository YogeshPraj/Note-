//! Floating tabs on screen: each is a window of its own with a Dock Back bar over its editor. The workspace decides what
//! floats and which view is current (see `workspace::floats`); this keeps the windows matching it and tells it where the
//! keyboard went.
//!
//! The windows have no parent, so that they minimise, maximise and sit on the taskbar on their own, and the menu shortcuts
//! are the main window's: here the keys go through the key map instead.

use notepp_core::session::FloatRect;
use notepp_core::shortcuts::Chord;

use super::*;
use crate::widgets::pane_header::{HeaderEvent, PaneHeader};

/// One floating window.
pub struct FloatWindow {
    tab: u64,
    frame: Frame,
    head: PaneHeader,
    stc: StyledTextCtrl,
    title: String,
}

impl std::fmt::Debug for FloatWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FloatWindow").field("tab", &self.tab).finish_non_exhaustive()
    }
}

/// Size of a new window before scaling.
const WIDTH: i32 = 900;
const HEIGHT: i32 = 650;

/// The active tab becomes a window.
pub(crate) fn undock(a: &Shared) {
    let i = {
        let Ok(mut app) = a.try_borrow_mut() else { return };
        let i = app.ws.active;
        if !app.ws.can_float(i) {
            app.flash = Some("Only a text tab can float, with another text tab left in the main window".into());
            app.refresh();
            return;
        }
        i
    };
    open(a, i, None);
    if let Ok(mut app) = a.try_borrow_mut() {
        app.refresh();
    }
}

/// The windows the last session had, once the main window is up.
pub(super) fn restore(a: &Shared) {
    let saved = a.borrow_mut().ws.take_saved_floats();
    for (tab, rect) in saved {
        let Some(i) = a.borrow().ws.tabs.iter().position(|t| t.id == tab) else { continue };
        open(a, i, Some(rect));
    }
    if let Ok(mut app) = a.try_borrow_mut() {
        app.refresh();
    }
}

fn frame_for(title: &str, rect: Option<FloatRect>) -> Frame {
    let t = crate::widgets::theme::current();
    let b = Frame::builder().with_title(title);
    // A window saved on a monitor that has gone since opens where the system puts new ones.
    match rect.filter(|r| crate::platform::current().on_screen(r.x, r.y, r.w, r.h)) {
        Some(r) => b.with_position(Point::new(r.x, r.y)).with_size(Size::new(r.w, r.h)).build(),
        None => b.with_size(Size::new(t.px(WIDTH), t.px(HEIGHT))).build(),
    }
}

/// Make the window for tab `i`.
fn open(a: &Shared, i: usize, rect: Option<FloatRect>) {
    let Ok(mut app) = a.try_borrow_mut() else { return };
    if !app.ws.can_float(i) {
        return;
    }
    let (tab, title) = (app.ws.tabs[i].id, app.ws.title_of(i));
    let frame = frame_for(&title, rect);
    let panel = Panel::builder(&frame).build();
    let head = PaneHeader::with_action(&panel, Some("float"), "Dock Back");
    let stc = StyledTextCtrl::builder(&panel).build();
    no_border(&stc);
    let column = BoxSizer::builder(Orientation::Vertical).build();
    column.add(&head.panel(), 0, SizerFlag::Expand, 0);
    column.add(&stc, 1, SizerFlag::Expand, 0);
    panel.set_sizer(column, true);
    if !app.ws.float_tab(i, WxSci::of(&stc)) {
        frame.destroy();
        return;
    }
    app.float_windows.push(FloatWindow { tab, frame, head: head.clone(), stc, title });
    let main = app.frame;
    drop(app);
    wire(a, main, frame, tab, &head, stc);
    crate::widgets::theme::style_native(&frame);
    crate::widgets::theme::subscribe(move |_| {
        if frame.is_valid() {
            crate::widgets::theme::style_native(&frame);
        }
    });
    crate::widgets::native::current().set_app_icon(frame.get_handle() as isize);
    frame.show(true);
    if rect.is_some_and(|r| r.maximized) {
        frame.maximize(true);
    }
    frame.layout();
    if let Ok(mut app) = a.try_borrow_mut() {
        app.ws.refresh_float(tab);
    }
    if rect.is_none() {
        stc.set_focus();
    }
}

/// The keyboard went into the window of `tab`: it becomes the current one.
fn float_focused(app: &Shared, main: Frame, tab: u64) {
    let Ok(mut a) = app.try_borrow_mut() else {
        // A command is running: look again when it has finished.
        let again = app.clone();
        return crate::widgets::defer::later(&main, move || {
            if let Ok(mut a) = again.try_borrow_mut() {
                a.ws.focus_float(tab);
                a.refresh();
            }
        });
    };
    if a.ws.current_float() != Some(tab) {
        a.ws.focus_float(tab);
        a.refresh();
    }
}

/// Closing the window closes the document, asking first as closing a tab does.
fn close(app: &Shared, tab: u64) {
    let Ok(mut a) = app.try_borrow_mut() else { return };
    let Some(i) = a.ws.tabs.iter().position(|t| t.id == tab) else { return };
    let app = &mut *a;
    app.ws.close(&app.ui, i);
    app.refresh();
}

fn dock_back(app: &Shared, tab: u64) {
    if let Ok(mut a) = app.try_borrow_mut() {
        a.dock_back_window(tab);
    }
}

/// The menu shortcuts belong to the main window, so a key meant for one is looked up here. Everything else goes on to
/// the editor, which also keeps keys such as Del and Tab its own.
fn wire_keys(a: &Shared, frame: Frame) {
    let a = a.clone();
    frame.on_char_hook(move |e| {
        let wxdragon::event::WindowEventData::Keyboard(k) = &e else {
            e.skip(true);
            return;
        };
        let code = k.get_key_code().unwrap_or(0);
        let (ctrl, alt, shift) = (k.control_down(), k.alt_down(), k.shift_down());
        if crate::widgets::popup_menu::key(code, !ctrl && !alt) {
            return;
        }
        // The next tab is in the other window: Ctrl+Tab has nothing to go to, and must not insert a tab.
        if code == KEY_TAB && ctrl {
            return;
        }
        let chord = crate::dialogs::vk_of(code).map(|vk| Chord { ctrl, alt, shift, vk });
        let id = chord.and_then(|c| a.try_borrow().ok()?.keymap.command_for(&c));
        match id {
            Some(id) => crate::app::run_command(&a, id),
            None => e.skip(true),
        }
    });
}

/// What a floating window needs from the main view's wiring.
fn wire(a: &Shared, main: Frame, frame: Frame, tab: u64, head: &PaneHeader, stc: StyledTextCtrl) {
    stc.on_stc_change(|_| {
        EDITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    let app = a.clone();
    stc.on_stc_update_ui(move |_| {
        if let Ok(mut app) = app.try_borrow_mut() {
            app.refresh();
        }
    });
    let app = a.clone();
    stc.on_set_focus(move |e| {
        e.skip(true);
        float_focused(&app, main, tab);
    });
    attach_drop_targets(a, frame, [stc]);
    replace_editor_popup(a, stc);
    wire_keys(a, frame);
    let app = a.clone();
    frame.on_close(move |e| {
        // The window stays until the document has gone, and a Cancel at the save prompt keeps both. Destroyed here, the
        // workspace would still be using its editor.
        match &e {
            wxdragon::event::WindowEventData::General(close) => close.veto(),
            wxdragon::event::WindowEventData::Session(close) => close.event.veto(),
            _ => {}
        }
        let app = app.clone();
        crate::widgets::defer::later(&main, move || close(&app, tab));
    });
    frame.on_size(|e| {
        crate::widgets::popup_menu::close_all();
        e.skip(true);
    });
    frame.on_move_event(|e| {
        crate::widgets::popup_menu::close_all();
        e.skip(true);
    });
    let app = a.clone();
    head.on_event(move |e| match e {
        HeaderEvent::Focus => stc.set_focus(),
        HeaderEvent::Close => {
            let app = app.clone();
            crate::widgets::defer::later(&main, move || dock_back(&app, tab));
        }
    });
}

impl App {
    /// The editor of the floating window the user is in.
    pub(super) fn float_stc(&self) -> Option<StyledTextCtrl> {
        let tab = self.ws.current_float()?;
        self.float_windows.iter().find(|w| w.tab == tab).map(|w| w.stc)
    }

    /// Match the windows to the workspace: close those whose tab has gone, name the others, bring one forward if asked.
    pub(super) fn sync_floats(&mut self) {
        let floating = self.ws.floating_tabs();
        self.float_windows.retain(|w| {
            let keep = floating.contains(&w.tab);
            if !keep {
                w.frame.destroy();
            }
            keep
        });
        let current = self.ws.current_float();
        for w in &mut self.float_windows {
            let Some(i) = self.ws.tabs.iter().position(|t| t.id == w.tab) else { continue };
            let title = self.ws.title_of(i);
            if title != w.title {
                w.frame.set_title(&title);
                w.title = title;
            }
            w.head.set(&self.ws.tabs[i].title, self.ws.is_dirty(i), current == Some(w.tab));
        }
        if let Some(tab) = self.ws.take_raise()
            && let Some(w) = self.float_windows.iter().find(|w| w.tab == tab)
        {
            if w.frame.is_iconized() {
                w.frame.iconize(false);
            }
            w.frame.raise();
            w.stc.set_focus();
        }
    }

    /// The tab goes back to the strip and the keyboard with it to the main window.
    pub(super) fn dock_back_window(&mut self, tab: u64) {
        self.ws.dock_back(tab);
        self.refresh();
        crate::widgets::native::current().focus_window(self.frame.get_handle() as isize);
        self.stc().set_focus();
    }

    /// Note where the windows are, for the session.
    pub(super) fn keep_float_rects(&mut self) {
        for w in &self.float_windows {
            if w.frame.is_iconized() {
                continue;
            }
            let maximized = w.frame.is_maximized();
            let was = self.ws.float_rect(w.tab);
            let (p, s) = (w.frame.get_position(), w.frame.get_size());
            // A maximised window fills the screen: the size to go back to is the one it had before.
            let rect = match was {
                Some(r) if maximized => FloatRect { maximized, ..r },
                _ => FloatRect { x: p.x, y: p.y, w: s.width, h: s.height, maximized },
            };
            self.ws.set_float_rect(w.tab, rect);
        }
    }

    /// The program is ending: its windows go with it.
    pub(super) fn close_float_windows(&mut self) {
        // Out of their views first: nothing may reach an editor that has gone.
        self.ws.focus_pane(false);
        for w in self.float_windows.drain(..) {
            w.frame.destroy();
        }
    }
}
