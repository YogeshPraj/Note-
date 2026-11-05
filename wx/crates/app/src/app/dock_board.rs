//! A whiteboard or picture in the dock: a page beside the main editor, one of each kind, made the first time one docks.
//! The workspace leaves the second editor view alone for them (see `workspace::dock`).

use notepp_core::session::DockSide;
use notepp_core::workspace::TabKind;

use super::dock_pane::DockPanes;
use super::*;
use crate::state::WbThen;

/// A page of the dock, and what it shows.
pub struct DockBoard {
    pub(super) page: WebPane,
    /// The tab whose scene the page has.
    shown: Option<u64>,
    /// Dock Back goes on once the page has handed back its latest scene.
    pub(super) waiting: bool,
}

impl std::fmt::Debug for DockBoard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockBoard").field("shown", &self.shown).finish_non_exhaustive()
    }
}

/// Dock the active tab on `side`. A whiteboard in the main window hands back its last strokes first.
pub(crate) fn dock(a: &Shared, side: DockSide) {
    {
        let Ok(mut app) = a.try_borrow_mut() else { return };
        let i = app.ws.active;
        if app.ws.docked_index() != Some(i) && !app.ws.can_dock(i) {
            app.flash = Some("A text tab has to stay beside what is docked".into());
            app.refresh();
            return;
        }
        let page = app.current_page();
        if app.ws.is_page() && app.ws.docked_index() != Some(i) && page.is_ready() {
            app.wb.then = Some(WbThen::Dock(side));
            let kind = app.ws.tabs[i].kind;
            request_from_main_loop(&app.frame, page, kind);
            return;
        }
    }
    dock_now(a, side);
}

/// From the main loop: a menu that is closing is still in charge now, and a script sent then is lost.
pub(super) fn request_from_main_loop(frame: &Frame, page: WebPane, kind: TabKind) {
    crate::widgets::defer::later(frame, move || {
        if kind == TabKind::Image {
            crate::imageedit::flush(&page);
        } else {
            wbm::request_data(&page);
        }
    });
}

fn dock_now(a: &Shared, side: DockSide) {
    let kind = a.try_borrow().ok().map(|app| app.ws.tabs[app.ws.active].kind);
    if let Some(kind) = kind {
        ensure_page(a, kind);
    }
    let Ok(mut app) = a.try_borrow_mut() else { return };
    let i = app.ws.active;
    if !app.ws.dock_tab(i, side) {
        app.flash = Some("A text tab has to stay beside what is docked".into());
    }
    app.refresh();
    app.focus_shown();
}

/// The scene the app asked for has arrived: do what it was asked for.
pub(super) fn scene_arrived(a: &Shared) {
    let Ok(mut app) = a.try_borrow_mut() else { return };
    let Some(then) = app.wb.then.take() else { return };
    let frame = app.frame;
    drop(app);
    let a = a.clone();
    crate::widgets::defer::later(&frame, move || match then {
        WbThen::Float => float_pane::undock_now(&a),
        WbThen::Dock(side) => dock_now(&a, side),
    });
}

/// The dock the last session had, once the window has its size.
pub(super) fn restore(a: &Shared) {
    let saved = a.borrow_mut().ws.take_saved_dock();
    let Some(d) = saved else { return };
    let kind = a.borrow().ws.tabs.get(d.tab).map(|t| t.kind);
    if let Some(kind) = kind {
        ensure_page(a, kind);
    }
    let mut app = a.borrow_mut();
    app.ws.set_dock_share(d.share);
    app.ws.dock_tab(d.tab, d.side);
    app.refresh();
}

/// Make the dock's page for `kind`, the first time. A text tab needs none.
fn ensure_page(a: &Shared, kind: TabKind) {
    let Some(slot) = DockPanes::slot(kind) else { return };
    let Ok(mut app) = a.try_borrow_mut() else { return };
    if app.dock_panes.pages[slot].is_some() {
        return;
    }
    let host = app.dock_panes.boxes[slot];
    let page = if kind == TabKind::Image { Some(float_picture::page(&host)) } else { float_board::page(&host) };
    let Some(page) = page else {
        app.flash = Some("Whiteboard unavailable: assets/whiteboard not found".into());
        return;
    };
    let column = BoxSizer::builder(Orientation::Vertical).build();
    column.add(&page, 1, SizerFlag::Expand, 0);
    host.set_sizer(column, true);
    app.dock_panes.pages[slot] = Some(DockBoard { page: page.clone(), shown: None, waiting: false });
    drop(app);
    if kind == TabKind::Image {
        float_picture::wire(a, None, &page);
    } else {
        wire(a, &page);
    }
}

/// Messages from the dock's whiteboard page.
fn wire(a: &Shared, page: &WebPane) {
    let app = a.clone();
    page.on_script_message_received(move |e| {
        let Some(ev) = wbm::parse(&e.get_string().unwrap_or_default()) else { return };
        match ev {
            wbm::Event::Tab(delta) => return switch_tab(&app, delta),
            wbm::Event::Menu(key) => return open_menu_for_key(&app, &key),
            _ => {}
        }
        let scene_is_in = matches!(ev, wbm::Event::Data(_));
        let Some(tab) = app.try_borrow().ok().and_then(|a| a.ws.docked_tab()) else { return };
        float_board::message(&app, tab, ev);
        if scene_is_in {
            scene_arrived(&app);
        }
    });
}

impl DockPanes {
    /// Which of the dock's pages shows `kind`.
    pub(super) fn slot(kind: TabKind) -> Option<usize> {
        match kind {
            TabKind::Whiteboard => Some(0),
            TabKind::Image => Some(1),
            TabKind::Text => None,
        }
    }

    pub(super) fn page(&self, kind: TabKind) -> Option<&DockBoard> {
        self.pages[Self::slot(kind)?].as_ref()
    }

    pub(super) fn page_mut(&mut self, kind: TabKind) -> Option<&mut DockBoard> {
        self.pages[Self::slot(kind)?].as_mut()
    }
}

impl App {
    /// Show the dock's page or its editor, whichever the docked tab needs, and give the page its scene.
    pub(super) fn sync_dock_board(&mut self) {
        let kind = self.ws.dock_kind().filter(|k| *k != TabKind::Text);
        let want = kind.and_then(DockPanes::slot);
        let mut changed = false;
        for (slot, host) in self.dock_panes.boxes.into_iter().enumerate() {
            if host.is_shown() != (want == Some(slot)) {
                host.show(want == Some(slot));
                changed = true;
            }
            if want != Some(slot)
                && let Some(b) = &mut self.dock_panes.pages[slot]
            {
                b.shown = None;
            }
        }
        if changed {
            self.dock_stc.show(want.is_none());
            self.dock_panes.layout_dock();
        }
        let (Some(kind), Some(id)) = (kind, self.ws.docked_tab()) else { return };
        let Some(page) = self.dock_panes.page(kind).filter(|b| b.page.is_ready() && b.shown != Some(id)).map(|b| b.page.clone()) else {
            return;
        };
        if let Some(b) = self.dock_panes.page_mut(kind) {
            b.shown = Some(id);
        }
        let dark = self.ws.theme.dark;
        if kind == TabKind::Image {
            crate::imageedit::set_theme(&page, dark);
            self.give_picture(id, &page);
        } else if let Some(t) = self.ws.tabs.iter().find(|t| t.id == id) {
            wbm::set_theme(&page, dark);
            wbm::load(&page, &t.content);
        }
    }

    /// Dock Back on a docked whiteboard or picture: it hands back its last strokes first.
    pub(super) fn dock_back_page(&mut self) {
        match self.ws.docked_tab() {
            Some(tab) if self.ask_board_first(tab, float_pane::After::DockBack) => {}
            _ => self.ws.undock(),
        }
    }
}
