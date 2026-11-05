//! The preview pane beside the editor: showing it, maximising it, keeping it in step with the editor.

use super::*;

impl App {
    /// Show or hide the preview pane beside the editor.
    pub fn set_preview(&mut self, on: bool) {
        if on == self.preview.on {
            return;
        }
        self.preview.on = on;
        self.sync_preview_pane();
    }

    /// The pane is only beside the editor while preview is on and the active tab has something to preview.
    pub(crate) fn sync_preview_pane(&mut self) {
        let previewable = !self.ws.is_page() && self.ws.preview_kind().is_some();
        self.show_preview_pane(self.preview.on && previewable);
    }

    /// Put the pane beside the editor or take it away (a whiteboard has nothing to preview).
    pub(crate) fn show_preview_pane(&mut self, want: bool) {
        if want == self.preview.split {
            return;
        }
        if !want && self.preview.maximized {
            self.set_preview_maximized(false);
        }
        self.preview.split = want;
        self.preview.sent = None;
        if want {
            if !self.view.is_loaded() {
                self.view.load_html(&preview::page_html(self.ws.settings.preview_remote_images));
            }
            self.view.show(true);
            self.split_at_kept_sash();
            self.preview.key = None;
        } else {
            self.keep_preview_share();
            self.splitter.unsplit(Some(&self.view));
            self.view.show(false);
        }
    }

    fn keep_preview_share(&mut self) {
        let (sash, width) = (self.splitter.sash_position(), self.splitter.get_size().width);
        self.preview.keep_share(sash, width);
    }

    fn split_at_kept_sash(&self) {
        let sash = self.preview.sash(self.splitter.get_size().width);
        self.splitter.split_vertically(&self.editor_pane, &self.view, sash);
    }

    /// Let the preview fill the window (the editor is hidden) or go back beside the editor.
    pub(crate) fn set_preview_maximized(&mut self, on: bool) {
        if on == self.preview.maximized || !self.preview.split {
            return;
        }
        self.preview.maximized = on;
        if on {
            self.keep_preview_share();
            self.splitter.unsplit(Some(&self.editor_pane));
        } else {
            self.editor_pane.show(true);
            self.split_at_kept_sash();
        }
        self.view.eval(&format!("window.np.setMax({on})"));
    }

    /// Keep the page scrolled to where the editor is: by source line for Markdown, by proportion for the JSON tree.
    pub(crate) fn sync_preview_scroll(&mut self) {
        if !self.preview.split || !self.view.is_ready() {
            return;
        }
        let top = self.ws.top_doc_line();
        let echo = self.preview.applied.take() == Some(top);
        if echo || self.preview.sent == Some(top) {
            self.preview.sent = Some(top);
            return;
        }
        self.preview.sent = Some(top);
        match self.ws.preview_kind() {
            Some(pcore::Kind::Markdown) => self.view.call("window.np.scrollToLine", &[&top.to_string(), &self.ws.line_total().to_string()]),
            Some(pcore::Kind::Json) => self.view.call("window.np.scrollToRatio", &[&self.ws.scroll_permille().to_string()]),
            _ => {}
        }
    }

    /// A message from the page that is about the document: an edit in the JSON tree, "fix" on broken JSON, a node
    /// picked in the tree, or a scroll.
    pub(crate) fn page_message(&mut self, msg: &str) {
        if let Some(text) = msg.strip_prefix("json:").or_else(|| msg.strip_prefix("jsonfix:")) {
            self.take_json(text);
        } else if let Some(path) = msg.strip_prefix("jpath:") {
            self.go_to_json_node(path);
        } else if let Some(url) = msg.strip_prefix("open:") {
            if let Some(url) = pcore::external_link(url) {
                crate::platform::current().open_default(url);
            }
        } else {
            self.page_scrolled(msg);
        }
    }

    /// Is the page showing the active tab? It can lag behind a tab switch, and must not then write into the new tab.
    fn page_shows_active_tab(&self) -> bool {
        let doc = self.ws.tabs.get(self.ws.active).map(|t| t.doc);
        self.ws.preview_kind() == Some(pcore::Kind::Json) && self.preview.key.is_some_and(|k| Some(k.0) == doc)
    }

    /// The JSON view changed the document: take its text (one undo step).
    fn take_json(&mut self, text: &str) {
        if self.page_shows_active_tab() {
            self.ws.replace_text(text);
            self.refresh();
        }
    }

    /// A node was picked in the JSON tree (`path` as the page sends it): put the editor on its line.
    fn go_to_json_node(&mut self, path: &str) {
        use notepp_core::jsonpath;
        if !self.page_shows_active_tab() {
            return;
        }
        let text = notepp_core::sci::get_text(&self.ws.sci);
        if let Some(line) = jsonpath::parse_path(path).and_then(|p| jsonpath::line_of(&text, &p)) {
            notepp_core::search::goto_line(&self.ws.sci, line + 1);
            self.preview.applied = Some(self.ws.top_doc_line());
            self.refresh();
        }
    }

    /// The page scrolled by itself: bring the editor along.
    fn page_scrolled(&mut self, msg: &str) {
        if let Some(line) = msg.strip_prefix("line:").and_then(|v| v.split('.').next()).and_then(|v| v.parse::<usize>().ok()) {
            self.preview.applied = Some(line);
            self.ws.set_top_doc_line(line);
        } else if let Some(permille) = msg.strip_prefix("ratio:").and_then(|v| v.parse::<usize>().ok()) {
            self.ws.set_scroll_permille(permille);
            self.preview.applied = Some(self.ws.top_doc_line());
        }
    }

    /// Re-render the preview when the document, its text or the theme changed.
    pub fn preview_update(&mut self, force: bool) {
        self.sync_preview_pane();
        if !self.preview.split || !self.view.is_ready() {
            return;
        }
        let dark = self.ws.theme.dark;
        if self.view.sync_dark(dark) {
            preview::set_theme(&self.view, dark);
            self.preview.key = None;
        }
        let key = self.ws.preview_key();
        if !force && self.preview.key == Some(key) {
            return;
        }
        self.preview.key = Some(key);
        self.preview.sent = None;
        let view = self.view.clone();
        let Some(kind) = self.ws.preview_kind() else {
            return preview::render(&view, "message", "Preview is available for Markdown, HTML, Mermaid and JSON files.");
        };
        let text = sci::get_text(&self.ws.sci);
        match kind {
            pcore::Kind::Markdown if pcore::is_bare_mermaid(&text) => preview::render(&view, "mermaid", &text),
            pcore::Kind::Markdown => {
                let base = self.ws.tabs[self.ws.active].path.as_deref().and_then(|p| p.parent());
                preview::render(&view, "markdown", &pcore::markdown_to_html_in(&text, base));
            }
            pcore::Kind::Html => preview::render(&view, "html", &text),
            pcore::Kind::Mermaid => preview::render(&view, "mermaid", &text),
            pcore::Kind::Json => preview::render(&view, "json", &text),
            pcore::Kind::Log | pcore::Kind::Http => self.stream_preview(kind, force),
        }
    }
}
