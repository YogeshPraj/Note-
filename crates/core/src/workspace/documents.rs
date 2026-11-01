//! Opening and closing documents, languages and encodings, and the recent-files list.

use super::*;

impl<S: Send> Workspace<S> {
    /// Create a tab holding `text`. The tab is clean (at the save point).
    pub(super) fn open_doc(&mut self, title: String, path: Option<PathBuf>, text: &str, enc: Enc, eol: Eol, lang: &'static str) -> usize {
        self.remember_view();
        let doc = self.send(sci::SCI_CREATEDOCUMENT, 0, 0);
        self.set_doc(doc);
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        sci::add_text(&self.sci, text);
        self.send(sci::SCI_SETSAVEPOINT, 0, 0);
        self.send(sci::SCI_EMPTYUNDOBUFFER, 0, 0);
        lang::apply_lexer(&self.sci, lang, &self.theme);
        let id = self.take_id();
        let lang_label = lang::label_for(lang, path.as_deref());
        let disk = path.as_deref().and_then(stamp_of);
        self.tabs.push(Tab {
            id,
            kind: TabKind::Text,
            title,
            path,
            doc,
            content: String::new(),
            dirty: false,
            forced_dirty: false,
            enc,
            eol,
            lang,
            lang_label,
            encrypted: false,
            pinned: false,
            color: None,
            caret: 0,
            top: 0,
            disk,
        });
        self.active = self.tabs.len() - 1;
        self.apply_view();
        self.active
    }

    /// Add a whiteboard tab holding scene JSON `content`.
    pub fn open_whiteboard(&mut self, title: String, path: Option<PathBuf>, content: String, dirty: bool) -> usize {
        self.remember_view();
        let id = self.take_id();
        let disk = path.as_deref().and_then(stamp_of);
        self.tabs.push(Tab {
            id,
            kind: TabKind::Whiteboard,
            title,
            path,
            doc: 0,
            content,
            dirty,
            forced_dirty: false,
            enc: Enc::Utf8,
            eol: Eol::Lf,
            lang: "null",
            lang_label: "Normal text",
            encrypted: false,
            pinned: false,
            color: None,
            caret: 0,
            top: 0,
            disk,
        });
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn new_whiteboard(&mut self) -> usize {
        let title = crate::whiteboard::next_title(self.tabs.iter().map(|t| t.title.as_str()));
        self.open_whiteboard(title, None, crate::whiteboard::empty_scene(), false)
    }

    /// Choose a Language-menu entry for the active tab.
    pub fn set_language(&mut self, label: &'static str) {
        let lexer = lang::lexer_of_label(label);
        let t = &mut self.tabs[self.active];
        t.lang = lexer;
        t.lang_label = label;
        lang::apply_lexer(&self.sci, lexer, &self.theme);
        self.apply_palettes();
    }

    /// "Convert to": keep the text, change how it will be written. Returns a
    /// message when the user backs out.
    pub fn convert_to(&mut self, ui: &dyn Ui, enc: Enc) -> Option<String> {
        if self.tabs[self.active].enc == enc {
            return None;
        }
        if !self.is_whiteboard() && !doc::can_encode(&sci::get_text(&self.sci), enc) && !ui.ask_yes_no(&unmappable_warning(enc)) {
            return Some("Conversion cancelled".into());
        }
        self.set_encoding(enc);
        None
    }

    /// "Encode in": read the file's bytes again as `enc`. For a tab with no file
    /// the current text is re-read the same way. Returns false if cancelled.
    pub fn reload_as(&mut self, ui: &dyn Ui, enc: Enc) -> bool {
        if self.is_whiteboard() {
            return false;
        }
        let t = &self.tabs[self.active];
        let bytes = match &t.path {
            Some(p) => {
                if self.is_dirty(self.active) && !ui.ask_yes_no(&format!("\"{}\" has unsaved changes.\n\nReload it as {} and lose them?", t.title, enc.label()))
                {
                    return false;
                }
                match std::fs::read(p) {
                    Ok(b) => b,
                    Err(e) => {
                        ui.error(&format!("Cannot read {}\n\n{e}", p.display()));
                        return false;
                    }
                }
            }
            None => doc::encode(&sci::get_text(&self.sci), t.enc),
        };
        let (text, eol) = doc::decode_with(&bytes, enc);
        let had_path = t.path.is_some();
        let (caret, top) = (t.caret, crate::num::to_usize(self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0)));
        let was_read_only = self.is_read_only();
        self.set_read_only(false);
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        sci::set_text(&self.sci, &text);
        self.set_read_only(was_read_only);
        if had_path {
            self.send(sci::SCI_SETSAVEPOINT, 0, 0);
            self.send(sci::SCI_EMPTYUNDOBUFFER, 0, 0);
        }
        let t = &mut self.tabs[self.active];
        t.enc = enc;
        t.eol = eol;
        t.forced_dirty = !had_path;
        if let Some(p) = t.path.clone() {
            t.disk = stamp_of(&p);
        }
        let len = crate::num::to_usize(sci::length(&self.sci).max(0));
        self.send(sci::SCI_GOTOPOS, caret.min(len), 0);
        self.send(sci::SCI_SETFIRSTVISIBLELINE, top, 0);
        true
    }

    pub fn set_encoding(&mut self, enc: Enc) {
        let t = &mut self.tabs[self.active];
        if t.enc != enc {
            t.enc = enc;
            t.forced_dirty = true;
        }
    }

    pub(super) fn note_recent(&mut self, p: &Path) {
        push_recent(&mut self.recent, p, MAX_RECENT);
    }

    /// Drop recent entries whose files no longer exist.
    pub fn prune_recent(&mut self) {
        self.recent.retain(|p| p.exists());
    }

    pub fn open_path(&mut self, ui: &dyn Ui, path: &Path) {
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref().is_some_and(|p| same_path(p, path))) {
            self.activate(i);
            self.note_recent(path);
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return ui.error(&format!("Cannot open {}\n\n{e}", path.display())),
        };
        let Some((text, enc, eol, encrypted)) = self.read_text(ui, &bytes, &file_title(path)) else { return };
        if !encrypted && crate::whiteboard::is_whiteboard_file(&text, path) {
            self.open_whiteboard(file_title(path), Some(path.to_path_buf()), text, false);
            self.note_recent(path);
            return;
        }
        let replace_pristine =
            self.tabs.len() == 1 && self.tabs[0].kind == TabKind::Text && self.tabs[0].path.is_none() && !self.is_dirty(0) && sci::length(&self.sci) == 0;
        self.open_doc(file_title(path), Some(path.to_path_buf()), &text, enc, eol, lang_for(path));
        self.tabs[self.active].encrypted = encrypted;
        self.note_recent(path);
        if replace_pristine {
            let old = self.tabs.remove(0);
            self.active -= 1;
            self.send(sci::SCI_RELEASEDOCUMENT, 0, old.doc);
        }
    }

    pub fn open_dialog(&mut self, ui: &dyn Ui) {
        for p in ui.pick_open() {
            self.open_path(ui, &p);
        }
    }

    /// Returns false if the user cancelled.
    pub fn close(&mut self, ui: &dyn Ui, i: usize) -> bool {
        if i >= self.tabs.len() {
            return false;
        }
        self.activate(i);
        if self.is_dirty(i) {
            match ui.confirm_save(&self.tabs[i].title) {
                Confirm::Cancel => return false,
                Confirm::Save if !self.save(ui) => return false,
                _ => {}
            }
        }
        self.remove_tab(i, true);
        true
    }

    /// Close every tab selected by `op`, asking about unsaved ones. False if cancelled.
    pub fn close_many(&mut self, ui: &dyn Ui, op: CloseOp) -> bool {
        let dirty: Vec<bool> = (0..self.tabs.len()).map(|i| self.is_dirty(i)).collect();
        // Tab ids stay stable while indices shift as tabs go away.
        // Pinned tabs are left alone by every "close many" command.
        let ids: Vec<u64> = close_targets(op, self.active, &dirty).into_iter().filter(|&i| !self.tabs[i].pinned).map(|i| self.tabs[i].id).collect();
        for id in ids {
            let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
            if !self.close(ui, i) {
                return false;
            }
        }
        true
    }

    /// Drop tab `i`, showing a neighbour (or a fresh tab if it was the last one).
    pub(super) fn remove_tab(&mut self, i: usize, remember: bool) {
        self.activate(i);
        let (doc, is_text) = (self.tabs[i].doc, self.tabs[i].kind == TabKind::Text);
        if remember && let Some(p) = self.tabs[i].path.clone() {
            push_recent(&mut self.closed, &p, MAX_CLOSED);
        }
        if self.tabs.len() == 1 {
            self.new_tab();
            self.tabs.remove(0);
            self.active = 0;
        } else {
            let target = if i + 1 < self.tabs.len() { i + 1 } else { i - 1 };
            let show_doc = (self.tabs[target].kind == TabKind::Text).then(|| self.tabs[target].doc);
            if let Some(d) = show_doc {
                self.set_doc(d);
            }
            self.tabs.remove(i);
            self.active = if target > i { target - 1 } else { target };
            if show_doc.is_some() {
                self.apply_view();
            }
        }
        if is_text {
            self.send(sci::SCI_RELEASEDOCUMENT, 0, doc);
        }
    }

    /// Reopen the most recently closed file that still exists.
    pub fn restore_closed(&mut self, ui: &dyn Ui) -> bool {
        while !self.closed.is_empty() {
            let p = self.closed.remove(0);
            if p.exists() {
                self.open_path(ui, &p);
                return true;
            }
        }
        false
    }

    /// Open every file in the recent list.
    pub fn open_all_recent(&mut self, ui: &dyn Ui) {
        for p in self.recent.clone().into_iter().rev() {
            if p.exists() {
                self.open_path(ui, &p);
            }
        }
    }

    pub fn set_eol(&mut self, eol: Eol) {
        self.send(sci::SCI_CONVERTEOLS, eol.sci_mode(), 0);
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        self.tabs[self.active].eol = eol;
    }
}
