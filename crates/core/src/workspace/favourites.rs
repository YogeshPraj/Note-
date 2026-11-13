//! Favourites: files (or tabs with no file yet) the user marked to come back to from the Favourites menu.

use super::*;

impl<S: Send> Workspace<S> {
    /// True when the active tab is a favourite: its file is in the list, or (with no file yet) the tab is marked.
    pub fn active_is_favourite(&self) -> bool {
        let t = &self.tabs[self.active];
        t.favourite || t.path.as_deref().is_some_and(|p| self.favourites.iter().any(|f| same_path(f, p)))
    }

    /// The indexes of the tabs marked as favourites while they have no file.
    fn favourite_tabs(&self) -> Vec<usize> {
        (0..self.tabs.len()).filter(|&i| self.tabs[i].favourite && self.tabs[i].path.is_none()).collect()
    }

    /// The Favourites menu: each file by its name, then each unsaved favourite tab by its tab name; with the tool tip
    /// (the full path, or a word that it is not saved).
    pub fn favourite_list(&self) -> Vec<(String, String)> {
        let files = self
            .favourites
            .iter()
            .map(|p| (p.file_name().map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().into_owned()), p.display().to_string()));
        let tabs = self.favourite_tabs().into_iter().map(|i| (self.tabs[i].title.clone(), "Not saved yet".to_string()));
        files.chain(tabs).collect()
    }

    /// Go to favourite `i` of `favourite_list`. A file that is gone is taken off the list and its path put in `gone`.
    pub fn open_favourite(&mut self, ui: &dyn Ui, i: usize, gone: &mut Option<PathBuf>) {
        if let Some(p) = self.favourites.get(i).cloned() {
            if p.exists() {
                self.open_path(ui, &p);
            } else {
                self.favourites.remove(i);
                *gone = Some(p);
            }
        } else if let Some(&t) = self.favourite_tabs().get(i - self.favourites.len()) {
            self.activate(t);
        }
    }

    /// Add the active tab to the favourites, or take it out. A saved file is kept by its path (it stays when the tab is
    /// closed); a tab with no file is marked, and is gone from the list when it is closed unsaved. False when the list
    /// is full.
    pub fn toggle_favourite(&mut self) -> bool {
        if self.active_is_favourite() {
            let t = &mut self.tabs[self.active];
            t.favourite = false;
            if let Some(p) = t.path.clone() {
                self.favourites.retain(|f| !same_path(f, &p));
            }
            return true;
        }
        if self.favourites.len() + self.favourite_tabs().len() >= crate::registry::FAV_SLOTS {
            return false;
        }
        match self.tabs[self.active].path.clone() {
            Some(p) => self.favourites.push(p),
            None => self.tabs[self.active].favourite = true,
        }
        true
    }
}
