//! Reading, writing and watching files: save, reload, rename, encryption and external changes.

use super::*;

impl<S: Send> Workspace<S> {
    /// The active tab as it would be written to disk.
    pub(super) fn current_bytes(&self) -> Vec<u8> {
        if self.is_whiteboard() {
            self.tabs[self.active].content.clone().into_bytes()
        } else {
            doc::encode(&sci::get_text(&self.sci), self.tabs[self.active].enc)
        }
    }

    /// The active tab as it goes to `path`: encrypted when the tab is protected (asking for the
    /// password first if the profile is locked). None when that was declined or failed.
    pub(super) fn bytes_for(&mut self, ui: &dyn Ui, path: &Path) -> Option<Vec<u8>> {
        if self.is_whiteboard() || !self.tabs[self.active].encrypted {
            return Some(self.current_bytes());
        }
        let title = self.tabs[self.active].title.clone();
        if !self.vault.unlocked() && !ui.unlock(&mut self.vault, &title) {
            ui.error(&format!("\"{title}\" is encrypted, so it was not saved.\n\nUnlock encryption to save it."));
            return None;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let text = sci::get_text(&self.sci);
        match self.vault.dek.as_ref().map(|d| crypto::encrypt_text(&text, d, ext)) {
            Some(Ok(s)) => Some(s.into_bytes()),
            Some(Err(e)) => {
                ui.error(&format!("Cannot encrypt \"{title}\"\n\n{e}"));
                None
            }
            None => None,
        }
    }

    pub(super) fn write_to(&mut self, ui: &dyn Ui, path: &Path) -> bool {
        let whiteboard = self.is_whiteboard();
        if !whiteboard && !self.tabs[self.active].encrypted {
            let enc = self.tabs[self.active].enc;
            if !doc::can_encode(&sci::get_text(&self.sci), enc) && !ui.ask_yes_no(&unmappable_warning(enc)) {
                return false;
            }
        }
        let Some(bytes) = self.bytes_for(ui, path) else { return false };
        match files::atomic_write(path, &bytes) {
            Ok(()) => {
                if !whiteboard {
                    self.send(sci::SCI_SETSAVEPOINT, 0, 0);
                }
                let t = &mut self.tabs[self.active];
                t.dirty = false;
                t.forced_dirty = false;
                t.disk = stamp_of(path);
                true
            }
            Err(e) => {
                ui.error(&format!("Cannot save {}\n\n{e}", path.display()));
                false
            }
        }
    }

    pub fn save(&mut self, ui: &dyn Ui) -> bool {
        match self.tabs[self.active].path.clone() {
            Some(p) => self.write_to(ui, &p),
            None => self.save_as(ui),
        }
    }

    pub fn save_as(&mut self, ui: &dyn Ui) -> bool {
        let t = &self.tabs[self.active];
        let suggested = if t.kind == TabKind::Whiteboard && !t.title.contains('.') { format!("{}.excalidraw", t.title) } else { t.title.clone() };
        let Some(p) = ui.pick_save(&suggested) else { return false };
        self.write_and_adopt(ui, p)
    }

    /// Write the active tab to another file, leaving the tab itself untouched.
    pub fn save_copy_as(&mut self, ui: &dyn Ui) -> Option<PathBuf> {
        let t = &self.tabs[self.active];
        let p = ui.pick_save(&t.title)?;
        let bytes = self.bytes_for(ui, &p)?;
        match files::atomic_write(&p, &bytes) {
            Ok(()) => Some(p),
            Err(e) => {
                ui.error(&format!("Cannot save {}\n\n{e}", p.display()));
                None
            }
        }
    }

    /// Re-read the active file from disk, discarding unsaved changes after asking.
    /// Returns false if there is nothing to reload or the user declined.
    pub fn reload_from_disk(&mut self, ui: &dyn Ui) -> bool {
        if self.tabs[self.active].path.is_none() {
            return false;
        }
        if self.is_dirty(self.active)
            && !ui.ask_yes_no(&format!("\"{}\" has unsaved changes.\n\nReload from disk and lose them?", self.tabs[self.active].title))
        {
            return false;
        }
        self.reload_now(ui)
    }

    /// Re-read the active file without asking.
    pub(super) fn reload_now(&mut self, ui: &dyn Ui) -> bool {
        let Some(path) = self.tabs[self.active].path.clone() else { return false };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                ui.error(&format!("Cannot read {}\n\n{e}", path.display()));
                return false;
            }
        };
        let title = self.tabs[self.active].title.clone();
        let Some((text, enc, eol, encrypted)) = self.read_text(ui, &bytes, &title) else { return false };
        self.tabs[self.active].disk = stamp_of(&path);
        if self.is_whiteboard() {
            let t = &mut self.tabs[self.active];
            t.content = text;
            t.dirty = false;
            return true;
        }
        let (caret, top) = (self.tabs[self.active].caret, self.top_doc_line());
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        let was_read_only = self.is_read_only();
        self.set_read_only(false);
        sci::set_text(&self.sci, &text);
        self.set_read_only(was_read_only);
        self.send(sci::SCI_SETSAVEPOINT, 0, 0);
        self.send(sci::SCI_EMPTYUNDOBUFFER, 0, 0);
        let t = &mut self.tabs[self.active];
        t.enc = enc;
        t.eol = eol;
        t.encrypted = encrypted;
        t.forced_dirty = false;
        t.dirty = false;
        let len = crate::num::to_usize(sci::length(&self.sci).max(0));
        self.send(sci::SCI_GOTOPOS, caret.min(len), 0);
        self.set_top_doc_line(top);
        true
    }

    /// A file's text, decrypting it when it is an encrypted file. None when the user backed out
    /// or it could not be decrypted. The last value tells whether the file was encrypted.
    pub(super) fn read_text(&mut self, ui: &dyn Ui, bytes: &[u8], name: &str) -> Option<(String, Enc, Eol, bool)> {
        if let Some(env) = crypto::parse_envelope(bytes) {
            let plain = self.decrypt_for(ui, &env, name)?;
            let (text, _, eol) = doc::decode(plain.as_bytes());
            Some((text, Enc::Utf8, eol, true))
        } else {
            let (text, enc, eol) = doc::decode(bytes);
            Some((text, enc, eol, false))
        }
    }

    /// Protect the active file: from now on it is saved encrypted. Returns a message for the user.
    pub fn encrypt_active(&mut self, ui: &dyn Ui) -> Option<String> {
        if self.is_whiteboard() {
            return Some("Whiteboards cannot be encrypted".into());
        }
        if self.tabs[self.active].encrypted {
            return Some("This file is already encrypted".into());
        }
        let name = self.tabs[self.active].title.clone();
        if self.vault.profile.is_none() {
            self.vault = crypto::Vault::load();
        }
        if self.vault.profile.is_none() {
            if !ui.setup_encryption(&mut self.vault) {
                return None;
            }
        } else if !self.vault.unlocked() && !ui.unlock(&mut self.vault, &name) {
            return None;
        }
        if !ui.ask_yes_no(&format!("Encrypt \"{name}\" with your Note++ encryption profile?\n\nIt is saved encrypted the next time you save.")) {
            return None;
        }
        let t = &mut self.tabs[self.active];
        t.encrypted = true;
        t.forced_dirty = true;
        Some(format!("\"{name}\" will be saved encrypted"))
    }

    /// Stop protecting the active file: the next save writes plain text.
    pub fn decrypt_active(&mut self, ui: &dyn Ui) -> Option<String> {
        if !self.tabs[self.active].encrypted {
            return Some("This file is not encrypted".into());
        }
        let name = self.tabs[self.active].title.clone();
        if !ui.ask_yes_no(&format!("Remove the encryption from \"{name}\"?\n\nThe next save writes it as plain, readable text.")) {
            return None;
        }
        let t = &mut self.tabs[self.active];
        t.encrypted = false;
        t.forced_dirty = true;
        Some(format!("\"{name}\" will be saved as plain text"))
    }

    pub fn is_encrypted(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|t| t.encrypted)
    }

    /// Notice files changed or deleted by other programs and ask what to do.
    /// Returns true if any tab was reloaded, kept or closed.
    pub fn check_external(&mut self, ui: &dyn Ui) -> bool {
        let ids: Vec<u64> = self.tabs.iter().filter(|t| t.path.is_some()).map(|t| t.id).collect();
        let mut changed = false;
        for id in ids {
            let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
            let Some(path) = self.tabs[i].path.clone() else { continue };
            let now = stamp_of(&path);
            if self.tabs[i].disk == now {
                continue;
            }
            self.tabs[i].disk = now;
            changed = true;
            self.activate(i);
            if now.is_none() {
                let msg = format!("{}\n\nThis file has been deleted by another program.\nKeep this file in the editor?", path.display());
                if ui.ask_yes_no(&msg) {
                    self.tabs[i].forced_dirty = true;
                } else {
                    self.remove_tab(i, false);
                }
            } else {
                let lose = if self.is_dirty(i) { "\n\nYour unsaved changes will be lost." } else { "" };
                let msg = format!("{}\n\nThis file has been modified by another program.\nDo you want to reload it?{lose}", path.display());
                if ui.ask_yes_no(&msg) {
                    self.reload_now(ui);
                }
            }
        }
        changed
    }

    /// Move/rename the active file on disk (or just give an unsaved tab a name).
    pub fn rename(&mut self, ui: &dyn Ui) -> bool {
        let suggested = self.tabs[self.active].title.clone();
        let Some(new) = ui.pick_save(&suggested) else { return false };
        let Some(old) = self.tabs[self.active].path.clone() else {
            // Unsaved: renaming is just Save As.
            return self.write_and_adopt(ui, new);
        };
        if same_path(&old, &new) {
            return false;
        }
        if let Err(e) = std::fs::rename(&old, &new).or_else(|_| std::fs::copy(&old, &new).and_then(|_| std::fs::remove_file(&old))) {
            ui.error(&format!("Cannot rename to {}\n\n{e}", new.display()));
            return false;
        }
        self.recent.retain(|r| !same_path(r, &old));
        self.adopt_path(new);
        true
    }

    pub(super) fn write_and_adopt(&mut self, ui: &dyn Ui, p: PathBuf) -> bool {
        if !self.write_to(ui, &p) {
            return false;
        }
        self.adopt_path(p);
        true
    }

    /// Point the active tab at `p` (title, language, recent list).
    pub(super) fn adopt_path(&mut self, p: PathBuf) {
        self.note_recent(&p);
        let whiteboard = self.is_whiteboard();
        let lang = lang_for(&p);
        let label = lang::label_for(lang, Some(&p));
        let changed = !whiteboard && self.tabs[self.active].lang != lang;
        let t = &mut self.tabs[self.active];
        t.title = file_title(&p);
        t.disk = stamp_of(&p);
        t.path = Some(p);
        if !whiteboard {
            t.lang = lang;
            t.lang_label = label;
        }
        if changed {
            lang::apply_lexer(&self.sci, lang, &self.theme);
            self.apply_palettes();
        }
    }

    /// Delete the active file (to the Recycle Bin) and close its tab.
    pub fn move_to_recycle_bin(&mut self, ui: &dyn Ui) -> bool {
        let Some(path) = self.tabs[self.active].path.clone() else { return false };
        if !ui.ask_yes_no(&format!("Move \"{}\" to the Recycle Bin?", path.display())) {
            return false;
        }
        if let Err(e) = ui.recycle(&path) {
            ui.error(&format!("Cannot move {} to the Recycle Bin\n\n{e}", path.display()));
            return false;
        }
        self.recent.retain(|r| !same_path(r, &path));
        self.remove_tab(self.active, false);
        true
    }

    pub fn save_all(&mut self, ui: &dyn Ui) {
        let keep = self.tabs[self.active].id;
        for i in 0..self.tabs.len() {
            if self.is_dirty(i) {
                self.activate(i);
                if !self.save(ui) {
                    break;
                }
            }
        }
        if let Some(i) = self.tabs.iter().position(|t| t.id == keep) {
            self.activate(i);
        }
    }
}
