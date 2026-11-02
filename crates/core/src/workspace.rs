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
use super::session::{Session, SessionTab};
use super::settings::{self, Settings};
use super::textools;
use super::view;
use std::path::{Path, PathBuf};

mod types;
pub use self::types::*;
mod documents;
mod editing;
mod file_io;
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
    next_id: u64,
}

pub const MAX_RECENT: usize = 10;

impl<S: Send> Workspace<S> {
    pub fn new(sci: S) -> Self {
        let theme = Theme::default();
        let ws = Self {
            sci,
            mini: None,
            probe: None,
            tabs: Vec::new(),
            active: 0,
            theme,
            wrap: true,
            minimap: true,
            search: Opts { wrap: true, ..Default::default() },
            recent: Vec::new(),
            closed: Vec::new(),
            workspace: None,
            search_engine: 0,
            symbols: view::Symbols::default(),
            settings: Settings::default(),
            date_format: "yyyy-MM-dd HH:mm:ss".into(),
            vault: crypto::Vault::load(),
            select_anchor: None,
            smart_key: (0, 0, 0),
            detect_seen: (0, 0),
            lang_locked: Vec::new(),
            next_id: 1,
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
mod tests {
    use super::*;
    use std::cell::Cell;

    /// A stand-in for Scintilla that tracks which document the view shows.
    #[derive(Default)]
    struct FakeSci {
        shown: Cell<isize>,
        next: Cell<isize>,
        selected_zero: Cell<bool>,
        swaps: Cell<usize>,
    }

    impl sci::Send for FakeSci {
        fn send(&self, msg: u32, _w: usize, l: isize) -> isize {
            match msg {
                sci::SCI_CREATEDOCUMENT => {
                    self.next.set(self.next.get() + 1);
                    self.next.get()
                }
                sci::SCI_SETDOCPOINTER => {
                    // Selecting document 0 makes the real Scintilla create a stray document.
                    if l == 0 {
                        self.selected_zero.set(true);
                    }
                    self.swaps.set(self.swaps.get() + 1);
                    self.shown.set(l);
                    0
                }
                sci::SCI_GETDOCPOINTER => self.shown.get(),
                _ => 0,
            }
        }
    }

    #[test]
    fn measuring_a_text_tab_while_a_whiteboard_is_active_leaves_the_view_alone() {
        // The session autosave measures every tab; with a whiteboard in front it used to put the
        // view back on the whiteboard's "document" (0), which crashed Scintilla when the tab closed.
        let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
        ws.new_tab();
        ws.new_whiteboard();
        let before = ws.sci.shown.get();
        assert_ne!(before, 0);
        ws.session_fingerprint();
        assert!(!ws.sci.selected_zero.get(), "the view was pointed at document 0");
        assert_eq!(ws.sci.shown.get(), before, "the view must keep showing the same document");
    }

    #[test]
    fn with_a_probe_the_editor_never_swaps_documents() {
        // Swapping a document into the editor costs it its multiple selections and horizontal scroll, so the session
        // autosave and the tab tool tips read hidden tabs through the probe view instead.
        let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
        ws.new_tab();
        ws.new_tab();
        ws.attach_probe(FakeSci { shown: Cell::new(7), ..FakeSci::default() });
        let (shown, swaps) = (ws.sci.shown.get(), ws.sci.swaps.get());
        ws.session_fingerprint();
        ws.snapshot();
        ws.tab_preview(0, 3, 10);
        assert_eq!(ws.sci.swaps.get(), swaps, "the editor's document was swapped");
        assert_eq!(ws.sci.shown.get(), shown);
        let probe = ws.probe.as_ref().map(|(p, idle)| (p.shown.get(), *idle));
        assert_eq!(probe, Some((7, 7)), "the probe must rest on its own document again");
    }

    #[test]
    fn thousands_separators() {
        assert_eq!((commafy(0), commafy(999), commafy(1000), commafy(22_187_160)), ("0".into(), "999".into(), "1,000".into(), "22,187,160".into()));
    }

    #[test]
    fn sort_order_by_each_field() {
        let key = |name: &str, path: &str, kind: &str, len, modified| SortKeys { name: name.into(), path: path.into(), kind: kind.into(), len, modified };
        let keys = vec![
            key("b.txt", "C:\\z\\b.txt", "Rust", 30, 5),
            key("A.txt", "C:\\y\\A.txt", "Normal Text", 10, 9),
            key("c.txt", "", "Normal Text", 20, 0),
            key("d.txt", "C:\\a\\d.txt", "Normal Text", 20, 7),
        ];
        assert_eq!(sort_order(&keys, SortField::Name, false), vec![1, 0, 2, 3]);
        assert_eq!(sort_order(&keys, SortField::Name, true), vec![3, 2, 0, 1]);
        assert_eq!(sort_order(&keys, SortField::Path, false), vec![2, 3, 1, 0]);
        assert_eq!(sort_order(&keys, SortField::Type, false), vec![2, 3, 1, 0]);
        // Equal lengths fall back to the path.
        assert_eq!(sort_order(&keys, SortField::Length, false), vec![1, 2, 3, 0]);
        assert_eq!(sort_order(&keys, SortField::Modified, true), vec![1, 3, 0, 2]);
    }

    #[test]
    fn close_targets_cover_every_menu_option() {
        let dirty = [false, true, false, false, true];
        assert_eq!(close_targets(CloseOp::All, 2, &dirty), vec![0, 1, 2, 3, 4]);
        assert_eq!(close_targets(CloseOp::AllButActive, 2, &dirty), vec![0, 1, 3, 4]);
        assert_eq!(close_targets(CloseOp::ToTheLeft, 2, &dirty), vec![0, 1]);
        assert_eq!(close_targets(CloseOp::ToTheRight, 2, &dirty), vec![3, 4]);
        assert_eq!(close_targets(CloseOp::Unchanged, 2, &dirty), vec![0, 2, 3]);
    }

    #[test]
    fn close_targets_edges() {
        assert!(close_targets(CloseOp::ToTheLeft, 0, &[false, false]).is_empty());
        assert!(close_targets(CloseOp::ToTheRight, 1, &[false, false]).is_empty());
        assert!(close_targets(CloseOp::AllButActive, 0, &[false]).is_empty());
        assert!(close_targets(CloseOp::All, 0, &[]).is_empty());
    }

    #[test]
    fn recent_list_is_deduped_newest_first_and_capped() {
        let mut l = Vec::new();
        for n in 0..12 {
            push_recent(&mut l, Path::new(&format!("C:\\f{n}.txt")), 10);
        }
        assert_eq!(l.len(), 10);
        assert_eq!(l[0], PathBuf::from("C:\\f11.txt"));
        push_recent(&mut l, Path::new("C:\\f5.txt"), 10);
        assert_eq!(l[0], PathBuf::from("C:\\f5.txt"));
        assert_eq!(l.iter().filter(|p| p.as_path() == Path::new("C:\\f5.txt")).count(), 1);
        assert_eq!(l.len(), 10);
    }
}
