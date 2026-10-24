//! Tabs, documents, open/save/close and session restore.
//!
//! Portable: it only talks to Scintilla through `sci::Send` and to the user
//! through the `Ui` trait, so the same logic drives the Win32, GTK and Cocoa
//! shells. One Scintilla view is shared by all tabs; each tab owns a Scintilla
//! *document* that is swapped in with SCI_SETDOCPOINTER (this is how
//! Notepad++ does it too), which keeps undo history and lexer state per tab.

use super::bookmarks;
use super::complete;
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
    /// Open (`save == false`) or save a session file.
    fn pick_session(&self, save: bool) -> Option<PathBuf>;
    fn confirm_save(&self, name: &str) -> Confirm;
    fn ask_yes_no(&self, msg: &str) -> bool;
    /// Move a file to the Recycle Bin.
    fn recycle(&self, path: &Path) -> Result<(), String>;
    fn error(&self, msg: &str);
}

/// Which tabs a "Close Multiple Documents" command targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CloseOp {
    All,
    AllButActive,
    ToTheLeft,
    ToTheRight,
    Unchanged,
}

/// Indices (in tab order) selected by `op`, given the active tab and which tabs are modified.
pub fn close_targets(op: CloseOp, active: usize, dirty: &[bool]) -> Vec<usize> {
    (0..dirty.len())
        .filter(|&i| match op {
            CloseOp::All => true,
            CloseOp::AllButActive => i != active,
            CloseOp::ToTheLeft => i < active,
            CloseOp::ToTheRight => i > active,
            CloseOp::Unchanged => !dirty[i],
        })
        .collect()
}

/// How many recently closed files are remembered.
pub const MAX_CLOSED: usize = 20;

/// Push `path` to the front of a most-recent-first list without duplicates, capped at `max`.
pub fn push_recent(list: &mut Vec<PathBuf>, path: &Path, max: usize) {
    list.retain(|r| !same_path(r, path));
    list.insert(0, path.to_path_buf());
    list.truncate(max);
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
    /// Recently closed files, most recent first, for "Restore Recent Closed File".
    pub closed: Vec<PathBuf>,
    /// Folder shown in the workspace tree (also the root for Quick Open and Find in Files).
    pub workspace: Option<PathBuf>,
    /// Index into the platform's list of web search engines.
    pub search_engine: usize,
    /// Picture used by Insert > Date Time (customized).
    pub date_format: String,
    /// Where "Begin/End Select" was started: (document, caret position).
    select_anchor: Option<(isize, usize)>,
    smart_key: (usize, usize, usize),
    next_id: u64,
}

pub const MAX_RECENT: usize = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineOp {
    Sort(edit::SortKind, bool),
    RemoveDuplicates,
    RemoveConsecutiveDuplicates,
    RemoveEmpty,
    RemoveBlank,
    Reverse,
    Randomize,
    Join,
    Split,
    TrimTrailing,
    TrimLeading,
    TrimBoth,
    EolToSpace,
    RemoveUnnecessaryBlanks,
    TabsToSpaces,
    SpacesToTabsAll,
    SpacesToTabsLeading,
    ToggleComment,
    Comment,
    Uncomment,
    BlockComment,
    BlockUncomment,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaseOp {
    Proper,
    ProperBlend,
    Sentence,
    SentenceBlend,
    Invert,
    Random,
}

/// Which auto-completion source to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Completion {
    Words,
    Functions,
    Path,
    Hint,
}

/// Which path text "Copy to Clipboard" puts on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathText {
    FullPath,
    FileName,
    Directory,
    AllNames,
    AllPaths,
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
            closed: Vec::new(),
            workspace: None,
            search_engine: 0,
            date_format: "yyyy-MM-dd HH:mm:ss".into(),
            select_anchor: None,
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

    /// Apply a line operation to the selection (or whole document / current line,
    /// depending on the operation). Returns a message to show the user when
    /// nothing could be done.
    pub fn line_op(&mut self, op: LineOp) -> Option<String> {
        use edit::Scope::{CurrentLine, WholeDocument};
        let t = &self.tabs[self.active];
        let eol = match t.eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let lang = t.lang;
        let tab = self.send(sci::SCI_GETTABWIDTH, 0, 0).max(1) as usize;
        let s = &self.sci;
        let has_selection = self.send(sci::SCI_GETSELECTIONSTART, 0, 0) != self.send(sci::SCI_GETSELECTIONEND, 0, 0);
        let lines = |scope, f: &dyn Fn(&str) -> String| edit::apply_lines(s, scope, f);
        let changed = match op {
            LineOp::Sort(kind, desc) => lines(WholeDocument, &|x| edit::sort_lines_by(x, eol, kind, desc)),
            LineOp::RemoveDuplicates => lines(WholeDocument, &|x| edit::remove_duplicate_lines(x, eol)),
            LineOp::RemoveConsecutiveDuplicates => lines(WholeDocument, &|x| edit::remove_consecutive_duplicates(x, eol)),
            LineOp::RemoveEmpty => lines(WholeDocument, &|x| edit::remove_empty_lines(x, eol)),
            LineOp::RemoveBlank => lines(WholeDocument, &|x| edit::remove_blank_lines(x, eol)),
            LineOp::Reverse => lines(WholeDocument, &|x| edit::reverse_lines(x, eol)),
            LineOp::Randomize => {
                let seed = edit::seed_now();
                lines(WholeDocument, &|x| edit::randomize_lines(x, eol, seed))
            }
            LineOp::Join | LineOp::Split if !has_selection => return Some("Select the lines first".into()),
            LineOp::Join => lines(CurrentLine, &|x| edit::join_lines(x)),
            LineOp::Split => lines(CurrentLine, &|x| edit::split_lines(x, eol, 80)),
            LineOp::TrimTrailing => lines(WholeDocument, &|x| edit::trim_trailing(x, eol)),
            LineOp::TrimLeading => lines(WholeDocument, &|x| edit::trim_leading(x, eol)),
            LineOp::TrimBoth => lines(WholeDocument, &|x| edit::trim_both(x, eol)),
            LineOp::EolToSpace => lines(WholeDocument, &|x| edit::join_lines(x)),
            LineOp::RemoveUnnecessaryBlanks => lines(WholeDocument, &|x| edit::remove_unnecessary_blanks(x, eol)),
            LineOp::TabsToSpaces => lines(WholeDocument, &|x| edit::tabs_to_spaces(x, eol, tab)),
            LineOp::SpacesToTabsAll => lines(WholeDocument, &|x| edit::spaces_to_tabs(x, eol, tab, false)),
            LineOp::SpacesToTabsLeading => lines(WholeDocument, &|x| edit::spaces_to_tabs(x, eol, tab, true)),
            LineOp::ToggleComment | LineOp::Comment | LineOp::Uncomment => {
                let Some(prefix) = edit::comment_prefix(lang) else {
                    return Some("No line-comment syntax for this language".into());
                };
                lines(CurrentLine, &|x| match op {
                    LineOp::ToggleComment => edit::toggle_comment(x, eol, prefix),
                    LineOp::Comment => edit::comment_lines(x, eol, prefix),
                    _ => edit::uncomment_lines(x, eol, prefix),
                })
            }
            LineOp::BlockComment | LineOp::BlockUncomment => {
                let Some((open, close)) = edit::comment_block(lang) else {
                    return Some("No block-comment syntax for this language".into());
                };
                edit::apply_range(s, true, &|x| {
                    if op == LineOp::BlockComment { edit::block_comment(x, open, close) } else { edit::block_uncomment(x, open, close) }
                })
            }
        };
        (!changed).then(|| "Nothing to change".into())
    }

    /// Change the case of the selected text. Column selections are left alone.
    pub fn case_op(&mut self, op: CaseOp) -> Option<String> {
        if self.send(sci::SCI_GETSELECTIONS, 0, 0) != 1 || self.send(sci::SCI_SELECTIONISRECTANGLE, 0, 0) != 0 {
            return Some("Select a single block of text first".into());
        }
        let seed = edit::seed_now();
        let changed = edit::apply_range(&self.sci, false, &|x| match op {
            CaseOp::Proper => edit::proper_case(x, false),
            CaseOp::ProperBlend => edit::proper_case(x, true),
            CaseOp::Sentence => edit::sentence_case(x, false),
            CaseOp::SentenceBlend => edit::sentence_case(x, true),
            CaseOp::Invert => edit::invert_case(x),
            CaseOp::Random => edit::random_case(x, seed),
        });
        (!changed).then(|| "Select some text first".into())
    }

    /// First call remembers the caret; the second selects from there to the new
    /// caret as a stream or (for `column`) a rectangle.
    pub fn begin_end_select(&mut self, column: bool) -> Option<String> {
        let doc = self.tabs[self.active].doc;
        let caret = self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
        match self.select_anchor.take() {
            Some((d, anchor)) if d == doc => {
                if column {
                    self.send(sci::SCI_SETRECTANGULARSELECTIONANCHOR, anchor, 0);
                    self.send(sci::SCI_SETRECTANGULARSELECTIONCARET, caret, 0);
                } else {
                    self.send(sci::SCI_SETSEL, anchor, caret as isize);
                }
                None
            }
            _ => {
                self.select_anchor = Some((doc, caret));
                Some("Selection start marked: move the caret and repeat to select".into())
            }
        }
    }

    /// The lines the column editor works on, and the character column it inserts at.
    fn column_target(&self) -> (usize, usize, usize) {
        let text = sci::get_text(&self.sci);
        let a = self.send(sci::SCI_GETANCHOR, 0, 0).max(0) as usize;
        let c = self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
        let col_of = |pos: usize| {
            let line = self.send(sci::SCI_LINEFROMPOSITION, pos, 0) as usize;
            let start = self.send(sci::SCI_POSITIONFROMLINE, line, 0) as usize;
            text.get(start..pos).map_or(0, |s| s.chars().count())
        };
        let (la, lc) = (self.send(sci::SCI_LINEFROMPOSITION, a, 0) as usize, self.send(sci::SCI_LINEFROMPOSITION, c, 0) as usize);
        (la.min(lc), la.max(lc), col_of(a).min(col_of(c)))
    }

    /// Insert text or a number sequence at the caret column of every selected line.
    pub fn column_edit(&mut self, text: Option<&str>, numbers: Option<&edit::ColumnNumbers>) -> Option<String> {
        let (first, last, col) = self.column_target();
        let count = last - first + 1;
        let insert: Vec<String> = match (numbers, text) {
            (Some(n), _) => edit::number_sequence(n, count),
            (None, Some(t)) => vec![t.to_string(); count],
            (None, None) => return Some("Nothing to insert".into()),
        };
        let doc = sci::get_text(&self.sci);
        let start = self.send(sci::SCI_POSITIONFROMLINE, first, 0) as usize;
        let end = self.send(sci::SCI_GETLINEENDPOSITION, last, 0) as usize;
        let old = &doc[start..end];
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let src: Vec<&str> = old.split(['\n']).map(|l| l.trim_end_matches('\r')).collect();
        let new = edit::insert_at_column(&src, col, &insert).join(eol);
        if new == old {
            return Some("Nothing to change".into());
        }
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETSTART, start, 0);
        self.send(sci::SCI_SETTARGETEND, end, 0);
        self.send(sci::SCI_REPLACETARGET, new.len(), new.as_ptr() as isize);
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        self.send(sci::SCI_SETSEL, start, (start + new.len()) as isize);
        None
    }

    /// Replace the selection (or insert at the caret) with `text`.
    pub fn insert_text(&mut self, text: &str) {
        self.insert_bytes(text.as_bytes());
    }

    /// Like `insert_text` for raw bytes (binary paste may hold NULs).
    pub fn insert_bytes(&mut self, bytes: &[u8]) {
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_CLEAR, 0, 0);
        self.send(sci::SCI_ADDTEXT, bytes.len(), bytes.as_ptr() as isize);
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
    }

    /// Add an empty line above or below the caret's line.
    pub fn insert_blank_line(&mut self, above: bool) {
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let caret = self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
        let line = self.send(sci::SCI_LINEFROMPOSITION, caret, 0) as usize;
        let at = if above {
            self.send(sci::SCI_POSITIONFROMLINE, line, 0)
        } else {
            self.send(sci::SCI_GETLINEENDPOSITION, line, 0)
        } as usize;
        let text = std::ffi::CString::new(eol).unwrap_or_default();
        self.send(sci::SCI_INSERTTEXT, at, text.as_ptr() as isize);
        if !above {
            let next = self.send(sci::SCI_POSITIONFROMLINE, line + 1, 0);
            self.send(sci::SCI_GOTOPOS, next as usize, 0);
        }
    }

    /// Indent or dedent by one level: the selected lines, or the caret's line.
    pub fn indent(&mut self, increase: bool) {
        let multi = {
            let a = self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
            let b = self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
            self.send(sci::SCI_LINEFROMPOSITION, a, 0) != self.send(sci::SCI_LINEFROMPOSITION, b, 0)
        };
        if !increase {
            self.send(sci::SCI_BACKTAB, 0, 0);
        } else if multi {
            self.send(sci::SCI_TAB, 0, 0);
        } else {
            let caret = self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
            let line = self.send(sci::SCI_LINEFROMPOSITION, caret, 0) as usize;
            let width = self.send(sci::SCI_GETTABWIDTH, 0, 0).max(1);
            let current = self.send(sci::SCI_GETLINEINDENTATION, line, 0);
            self.send(sci::SCI_SETLINEINDENTATION, line, current + width);
        }
    }

    pub fn is_read_only(&self) -> bool {
        self.send(sci::SCI_GETREADONLY, 0, 0) != 0
    }

    pub fn set_read_only(&mut self, on: bool) {
        self.send(sci::SCI_SETREADONLY, on as usize, 0);
    }

    /// Show an auto-completion list or call tip at the caret. Returns a message
    /// when there is nothing to offer.
    pub fn complete(&mut self, kind: Completion) -> Option<String> {
        let text = sci::get_text(&self.sci);
        let caret = (self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize).min(text.len());
        let line = self.send(sci::SCI_LINEFROMPOSITION, caret, 0) as usize;
        let line_start = self.send(sci::SCI_POSITIONFROMLINE, line, 0) as usize;
        let before_on_line = text.get(line_start..caret).unwrap_or("");
        let show = |sci: &S, entered: usize, items: &[String]| {
            let list = std::ffi::CString::new(items.join(" ")).unwrap_or_default();
            sci.send(sci::SCI_AUTOCSETIGNORECASE, 1, 0);
            sci.send(sci::SCI_AUTOCSETCHOOSESINGLE, 1, 0);
            sci.send(sci::SCI_AUTOCSHOW, entered, list.as_ptr() as isize);
        };
        match kind {
            Completion::Words | Completion::Functions => {
                let (at, prefix) = complete::word_before(&text, caret);
                let items = if kind == Completion::Words {
                    complete::words_with_prefix(&text, prefix, at)
                } else {
                    complete::function_names(&text, prefix, at)
                };
                if items.is_empty() {
                    return Some("No suggestions".into());
                }
                show(&self.sci, prefix.len(), &items);
            }
            Completion::Path => {
                let base = self.tabs[self.active].path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
                let (typed, items) = complete::path_candidates(complete::path_fragment(before_on_line), base.as_deref());
                if items.is_empty() {
                    return Some("No matching paths".into());
                }
                show(&self.sci, typed, &items);
            }
            Completion::Hint => {
                // The call the caret is inside: the name before the nearest unmatched "(".
                let mut depth = 0;
                let open = before_on_line.char_indices().rev().find(|(_, c)| {
                    match c {
                        ')' => depth += 1,
                        '(' if depth == 0 => return true,
                        '(' => depth -= 1,
                        _ => {}
                    }
                    false
                });
                let name = open.map(|(i, _)| complete::word_before(before_on_line, i).1).unwrap_or("");
                let Some(sig) = (!name.is_empty()).then(|| complete::signature_for(&text, name)).flatten() else {
                    return Some("No signature found".into());
                };
                let tip = std::ffi::CString::new(sig).unwrap_or_default();
                self.send(sci::SCI_CALLTIPSHOW, caret, tip.as_ptr() as isize);
            }
        }
        None
    }
    pub fn path_text(&self, kind: PathText) -> Option<String> {
        let one = |t: &Tab, f: &dyn Fn(&Path) -> String| Some(t.path.as_deref().map(f).unwrap_or_else(|| t.title.clone()));
        let t = &self.tabs[self.active];
        match kind {
            PathText::FullPath => t.path.as_ref().map(|p| p.display().to_string()),
            PathText::FileName => one(t, &|p| file_title(p)),
            PathText::Directory => t.path.as_ref().and_then(|p| p.parent()).map(|d| d.display().to_string()),
            PathText::AllNames => Some(self.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>().join("\r\n")),
            PathText::AllPaths => Some(
                self.tabs
                    .iter()
                    .map(|t| t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| t.title.clone()))
                    .collect::<Vec<_>>()
                    .join("\r\n"),
            ),
        }
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
        push_recent(&mut self.recent, p, MAX_RECENT);
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

    /// The active tab as it would be written to disk.
    fn current_bytes(&self) -> Vec<u8> {
        if self.is_whiteboard() {
            self.tabs[self.active].content.clone().into_bytes()
        } else {
            doc::encode(&sci::get_text(&self.sci), self.tabs[self.active].enc)
        }
    }

    fn write_to(&mut self, ui: &dyn Ui, path: &Path) -> bool {
        let whiteboard = self.is_whiteboard();
        match std::fs::write(path, self.current_bytes()) {
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
        self.write_and_adopt(ui, p)
    }

    /// Write the active tab to another file, leaving the tab itself untouched.
    pub fn save_copy_as(&mut self, ui: &dyn Ui) -> Option<PathBuf> {
        let t = &self.tabs[self.active];
        let p = ui.pick_save(&t.title)?;
        match std::fs::write(&p, self.current_bytes()) {
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
        let Some(path) = self.tabs[self.active].path.clone() else { return false };
        if self.is_dirty(self.active) && !ui.ask_yes_no(&format!("\"{}\" has unsaved changes.\n\nReload from disk and lose them?", self.tabs[self.active].title)) {
            return false;
        }
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                ui.error(&format!("Cannot read {}\n\n{e}", path.display()));
                return false;
            }
        };
        let (text, enc, eol) = doc::decode(&bytes);
        if self.is_whiteboard() {
            let t = &mut self.tabs[self.active];
            t.content = text;
            t.dirty = false;
            return true;
        }
        let (caret, top) = (self.tabs[self.active].caret, self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0) as usize);
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
        t.forced_dirty = false;
        t.dirty = false;
        let len = sci::length(&self.sci).max(0) as usize;
        self.send(sci::SCI_GOTOPOS, caret.min(len), 0);
        self.send(sci::SCI_SETFIRSTVISIBLELINE, top, 0);
        true
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

    fn write_and_adopt(&mut self, ui: &dyn Ui, p: PathBuf) -> bool {
        if !self.write_to(ui, &p) {
            return false;
        }
        self.adopt_path(p);
        true
    }

    /// Point the active tab at `p` (title, language, recent list).
    fn adopt_path(&mut self, p: PathBuf) {
        self.note_recent(&p);
        let whiteboard = self.is_whiteboard();
        let lang = lang_for(&p);
        let changed = !whiteboard && self.tabs[self.active].lang != lang;
        let t = &mut self.tabs[self.active];
        t.title = file_title(&p);
        t.path = Some(p);
        if !whiteboard {
            t.lang = lang;
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
        let ids: Vec<u64> = close_targets(op, self.active, &dirty).into_iter().map(|i| self.tabs[i].id).collect();
        for id in ids {
            let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
            if !self.close(ui, i) {
                return false;
            }
        }
        true
    }

    /// Drop tab `i`, showing a neighbour (or a fresh tab if it was the last one).
    fn remove_tab(&mut self, i: usize, remember: bool) {
        self.activate(i);
        let (doc, is_text) = (self.tabs[i].doc, self.tabs[i].kind == TabKind::Text);
        if remember {
            if let Some(p) = self.tabs[i].path.clone() {
                push_recent(&mut self.closed, &p, MAX_CLOSED);
            }
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
            workspace: self.workspace.as_ref().map(|p| p.to_string_lossy().into_owned()),
            search_engine: self.search_engine,
            date_format: self.date_format.clone(),
        }
    }

    /// Restore the whole saved state (startup): settings, workspace and tabs.
    pub fn restore(&mut self, s: &Session) {
        self.minimap = s.minimap;
        self.search_engine = s.search_engine;
        if !s.date_format.is_empty() {
            self.date_format = s.date_format.clone();
        }
        self.recent = s.recent.iter().map(PathBuf::from).filter(|p| p.exists()).take(MAX_RECENT).collect();
        self.workspace = s.workspace.as_ref().map(PathBuf::from).filter(|p| p.is_dir());
        if s.dark {
            self.set_theme(Theme::dark());
        }
        self.set_wrap(s.wrap);
        self.restore_tabs(s);
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

    /// "Load Session": add the session's tabs to the ones already open.
    /// Returns how many tabs were opened.
    pub fn open_session(&mut self, s: &Session) -> usize {
        let start = self.tabs.len();
        let had_pristine_tab = self.tabs.len() == 1
            && self.tabs[0].kind == TabKind::Text
            && self.tabs[0].path.is_none()
            && !self.is_dirty(0)
            && sci::length(&self.sci) == 0;
        self.restore_tabs(s);
        let opened = self.tabs.len() - start;
        if opened > 0 {
            if had_pristine_tab {
                // The empty starter tab is just in the way now.
                self.remove_tab(0, false);
                let want = s.active.min(opened - 1);
                self.activate(want);
            } else {
                self.activate(start + s.active.min(opened - 1));
            }
        }
        opened
    }

    /// Open the tabs described by `s` (skipping files that are already open).
    fn restore_tabs(&mut self, s: &Session) {
        for st in &s.tabs {
            let path = st.path.as_ref().map(PathBuf::from);
            if let Some(p) = &path {
                if self.tabs.iter().any(|t| t.path.as_deref().is_some_and(|q| same_path(q, p))) {
                    continue;
                }
            }
            if st.kind == "whiteboard" {
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
            let idx = match &path {
                Some(path) => match std::fs::read(path) {
                    Ok(bytes) => {
                        let (text, enc, eol) = doc::decode(&bytes);
                        self.open_doc(file_title(path), Some(path.clone()), &text, enc, eol, lang_for(path))
                    }
                    // File vanished: keep the unsaved text if we have it, otherwise skip.
                    Err(_) if st.text.is_some() => self.open_doc(file_title(path), Some(path.clone()), "", enc, eol, lang_for(path)),
                    Err(_) => continue,
                },
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
    }
}

fn scene_element_count(content: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("elements").and_then(|e| e.as_array()).map(|e| e.iter().filter(|x| x.get("isDeleted").and_then(|d| d.as_bool()) != Some(true)).count()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(l.iter().filter(|p| **p == PathBuf::from("C:\\f5.txt")).count(), 1);
        assert_eq!(l.len(), 10);
    }
}
