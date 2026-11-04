//! The docked tab on screen: a splitter holds the main editor and the docked one. The workspace decides what is docked
//! and which view is current (see `workspace::dock`); this keeps the window matching it and tells it where the keyboard went.

use notepp_core::commands as c;
use notepp_core::session::DockSide;

use super::*;
use crate::command::CommandHandler;

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
        if self.ws.in_dock() { self.dock_stc } else { self.main_stc }
    }

    /// The length the splitter divides for a dock on `side`.
    fn dock_room(&self, side: DockSide) -> i32 {
        let area = self.dock_split.get_client_size();
        if vertical(side) { area.width } else { area.height }
    }

    /// Show the docked view when the workspace has one, on its side; hide it when it has none.
    pub(crate) fn sync_dock(&mut self) {
        let side = self.ws.dock_side();
        if side == self.dock_shown {
            return;
        }
        if self.dock_shown.is_some() {
            self.keep_dock_share();
            self.dock_split.unsplit(Some(&self.dock_stc));
        }
        self.dock_shown = side;
        let Some(side) = side else { return };
        let room = self.dock_room(side);
        let dock_px = room * crate::platform::wx::to_i32(self.ws.dock_share()) / 1000;
        let (first, second, sash) = if dock_first(side) { (self.dock_stc, self.main_stc, dock_px) } else { (self.main_stc, self.dock_stc, room - dock_px) };
        if vertical(side) {
            self.dock_split.split_vertically(&first, &second, sash);
        } else {
            self.dock_split.split_horizontally(&first, &second, sash);
        }
        self.ws.refresh_docked();
    }

    /// Note where the user left the splitter, so the session and the next dock keep it.
    pub(crate) fn keep_dock_share(&mut self) {
        let Some(side) = self.dock_shown else { return };
        let room = self.dock_room(side);
        if room <= 0 {
            return;
        }
        let sash = self.dock_split.sash_position();
        let dock_px = if dock_first(side) { sash } else { room - sash };
        self.ws.set_dock_share(crate::platform::wx::to_u32(dock_px * 1000 / room));
    }

    /// The dock the last session had, once the window has its size.
    pub(crate) fn restore_dock(&mut self) {
        if let Some(d) = self.ws.take_saved_dock() {
            self.ws.set_dock_share(d.share);
            self.ws.dock_tab(d.tab, d.side);
            self.refresh();
        }
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
    if a.ws.dock_side().is_some() && a.ws.in_dock() != dock {
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
}

/// Tab: Dock, Undock and moving between the views.
pub struct DockCommands;

impl CommandHandler<App> for DockCommands {
    fn name(&self) -> &'static str {
        "dock"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_UNDOCK => app.ws.undock(),
            c::ID_FOCUS_OTHER => app.ws.focus_other_pane(),
            id if (c::ID_DOCK_BASE..c::ID_DOCK_BASE + DockSide::ALL.len()).contains(&id) => {
                let i = app.ws.active;
                if !app.ws.dock_tab(i, DockSide::ALL[id - c::ID_DOCK_BASE]) {
                    app.flash = Some("Only a text tab can be docked, with another text tab open beside it".into());
                }
            }
            _ => return false,
        }
        app.refresh();
        if !app.ws.is_whiteboard() {
            app.stc().set_focus();
        }
        true
    }
}
