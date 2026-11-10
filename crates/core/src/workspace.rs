//! Tabs, documents, open/save/close and session restore.
//!
//! Portable: it only talks to Scintilla through `sci::Send` and to the user
//! through the `Ui` trait, so the same logic drives the Win32, GTK and Cocoa
//! shells. One Scintilla view is shared by all tabs; each tab owns a Scintilla
//! *document* that is swapped in with `SCI_SETDOCPOINTER` (the usual approach), which keeps undo history and lexer state per tab.

use super::bookmarks;
use super::complete;
use super::crypto;
use super::doc::{self, Enc, Eol};
use super::edit;
use super::files;
use super::lang;
use super::lexer;
use super::mdformat;
use super::preview;
use super::sci::{self, Send, Theme};
use super::search::{self, Opts};
use super::session::{DockSide, FloatRect, Session, SessionDock, SessionFloat, SessionTab};
use super::settings::{self, Settings};
use super::textools;
use super::view;
use std::path::{Path, PathBuf};

mod types;
pub use self::types::*;
mod agent;
mod agent_board;
pub use self::agent_board::{BoardRequest, PREVIEW_PAGE};
mod agent_commands;
mod agent_edit;
mod dock;
mod documents;
mod editing;
mod file_io;
mod floats;
mod session_ops;
mod tabs;
mod view_ops;

/// How many recently closed files are remembered.
pub const MAX_CLOSED: usize = 20;

/// The colours a tab can be marked with (name, RGB).
pub const TAB_COLORS: [(&str, (u8, u8, u8)); 8] = [
    ("Red", (0xE0, 0x5A, 0x5A)),
    ("Orange", (0xE8, 0x8B, 0x3A)),
    ("Yellow", (0xD1, 0xA8, 0x2C)),
    ("Green", (0x4C, 0xAF, 0x68)),
    ("Teal", (0x1F, 0x9E, 0xA6)),
    ("Blue", (0x4A, 0x8F, 0xE8)),
    ("Purple", (0x8E, 0x5B, 0xC9)),
    ("Pink", (0xD0, 0x5E, 0xA8)),
];

/// Modification time and size of a file.
pub type Stamp = (std::time::SystemTime, u64);

#[derive(Debug)]
pub struct Workspace<S: Send> {
    pub sci: S,
    /// Optional second view onto the same document, drawn tiny as a minimap.
    pub mini: Option<S>,
    /// A hidden view for reading other tabs' documents, and the empty document it rests on (see `with_doc`).
    probe: Option<(S, isize)>,
    /// The editor view that is not current (see `dock`).
    second: Option<dock::Second<S>>,
    /// The floating windows' views.
    floats: Vec<floats::Float<S>>,
    /// While the user is in a floating window: the tab the main window shows.
    in_float: Option<u64>,
    /// The floating tab asked for by something other than the keyboard: its window comes forward.
    raise: Option<u64>,
    saved_floats: Vec<SessionFloat>,
    /// Where the main window is, kept for the session (see `placement`).
    pub main_window: Option<FloatRect>,
    dock: Option<dock::Docked>,
    /// How much of the shared space the docked view has, in thousandths.
    dock_share: u32,
    /// The dock the session saved, until the shell has a window to show it in.
    saved_dock: Option<SessionDock>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub theme: Theme,
    pub wrap: bool,
    pub minimap: bool,
    /// Options of the last search, so F3 works without the dialog.
    pub search: Opts,
    /// Most recently opened/saved files, newest first (max `MAX_RECENT`).
    pub recent: Vec<PathBuf>,
    /// Recently closed files, most recent first, for "Restore Recent Closed File".
    pub closed: Vec<PathBuf>,
    /// Folder shown in the workspace tree (also the root for Quick Open and Find in Files).
    pub workspace: Option<PathBuf>,
    /// The tree was closed by the user (kept in the session, so it stays closed on the next start).
    pub tree_hidden: bool,
    /// Index into the platform's list of web search engines.
    pub search_engine: usize,
    /// User preferences (Settings menu).
    pub settings: Settings,
    /// Which invisible characters and guides the editor draws (View > Show Symbol).
    pub symbols: view::Symbols,
    /// Picture used by Insert > Date Time (customized).
    pub date_format: String,
    /// The encryption profile and, while unlocked, its key.
    pub vault: crypto::Vault,
    /// Where "Begin/End Select" was started: (document, caret position).
    select_anchor: Option<(isize, usize)>,
    smart_key: (usize, usize, usize),
    /// The active tab and its text length at the last look, to notice pasted text.
    detect_seen: (u64, usize),
    /// Tabs whose language the user chose by hand: never guessed again.
    lang_locked: Vec<u64>,
    /// Saved tabs that were not opened this time (an encrypted file whose password prompt was declined): written back
    /// into the session as they were, so their unsaved text comes back once the user unlocks.
    parked: Vec<SessionTab>,
    /// Saved tabs of kinds this app cannot show, passed through the session untouched (see `Session::kept`).
    kept: Vec<serde_json::Value>,
    next_id: u64,
    /// What the user has said about agents and encrypted tabs.
    agent_grants: agent::Grants,
    /// The display scale in percent (100 = 100%): margin widths are given at 100% and scaled by it.
    ui_percent: isize,
}

pub const MAX_RECENT: usize = 10;

impl<S: Send> Workspace<S> {
    pub fn new(sci: S) -> Self {
        let theme = Theme::default();
        let ws = Self {
            sci,
            mini: None,
            probe: None,
            second: None,
            floats: Vec::new(),
            in_float: None,
            raise: None,
            saved_floats: Vec::new(),
            main_window: None,
            dock: None,
            dock_share: 500,
            saved_dock: None,
            tabs: Vec::new(),
            active: 0,
            theme,
            wrap: true,
            minimap: true,
            search: Opts { wrap: true, ..Default::default() },
            recent: Vec::new(),
            closed: Vec::new(),
            workspace: None,
            tree_hidden: false,
            search_engine: 0,
            symbols: view::Symbols::default(),
            settings: Settings::default(),
            date_format: "yyyy-MM-dd HH:mm:ss".into(),
            vault: crypto::Vault::load(),
            select_anchor: None,
            smart_key: (0, 0, 0),
            detect_seen: (0, 0),
            lang_locked: Vec::new(),
            parked: Vec::new(),
            kept: Vec::new(),
            next_id: 1,
            agent_grants: agent::Grants::default(),
            ui_percent: 100,
        };
        ws.style_view(&ws.sci, false);
        sci::set_wrap(&ws.sci, true);
        view::apply_symbols(&ws.sci, &ws.symbols);
        ws
    }

    fn send(&self, m: u32, w: usize, l: isize) -> isize {
        self.sci.send(m, w, l)
    }

    // ---- encryption ---------------------------------------------------------------------

    /// Decrypt `env` for the file called `name`, setting up what is missing on the way: adopting
    /// the Electron app's profile when it owns the file, then asking for the password.
    fn decrypt_for(&mut self, ui: &dyn Ui, env: &crypto::Envelope, name: &str) -> Option<String> {
        if self.vault.profile.is_none() {
            self.vault = crypto::Vault::load();
        }
        match self.vault.fingerprint().map(str::to_string) {
            None => {
                let theirs = crypto::electron_profile_path()
                    .and_then(|p| std::fs::read_to_string(p).ok())
                    .and_then(|t| crypto::Profile::parse(&t))
                    .filter(|p| p.fingerprint == env.profile);
                if let Some(p) = theirs {
                    let msg = format!(
                        "\"{name}\" is protected by your Note++ encryption profile ({}).\n\nUse that profile here? It is copied; your password and recovery key stay the same.",
                        p.fingerprint_display()
                    );
                    if !ui.ask_yes_no(&msg) {
                        return None;
                    }
                    if let Err(e) = crypto::save_profile(&p) {
                        ui.error(&format!("Cannot save the profile\n\n{e}"));
                        return None;
                    }
                    self.vault.profile = Some(p);
                } else {
                    ui.error(&format!(
                        "\"{name}\" was encrypted with profile {}, which is not set up here.\n\nRestore that profile (Settings > Encryption > Import profile...) and open the file again.",
                        crypto::display_fingerprint(&env.profile)
                    ));
                    return None;
                }
            }
            Some(fp) if fp != env.profile => {
                ui.error(&format!(
                    "\"{name}\" belongs to a different encryption profile ({}).\n\nIt cannot be unlocked with the current profile ({}).",
                    crypto::display_fingerprint(&env.profile),
                    crypto::display_fingerprint(&fp)
                ));
                return None;
            }
            Some(_) => {}
        }
        if !self.vault.unlocked() && !ui.unlock(&mut self.vault, name) {
            return None;
        }
        let dek = self.vault.dek.as_ref()?;
        match crypto::decrypt_envelope(env, dek) {
            Ok(t) => Some(t),
            Err(e) => {
                ui.error(&format!("Cannot open \"{name}\"\n\n{e}"));
                None
            }
        }
    }

    // ---- sessions ----------------------------------------------------------

    /// A cheap fingerprint of what a session snapshot would hold (the tabs, which one is active
    /// and how much unsaved text there is), so identical snapshots can be skipped.
    pub fn session_fingerprint(&self) -> (usize, usize, usize, u64) {
        let (mut unsaved, mut ids) = (0usize, 0u64);
        for i in 0..self.tabs.len() {
            ids = ids.wrapping_mul(31).wrapping_add(self.tabs[i].id);
            if self.is_dirty(i) || self.tabs[i].path.is_none() {
                unsaved += self.doc_length(i);
            }
        }
        (self.tabs.len(), self.active, unsaved, ids)
    }
}

#[cfg(test)]
mod tests;
