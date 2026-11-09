//! The whiteboard page: its messages, saving a scene, and showing it instead of the editor.

use super::*;

impl App {
    pub(crate) fn wb_event(&mut self, ev: wbm::Event) {
        match ev {
            wbm::Event::Ready => {
                self.wbv.mark_ready();
                self.wb.shown = None;
                crate::start_check::note("whiteboard");
            }
            wbm::Event::State(s) => self.wb_apply_scene(s),
            wbm::Event::Data(s) => {
                self.wb_apply_scene(s);
                if self.wb.then.is_none() {
                    self.wb_finish_pending();
                }
            }
            wbm::Event::SaveRequest => {
                self.wb.pending = Some(Pending::Save);
                wbm::request_data(&self.wbv);
            }
            // Beside a dock or a floating window, the click brings the user back to the main window.
            wbm::Event::Focus => self.ws.focus_pane(false),
            wbm::Event::AgentDone(done) => self.mcp_board_done(done),
            wbm::Event::Tab(_) | wbm::Event::Menu(_) => {} // handled before this, with nothing borrowed
        }
        self.refresh();
    }

    /// A scene from the main window's page.
    pub(crate) fn wb_apply_scene(&mut self, s: String) {
        let current = self.ws.main_is_whiteboard().then(|| self.ws.tabs[self.ws.main_index()].id);
        // Ignore echoes for a scene that is no longer the one on screen.
        if let (Some(id), true) = (current, current == self.wb.shown) {
            self.scene_changed(id, s);
        }
    }

    /// The page showing `tab` has this scene now. By tab, not the active one: the page may not be where the keyboard is.
    pub(crate) fn scene_changed(&mut self, tab: u64, s: String) {
        let Some(t) = self.ws.tabs.iter_mut().find(|t| t.id == tab) else { return };
        if t.content != s {
            // Pan/zoom and Excalidraw's own re-serialisation are not edits.
            let edited = !notepp_core::whiteboard::same_drawing(&t.content, &s);
            t.content = s;
            if edited {
                t.dirty = true;
            }
        }
    }

    pub(crate) fn wb_finish_pending(&mut self) {
        let Some(p) = self.wb.pending.take() else { return };
        if !self.ws.is_whiteboard() {
            return;
        }
        let App { ws, ui, .. } = self;
        match p {
            Pending::Save => ws.save(ui),
            Pending::SaveAs => ws.save_as(ui),
            Pending::SaveCopy => ws.save_copy_as(ui).is_some(),
        };
    }

    /// Show the whiteboard instead of the editor while the main window's tab is one, and push the tab's scene (and the
    /// theme) into the page when needed.
    pub(crate) fn wb_sync(&mut self) {
        let wb = self.ws.main_is_whiteboard();
        let page = self.ws.main_is_page();
        self.sync_preview_pane();
        let mut relayout = false;
        if self.dock_panes.split.is_shown() == page {
            self.dock_panes.split.show(!page);
            relayout = true;
        }
        let want_mini = self.ws.minimap && !page;
        if self.mini.is_shown() != want_mini {
            self.mini.show(want_mini);
            relayout = true;
        }
        if self.wbv.is_shown() != wb {
            self.wbv.show(wb);
            relayout = true;
            if wb {
                self.wb.shown = None;
                if !self.wbv.is_loaded() {
                    match wbm::page_url() {
                        Some(url) => self.wbv.load_url(&url),
                        None => self.flash = Some("Whiteboard unavailable: assets/whiteboard not found".into()),
                    }
                }
            }
        }
        if relayout {
            self.editor_pane.layout();
        }
        if wb && self.wbv.is_ready() {
            let dark = self.ws.theme.dark;
            if self.wbv.sync_dark(dark) {
                wbm::set_theme(&self.wbv, dark);
            }
            let (id, content) = {
                let t = &self.ws.tabs[self.ws.main_index()];
                (t.id, t.content.clone())
            };
            if self.wb.shown != Some(id) {
                self.wb.shown = Some(id);
                wbm::load(&self.wbv, &content);
                if self.ws.current_float().is_none() {
                    self.wbv.set_focus();
                }
            }
        }
    }
}
