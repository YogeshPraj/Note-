//! Brain and the workspace: adding an entry when the day's note may be open in a tab, and which key the notes use.

use super::*;
use crate::brain::store::Store;
use crate::brain::{self, Date, Source};
use crate::edit::DateTime;
use crate::error::Error;

impl<S: Send> Workspace<S> {
    /// The brain in this profile.
    pub fn brain_store(&self) -> Store {
        Store::new(brain::root(&crate::session::config_dir()))
    }

    /// True when the Brain needs the profile unlocked before it can be read or added to: its notes are to be encrypted,
    /// or some already are, and the key is not there.
    pub fn brain_locked(&self) -> bool {
        !self.vault.unlocked() && (self.settings.brain_encrypt || self.brain_store().has_encrypted())
    }

    /// The key new notes are written with: the profile's when the Brain is encrypted, none otherwise.
    ///
    /// # Errors
    /// `Locked` when the Brain is encrypted and the profile is locked.
    pub fn brain_write_key(&self) -> Result<Option<&crypto::Dek>, Error> {
        if self.settings.brain_encrypt { self.vault.dek.as_ref().map(Some).ok_or(Error::Locked) } else { Ok(None) }
    }

    /// Add an entry at the end of the Brain's file, under the heading of the day. When the file is open in a tab the
    /// entry goes into the tab's text as one undo step (unsaved words in it are kept), and a tab that had nothing
    /// unsaved is saved straight away; otherwise the file is written. Returns the file's path and the entry's line.
    ///
    /// # Errors
    /// `Locked` when the Brain is encrypted and the profile is locked; the store's error; a read-only tab.
    pub fn brain_add(&mut self, ui: &dyn Ui, when: &DateTime, text: &str, from: Option<&Source>) -> Result<(PathBuf, usize), Error> {
        let store = self.brain_store();
        self.brain_add_in(ui, &store, when, text, from)
    }

    /// `brain_add` for the brain in `store`.
    pub(super) fn brain_add_in(&mut self, ui: &dyn Ui, store: &Store, when: &DateTime, text: &str, from: Option<&Source>) -> Result<(PathBuf, usize), Error> {
        let (path, note) = self.brain_change_in(ui, store, &|old| brain::with_entry(old, when, text, from))?;
        Ok((path, brain::entries(&note).last().map_or(1, |e| e.line)))
    }

    /// Change the Brain's file by adding to its end (`change` gets the text and gives the new text). Done in the tab
    /// when the file is open in one, on disk otherwise. Returns the path and the new text.
    fn brain_change_in(&mut self, ui: &dyn Ui, store: &Store, change: &dyn Fn(&str) -> String) -> Result<(PathBuf, String), Error> {
        let path = brain::note_path(&store.root);
        let Some(i) = self.tabs.iter().position(|t| t.path.as_deref().is_some_and(|p| same_path(p, &path))) else {
            return store.edit(self.brain_write_key()?, change);
        };
        let was_clean = !self.tabs[i].dirty && !self.tabs[i].forced_dirty;
        let new = self.agent_edit(i, |s| {
            let old = sci::get_text(s);
            let new = change(&old);
            if new != old {
                let (start, tail) = brain::tail_edit(&old, &new);
                crate::search::replace_range(s, start, old.len(), tail);
            }
            Ok(new)
        })?;
        if was_clean {
            let keep = self.tabs[self.active].id;
            self.activate(i);
            self.save(ui);
            if let Some(back) = self.tabs.iter().position(|t| t.id == keep) {
                self.activate(back);
            }
        }
        Ok((path, new))
    }

    /// Make the Brain usable: when it is locked, ask for the profile's password. False when it stays locked.
    pub fn brain_ready(&mut self, ui: &dyn Ui) -> bool {
        !self.brain_locked() || ui.unlock(&mut self.vault, "your quick notes") && !self.brain_locked()
    }

    /// Every note as text, for searching.
    ///
    /// # Errors
    /// `Locked` when a note is encrypted and the profile is locked.
    pub fn brain_notes(&self) -> Result<Vec<brain::Note>, Error> {
        brain::store::Index::default().notes(&self.brain_store(), self.vault.dek.as_ref())
    }

    /// Open the Brain's file in a tab, with today's heading at its end when it is not there yet. Returns the path and
    /// the last line (where to put the caret).
    ///
    /// # Errors
    /// `Locked`; the store's error.
    pub fn brain_open_day(&mut self, ui: &dyn Ui, when: &DateTime) -> Result<(PathBuf, usize), Error> {
        let store = self.brain_store();
        let (path, note) = self.brain_change_in(ui, &store, &|old| brain::with_day(old, when))?;
        self.open_path(ui, &path);
        Ok((path, note.lines().count().max(1)))
    }

    /// The selected text of the tab in front, and where it is from; None when nothing (but blanks) is selected.
    pub fn brain_selection(&self) -> Option<(String, Source)> {
        if self.is_page() {
            return None;
        }
        let text = search::selected_text(&self.sci);
        if text.trim().is_empty() {
            return None;
        }
        let at = crate::num::to_usize(self.sci.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0));
        let line = crate::num::to_usize(self.sci.send(sci::SCI_LINEFROMPOSITION, at, 0).max(0)) + 1;
        let t = &self.tabs[self.active];
        let file = t.path.as_ref().map_or_else(|| t.title.clone(), |p| p.display().to_string());
        Some((text, Source { file, line }))
    }

    /// Turn the encryption of the Brain's notes on or off: every note is rewritten in the new form, and then the setting
    /// is changed. Returns how many notes there are. Stopping part-way leaves every note readable (see `Store::convert`).
    ///
    /// # Errors
    /// There is no encryption profile, or it stays locked; a note that cannot be read or written (the setting is then
    /// not changed).
    pub fn brain_set_encrypted(&mut self, ui: &dyn Ui, on: bool) -> Result<usize, Error> {
        let store = self.brain_store();
        self.brain_set_encrypted_in(ui, &store, on)
    }

    pub(super) fn brain_set_encrypted_in(&mut self, ui: &dyn Ui, store: &Store, on: bool) -> Result<usize, Error> {
        if self.vault.profile.is_none() {
            return Err(Error::Agent("Set up encryption first (Settings, Security): the notes are encrypted with your profile.".into()));
        }
        if !self.vault.unlocked() && !ui.unlock(&mut self.vault, "your quick notes") {
            return Err(Error::Locked);
        }
        let key = self.vault.dek.as_ref().ok_or(Error::Locked)?;
        let count = store.convert(Some(key), on.then_some(key), &mut |_, _| true)?;
        self.settings.brain_encrypt = on;
        Ok(count)
    }

    /// An agent's way into the Brain: refused unless the user allowed it in Settings (`add` is the stronger permission).
    fn agent_brain_gate(&self, add: bool) -> Result<(), Error> {
        match (self.settings.brain_agents.as_str(), add) {
            ("add", _) | ("read", false) => Ok(()),
            ("read", true) => Err(Error::Agent("The user lets agents search and read Quick Notes but not add to it (Settings, Quick Notes).".into())),
            _ => Err(Error::Agent("The user has not allowed agents into Quick Notes (Settings, Quick Notes).".into())),
        }
    }

    /// The `brain_overview` tool: what the Brain holds, so an agent knows what to ask for.
    pub(super) fn agent_brain_overview(&self) -> Result<serde_json::Value, Error> {
        self.agent_brain_gate(false)?;
        let notes = self.brain_notes()?;
        let mut per_day: std::collections::BTreeMap<Date, usize> = std::collections::BTreeMap::new();
        for day in notes.iter().flat_map(|n| brain::entries(&n.text)).filter_map(|e| e.day) {
            *per_day.entry(day).or_insert(0) += 1;
        }
        let recent: Vec<_> = per_day.iter().rev().take(14).map(|(d, n)| serde_json::json!({ "day": d.to_string(), "entries": n })).collect();
        let files: Vec<_> = notes.iter().map(|n| serde_json::json!({ "name": n.name, "note": n.path.display().to_string() })).collect();
        let tags: Vec<_> = brain::tag_counts(&notes).into_iter().take(50).map(|(t, c)| serde_json::json!({ "tag": t, "entries": c })).collect();
        Ok(serde_json::json!({ "days": per_day.len(), "recent_days": recent, "notes": files, "tags": tags }))
    }

    /// The `brain_read` tool: one note of the Brain (by the `note` path a search or the overview gave), whole or in lines.
    pub(super) fn agent_brain_read(&self, args: crate::mcp::args::Args<'_>) -> Result<serde_json::Value, Error> {
        self.agent_brain_gate(false)?;
        let store = self.brain_store();
        let want = std::path::PathBuf::from(args.text("note")?);
        let listed = store
            .list()
            .into_iter()
            .find(|l| l.path == want)
            .ok_or_else(|| Error::Agent("That is not a note of Quick Notes. Use a path from brain_search or brain_overview.".into()))?;
        let text = store.read(&listed.path, self.vault.dek.as_ref())?;
        let start = crate::num::to_usize(args.number("start_line")?.unwrap_or(1));
        let count = args.number("line_count")?.map(crate::num::to_usize);
        let part = crate::mcp::text::lines(&text, start, count, 60_000);
        Ok(
            serde_json::json!({ "note": listed.name, "first_line": part.first, "last_line": part.last, "total_lines": part.total, "next_line": part.next, "text": part.text }),
        )
    }

    fn agent_now(&self) -> Result<DateTime, Error> {
        self.clock.map(|f| f()).ok_or_else(|| Error::Agent("Quick Notes cannot tell the time here.".into()))
    }

    /// The `brain_search` tool.
    pub(super) fn agent_brain_search(&self, args: crate::mcp::args::Args<'_>) -> Result<serde_json::Value, Error> {
        self.agent_brain_gate(false)?;
        let query = brain::parse_query(args.text("query")?, Date::of(&self.agent_now()?));
        if query.is_empty() {
            return Err(Error::Agent("Give some words, a #tag or a day to search for.".into()));
        }
        let limit = crate::num::to_usize(args.number("limit")?.unwrap_or(20)).clamp(1, 50);
        let hits = brain::search(&self.brain_notes()?, &query, limit);
        let rows: Vec<_> = hits
            .iter()
            .map(|h| serde_json::json!({ "note": h.path.display().to_string(), "line": h.line, "title": h.title, "snippet": h.snippet, "tags": h.tags }))
            .collect();
        Ok(serde_json::json!({ "count": rows.len(), "results": rows }))
    }

    /// The `brain_add` tool.
    pub(super) fn agent_brain_add(&mut self, args: crate::mcp::args::Args<'_>) -> Result<serde_json::Value, Error> {
        self.agent_brain_gate(true)?;
        let text = args.text("text")?.trim().to_string();
        if text.is_empty() {
            return Err(Error::Agent("\"text\" is empty.".into()));
        }
        let when = self.agent_now()?;
        let (path, line) = self.brain_add(&super::agent::Headless::default(), &when, &text, None)?;
        Ok(serde_json::json!({ "note": path.display().to_string(), "line": line }))
    }
}
