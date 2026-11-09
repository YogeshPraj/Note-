//! The docked tab: one tab shown in a second editor view beside the main one, both editable.
//!
//! `sci` and `active` always mean the view the user is in and its tab, so every command, the status bar and the preview
//! follow the keyboard without knowing about docking: moving between the views swaps them. The other view waits in
//! `second`. The docked tab is fixed: a tab chosen from the strip always opens in the main view.
//!
//! A docked whiteboard or picture has a page beside the main editor instead of a view: `second` stays on its empty document, nothing
//! is swapped, and `active` alone says which side the user is on.

use super::*;

/// The view that is not current, and the empty document it rests on while nothing is docked.
#[derive(Debug)]
pub(super) struct Second<S> {
    pub(super) view: S,
    pub(super) idle: isize,
}

/// What is docked, while something is.
#[derive(Debug, Clone, Copy)]
pub(super) struct Docked {
    /// The tab in the view that is not current.
    pub(super) other: u64,
    /// The docked tab is the current one.
    pub(super) in_dock: bool,
    pub(super) side: DockSide,
}

impl<S: Send> Workspace<S> {
    /// Give the workspace its second view, which needs the same look as the first.
    pub fn attach_second(&mut self, view: S) {
        self.style_view(&view, false);
        sci::set_wrap(&view, self.wrap);
        view::apply_symbols(&view, &self.symbols);
        // Its own empty document must survive others being swapped in (a view frees the one it leaves).
        let idle = view.send(sci::SCI_GETDOCPOINTER, 0, 0);
        view.send(sci::SCI_ADDREFDOCUMENT, 0, idle);
        self.second = Some(Second { view, idle });
    }

    /// The side the docked tab is on, while one is docked.
    pub fn dock_side(&self) -> Option<DockSide> {
        self.dock.map(|d| d.side)
    }

    /// Is the docked tab the current one?
    pub fn in_dock(&self) -> bool {
        self.dock.is_some_and(|d| d.in_dock)
    }

    /// The index of the docked tab.
    pub fn docked_index(&self) -> Option<usize> {
        if self.in_dock() { Some(self.active) } else { self.other_index() }
    }

    /// The index of the tab in the main view.
    pub fn main_index(&self) -> usize {
        if let Some(home) = self.in_float {
            return self.index_of(home).unwrap_or(0);
        }
        if self.in_dock() { self.other_index().unwrap_or(self.active) } else { self.active }
    }

    /// The index of the tab in the view that is not current.
    pub(super) fn other_index(&self) -> Option<usize> {
        let other = self.dock?.other;
        self.tabs.iter().position(|t| t.id == other)
    }

    /// Can tab `i` be docked? It must leave a text tab for the main view (which can be the docked tab, since that goes
    /// back among the others).
    pub fn can_dock(&self, i: usize) -> bool {
        let text = |t: &Tab| t.kind == TabKind::Text;
        let strip = &self.tabs[..self.main_count()];
        self.second.is_some() && i < strip.len() && strip.iter().enumerate().any(|(j, t)| j != i && text(t))
    }

    /// Is the docked tab a whiteboard or a picture?
    pub fn dock_is_page(&self) -> bool {
        self.dock_kind().is_some_and(|k| k != TabKind::Text)
    }

    /// What kind of tab is docked.
    pub fn dock_kind(&self) -> Option<TabKind> {
        self.docked_index().map(|i| self.tabs[i].kind)
    }

    /// The docked tab's id.
    pub fn docked_tab(&self) -> Option<u64> {
        self.docked_index().map(|i| self.tabs[i].id)
    }

    /// The tab nearest `i` for the main view to show instead of it: never `i` or the docked tab.
    pub(super) fn neighbour_of(&self, i: usize, text_only: bool) -> Option<usize> {
        let docked = self.docked_index();
        let fits = |j: &usize| *j != i && Some(*j) != docked && (!text_only || self.tabs[*j].kind == TabKind::Text);
        let strip = self.main_count();
        (i + 1..strip).find(|j| fits(j)).or_else(|| (0..i.min(strip)).rev().find(|j| fits(j)))
    }

    /// Dock tab `i` on `side`, or move it there if it is docked already. Whatever was docked goes back to the strip, and
    /// a tab the main view was showing gives way to its neighbour. False if it cannot be docked.
    pub fn dock_tab(&mut self, i: usize, side: DockSide) -> bool {
        self.leave_float();
        if self.docked_index() == Some(i) {
            if let Some(d) = &mut self.dock {
                d.side = side;
            }
            return true;
        }
        let id = self.tabs.get(i).map(|t| t.id);
        self.undock();
        let Some(i) = id.and_then(|id| self.tabs.iter().position(|t| t.id == id)) else { return false };
        if !self.can_dock(i) {
            return false;
        }
        if i == self.active
            && let Some(j) = self.neighbour_of(i, true)
        {
            self.activate(j);
        }
        let Some(second) = &self.second else { return false };
        // A page has no document: it is the app's.
        if self.tabs[i].kind == TabKind::Text {
            second.view.send(sci::SCI_SETDOCPOINTER, 0, self.tabs[i].doc);
            lang::apply_palette(&second.view, self.tabs[i].lang, &self.theme);
        }
        self.dock = Some(Docked { other: self.tabs[i].id, in_dock: false, side });
        self.with_docked(|w| w.apply_position());
        true
    }

    /// Put the docked tab back among the others, and the keyboard in the main view.
    pub fn undock(&mut self) {
        if self.dock.is_none() {
            return;
        }
        self.leave_dock();
        self.with_docked(Self::remember_view);
        self.dock = None;
        if let Some(s) = &self.second {
            s.view.send(sci::SCI_SETDOCPOINTER, 0, s.idle);
        }
    }

    /// Be in the view the user put the keyboard in (`dock`: the docked one).
    pub fn focus_pane(&mut self, dock: bool) {
        self.leave_float();
        if self.dock.is_some() && self.in_dock() != dock {
            self.swap_panes();
        }
    }

    /// Go to the other view.
    pub fn focus_other_pane(&mut self) {
        if let Some(i) = if self.in_dock() { self.other_index() } else { self.docked_index() } {
            self.activate(i);
        }
    }

    /// Be in the main view: a new tab or another document must not take the docked view's place.
    pub(super) fn leave_dock(&mut self) {
        if self.in_dock() {
            self.swap_panes();
        }
    }

    /// The tab strip asked for tab `i`: true if going to the view it is in was all there was to do.
    pub(super) fn route_to_pane(&mut self, i: usize) -> bool {
        let Some(docked) = self.docked_index() else { return false };
        if i != docked {
            self.leave_dock();
            return false;
        }
        if self.in_dock() {
            return true;
        }
        if self.is_page() {
            // The main view shows nothing behind a whiteboard: it needs a text tab beside the dock.
            let Some(j) = self.neighbour_of(i, true) else {
                self.undock();
                return false;
            };
            self.activate(j);
        }
        self.swap_panes();
        true
    }

    fn swap_panes(&mut self) {
        let Some(target) = self.other_index() else { return };
        self.remember_view();
        let id = self.tabs[self.active].id;
        let page = self.dock_is_page();
        if let (Some(s), Some(d)) = (&mut self.second, &mut self.dock) {
            if !page {
                std::mem::swap(&mut self.sci, &mut s.view);
            }
            d.other = id;
            d.in_dock = !d.in_dock;
            self.active = target;
        }
        if let Some(m) = &self.mini {
            let t = &self.tabs[self.active];
            if t.kind == TabKind::Text && !page {
                m.send(sci::SCI_SETDOCPOINTER, 0, t.doc);
                lang::apply_palette(m, t.lang, &self.theme);
            }
        }
    }

    /// Run `f` as if the docked tab were current, without moving the keyboard or the minimap.
    pub(super) fn with_docked(&mut self, f: impl FnOnce(&mut Self)) {
        if self.dock_is_page() {
            return;
        }
        if self.in_dock() {
            return f(self);
        }
        let Some(i) = self.docked_index() else { return };
        let main = self.active;
        self.swap_views();
        self.active = i;
        f(self);
        self.active = main;
        self.swap_views();
    }

    fn swap_views(&mut self) {
        if let Some(s) = &mut self.second {
            std::mem::swap(&mut self.sci, &mut s.view);
        }
    }

    /// The docked view has been laid out: put its scroll position back.
    pub fn refresh_docked(&mut self) {
        self.with_docked(|w| w.apply_position());
    }

    /// How much of the shared space the docked view has, in thousandths.
    pub fn dock_share(&self) -> u32 {
        self.dock_share
    }

    pub fn set_dock_share(&mut self, share: u32) {
        self.dock_share = share.clamp(100, 900);
    }

    /// How the dock splits a space `room` pixels long, when a tab is docked.
    pub fn dock_layout(&self, room: i32) -> Option<crate::dock_layout::DockLayout> {
        self.dock_side().map(|side| crate::dock_layout::layout(side, self.dock_share, room))
    }

    /// The user left the divider at `divider` in a space `room` long: keep that share for the session and the next dock. A
    /// space with no size yet says nothing.
    pub fn keep_dock_divider(&mut self, divider: i32, room: i32) {
        if let Some(share) = self.dock_side().and_then(|side| crate::dock_layout::share_from(side, divider, room)) {
            self.set_dock_share(share);
        }
    }

    /// What the two pane headers say, the main pane's first, when a tab is docked.
    pub fn dock_headings(&self) -> Option<(crate::dock_layout::PaneHeading, crate::dock_layout::PaneHeading)> {
        self.dock_side()?;
        let (main, in_dock) = (self.main_index(), self.in_dock());
        let dock = self.docked_index().unwrap_or(main);
        let heading = |i: usize, current: bool| crate::dock_layout::PaneHeading { title: self.tabs[i].title.clone(), modified: self.is_dirty(i), current };
        Some((heading(main, !in_dock), heading(dock, in_dock)))
    }

    /// The dock the session saved, once: it can be applied when the window is on screen.
    pub fn take_saved_dock(&mut self) -> Option<SessionDock> {
        self.saved_dock.take()
    }
}
