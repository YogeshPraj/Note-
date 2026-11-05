//! The docked tab on screen: a splitter holds the main editor and the docked one. The workspace decides what is docked
//! and which view is current (see `workspace::dock`); this keeps the window matching it and tells it where the keyboard went.

use notepp_core::commands as c;
use notepp_core::session::DockSide;

use super::*;
use crate::command::CommandHandler;
use crate::widgets::pane_header::{HeaderEvent, PaneHeader};

/// The two panes: the splitter, and in each pane a header above its editor.
pub struct DockPanes {
    pub(super) split: SplitterWindow,
    main_box: Panel,
    dock_box: Panel,
    main_head: PaneHeader,
    dock_head: PaneHeader,
    /// Where a docked whiteboard's page and a docked picture's go, in place of the docked editor.
    pub(super) boxes: [Panel; 2],
    /// Those pages, once one of the kind has been docked.
    pub(super) pages: [Option<dock_board::DockBoard>; 2],
    /// The side the docked pane is showing on.
    shown: Option<DockSide>,
}

impl std::fmt::Debug for DockPanes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DockPanes").field("shown", &self.shown).finish_non_exhaustive()
    }
}

impl DockPanes {
    /// One pane: a header (hidden until something is docked) over an editor.
    fn pane(split: SplitterWindow, badge: Option<&'static str>, closable: bool) -> (Panel, PaneHeader, StyledTextCtrl, [Panel; 2]) {
        let pane = Panel::builder(&split).build();
        let head = PaneHeader::new(&pane, badge, closable);
        head.panel().show(false);
        let editor = StyledTextCtrl::builder(&pane).build();
        let column = BoxSizer::builder(Orientation::Vertical).build();
        column.add(&head.panel(), 0, SizerFlag::Expand, 0);
        column.add(&editor, 1, SizerFlag::Expand, 0);
        let boxes = [Panel::builder(&pane).build(), Panel::builder(&pane).build()];
        for b in &boxes {
            b.show(false);
            column.add(b, 1, SizerFlag::Expand, 0);
        }
        pane.set_sizer(column, true);
        (pane, head, editor, boxes)
    }

    /// The panes inside `parent`, showing only the main one, and their two editors.
    pub fn build(parent: &Panel) -> (DockPanes, StyledTextCtrl, StyledTextCtrl) {
        let style = wxdragon::widgets::splitter_window::SplitterWindowStyle::ThinSash;
        let split = SplitterWindow::builder(parent).with_style(style).build();
        let (main_box, main_head, main_stc, _) = Self::pane(split, None, false);
        let (dock_box, dock_head, dock_stc, boxes) = Self::pane(split, Some("dock"), true);
        dock_box.show(false);
        crate::widgets::theme::own_sash(split, main_box);
        split.initialize(&main_box);
        split.set_minimum_pane_size(crate::widgets::theme::current().px(120));
        (DockPanes { split, main_box, dock_box, main_head, dock_head, boxes, pages: [None, None], shown: None }, main_stc, dock_stc)
    }

    pub(super) fn layout_dock(&self) {
        self.dock_box.layout();
    }

    fn show_headers(&self, on: bool) {
        self.main_head.panel().show(on);
        self.dock_head.panel().show(on);
        self.main_box.layout();
        self.dock_box.layout();
    }
}

fn vertical(side: DockSide) -> bool {
    matches!(side, DockSide::Left | DockSide::Right)
}

/// Does the docked view come first (left of or above the main one)?
fn dock_first(side: DockSide) -> bool {
    matches!(side, DockSide::Left | DockSide::Top)
}

impl App {
    /// The editor view the user is in.
    pub(crate) fn stc(&self) -> StyledTextCtrl {
        self.float_stc().unwrap_or(if self.ws.in_dock() { self.dock_stc } else { self.main_stc })
    }

    /// The length the splitter divides for a dock on `side`.
    fn dock_room(&self, side: DockSide) -> i32 {
        let area = self.dock_panes.split.get_client_size();
        if vertical(side) { area.width } else { area.height }
    }

    /// Show the docked view when the workspace has one, on its side; hide it when it has none.
    pub(crate) fn sync_dock(&mut self) {
        let side = self.ws.dock_side();
        if side != self.dock_panes.shown {
            self.move_dock(side);
        }
        if side.is_some() {
            self.name_panes();
        }
        self.sync_dock_board();
    }

    /// Say in each header what its pane holds.
    fn name_panes(&self) {
        let (main, in_dock) = (self.ws.main_index(), self.ws.in_dock());
        let dock = self.ws.docked_index().unwrap_or(main);
        self.dock_panes.main_head.set(&self.ws.tabs[main].title, self.ws.is_dirty(main), !in_dock);
        self.dock_panes.dock_head.set(&self.ws.tabs[dock].title, self.ws.is_dirty(dock), in_dock);
    }

    fn move_dock(&mut self, side: Option<DockSide>) {
        if self.dock_panes.shown.is_some() {
            self.keep_dock_share();
            self.dock_panes.split.unsplit(Some(&self.dock_panes.dock_box));
        }
        self.dock_panes.shown = side;
        self.dock_panes.show_headers(side.is_some());
        let Some(side) = side else { return };
        let room = self.dock_room(side);
        let dock_px = room * crate::platform::wx::to_i32(self.ws.dock_share()) / 1000;
        let (main, dock) = (self.dock_panes.main_box, self.dock_panes.dock_box);
        let (first, second, sash) = if dock_first(side) { (dock, main, dock_px) } else { (main, dock, room - dock_px) };
        if vertical(side) {
            self.dock_panes.split.split_vertically(&first, &second, sash);
        } else {
            self.dock_panes.split.split_horizontally(&first, &second, sash);
        }
        self.dock_panes.show_headers(true);
        self.ws.refresh_docked();
    }

    /// Note where the user left the splitter, so the session and the next dock keep it.
    pub(crate) fn keep_dock_share(&mut self) {
        let Some(side) = self.dock_panes.shown else { return };
        let room = self.dock_room(side);
        if room <= 0 {
            return;
        }
        let sash = self.dock_panes.split.sash_position();
        let dock_px = if dock_first(side) { sash } else { room - sash };
        self.ws.set_dock_share(crate::platform::wx::to_u32(dock_px * 1000 / room));
    }
}

/// The keyboard went into one of the views: it becomes the current one.
fn pane_focused(app: &Shared, frame: Frame, dock: bool) {
    let Ok(mut a) = app.try_borrow_mut() else {
        // A command is running: look again when it has finished.
        let again = app.clone();
        return crate::widgets::defer::later(&frame, move || {
            if let Ok(mut a) = again.try_borrow_mut() {
                a.ws.focus_pane(dock);
                a.refresh();
            }
        });
    };
    if a.ws.current_float().is_some() || (a.ws.dock_side().is_some() && a.ws.in_dock() != dock) {
        a.ws.focus_pane(dock);
        a.refresh();
    }
}

/// What both views need beyond the main one's wiring: the docked view's changes count, and focus picks the current view.
pub(super) fn wire_dock(app: &Shared, frame: Frame, main: StyledTextCtrl, dock: StyledTextCtrl) {
    dock.on_stc_change(|_| {
        EDITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    });
    let a = app.clone();
    dock.on_stc_update_ui(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.refresh();
        }
    });
    for (view, in_dock) in [(main, false), (dock, true)] {
        let a = app.clone();
        view.on_set_focus(move |e| {
            e.skip(true);
            pane_focused(&a, frame, in_dock);
        });
    }
    let dock_panes = &app.borrow().dock_panes;
    dock_panes.main_head.on_event(move |_| main.set_focus());
    let a = app.clone();
    dock_panes.dock_head.on_event(move |e| match e {
        HeaderEvent::Focus => {
            if let Ok(mut app) = a.try_borrow_mut() {
                app.ws.focus_pane(true);
                app.refresh();
                app.focus_shown();
            }
        }
        HeaderEvent::Close => crate::app::run_command(&a, c::ID_UNDOCK),
    });
}

/// Tab: Dock, Undock and moving between the views.
pub struct DockCommands;

impl CommandHandler<App> for DockCommands {
    fn name(&self) -> &'static str {
        "dock"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_UNDOCK => match app.ws.current_float() {
                Some(tab) => app.dock_back_current(tab),
                None if app.ws.dock_is_page() => app.dock_back_page(),
                None => app.ws.undock(),
            },
            c::ID_FOCUS_OTHER => app.ws.focus_other_pane(),
            _ => return false,
        }
        app.refresh();
        app.focus_shown();
        true
    }
}
