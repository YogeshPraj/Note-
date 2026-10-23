//! Tabs, documents, open/save/close and session restore.
//!
//! Portable: it only talks to Scintilla through `sci::Send` and to the user
//! through the `Ui` trait, so the same logic drives the Win32, GTK and Cocoa
//! shells. One Scintilla view is shared by all tabs; each tab owns a Scintilla
//! *document* that is swapped in with SCI_SETDOCPOINTER (this is how
//! Notepad++ does it too), which keeps undo history and lexer state per tab.

use super::bookmarks;
use super::doc::{self, Enc, Eol};
use super::edit;
use super::lang;
use super::lexer;
use super::preview;
use super::search::{self, Opts};
use super::sci::{self, Send, Theme};
use super::session::{Session, SessionTab};
use std::path::{Path, PathBuf};

pub enum Confirm {
    Save,
    Discard,
    Cancel,
}

/// Everything the workspace needs from the host: dialogs and error display.
pub trait Ui {
    fn pick_open(&self) -> Vec<PathBuf>;
    fn pick_save(&self, suggested: &str) -> Option<PathBuf>;
    fn confirm_save(&self, name: &str) -> Confirm;
    fn error(&self, msg: &str);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabKind {
    Text,
    /// An Excalidraw board: no Scintilla document, `content` holds the scene JSON.
    Whiteboard,
}

pub struct Tab {
    /// Stable identity (indices shift when tabs close).
    pub id: u64,
    pub kind: TabKind,
    pub title: String,
    pub path: Option<PathBuf>,
    pub doc: isize,
    /// Scene JSON for whiteboards; unused for text tabs.
    pub content: String,
    pub dirty: bool,
    /// Set when something other than the text changed (e.g. encoding) so the tab shows as modified.
    pub forced_dirty: bool,
    pub enc: Enc,
    pub eol: Eol,
    pub lang: &'static str,
    pub caret: usize,
    pub top: usize,
}

/// What the tab strip needs to draw one tab.
pub struct TabInfo {
    pub title: String,
    pub dirty: bool,
    /// Small pill shown before the title (e.g. "wb" for whiteboards).
    pub badge: Option<&'static str>,
}

pub struct Workspace<S: Send> {
    pub sci: S,
    /// Optional second view onto the same document, drawn tiny as a minimap.
    pub mini: Option<S>,
    pub tabs: Vec<Tab>,
    pub active: usize,
    pub theme: Theme,
    pub wrap: bool,
    pub minimap: bool,
    /// Options of the last search, so F3 works without the dialog.
    pub search: Opts,
    /// Most recently opened/saved files, newest first (max `MAX_RECENT`).
    pub recent: Vec<PathBuf>,
    smart_key: (usize, usize, usize),
    next_id: u64,
}

pub const MAX_RECENT: usize = 15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineOp {
    SortAsc,
    SortDesc,
    RemoveDuplicates,
    RemoveEmpty,
    TrimTrailing,
    ToggleComment,
}

pub struct Status {
    pub position: String,
    pub lines: String,
    pub length: String,
    pub eol: String,
    pub enc: String,
    pub lang: String,
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

fn lang_for(path: &Path) -> &'static str {
    path.extension()
        .and_then(|e| e.to_str())
        .map(lexer::lexer_for_extension)
        .unwrap_or("null")
}

fn file_title(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl<S: Send> Workspace<S> {
    pub fn new(sci: S) -> Self {
        let theme = Theme::default();
        let ws = Self {
            sci,
            mini: None,
            tabs: Vec::new(),
            active: 0,
            theme,
            wrap: true,
            minimap: true,
            search: Opts { wrap: true, ..Default::default() },
            recent: Vec::new(),
            smart_key: (0, 0, 0),
            next_id: 1,
        };
        ws.style_view(&ws.sci, false);
        sci::set_wrap(&ws.sci, true);
        ws
    }

    /// Colours, margins and indicators for one view. The minimap is the same
    /// document at a tiny zoom with no gutter, caret or scrollbars.
    fn style_view(&self, v: &S, mini: bool) {
        let theme = &self.theme;
        sci::configure(v, "Cascadia Code", 11, theme);
        if mini {
            for margin in 0..3 {
                v.send(sci::SCI_SETMARGINWIDTHN, margin, 0);
            }
            v.send(sci::SCI_SETZOOM, (-9i32) as usize, 0);
            v.send(sci::SCI_SETWRAPMODE, sci::SC_WRAP_CHAR, 0);
            v.send(sci::SCI_SETCARETSTYLE, 0, 0);
            v.send(sci::SCI_SETCARETLINEVISIBLE, 0, 0);
            v.send(sci::SCI_SETVSCROLLBAR, 0, 0);
            v.send(sci::SCI_SETHSCROLLBAR, 0, 0);
        } else {
            sci::setup_line_numbers(v, theme, 54);
            sci::setup_folding(v, theme);
            bookmarks::setup(v, theme);
        }
        // Find marks: amber box. Smart highlight: green box (Notepad++ style).
        for (ind, rgb, alpha) in [(search::INDIC_FIND, (0xFF, 0xB0, 0x00), 110), (search::INDIC_SMART, (0x4C, 0xC9, 0x5A), 90)] {
            v.send(sci::SCI_INDICSETSTYLE, ind, sci::INDIC_STRAIGHTBOX as isize);
            v.send(sci::SCI_INDICSETFORE, ind, sci::rgb(rgb.0, rgb.1, rgb.2));
            v.send(sci::SCI_INDICSETALPHA, ind, alpha);
            v.send(sci::SCI_INDICSETOUTLINEALPHA, ind, 200);
        }
    }

    pub fn attach_minimap(&mut self, mini: S) {
        self.style_view(&mini, true);
        if let Some(t) = self.tabs.get(self.active) {
            mini.send(sci::SCI_SETDOCPOINTER, 0, t.doc);
            lang::apply_palette(&mini, t.lang, &self.theme);
        }
        self.mini = Some(mini);
    }

    /// Point every view at `doc`.
    fn set_doc(&self, doc: isize) {
        self.send(sci::SCI_SETDOCPOINTER, 0, doc);
        if let Some(m) = &self.mini {
            m.send(sci::SCI_SETDOCPOINTER, 0, doc);
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.style_view(&self.sci, false);
        if let Some(m) = &self.mini {
            self.style_view(m, true);
        }
        sci::set_wrap(&self.sci, self.wrap);
        if !self.tabs.is_empty() {
            self.apply_palettes();
        }
    }

    /// Keep the minimap scrolled in proportion to the main view.
    pub fn sync_minimap(&self) {
        let Some(m) = &self.mini else { return };
        let total = sci::line_count(&self.sci).max(1);
        let main_first = self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0);
        let main_vis = self.send(sci::SCI_LINESONSCREEN, 0, 0).max(1);
        let mini_vis = m.send(sci::SCI_LINESONSCREEN, 0, 0).max(1);
        let first = if total <= mini_vis { 0 } else { main_first * (total - mini_vis) / (total - main_vis).max(1) };
        m.send(sci::SCI_SETFIRSTVISIBLELINE, first.max(0) as usize, 0);
    }

    /// Scroll the main view so document line `line` (0-based) is centred.
    pub fn scroll_to_line(&self, line: usize) {
        let vis = self.send(sci::SCI_LINESONSCREEN, 0, 0).max(1) as usize;
        self.send(sci::SCI_SETFIRSTVISIBLELINE, line.saturating_sub(vis / 2), 0);
        self.sync_minimap();
    }

    /// Update the smart highlight if the selection or document size changed.
    pub fn update_smart(&mut self) {
        let a = self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
        let b = self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
        let key = (a, b, sci::length(&self.sci).max(0) as usize);
        if key != self.smart_key {
            self.smart_key = key;
            search::smart_highlight(&self.sci);
        }
    }

    fn send(&self, m: u32, w: usize, l: isize) -> isize {
        self.sci.send(m, w, l)
    }

    pub fn is_dirty(&self, i: usize) -> bool {
        let t = &self.tabs[i];
        if t.kind == TabKind::Whiteboard {
            return t.dirty || t.forced_dirty;
        }
        t.forced_dirty || if i == self.active { self.send(sci::SCI_GETMODIFY, 0, 0) != 0 } else { t.dirty }
    }

    /// Is the active tab a whiteboard (so the Scintilla view is not in use)?
    pub fn is_whiteboard(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|t| t.kind == TabKind::Whiteboard)
    }

    fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    fn remember_view(&mut self) {
        if self.tabs.is_empty() || self.is_whiteboard() {
            return;
        }
        let dirty = self.send(sci::SCI_GETMODIFY, 0, 0) != 0;
        let caret = self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
        let top = self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize;
        let t = &mut self.tabs[self.active];
        t.dirty = dirty;
        t.caret = caret;
        t.top = top;
    }

    /// Re-apply language colours to every view (does not touch caret or scroll).
    fn apply_palettes(&self) {
        let t = &self.tabs[self.active];
        lang::apply_palette(&self.sci, t.lang, &self.theme);
        if let Some(m) = &self.mini {
            lang::apply_palette(m, t.lang, &self.theme);
        }
    }

    /// Show the active tab: colours, then its remembered caret and scroll position.
    fn apply_view(&self) {
        let t = &self.tabs[self.active];
        self.apply_palettes();
        self.send(sci::SCI_GOTOPOS, t.caret, 0);
        self.send(sci::SCI_SETFIRSTVISIBLELINE, t.top, 0);
        self.sync_minimap();
    }

    /// Create a tab holding `text`. The tab is clean (at the save point).
    fn open_doc(&mut self, title: String, path: Option<PathBuf>, text: &str, enc: Enc, eol: Eol, lang: &'static str) -> usize {
        self.remember_view();
        let doc = self.send(sci::SCI_CREATEDOCUMENT, 0, 0);
        self.set_doc(doc);
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        sci::add_text(&self.sci, text);
        self.send(sci::SCI_SETSAVEPOINT, 0, 0);
        self.send(sci::SCI_EMPTYUNDOBUFFER, 0, 0);
        lang::apply_lexer(&self.sci, lang, &self.theme);
        let id = self.take_id();
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
            caret: 0,
            top: 0,
        });
        self.active = self.tabs.len() - 1;
        self.apply_view();
        self.active
    }

    /// Add a whiteboard tab holding scene JSON `content`.
    pub fn open_whiteboard(&mut self, title: String, path: Option<PathBuf>, content: String, dirty: bool) -> usize {
        self.remember_view();
        let id = self.take_id();
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
            caret: 0,
            top: 0,
        });
        self.active = self.tabs.len() - 1;
        self.active
    }

    pub fn new_whiteboard(&mut self) -> usize {
        let title = super::whiteboard::next_title(self.tabs.iter().map(|t| t.title.as_str()));
        self.open_whiteboard(title, None, super::whiteboard::empty_scene(), false)
    }

    pub fn set_language(&mut self, lang: &'static str) {
        self.tabs[self.active].lang = lang;
        lang::apply_lexer(&self.sci, lang, &self.theme);
        self.apply_palettes();
    }

    pub fn set_encoding(&mut self, enc: Enc) {
        let t = &mut self.tabs[self.active];
        if t.enc != enc {
            t.enc = enc;
            t.forced_dirty = true;
        }
    }

    /// Apply a line operation to the selection (or whole document). Returns a
    /// message to show the user when nothing could be done.
    pub fn line_op(&mut self, op: LineOp) -> Option<String> {
        let t = &self.tabs[self.active];
        let eol = match t.eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let changed = match op {
            LineOp::SortAsc => edit::apply_lines(&self.sci, &|s| edit::sort_lines(s, eol, false)),
            LineOp::SortDesc => edit::apply_lines(&self.sci, &|s| edit::sort_lines(s, eol, true)),
            LineOp::RemoveDuplicates => edit::apply_lines(&self.sci, &|s| edit::remove_duplicate_lines(s, eol)),
            LineOp::RemoveEmpty => edit::apply_lines(&self.sci, &|s| edit::remove_empty_lines(s, eol)),
            LineOp::TrimTrailing => edit::apply_lines(&self.sci, &|s| edit::trim_trailing(s, eol)),
            LineOp::ToggleComment => {
                let Some(prefix) = edit::comment_prefix(t.lang) else {
                    return Some("No line-comment syntax for this language".into());
                };
                edit::apply_lines(&self.sci, &|s| edit::toggle_comment(s, eol, prefix))
            }
        };
        (!changed).then(|| "Nothing to change".into())
    }

    /// What the active tab can be previewed as, if anything.
    pub fn preview_kind(&self) -> Option<preview::Kind> {
        let t = self.tabs.get(self.active)?;
        preview::kind_for(t.lang, t.path.as_deref())
    }

    /// Identity of the active document's content, to skip redundant re-renders.
    pub fn preview_key(&self) -> (isize, usize, u64) {
        let text = sci::get_text(&self.sci);
        let mut h: u64 = 0xcbf29ce484222325; // FNV-1a
        for b in text.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100000001b3);
        }
        (self.tabs.get(self.active).map(|t| t.doc).unwrap_or(0), text.len(), h)
    }

    pub fn toggle_bookmark(&self) {
        bookmarks::toggle(&self.sci, bookmarks::current_line(&self.sci));
    }

    pub fn goto_bookmark(&self, forward: bool) -> bool {
        bookmarks::goto(&self.sci, forward)
    }

    pub fn clear_bookmarks(&self) {
        bookmarks::clear_all(&self.sci);
    }

    /// Toggle the bookmark on the line containing byte position `pos` (margin click).
    pub fn toggle_bookmark_at(&self, pos: usize) {
        let line = self.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0) as usize;
        bookmarks::toggle(&self.sci, line);
    }

    fn note_recent(&mut self, p: &Path) {
        self.recent.retain(|r| !same_path(r, p));
        self.recent.insert(0, p.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }

    /// Drop recent entries whose files no longer exist.
    pub fn prune_recent(&mut self) {
        self.recent.retain(|p| p.exists());
    }

    pub fn new_tab(&mut self) -> usize {
        let n = (1..)
            .find(|n| !self.tabs.iter().any(|t| t.path.is_none() && t.title == format!("new {n}")))
            .unwrap();
        self.open_doc(format!("new {n}"), None, "", Enc::Utf8, Eol::platform_default(), "null")
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
        let (text, enc, eol) = doc::decode(&bytes);
        if super::whiteboard::is_whiteboard_file(&text, path) {
            self.open_whiteboard(file_title(path), Some(path.to_path_buf()), text, false);
            self.note_recent(path);
            return;
        }
        let replace_pristine = self.tabs.len() == 1
            && self.tabs[0].kind == TabKind::Text
            && self.tabs[0].path.is_none()
            && !self.is_dirty(0)
            && sci::length(&self.sci) == 0;
        self.open_doc(file_title(path), Some(path.to_path_buf()), &text, enc, eol, lang_for(path));
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

    pub fn activate(&mut self, i: usize) {
        if i >= self.tabs.len() || i == self.active {
            return;
        }
        self.remember_view();
        self.active = i;
        if self.tabs[i].kind == TabKind::Text {
            self.set_doc(self.tabs[i].doc);
            self.apply_view();
        }
    }

    pub fn next_tab(&mut self, delta: isize) {
        let n = self.tabs.len() as isize;
        if n > 1 {
            self.activate((self.active as isize + delta).rem_euclid(n) as usize);
        }
    }

    fn write_to(&mut self, ui: &dyn Ui, path: &Path) -> bool {
        let whiteboard = self.is_whiteboard();
        let bytes = if whiteboard {
            self.tabs[self.active].content.clone().into_bytes()
        } else {
            let text = sci::get_text(&self.sci);
            doc::encode(&text, self.tabs[self.active].enc)
        };
        match std::fs::write(path, bytes) {
            Ok(()) => {
                if !whiteboard {
                    self.send(sci::SCI_SETSAVEPOINT, 0, 0);
                }
                let t = &mut self.tabs[self.active];
                t.dirty = false;
                t.forced_dirty = false;
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
        if !self.write_to(ui, &p) {
            return false;
        }
        self.note_recent(&p);
        if self.is_whiteboard() {
            let t = &mut self.tabs[self.active];
            t.title = file_title(&p);
            t.path = Some(p);
            return true;
        }
        let lang = lang_for(&p);
        let changed = self.tabs[self.active].lang != lang;
        let t = &mut self.tabs[self.active];
        t.title = file_title(&p);
        t.path = Some(p);
        t.lang = lang;
        if changed {
            lang::apply_lexer(&self.sci, lang, &self.theme);
            self.apply_palettes();
        }
        true
    }

    pub fn save_all(&mut self, ui: &dyn Ui) {
        let keep = self.active;
        for i in 0..self.tabs.len() {
            if self.is_dirty(i) {
                self.activate(i);
                if !self.save(ui) {
                    break;
                }
            }
        }
        self.activate(keep);
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
        let (doc, is_text) = (self.tabs[i].doc, self.tabs[i].kind == TabKind::Text);
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
        true
    }

    pub fn set_wrap(&mut self, on: bool) {
        self.wrap = on;
        sci::set_wrap(&self.sci, on);
    }

    pub fn set_eol(&mut self, eol: Eol) {
        self.send(sci::SCI_CONVERTEOLS, eol.sci_mode(), 0);
        self.send(sci::SCI_SETEOLMODE, eol.sci_mode(), 0);
        self.tabs[self.active].eol = eol;
    }

    /// What the tab strip draws for each tab, in order.
    pub fn tab_infos(&self) -> Vec<TabInfo> {
        (0..self.tabs.len())
            .map(|i| TabInfo {
                title: self.tabs[i].title.clone(),
                dirty: self.is_dirty(i),
                badge: (self.tabs[i].kind == TabKind::Whiteboard).then_some("wb"),
            })
            .collect()
    }

    pub fn window_title(&self) -> String {
        let t = &self.tabs[self.active];
        let name = t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t.title.clone());
        format!("{}{} - Note++", if self.is_dirty(self.active) { "* " } else { "" }, name)
    }

    pub fn status(&self) -> Status {
        let t = &self.tabs[self.active];
        if t.kind == TabKind::Whiteboard {
            return Status {
                position: "Whiteboard".into(),
                lines: String::new(),
                length: format!("elements: {}", scene_element_count(&t.content)),
                eol: String::new(),
                enc: "UTF-8".into(),
                lang: "Excalidraw".into(),
            };
        }
        let pos = self.send(sci::SCI_GETCURRENTPOS, 0, 0);
        let line = self.send(sci::SCI_LINEFROMPOSITION, pos as usize, 0) + 1;
        let col = self.send(sci::SCI_GETCOLUMN, pos as usize, 0) + 1;
        let (a, b) = (self.send(sci::SCI_GETSELECTIONSTART, 0, 0), self.send(sci::SCI_GETSELECTIONEND, 0, 0));
        let sel = (b - a).max(0);
        let sel_lines = if sel > 0 {
            self.send(sci::SCI_LINEFROMPOSITION, b as usize, 0) - self.send(sci::SCI_LINEFROMPOSITION, a as usize, 0) + 1
        } else {
            0
        };
        Status {
            position: format!("Ln: {line}  Col: {col}  Sel: {sel} | {sel_lines}"),
            lines: format!("lines: {}", sci::line_count(&self.sci)),
            length: format!("length: {}", sci::length(&self.sci)),
            eol: t.eol.label().into(),
            enc: t.enc.label().into(),
            lang: lang::display_name(t.lang),
        }
    }

    // ---- sessions ----------------------------------------------------------

    pub fn snapshot(&mut self) -> Session {
        self.remember_view();
        let cur = self.active;
        // Reading hidden tabs means swapping documents in; put back whatever was showing.
        let shown = self.send(sci::SCI_GETDOCPOINTER, 0, 0);
        let mut tabs = Vec::new();
        let mut active = 0;
        for i in 0..self.tabs.len() {
            let t = &self.tabs[i];
            let untitled = t.path.is_none();
            let mut text = None;
            if t.kind == TabKind::Whiteboard {
                if t.dirty || untitled {
                    text = Some(t.content.clone());
                }
                if untitled && super::whiteboard::is_empty_scene(&t.content) && self.tabs.len() > 1 {
                    continue;
                }
            } else {
                if t.dirty || t.forced_dirty || untitled {
                    self.send(sci::SCI_SETDOCPOINTER, 0, t.doc);
                    text = Some(sci::get_text(&self.sci));
                }
                if untitled && text.as_deref() == Some("") && self.tabs.len() > 1 {
                    continue;
                }
            }
            if i == cur {
                active = tabs.len();
            }
            tabs.push(SessionTab {
                title: t.title.clone(),
                path: t.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
                text,
                caret: t.caret,
                top: t.top,
                enc: t.enc.key().into(),
                eol: t.eol.key().into(),
                kind: if t.kind == TabKind::Whiteboard { "whiteboard".into() } else { "text".into() },
            });
        }
        self.send(sci::SCI_SETDOCPOINTER, 0, shown);
        Session {
            tabs,
            active,
            wrap: self.wrap,
            dark: self.theme.dark,
            minimap: self.minimap,
            recent: self.recent.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        }
    }

    pub fn restore(&mut self, s: &Session) {
        self.minimap = s.minimap;
        self.recent = s.recent.iter().map(PathBuf::from).filter(|p| p.exists()).take(MAX_RECENT).collect();
        if s.dark {
            self.set_theme(Theme::dark());
        }
        self.set_wrap(s.wrap);
        for st in &s.tabs {
            if st.kind == "whiteboard" {
                let path = st.path.as_ref().map(PathBuf::from);
                let disk = path.as_ref().and_then(|p| std::fs::read(p).ok()).map(|b| doc::decode(&b).0);
                let (content, dirty) = match (&st.text, disk) {
                    (Some(t), Some(d)) => (t.clone(), *t != d),
                    (Some(t), None) => (t.clone(), true),
                    (None, Some(d)) => (d, false),
                    (None, None) => continue,
                };
                self.open_whiteboard(st.title.clone(), path, content, dirty);
                continue;
            }
            let (enc, eol) = (Enc::from_key(&st.enc), Eol::from_key(&st.eol));
            let idx = match &st.path {
                Some(p) => {
                    let path = PathBuf::from(p);
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let (text, enc, eol) = doc::decode(&bytes);
                            self.open_doc(file_title(&path), Some(path.clone()), &text, enc, eol, lang_for(&path))
                        }
                        // File vanished: keep the unsaved text if we have it, otherwise skip.
                        Err(_) if st.text.is_some() => {
                            self.open_doc(file_title(&path), Some(path.clone()), "", enc, eol, lang_for(&path))
                        }
                        Err(_) => continue,
                    }
                }
                None => self.open_doc(st.title.clone(), None, "", enc, eol, "null"),
            };
            if let Some(text) = &st.text {
                // Replacing content after the save point leaves the tab dirty, as it was.
                sci::set_text(&self.sci, text);
            }
            let t = &mut self.tabs[idx];
            t.caret = st.caret;
            t.top = st.top;
        }
        if self.tabs.is_empty() {
            self.new_tab();
        } else {
            let want = s.active.min(self.tabs.len() - 1);
            if want != self.active {
                self.activate(want);
            } else if !self.is_whiteboard() {
                self.apply_view();
            }
        }
    }
}

fn scene_element_count(content: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("elements").and_then(|e| e.as_array()).map(|e| e.iter().filter(|x| x.get("isDeleted").and_then(|d| d.as_bool()) != Some(true)).count()))
        .unwrap_or(0)
}
