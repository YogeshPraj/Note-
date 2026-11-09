//! Floating tabs: a tab that left the strip and lives in a window of its own: a text tab with an editor view of its own,
//! a whiteboard or picture with a page of its own, which the workspace knows nothing about.
//!
//! As with the dock, `sci` and `active` mean the view the user is in and its tab, so commands, the status bar and the
//! preview need to know nothing about floating. Going into a floating window swaps its view into `sci` and leaves the
//! main view waiting in that window's slot; `in_float` says which tab the main window shows meanwhile.
//!
//! Floating tabs stay in `tabs`, always at the end: the strip draws the first [`Workspace::main_count`], so its indices
//! are `tabs` indices and every loop over the tabs (Save All, the session, closing) still sees them. A tab dropped back
//! is already next to its place, last in the strip.

use super::*;

use crate::session::FloatRect;

/// One floating window, by the tab it holds. While the user is in it, `view` is the main view. A page has none.
#[derive(Debug)]
pub(super) struct Float<S> {
    pub(super) tab: u64,
    pub(super) view: Option<S>,
    /// The empty document the view started on, kept for when it lets go of the tab.
    idle: isize,
    rect: Option<FloatRect>,
}

impl<S: Send> Workspace<S> {
    /// How many tabs are in the strip. The rest float.
    pub fn main_count(&self) -> usize {
        self.tabs.len() - self.floats.len()
    }

    pub fn is_floating(&self, i: usize) -> bool {
        i >= self.main_count() && i < self.tabs.len()
    }

    /// The tab of the floating window the user is in.
    pub fn current_float(&self) -> Option<u64> {
        self.in_float.map(|_| self.tabs[self.active].id)
    }

    /// The tabs in floating windows.
    pub fn floating_tabs(&self) -> Vec<u64> {
        self.floats.iter().map(|f| f.tab).collect()
    }

    pub(super) fn index_of(&self, tab: u64) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == tab)
    }

    /// The tab whose view is in this slot just now.
    fn slot_tab(&self, f: &Float<S>) -> u64 {
        match self.in_float {
            Some(home) if self.tabs[self.active].id == f.tab => home,
            _ => f.tab,
        }
    }

    /// Every view beside the current one and the minimap.
    pub(super) fn side_views(&self) -> impl Iterator<Item = &S> {
        self.second.iter().map(|s| &s.view).chain(self.floats.iter().filter_map(|f| f.view.as_ref()))
    }

    /// The floating slots, each with the index of the tab its view shows.
    pub(super) fn float_views(&self) -> impl Iterator<Item = (&S, usize)> {
        self.floats.iter().filter_map(|f| Some((f.view.as_ref()?, self.index_of(self.slot_tab(f))?)))
    }

    /// The view showing tab `i`, when it is not the current one.
    pub(super) fn side_view_of(&self, i: usize) -> Option<&S> {
        if self.other_index() == Some(i) {
            return self.second.as_ref().map(|s| &s.view);
        }
        self.float_views().find(|(_, j)| *j == i).map(|(v, _)| v)
    }

    /// Can tab `i` float? It must be in the strip, and leave a tab for the main window.
    pub fn can_float(&self, i: usize) -> bool {
        let docked = self.docked_index();
        i < self.main_count() && (0..self.main_count()).any(|j| j != i && Some(j) != docked)
    }

    /// Does the main window show a whiteboard or a picture? `is_page` is about where the keyboard is, which can be a
    /// floating window.
    pub fn main_is_page(&self) -> bool {
        self.main_kind() != TabKind::Text
    }

    pub fn main_is_whiteboard(&self) -> bool {
        self.main_kind() == TabKind::Whiteboard
    }

    pub fn main_is_image(&self) -> bool {
        self.main_kind() == TabKind::Image
    }

    fn main_kind(&self) -> TabKind {
        self.tabs.get(self.main_index()).map_or(TabKind::Text, |t| t.kind)
    }

    /// Put tab `i` in a floating window: `view` is the window's editor, for a text tab. The tab leaves the strip and stops
    /// being pinned, since it comes back last. False if it cannot float: `view` is then not used.
    pub fn float_tab(&mut self, i: usize, view: Option<S>) -> bool {
        if !self.can_float(i) || view.is_some() != (self.tabs[i].kind == TabKind::Text) {
            return false;
        }
        self.leave_float();
        self.leave_dock();
        let id = self.tabs[i].id;
        if self.docked_index() == Some(i) {
            self.undock();
        }
        if i == self.active
            && let Some(j) = self.neighbour_of(i, false)
        {
            self.activate(j);
        }
        self.remember_view();
        let i = self.index_of(id).unwrap_or(i);
        let idle = view.as_ref().map_or(0, |v| self.show_in(v, i));
        let active = self.tabs[self.active].id;
        let mut tab = self.tabs.remove(i);
        tab.pinned = false;
        self.tabs.push(tab);
        self.active = self.index_of(active).unwrap_or(0);
        self.floats.push(Float { tab: id, view, idle, rect: None });
        self.with_float(id, |w| w.apply_position());
        true
    }

    /// Make `view` look like the others and show tab `i`. Returns the document it started on.
    fn show_in(&self, view: &S, i: usize) -> isize {
        self.style_view(view, false);
        sci::set_wrap(view, self.wrap);
        view::apply_symbols(view, &self.symbols);
        // The view frees the document it leaves, so its first one needs a reference of ours to last.
        let idle = view.send(sci::SCI_GETDOCPOINTER, 0, 0);
        view.send(sci::SCI_ADDREFDOCUMENT, 0, idle);
        view.send(sci::SCI_SETDOCPOINTER, 0, self.tabs[i].doc);
        lang::apply_palette(view, self.tabs[i].lang, &self.theme);
        idle
    }

    /// Put floating tab `tab` back, last in the strip, as the active tab.
    pub fn dock_back(&mut self, tab: u64) {
        self.leave_float();
        let Some(slot) = self.floats.iter().position(|f| f.tab == tab) else { return };
        self.with_float(tab, Self::remember_view);
        let f = self.floats.remove(slot);
        Self::rest(&f);
        let active = self.tabs[self.active].id;
        let last = self.main_count() - 1;
        if let Some(i) = self.index_of(tab) {
            let t = self.tabs.remove(i);
            self.tabs.insert(last, t);
        }
        self.active = self.index_of(active).unwrap_or(0);
        self.activate(last);
    }

    /// The view lets go of its tab, and its own document becomes the view's alone.
    fn rest(f: &Float<S>) {
        if let Some(v) = &f.view {
            v.send(sci::SCI_SETDOCPOINTER, 0, f.idle);
            v.send(sci::SCI_RELEASEDOCUMENT, 0, f.idle);
        }
    }

    /// Tab `i` is closing, and it floats.
    pub(super) fn remove_floating(&mut self, i: usize, remember: bool) {
        let id = self.tabs[i].id;
        self.leave_float();
        if remember && let Some(p) = self.tabs[i].path.clone() {
            push_recent(&mut self.closed, &p, MAX_CLOSED);
        }
        let Some(slot) = self.floats.iter().position(|f| f.tab == id) else { return };
        let f = self.floats.remove(slot);
        Self::rest(&f);
        let Some(i) = self.index_of(id) else { return };
        let t = self.tabs.remove(i);
        if t.kind == TabKind::Text {
            self.send(sci::SCI_RELEASEDOCUMENT, 0, t.doc);
        }
    }

    /// The keyboard went into floating window `tab`: its view becomes the current one.
    pub fn focus_float(&mut self, tab: u64) {
        if self.current_float() == Some(tab) || !self.floats.iter().any(|f| f.tab == tab) {
            return;
        }
        self.leave_float();
        self.leave_dock();
        self.remember_view();
        let home = self.tabs[self.active].id;
        let Some(i) = self.index_of(tab) else { return };
        if let Some(f) = self.floats.iter_mut().find(|f| f.tab == tab) {
            if let Some(v) = &mut f.view {
                std::mem::swap(&mut self.sci, v);
            }
            self.active = i;
            self.in_float = Some(home);
        }
    }

    /// Back in the main window, on the tab it shows.
    pub(super) fn leave_float(&mut self) {
        let Some(home) = self.in_float else { return };
        self.remember_view();
        let tab = self.tabs[self.active].id;
        self.in_float = None;
        self.active = self.index_of(home).unwrap_or(0);
        if let Some(v) = self.floats.iter_mut().find(|f| f.tab == tab).and_then(|f| f.view.as_mut()) {
            std::mem::swap(&mut self.sci, v);
        }
    }

    /// Something asked for tab `i`: true if it floats, which brings its window forward.
    pub(super) fn route_to_float(&mut self, i: usize) -> bool {
        if !self.is_floating(i) {
            self.leave_float();
            return false;
        }
        let tab = self.tabs[i].id;
        self.focus_float(tab);
        self.raise = Some(tab);
        true
    }

    /// The window that has to come forward, once.
    pub fn take_raise(&mut self) -> Option<u64> {
        self.raise.take()
    }

    /// Run `f` as if floating tab `tab` were current, without moving the keyboard.
    pub(super) fn with_float(&mut self, tab: u64, f: impl FnOnce(&mut Self)) {
        if self.current_float() == Some(tab) {
            return f(self);
        }
        let Some(i) = self.index_of(tab) else { return };
        let Some(mut view) = self.floats.iter_mut().find(|x| x.tab == tab).and_then(|x| x.view.take()) else { return };
        let cur = self.active;
        std::mem::swap(&mut self.sci, &mut view);
        self.active = i;
        f(self);
        self.active = cur;
        std::mem::swap(&mut self.sci, &mut view);
        if let Some(x) = self.floats.iter_mut().find(|x| x.tab == tab) {
            x.view = Some(view);
        }
    }

    /// Put each floating view's scroll position back once it is laid out.
    pub fn refresh_float(&mut self, tab: u64) {
        self.with_float(tab, |w| w.apply_position());
    }

    pub(super) fn remember_floats(&mut self) {
        for tab in self.floating_tabs() {
            self.with_float(tab, Self::remember_view);
        }
    }

    /// Note where window `tab` is, for the session.
    pub fn set_float_rect(&mut self, tab: u64, rect: FloatRect) {
        if let Some(f) = self.floats.iter_mut().find(|f| f.tab == tab) {
            f.rect = Some(rect);
        }
    }

    /// What each floating window shows in its title bar and its header, by tab.
    pub fn float_headings(&self) -> Vec<(u64, String, crate::dock_layout::PaneHeading)> {
        let current = self.current_float();
        self.floats
            .iter()
            .filter_map(|f| {
                let i = self.index_of(f.tab)?;
                let heading =
                    crate::dock_layout::PaneHeading { title: self.tabs[i].title.clone(), modified: self.is_dirty(i), current: current == Some(f.tab) };
                Some((f.tab, self.title_of(i), heading))
            })
            .collect()
    }

    /// Dock Back for `tab`, whichever way it is out of the strip: a docked tab is undocked, a floating one goes back last.
    pub fn send_back(&mut self, tab: u64) {
        if self.docked_tab() == Some(tab) {
            self.undock();
        } else {
            self.dock_back(tab);
        }
    }

    pub fn float_rect(&self, tab: u64) -> Option<FloatRect> {
        self.floats.iter().find(|f| f.tab == tab).and_then(|f| f.rect)
    }

    /// The floating windows the session saved, once, as tab ids: placing one moves the others' indices.
    pub fn take_saved_floats(&mut self) -> Vec<(u64, FloatRect)> {
        let saved = std::mem::take(&mut self.saved_floats);
        saved.into_iter().filter_map(|f| Some((self.tabs.get(f.tab)?.id, f.rect))).collect()
    }
}
