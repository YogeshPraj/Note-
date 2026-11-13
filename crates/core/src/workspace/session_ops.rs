//! Saving the session and bringing it back.

use super::*;

impl<S: Send> Workspace<S> {
    pub fn snapshot(&mut self) -> Session {
        self.remember_view();
        self.with_docked(Self::remember_view);
        self.remember_floats();
        let cur = self.main_index();
        let docked = self.docked_index();
        let mut tabs = Vec::new();
        let mut active = 0;
        let mut dock = None;
        let mut floats = Vec::new();
        for i in 0..self.tabs.len() {
            let Some(tab) = self.snapshot_tab(i) else { continue };
            if i == cur {
                active = tabs.len();
            }
            if Some(i) == docked {
                dock = self.dock_side().map(|side| SessionDock { tab: tabs.len(), side, share: self.dock_share });
            }
            if self.is_floating(i)
                && let Some(rect) = self.float_rect(self.tabs[i].id)
            {
                floats.push(SessionFloat { tab: tabs.len(), rect });
            }
            tabs.push(tab);
        }
        // Tabs left closed this time go back as they were, unless their file has been opened since.
        let open = |p: &str| self.tabs.iter().any(|t| t.path.as_deref().is_some_and(|q| same_path(q, Path::new(p))));
        tabs.extend(self.parked.iter().filter(|st| !st.path.as_deref().is_some_and(open)).cloned());
        Session {
            tabs,
            active,
            wrap: self.wrap,
            dark: self.theme.dark,
            minimap: self.minimap,
            recent: self.recent.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
            favourites: self.favourites.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
            workspace: self.workspace.as_ref().map(|p| p.to_string_lossy().into_owned()),
            tree_hidden: self.tree_hidden,
            search_engine: self.search_engine,
            symbols: self.symbols,
            date_format: self.date_format.clone(),
            kept: self.kept.clone(),
            dock,
            floats,
            window: self.main_window,
        }
    }

    /// One tab as saved in the session; `None` for an untitled tab with nothing in it (unless it is the only one).
    fn snapshot_tab(&self, i: usize) -> Option<SessionTab> {
        let t = &self.tabs[i];
        let untitled = t.path.is_none();
        let mut text = None;
        if t.kind == TabKind::Image {
            // A picture is kept in the session until it is saved to a file of its own.
            if t.dirty || untitled {
                text = Some(t.content.clone());
            }
        } else if t.kind == TabKind::Whiteboard {
            if t.dirty || untitled {
                text = Some(t.content.clone());
            }
            if untitled && crate::whiteboard::is_empty_scene(&t.content) && self.tabs.len() > 1 {
                return None;
            }
        } else {
            if t.dirty || t.forced_dirty || untitled {
                let plain = if i == self.active { sci::get_text(&self.sci) } else { self.with_doc(t.doc, |s| sci::get_text(s)) };
                // Unsaved text of a protected file never reaches the session file as plain text.
                text = if t.encrypted { self.vault.dek.as_ref().and_then(|d| crypto::encrypt_text(&plain, d, "").ok()) } else { Some(plain) };
            }
            if untitled && text.as_deref() == Some("") && self.tabs.len() > 1 {
                return None;
            }
        }
        let has_text = text.is_some();
        Some(SessionTab {
            title: t.title.clone(),
            path: t.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            text,
            caret: t.caret,
            top: t.top,
            enc: t.enc.key().into(),
            eol: t.eol.key().into(),
            encrypted: t.encrypted && has_text,
            pinned: t.pinned,
            favourite: t.favourite,
            color: t.color,
            kind: match t.kind {
                TabKind::Whiteboard => "whiteboard".into(),
                TabKind::Image => "image".into(),
                TabKind::Text => "text".into(),
            },
            lang: (t.path.is_none() && t.kind == TabKind::Text && t.lang != "null").then(|| t.lang_label.to_string()),
        })
    }
    /// Restore the whole saved state (startup): settings, workspace and tabs.
    pub fn restore(&mut self, ui: &dyn Ui, s: &Session) {
        self.minimap = s.minimap;
        self.search_engine = s.search_engine;
        self.set_symbols(s.symbols);
        if !s.date_format.is_empty() {
            self.date_format.clone_from(&s.date_format);
        }
        self.kept.clone_from(&s.kept);
        self.saved_dock = s.dock;
        self.saved_floats.clone_from(&s.floats);
        self.main_window = s.window;
        self.favourites = s.favourites.iter().map(PathBuf::from).take(crate::registry::FAV_SLOTS).collect();
        self.recent = s.recent.iter().map(PathBuf::from).filter(|p| p.exists()).take(MAX_RECENT).collect();
        self.workspace = s.workspace.as_ref().map(PathBuf::from).filter(|p| p.is_dir());
        self.tree_hidden = s.tree_hidden;
        if s.dark {
            self.set_theme(Theme::dark());
        }
        self.set_wrap(s.wrap);
        self.restore_tabs(ui, s);
        if self.tabs.is_empty() {
            self.new_tab();
        } else {
            let want = s.active.min(self.tabs.len() - 1);
            if want != self.active {
                self.activate(want);
            } else if !self.is_page() {
                self.apply_view();
            }
        }
    }

    /// Open the document of one saved tab (from its file, or empty when it was never saved); `None` skips the tab.
    #[allow(clippy::too_many_arguments, reason = "the pieces of one saved tab; a struct would only rename them")]
    pub(super) fn restore_doc(
        &mut self,
        ui: &dyn Ui,
        st: &crate::session::SessionTab,
        path: Option<&Path>,
        has_saved_text: bool,
        enc: Enc,
        eol: Eol,
        declined: &mut bool,
        encrypted: &mut bool,
    ) -> Option<usize> {
        let idx = if let Some(path) = path {
            match ui.read_file(path) {
                Ok(bytes) => {
                    let is_env = crypto::parse_envelope(&bytes).is_some();
                    if is_env && *declined {
                        return None;
                    }
                    // A character set chosen with "Encode in" must survive a restart.
                    let (text, enc, eol) = if is_env {
                        if let Some((t, e, l, _)) = self.read_text(ui, &bytes, &file_title(path)) {
                            (t, e, l)
                        } else {
                            *declined = true;
                            return None;
                        }
                    } else if matches!(enc, Enc::Cs(_)) {
                        let (t, e) = doc::decode_with(&bytes, enc);
                        (t, enc, e)
                    } else {
                        doc::decode(&bytes)
                    };
                    *encrypted = is_env;
                    self.open_doc(file_title(path), Some(path.to_path_buf()), &text, enc, eol, lang_for(path))
                }
                // File vanished: keep the unsaved text if we have it, otherwise skip.
                Err(_) if has_saved_text => self.open_doc(file_title(path), Some(path.to_path_buf()), "", enc, eol, lang_for(path)),
                Err(_) => return None,
            }
        } else {
            let entry = st.lang.as_deref().and_then(|l| lang::LANGUAGES.iter().find(|(n, _)| *n == l));
            let idx = self.open_doc(st.title.clone(), None, "", enc, eol, entry.map_or("null", |e| e.1));
            if let Some(e) = entry {
                self.tabs[idx].lang_label = e.0;
            }
            idx
        };
        Some(idx)
    }

    /// One saved whiteboard tab: its unsaved scene if it has one, else the file on disk.
    pub(super) fn restore_whiteboard(&mut self, st: &crate::session::SessionTab, path: Option<PathBuf>) {
        let disk = path.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| doc::decode(&b).0);
        let (content, dirty) = match (&st.text, disk) {
            (Some(t), Some(d)) => (t.clone(), *t != d),
            (Some(t), None) => (t.clone(), true),
            (None, Some(d)) => (d, false),
            (None, None) => return,
        };
        let i = self.open_whiteboard(st.title.clone(), path, content, dirty);
        self.tabs[i].pinned = st.pinned;
        self.tabs[i].favourite = st.favourite;
        self.tabs[i].color = st.color;
    }

    /// One saved picture tab: its unsaved bytes if it has any, else the file on disk.
    pub(super) fn restore_image(&mut self, st: &crate::session::SessionTab, path: Option<PathBuf>) {
        let disk = path.as_ref().and_then(|p| std::fs::read(p).ok()).and_then(|b| crate::image::for_page(&b));
        let (content, dirty, path) = match (&st.text, disk) {
            (Some(t), Some(d)) => (t.clone(), *t != crate::image::to_content(&d), path),
            (Some(t), None) => (t.clone(), true, path),
            (None, Some(d)) => (crate::image::to_content(&d), false, path),
            (None, None) => return,
        };
        let i = self.open_image(st.title.clone(), path, content, dirty);
        self.tabs[i].pinned = st.pinned;
        self.tabs[i].favourite = st.favourite;
        self.tabs[i].color = st.color;
    }

    /// Open the tabs described by `s` (skipping files that are already open).
    pub(super) fn restore_tabs(&mut self, ui: &dyn Ui, s: &Session) {
        // Once the user declines to unlock, the other protected tabs are skipped without asking again.
        let mut declined = false;
        for st in &s.tabs {
            let path = st.path.as_ref().map(PathBuf::from);
            if let Some(p) = &path
                && self.tabs.iter().any(|t| t.path.as_deref().is_some_and(|q| same_path(q, p)))
            {
                continue;
            }
            if st.kind == "image" {
                self.restore_image(st, path);
                continue;
            }
            if st.kind == "whiteboard" {
                self.restore_whiteboard(st, path);
                continue;
            }
            let (enc, eol) = (Enc::from_key(&st.enc), Eol::from_key(&st.eol));
            // Unsaved text of a protected tab comes back from its encrypted copy.
            let mut saved_text = st.text.clone();
            let mut encrypted = st.encrypted;
            if st.encrypted {
                saved_text = None;
                if let Some(env) = st.text.as_deref().and_then(|t| crypto::parse_envelope(t.as_bytes())) {
                    // Not unlocked: the tab stays closed, and its saved entry (unsaved text and all) is kept.
                    saved_text = (!declined).then(|| self.decrypt_for(ui, &env, &st.title)).flatten();
                    if saved_text.is_none() {
                        declined = true;
                        self.parked.push(st.clone());
                        continue;
                    }
                }
            }
            let Some(idx) = self.restore_doc(ui, st, path.as_deref(), saved_text.is_some(), enc, eol, &mut declined, &mut encrypted) else {
                // An encrypted file that was not unlocked is kept for next time; a file that is gone is dropped.
                if declined && path.as_deref().is_some_and(Path::exists) {
                    self.parked.push(st.clone());
                }
                continue;
            };
            if let Some(text) = &saved_text {
                // Replacing content after the save point leaves the tab dirty, as it was.
                sci::set_text(&self.sci, text);
            }
            let t = &mut self.tabs[idx];
            t.encrypted = encrypted;
            t.pinned = st.pinned;
            t.favourite = st.favourite;
            t.color = st.color;
            t.caret = st.caret;
            t.top = st.top;
        }
    }
}
