//! Floating tabs on screen: each is a window of its own with a Dock Back bar over its editor or whiteboard page. The
//! workspace decides what floats and which view is current (see `workspace::floats`); this keeps the windows matching it
//! and tells it where the keyboard went.
//!
//! The windows have no parent, so that they minimise, maximise and sit on the taskbar on their own, and the menu shortcuts
//! are the main window's: here the keys go through the key map instead.

use notepp_core::session::FloatRect;
use notepp_core::workspace::TabKind;

use super::*;
use crate::widgets::pane_header::{HeaderEvent, PaneHeader};

/// What a floating window shows.
#[derive(Clone)]
pub(super) enum Body {
    Editor(StyledTextCtrl),
    Board(WebPane),
    Picture(WebPane),
}

/// What to do once a whiteboard page has handed back its latest scene.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum After {
    Close,
    DockBack,
}

/// One floating window.
pub struct FloatWindow {
    pub(super) tab: u64,
    frame: Frame,
    head: PaneHeader,
    pub(super) body: Body,
    title: String,
    pub(super) after: Option<After>,
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
    {
        let Ok(mut app) = a.try_borrow_mut() else { return };
        let i = app.ws.active;
        if !app.ws.can_float(i) {
            app.flash = Some("One tab always stays in the main window".into());
            app.refresh();
            return;
        }
        // The last strokes are still in the page: the window opens when they have come back.
        let page = app.current_page();
        if app.ws.is_page() && page.is_ready() {
            app.wb.then = Some(crate::state::WbThen::Float);
            let kind = app.ws.tabs[i].kind;
            dock_board::request_from_main_loop(&app.frame, page, kind);
            return;
        }
    }
    undock_now(a);
}

/// The active tab becomes a window, with nothing left to wait for.
pub(crate) fn undock_now(a: &Shared) {
    let Some(i) = a.try_borrow().ok().map(|app| app.ws.active) else { return };
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
    // Where it was, if its title bar is still on a screen, or a size that fits the main screen: the same rule as the main window.
    let r = notepp_core::placement::place(rect, t.px(WIDTH), t.px(HEIGHT), &crate::platform::wx::screen_areas());
    Frame::builder().with_title(title).with_position(Point::new(r.x, r.y)).with_size(Size::new(r.w, r.h)).build()
}

/// Where a window is now, as the session keeps it.
fn rect_of(frame: &Frame) -> FloatRect {
    let (p, s) = (frame.get_position(), frame.get_size());
    FloatRect { x: p.x, y: p.y, w: s.width, h: s.height, maximized: frame.is_maximized() }
}

/// The window's content, added to `column`, with its tab moved to the workspace's floating ones. `None` if it cannot be made.
fn fill(app: &mut App, panel: &Panel, i: usize, column: &BoxSizer) -> Option<Body> {
    if app.ws.tabs[i].kind == TabKind::Image {
        let page = float_picture::page(panel);
        column.add(&page, 1, SizerFlag::Expand, 0);
        return app.ws.float_tab(i, None).then_some(Body::Picture(page));
    }
    if app.ws.tabs[i].kind == TabKind::Whiteboard {
        let Some(page) = float_board::page(panel) else {
            app.flash = Some("Whiteboard unavailable: assets/whiteboard not found".into());
            return None;
        };
        column.add(&page, 1, SizerFlag::Expand, 0);
        return app.ws.float_tab(i, None).then_some(Body::Board(page));
    }
    let stc = StyledTextCtrl::builder(panel).build();
    no_border(&stc);
    column.add(&stc, 1, SizerFlag::Expand, 0);
    app.ws.float_tab(i, Some(WxSci::of(&stc))).then_some(Body::Editor(stc))
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
    let column = BoxSizer::builder(Orientation::Vertical).build();
    column.add(&head.panel(), 0, SizerFlag::Expand, 0);
    let Some(body) = fill(&mut app, &panel, i, &column) else {
        frame.destroy();
        return;
    };
    panel.set_sizer(column, true);
    app.float_windows.push(FloatWindow { tab, frame, head: head.clone(), body: body.clone(), title, after: None });
    let main = app.frame;
    drop(app);
    wire_window(a, main, frame, tab, &head);
    match &body {
        Body::Editor(stc) => wire_editor(a, main, frame, tab, *stc),
        Body::Board(page) => float_board::wire(a, tab, page),
        Body::Picture(page) => float_picture::wire(a, Some(tab), page),
    }
    show(a, frame, tab, rect, &body);
}

/// Put the finished window on screen.
fn show(a: &Shared, frame: Frame, tab: u64, rect: Option<FloatRect>, body: &Body) {
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
    if rect.is_none()
        && let Body::Editor(stc) = body
    {
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

/// The user closed the window, or pressed Dock Back: a whiteboard hands back its last strokes first.
fn finish(app: &Shared, tab: u64, then: After) {
    let Ok(mut a) = app.try_borrow_mut() else { return };
    if !a.ask_board_first(tab, then) {
        a.finish_window(tab, then);
    }
}

/// What every floating window needs.
fn wire_window(a: &Shared, main: Frame, frame: Frame, tab: u64, head: &PaneHeader) {
    float_keys::wire(a, frame);
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
        crate::widgets::defer::later(&main, move || finish(&app, tab, After::Close));
    });
    // A page takes the keyboard without the toolkit hearing of it, so the window's activation says where the user is.
    let app = a.clone();
    frame.on_activate(move |e| {
        if let wxdragon::event::WindowEventData::Activate(act) = &e
            && act.is_active()
        {
            let app = app.clone();
            crate::widgets::defer::later(&main, move || float_focused(&app, main, tab));
        }
        e.skip(true);
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
        HeaderEvent::Focus => {
            if let Ok(a) = app.try_borrow() {
                a.focus_window(tab);
            }
        }
        HeaderEvent::Close => {
            let app = app.clone();
            crate::widgets::defer::later(&main, move || finish(&app, tab, After::DockBack));
        }
    });
}

/// What a floating editor needs from the main view's wiring.
fn wire_editor(a: &Shared, main: Frame, frame: Frame, tab: u64, stc: StyledTextCtrl) {
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
}

impl App {
    pub(super) fn window_of(&self, tab: u64) -> Option<&FloatWindow> {
        self.float_windows.iter().find(|w| w.tab == tab)
    }

    /// The editor of the floating window the user is in.
    pub(super) fn float_stc(&self) -> Option<StyledTextCtrl> {
        match self.window_of(self.ws.current_float()?)?.body {
            Body::Editor(stc) => Some(stc),
            Body::Board(_) | Body::Picture(_) => None,
        }
    }

    fn focus_window(&self, tab: u64) {
        match self.window_of(tab).map(|w| &w.body) {
            Some(Body::Editor(stc)) => stc.set_focus(),
            Some(Body::Board(page) | Body::Picture(page)) => page.set_focus(),
            None => {}
        }
    }

    pub(super) fn finish_window(&mut self, tab: u64, then: After) {
        match then {
            After::DockBack => self.dock_back_window(tab),
            After::Close => {
                if let Some(i) = self.ws.tabs.iter().position(|t| t.id == tab) {
                    self.ws.close(&self.ui, i);
                }
                self.refresh();
            }
        }
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
        let dark = self.ws.theme.dark;
        let headings = self.ws.float_headings();
        for w in &mut self.float_windows {
            let Some((_, title, heading)) = headings.iter().find(|(tab, ..)| *tab == w.tab) else { continue };
            if *title != w.title {
                w.frame.set_title(title);
                w.title.clone_from(title);
            }
            w.head.set(&heading.title, heading.modified, heading.current);
            match &w.body {
                Body::Board(page) if page.is_ready() && page.sync_dark(dark) => wbm::set_theme(page, dark),
                Body::Picture(page) if page.is_ready() && page.sync_dark(dark) => crate::imageedit::set_theme(page, dark),
                _ => {}
            }
        }
        if let Some(tab) = self.ws.take_raise()
            && let Some(w) = self.window_of(tab)
        {
            if w.frame.is_iconized() {
                w.frame.iconize(false);
            }
            w.frame.raise();
            self.focus_window(tab);
        }
    }

    /// The tab goes back to the strip and the keyboard with it to the main window.
    pub(super) fn dock_back_window(&mut self, tab: u64) {
        self.ws.send_back(tab);
        self.refresh();
        crate::widgets::native::current().focus_window(self.frame.get_handle() as isize);
        self.focus_shown();
    }

    /// Dock Back from a menu: a whiteboard hands back its last strokes first.
    pub(super) fn dock_back_current(&mut self, tab: u64) {
        if !self.ask_board_first(tab, After::DockBack) {
            self.dock_back_window(tab);
        }
    }

    /// Note where the main window is, for the session (the rule is `placement::remember`).
    pub(super) fn keep_main_rect(&mut self) {
        self.ws.main_window = notepp_core::placement::remember(self.ws.main_window, rect_of(&self.frame), self.frame.is_iconized());
    }

    /// Note where the windows are, for the session.
    pub(super) fn keep_float_rects(&mut self) {
        for w in &self.float_windows {
            if let Some(rect) = notepp_core::placement::remember(self.ws.float_rect(w.tab), rect_of(&w.frame), w.frame.is_iconized()) {
                self.ws.set_float_rect(w.tab, rect);
            }
        }
    }

    /// The main window came forward. A page takes the keyboard without a word, so nothing else would say so when a
    /// floating page was current, or when the main window shows one.
    pub(super) fn main_activated(&mut self) {
        if self.ws.current_float().is_some() && (self.ws.is_page() || self.ws.main_is_page()) {
            self.ws.focus_pane(false);
            self.refresh();
        }
    }
}
