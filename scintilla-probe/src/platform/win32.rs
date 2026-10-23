//! Win32 shell: the host window, layout, menus, commands and message loop.
//!
//! The ONLY place that wires Windows UI to the portable `core`. Drawing of the
//! menu bar / toolbar / tabs / status bar lives in `chrome.rs`, the Find panel
//! in `findpanel.rs`. All editor behaviour is in `core` (`Workspace`, `search`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::Win32::UI::Controls::Dialogs::*;
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetCapture, ReleaseCapture, SetCapture, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, PWSTR, w};

use super::chrome::{self, Chrome, StatusText, ToolItem};
use super::findpanel::{self, Mode};
use super::palette::{self, Item, Kind};
use super::preview as pview;
use super::whiteboard as wb;
use crate::core::preview as pcore;
use crate::core::doc::{Enc, Eol};
use crate::core::files;
use crate::core::lang::LANGUAGES;
use crate::core::sci::{self, Send, Theme};
use crate::core::search::{self, Hit};
use crate::core::session::{Session, config_dir, session_path};
use crate::core::whiteboard;
use crate::core::workspace::{Confirm, LineOp, Ui, Workspace};

const WM_REFRESH: u32 = WM_APP + 1;
const WM_MINIMAP: u32 = WM_APP + 2;
const WM_SPLIT: u32 = WM_APP + 60;
const TIMER_PREVIEW: usize = 1;
const MINI_W: i32 = 110;
const SPLIT_W: i32 = 5;
const ID_PREVIEW: usize = 5301;
const ID_NEW_WB: usize = 1008;

const ID_NEW: usize = 1001;
const ID_OPEN: usize = 1002;
const ID_SAVE: usize = 1003;
const ID_SAVEAS: usize = 1004;
const ID_SAVEALL: usize = 1005;
const ID_CLOSE: usize = 1006;
const ID_EXIT: usize = 1007;
const ID_UNDO: usize = 2001;
const ID_REDO: usize = 2002;
const ID_CUT: usize = 2003;
const ID_COPY: usize = 2004;
const ID_PASTE: usize = 2005;
const ID_SELALL: usize = 2006;
const ID_EOL_CRLF: usize = 2010;
const ID_EOL_LF: usize = 2011;
const ID_EOL_CR: usize = 2012;
const ID_WRAP: usize = 3001;
const ID_ZOOMIN: usize = 3002;
const ID_ZOOMOUT: usize = 3003;
const ID_ZOOMRESET: usize = 3004;
const ID_THEME: usize = 3005;
const ID_MINIMAP: usize = 3006;
const ID_NEXT_TAB: usize = 4001;
const ID_PREV_TAB: usize = 4002;
const ID_FIND: usize = 5001;
const ID_REPLACE: usize = 5002;
const ID_FINDNEXT: usize = 5003;
const ID_FINDPREV: usize = 5004;
const ID_GOTO: usize = 5005;
const ID_FIF: usize = 5006;
const ID_MARK: usize = 5007;
const ID_PALETTE: usize = 5201;
const ID_QUICKOPEN: usize = 5202;
const ID_BM_TOGGLE: usize = 5101;
const ID_BM_NEXT: usize = 5102;
const ID_BM_PREV: usize = 5103;
const ID_BM_CLEAR: usize = 5104;
const ID_L_DUP: usize = 2101;
const ID_L_DEL: usize = 2102;
const ID_L_UP: usize = 2103;
const ID_L_DOWN: usize = 2104;
const ID_L_SORT_ASC: usize = 2105;
const ID_L_SORT_DESC: usize = 2106;
const ID_L_DEDUP: usize = 2107;
const ID_L_NOEMPTY: usize = 2108;
const ID_L_TRIM: usize = 2109;
const ID_C_UPPER: usize = 2110;
const ID_C_LOWER: usize = 2111;
const ID_COMMENT: usize = 2112;
const ID_RECENT_BASE: usize = 10000; // + index into the recent list
const ID_RECENT_CLEAR: usize = 10100;
const ID_ABOUT: usize = 9001;
const ID_ENC_BASE: usize = 7000; // + index into ENCODINGS
const ID_LANG_BASE: usize = 8000; // + index into LANGUAGES

const ENCODINGS: [Enc; 5] = [Enc::Utf8, Enc::Utf8Bom, Enc::Utf16Le, Enc::Utf16Be, Enc::Ansi];

/// Everything the Command Palette offers: (label, command id, shortcut).
const PALETTE_COMMANDS: &[(&str, usize, &str)] = &[
    ("File: New", ID_NEW, "Ctrl+N"),
    ("File: New Whiteboard", ID_NEW_WB, ""),
    ("File: Open...", ID_OPEN, "Ctrl+O"),
    ("File: Save", ID_SAVE, "Ctrl+S"),
    ("File: Save As...", ID_SAVEAS, "Ctrl+Shift+S"),
    ("File: Save All", ID_SAVEALL, "Ctrl+Alt+S"),
    ("File: Close Tab", ID_CLOSE, "Ctrl+W"),
    ("File: Clear Recent Files List", ID_RECENT_CLEAR, ""),
    ("File: Quick Open...", ID_QUICKOPEN, "Ctrl+P"),
    ("Edit: Undo", ID_UNDO, "Ctrl+Z"),
    ("Edit: Redo", ID_REDO, "Ctrl+Y"),
    ("Edit: Select All", ID_SELALL, "Ctrl+A"),
    ("Edit: Duplicate Line", ID_L_DUP, "Ctrl+D"),
    ("Edit: Delete Line", ID_L_DEL, "Ctrl+L"),
    ("Edit: Move Line Up", ID_L_UP, "Ctrl+Shift+Up"),
    ("Edit: Move Line Down", ID_L_DOWN, "Ctrl+Shift+Down"),
    ("Edit: Sort Lines Ascending", ID_L_SORT_ASC, ""),
    ("Edit: Sort Lines Descending", ID_L_SORT_DESC, ""),
    ("Edit: Remove Duplicate Lines", ID_L_DEDUP, ""),
    ("Edit: Remove Empty Lines", ID_L_NOEMPTY, ""),
    ("Edit: Trim Trailing Space", ID_L_TRIM, ""),
    ("Edit: UPPERCASE", ID_C_UPPER, "Ctrl+Shift+U"),
    ("Edit: lowercase", ID_C_LOWER, "Ctrl+U"),
    ("Edit: Toggle Line Comment", ID_COMMENT, "Ctrl+/"),
    ("Edit: EOL to Windows (CR LF)", ID_EOL_CRLF, ""),
    ("Edit: EOL to Unix (LF)", ID_EOL_LF, ""),
    ("Edit: EOL to Macintosh (CR)", ID_EOL_CR, ""),
    ("Search: Find...", ID_FIND, "Ctrl+F"),
    ("Search: Replace...", ID_REPLACE, "Ctrl+H"),
    ("Search: Find in Files...", ID_FIF, "Ctrl+Shift+F"),
    ("Search: Mark...", ID_MARK, "Ctrl+M"),
    ("Search: Find Next", ID_FINDNEXT, "F3"),
    ("Search: Find Previous", ID_FINDPREV, "Shift+F3"),
    ("Search: Go to Line...", ID_GOTO, "Ctrl+G"),
    ("Bookmark: Toggle", ID_BM_TOGGLE, "Ctrl+F2"),
    ("Bookmark: Next", ID_BM_NEXT, "F2"),
    ("Bookmark: Previous", ID_BM_PREV, "Shift+F2"),
    ("Bookmark: Clear All", ID_BM_CLEAR, ""),
    ("View: Toggle Word Wrap", ID_WRAP, ""),
    ("View: Toggle Live Preview", ID_PREVIEW, "Ctrl+Shift+V"),
    ("View: Toggle Dark Mode", ID_THEME, ""),
    ("View: Toggle Minimap", ID_MINIMAP, ""),
    ("View: Zoom In", ID_ZOOMIN, "Ctrl+Num +"),
    ("View: Zoom Out", ID_ZOOMOUT, "Ctrl+Num -"),
    ("View: Reset Zoom", ID_ZOOMRESET, ""),
    ("View: Next Tab", ID_NEXT_TAB, "Ctrl+Tab"),
    ("View: Previous Tab", ID_PREV_TAB, "Ctrl+Shift+Tab"),
    ("Help: About Note++", ID_ABOUT, ""),
];

fn command_items() -> Vec<Item> {
    let mut v: Vec<Item> = PALETTE_COMMANDS.iter().map(|(l, id, k)| Item { label: l.to_string(), hint: k.to_string(), id: *id }).collect();
    for (i, e) in ENCODINGS.iter().enumerate() {
        v.push(Item { label: format!("Encoding: {}", e.label()), hint: String::new(), id: ID_ENC_BASE + i });
    }
    for (i, (label, _)) in LANGUAGES.iter().enumerate() {
        v.push(Item { label: format!("Language: {label}"), hint: String::new(), id: ID_LANG_BASE + i });
    }
    v
}

/// Win32 delivers Scintilla messages with SendMessage. GTK would use
/// `scintilla_send_message`, Cocoa `[ScintillaView message:]`.
#[derive(Clone, Copy)]
pub struct Win32Sci(pub HWND);

impl Send for Win32Sci {
    fn send(&self, msg: u32, w: usize, l: isize) -> isize {
        unsafe { SendMessageW(self.0, msg, Some(WPARAM(w)), Some(LPARAM(l))).0 }
    }
}

#[derive(Clone, Copy)]
pub struct Handles {
    host: HWND,
    edit: HWND,
    mini: HWND,
    split: HWND,
    chrome: Chrome,
    panel: HWND,
}

#[derive(Clone, Copy)]
pub struct Menus {
    view: HMENU,
    enc: HMENU,
    eol: HMENU,
    lang: HMENU,
    recent: HMENU,
}

struct App {
    ws: Workspace<Win32Sci>,
    ui: Win32Ui,
    menus: Menus,
    hits: Vec<Hit>,
    hits_root: PathBuf,
    /// One-shot message shown in place of the cursor position.
    flash: Option<String>,
    recent_shown: Vec<PathBuf>,
    /// Files offered by Quick Open (index = palette item id).
    quick_files: Vec<PathBuf>,
    /// Content identity last sent to the preview, to skip redundant renders.
    preview_key: Option<(isize, usize, u64)>,
    /// Document the auto-open check last looked at.
    preview_doc: isize,
    preview_dark: bool,
    /// Excalidraw page has loaded and can receive a scene.
    wb_ready: bool,
    /// Tab id whose scene is currently loaded in the whiteboard view.
    wb_shown: Option<u64>,
    wb_dark: bool,
    /// A Save / Save As waiting for the page to hand over its latest scene.
    wb_pending: Option<WbPending>,
}

#[derive(Clone, Copy, PartialEq)]
enum WbPending {
    Save,
    SaveAs,
}

/// Commands that make sense on a whiteboard tab (everything else targets the text editor).
fn wb_allows(id: usize) -> bool {
    matches!(
        id,
        ID_NEW | ID_NEW_WB | ID_OPEN | ID_SAVE | ID_SAVEAS | ID_SAVEALL | ID_CLOSE | ID_EXIT | ID_THEME | ID_NEXT_TAB | ID_PREV_TAB
            | ID_ABOUT | ID_PALETTE | ID_QUICKOPEN | ID_RECENT_CLEAR
    ) || (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id)
}

thread_local! {
    static HANDLES: Cell<Option<Handles>> = const { Cell::new(None) };
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static MINI_ON: Cell<bool> = const { Cell::new(true) };
    static PREVIEW_ON: Cell<bool> = const { Cell::new(false) };
    static PREVIEW_MAX: Cell<bool> = const { Cell::new(false) };
    /// The active tab is a whiteboard, so the whiteboard view replaces the editor.
    static WB_ON: Cell<bool> = const { Cell::new(false) };
    /// Fraction of the editor row given to the preview.
    static PREVIEW_RATIO: Cell<f32> = const { Cell::new(0.5) };
}

static MINI_PREV: AtomicIsize = AtomicIsize::new(0);

/// Run `f` on the app unless it is already borrowed (e.g. a message arrives
/// while a modal dialog is open inside a command), in which case skip.
fn with_app(f: impl FnOnce(&mut App)) {
    APP.with(|c| {
        if let Ok(mut b) = c.try_borrow_mut() {
            if let Some(a) = b.as_mut() {
                f(a);
            }
        }
    });
}

fn handles() -> Option<Handles> {
    HANDLES.with(|c| c.get())
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Build a double-NUL terminated "label\0pattern\0...\0\0" filter string.
fn wide_multi(parts: &[&str]) -> Vec<u16> {
    let mut v = Vec::new();
    for p in parts {
        v.extend(p.encode_utf16());
        v.push(0);
    }
    v.push(0);
    v
}

// ---- file dialogs ----------------------------------------------------------

#[derive(Clone, Copy)]
struct Win32Ui {
    host: HWND,
}

impl Ui for Win32Ui {
    fn pick_open(&self) -> Vec<PathBuf> {
        let mut buf = vec![0u16; 65536];
        let filter = wide_multi(&["All files (*.*)", "*.*", "Text (*.txt)", "*.txt"]);
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.host,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_EXPLORER | OFN_ALLOWMULTISELECT | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY,
            ..Default::default()
        };
        if !unsafe { GetOpenFileNameW(&mut ofn) }.as_bool() {
            return Vec::new();
        }
        // Result: "dir\0file1\0file2\0\0", or just "fullpath\0\0" for one file.
        let parts: Vec<String> =
            buf.split(|c| *c == 0).take_while(|s| !s.is_empty()).map(String::from_utf16_lossy).collect();
        match parts.len() {
            0 => Vec::new(),
            1 => vec![PathBuf::from(&parts[0])],
            _ => parts[1..].iter().map(|f| PathBuf::from(&parts[0]).join(f)).collect(),
        }
    }

    fn pick_save(&self, suggested: &str) -> Option<PathBuf> {
        let mut buf = vec![0u16; 32768];
        for (i, c) in suggested.encode_utf16().take(buf.len() - 1).enumerate() {
            buf[i] = c;
        }
        let filter = wide_multi(&["All files (*.*)", "*.*", "Text (*.txt)", "*.txt"]);
        let mut ofn = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: self.host,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: PWSTR(buf.as_mut_ptr()),
            nMaxFile: buf.len() as u32,
            Flags: OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST,
            ..Default::default()
        };
        if !unsafe { GetSaveFileNameW(&mut ofn) }.as_bool() {
            return None;
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
    }

    fn confirm_save(&self, name: &str) -> Confirm {
        let text = wide(&format!("Save changes to \"{name}\"?"));
        let r = unsafe { MessageBoxW(Some(self.host), PCWSTR(text.as_ptr()), w!("Note++"), MB_YESNOCANCEL | MB_ICONWARNING) };
        match r {
            IDYES => Confirm::Save,
            IDNO => Confirm::Discard,
            _ => Confirm::Cancel,
        }
    }

    fn error(&self, msg: &str) {
        let text = wide(msg);
        unsafe {
            MessageBoxW(Some(self.host), PCWSTR(text.as_ptr()), w!("Note++"), MB_OK | MB_ICONERROR);
        }
    }
}

// ---- app behaviour -----------------------------------------------------------

fn popup_at_cursor(menu: HMENU, host: HWND) {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Show above the status bar so the menu stays on screen.
        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, Some(0), host, None);
    }
}

impl App {
    fn command(&mut self, id: usize) {
        if self.ws.is_whiteboard() {
            if !wb_allows(id) {
                self.flash = Some("Not available on a whiteboard".into());
                return self.refresh();
            }
            // Saving needs the page's latest scene first (its change events are debounced).
            if (id == ID_SAVE || id == ID_SAVEAS) && self.wb_ready {
                self.wb_pending = Some(if id == ID_SAVE { WbPending::Save } else { WbPending::SaveAs });
                return wb::request_data();
            }
        }
        let App { ws, ui, .. } = self;
        let sci = ws.sci;
        match id {
            ID_NEW_WB => {
                ws.new_whiteboard();
            }
            ID_NEW => {
                ws.new_tab();
            }
            ID_OPEN => ws.open_dialog(ui),
            ID_SAVE => {
                ws.save(ui);
            }
            ID_SAVEAS => {
                ws.save_as(ui);
            }
            ID_SAVEALL => ws.save_all(ui),
            ID_CLOSE => {
                let a = ws.active;
                ws.close(ui, a);
            }
            ID_EXIT => unsafe {
                let _ = PostMessageW(Some(ui.host), WM_CLOSE, WPARAM(0), LPARAM(0));
            },
            ID_UNDO => drop(sci.send(sci::SCI_UNDO, 0, 0)),
            ID_REDO => drop(sci.send(sci::SCI_REDO, 0, 0)),
            ID_CUT => drop(sci.send(sci::SCI_CUT, 0, 0)),
            ID_COPY => drop(sci.send(sci::SCI_COPY, 0, 0)),
            ID_PASTE => drop(sci.send(sci::SCI_PASTE, 0, 0)),
            ID_SELALL => drop(sci.send(sci::SCI_SELECTALL, 0, 0)),
            ID_EOL_CRLF => ws.set_eol(Eol::Crlf),
            ID_EOL_LF => ws.set_eol(Eol::Lf),
            ID_EOL_CR => ws.set_eol(Eol::Cr),
            ID_WRAP => {
                let on = !ws.wrap;
                ws.set_wrap(on);
            }
            ID_THEME => {
                let next = if ws.theme.dark { Theme::light() } else { Theme::dark() };
                ws.set_theme(next);
            }
            ID_MINIMAP => {
                ws.minimap = !ws.minimap;
                MINI_ON.with(|c| c.set(ws.minimap));
                layout();
            }
            ID_PREVIEW => {
                let on = !PREVIEW_ON.with(|c| c.get());
                self.set_preview(on);
            }
            ID_ZOOMIN => drop(sci.send(sci::SCI_ZOOMIN, 0, 0)),
            ID_ZOOMOUT => drop(sci.send(sci::SCI_ZOOMOUT, 0, 0)),
            ID_ZOOMRESET => drop(sci.send(sci::SCI_SETZOOM, 0, 0)),
            ID_L_DUP => drop(sci.send(sci::SCI_LINEDUPLICATE, 0, 0)),
            ID_L_DEL => drop(sci.send(sci::SCI_LINEDELETE, 0, 0)),
            ID_L_UP => drop(sci.send(sci::SCI_MOVESELECTEDLINESUP, 0, 0)),
            ID_L_DOWN => drop(sci.send(sci::SCI_MOVESELECTEDLINESDOWN, 0, 0)),
            ID_C_UPPER => drop(sci.send(sci::SCI_UPPERCASE, 0, 0)),
            ID_C_LOWER => drop(sci.send(sci::SCI_LOWERCASE, 0, 0)),
            ID_L_SORT_ASC => self.flash = ws.line_op(LineOp::SortAsc),
            ID_L_SORT_DESC => self.flash = ws.line_op(LineOp::SortDesc),
            ID_L_DEDUP => self.flash = ws.line_op(LineOp::RemoveDuplicates),
            ID_L_NOEMPTY => self.flash = ws.line_op(LineOp::RemoveEmpty),
            ID_L_TRIM => self.flash = ws.line_op(LineOp::TrimTrailing),
            ID_COMMENT => self.flash = ws.line_op(LineOp::ToggleComment),
            ID_BM_TOGGLE => ws.toggle_bookmark(),
            ID_BM_NEXT | ID_BM_PREV => {
                if !ws.goto_bookmark(id == ID_BM_NEXT) {
                    self.flash = Some("No bookmarks".into());
                }
            }
            ID_BM_CLEAR => ws.clear_bookmarks(),
            ID_RECENT_CLEAR => ws.recent.clear(),
            _ if (ID_RECENT_BASE..ID_RECENT_BASE + 100).contains(&id) => {
                if let Some(p) = ws.recent.get(id - ID_RECENT_BASE).cloned() {
                    if p.exists() {
                        ws.open_path(ui, &p);
                    } else {
                        ws.prune_recent();
                        self.flash = Some(format!("{} no longer exists", p.display()));
                    }
                }
            }
            ID_NEXT_TAB => ws.next_tab(1),
            ID_PREV_TAB => ws.next_tab(-1),
            ID_ABOUT => {
                let build = if cfg!(debug_assertions) { "debug" } else { "release" };
                let t = wide(&format!(
                    "Note++ (native)\n\
                     Version {} ({build} build)\n\n\
                     A developer-focused text editor in Rust, built on the Scintilla editing component \
                     and Lexilla lexers. It is the native counterpart of the Note++ Electron app (v1.9.0).\n\n\
                     Author: Yogesh Prajapati\n\
                     https://github.com/YogeshPraj/Note-\n\n\
                     Components:\n\
                     \u{2022} Scintilla 5.5.2 and Lexilla 5.5.4 \u{2014} Neil Hodgson (HPND license)\n\
                     \u{2022} fancy-regex \u{2014} regular-expression search\n\
                     \u{2022} resvg \u{2014} toolbar icon rendering\n\
                     \u{2022} windows-rs \u{2014} Win32 bindings",
                    env!("CARGO_PKG_VERSION")
                ));
                unsafe {
                    MessageBoxW(Some(ui.host), PCWSTR(t.as_ptr()), w!("About Note++"), MB_OK | MB_ICONINFORMATION);
                }
            }
            // The panel and dialogs take focus themselves, so skip the refocus below.
            ID_PALETTE => {
                palette::open(ui.host, Kind::Commands, command_items());
                return;
            }
            ID_QUICKOPEN => {
                // Open tabs, then recent files, then everything under the project root.
                let mut files: Vec<PathBuf> = Vec::new();
                let push = |p: &PathBuf, files: &mut Vec<PathBuf>| {
                    if !files.contains(p) {
                        files.push(p.clone());
                    }
                };
                for t in &ws.tabs {
                    if let Some(p) = &t.path {
                        push(p, &mut files);
                    }
                }
                for p in &ws.recent {
                    push(p, &mut files);
                }
                let start = ws.tabs[ws.active].path.as_ref().and_then(|p| p.parent().map(|d| d.to_path_buf()));
                let root = start.as_ref().map(|d| files::project_root(d));
                if let Some(root) = &root {
                    for p in files::scan_files(root, 8000) {
                        push(&p, &mut files);
                    }
                }
                let items: Vec<Item> = files
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let dir = p.parent().map(|d| root.as_ref().and_then(|r| d.strip_prefix(r).ok()).unwrap_or(d));
                        Item {
                            label: p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                            hint: dir.map(|d| d.display().to_string()).unwrap_or_default(),
                            id: i,
                        }
                    })
                    .collect();
                self.quick_files = files;
                palette::open(ui.host, Kind::Files, items);
                return;
            }
            // The panel and dialogs take focus themselves, so skip the refocus below.
            ID_FIND | ID_REPLACE | ID_FIF | ID_MARK => {
                let mode = match id {
                    ID_FIND => Mode::Find,
                    ID_REPLACE => Mode::Replace,
                    ID_FIF => Mode::InFiles,
                    _ => Mode::Mark,
                };
                let dir = ws
                    .tabs[ws.active]
                    .path
                    .as_ref()
                    .and_then(|p| p.parent())
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                findpanel::open(mode, &search::selected_text(&sci), &ws.search.find, &dir);
                self.refresh();
                return;
            }
            ID_GOTO => {
                findpanel::open_goto(ui.host, sci::line_count(&sci));
                return;
            }
            ID_FINDNEXT | ID_FINDPREV => {
                if ws.search.find.is_empty() {
                    findpanel::open(Mode::Find, &search::selected_text(&sci), "", "");
                    self.refresh();
                    return;
                }
                let o = ws.search.clone();
                search::find(&sci, &o, id == ID_FINDNEXT);
            }
            _ if (ID_ENC_BASE..ID_ENC_BASE + ENCODINGS.len()).contains(&id) => ws.set_encoding(ENCODINGS[id - ID_ENC_BASE]),
            _ if (ID_LANG_BASE..ID_LANG_BASE + LANGUAGES.len()).contains(&id) => ws.set_language(LANGUAGES[id - ID_LANG_BASE].1),
            _ => {}
        }
        self.refresh();
        self.focus_active();
    }

    /// Give keyboard focus to whichever surface the active tab uses.
    fn focus_active(&self) {
        if self.ws.is_whiteboard() && wb::is_created() {
            wb::focus();
        } else {
            unsafe {
                let _ = SetFocus(Some(self.ws.sci.0));
            }
        }
    }

    /// Events from the Excalidraw page: scene changes, readiness, save requests.
    fn wb_events(&mut self) {
        for ev in wb::take_events() {
            match ev {
                wb::Event::Ready => {
                    self.wb_ready = true;
                    self.wb_shown = None;
                    self.wb_dark = !self.ws.theme.dark; // force a theme push
                }
                wb::Event::State(s) => self.wb_apply_scene(s),
                wb::Event::Data(s) => {
                    self.wb_apply_scene(s);
                    self.wb_finish_pending();
                }
                wb::Event::SaveRequest => {
                    self.wb_pending = Some(WbPending::Save);
                    wb::request_data();
                }
            }
        }
        self.refresh();
    }

    /// Store a scene reported by the page into the active whiteboard tab.
    fn wb_apply_scene(&mut self, s: String) {
        let current = self.ws.is_whiteboard().then(|| self.ws.tabs[self.ws.active].id);
        // Ignore echoes for a scene that is no longer the one on screen.
        if current.is_some() && current == self.wb_shown {
            let t = &mut self.ws.tabs[self.ws.active];
            if t.content != s {
                // Pan/zoom and Excalidraw's own re-serialisation are not edits.
                let edited = !whiteboard::same_drawing(&t.content, &s);
                t.content = s;
                if edited {
                    t.dirty = true;
                }
            }
        }
    }

    /// Finish a Save / Save As that was waiting for the latest scene.
    fn wb_finish_pending(&mut self) {
        let Some(p) = self.wb_pending.take() else { return };
        let App { ws, ui, .. } = self;
        if !ws.is_whiteboard() {
            return;
        }
        match p {
            WbPending::Save => ws.save(ui),
            WbPending::SaveAs => ws.save_as(ui),
        };
        self.refresh();
    }

    /// Actions posted by the Find panel and the Go To dialog.
    fn panel_command(&mut self, id: usize, arg: isize) {
        let sci = self.ws.sci;
        let mode = findpanel::mode();
        let opts = findpanel::read_opts();
        if matches!(id, findpanel::P_NEXT | findpanel::P_PREV | findpanel::P_REPLACE | findpanel::P_REPLACE_ALL | findpanel::P_COUNT | findpanel::P_FINDALL | findpanel::P_FIF | findpanel::P_MARK) {
            self.ws.search = opts.clone();
        }
        let found = |f: search::Found| match f {
            search::Found::At { wrapped: true } => "Found (wrapped around the document)".to_string(),
            search::Found::At { wrapped: false } => String::new(),
            search::Found::NotFound => "Not found".to_string(),
            search::Found::BadPattern(e) => format!("Invalid regular expression: {e}"),
        };
        let outcome = |r: Result<usize, String>, what: &str| match r {
            Ok(n) => format!("{n} {what}"),
            Err(e) => format!("Invalid regular expression: {e}"),
        };
        let mut note: Option<String> = None;
        match id {
            findpanel::P_NEXT => match mode {
                Mode::InFiles => return self.panel_command(findpanel::P_FIF, 0),
                Mode::Mark => return self.panel_command(findpanel::P_MARK, 0),
                _ => note = Some(found(search::find(&sci, &opts, true))),
            },
            findpanel::P_PREV => note = Some(found(search::find(&sci, &opts, false))),
            findpanel::P_REPLACE => {
                let (replaced, f) = search::replace_current(&sci, &opts);
                let n = found(f);
                note = Some(if replaced && n.is_empty() { "Replaced".into() } else { n });
            }
            findpanel::P_REPLACE_ALL => note = Some(outcome(search::replace_all(&sci, &opts), "replaced")),
            findpanel::P_COUNT => note = Some(outcome(search::count(&sci, &opts), "matches")),
            findpanel::P_MARK => note = Some(outcome(search::mark_all(&sci, &opts, search::INDIC_FIND), "matches marked")),
            findpanel::P_CLEAR => {
                search::clear_marks(&sci, search::INDIC_FIND);
                note = Some("Marks cleared".into());
            }
            findpanel::P_FINDALL => match search::find_all(&sci, &opts) {
                Ok(hits) => {
                    let lines: Vec<String> = hits.iter().map(|h| format!("Line {}:  {}", h.line, h.text)).collect();
                    note = Some(format!("{} hits in the current document", hits.len()));
                    self.hits = hits;
                    findpanel::set_results(&lines);
                }
                Err(e) => note = Some(format!("Invalid regular expression: {e}")),
            },
            findpanel::P_FIF => {
                let (dir, filters) = findpanel::read_dir_filter();
                if dir.as_os_str().is_empty() || !dir.is_dir() {
                    note = Some("Choose an existing directory first".into());
                } else {
                    match search::find_in_files(&dir, &filters, &opts) {
                        Ok(hits) => {
                            let files: std::collections::HashSet<_> = hits.iter().filter_map(|h| h.path.as_ref()).collect();
                            note = Some(format!("{} hits in {} files{}", hits.len(), files.len(), if hits.len() >= search::MAX_HITS { " (limit reached)" } else { "" }));
                            let lines: Vec<String> = hits
                                .iter()
                                .map(|h| {
                                    let p = h.path.as_ref().unwrap();
                                    let rel = p.strip_prefix(&dir).unwrap_or(p);
                                    format!("{} ({}):  {}", rel.display(), h.line, h.text)
                                })
                                .collect();
                            self.hits = hits;
                            self.hits_root = dir;
                            findpanel::set_results(&lines);
                        }
                        Err(e) => note = Some(format!("Invalid regular expression: {e}")),
                    }
                }
            }
            findpanel::P_BROWSE => {
                if let Some(d) = findpanel::browse_folder(self.ui.host) {
                    findpanel::set_dir(&d.display().to_string());
                }
            }
            findpanel::P_LIST_OPEN => {
                if let Some(h) = self.hits.get(arg as usize).cloned() {
                    let App { ws, ui, .. } = self;
                    if let Some(p) = &h.path {
                        ws.open_path(ui, p);
                    }
                    let a = ws.active;
                    if ws.is_dirty(a) && h.path.is_some() {
                        search::goto_line(&sci, h.line);
                    } else {
                        search::select_range(&sci, h.start, h.end);
                    }
                    unsafe {
                        let _ = SetFocus(Some(sci.0));
                    }
                }
            }
            findpanel::P_GOTO => {
                search::goto_line(&sci, arg.max(1) as usize);
                unsafe {
                    let _ = SetFocus(Some(sci.0));
                }
            }
            findpanel::P_CLOSE => {
                findpanel::close();
                layout();
                unsafe {
                    let _ = SetFocus(Some(sci.0));
                }
            }
            findpanel::P_RELAYOUT => layout(),
            _ => {}
        }
        if let Some(n) = note {
            findpanel::set_status(&n);
        }
        self.refresh();
    }

    fn status_click(&mut self, seg: isize) {
        let menu = match seg {
            chrome::SEG_ENC => self.menus.enc,
            chrome::SEG_EOL => self.menus.eol,
            chrome::SEG_LANG => self.menus.lang,
            _ => return,
        };
        popup_at_cursor(menu, self.ui.host);
    }

    /// Show the whiteboard view while a whiteboard tab is active, and push the
    /// tab's scene (and the theme) into it when needed.
    fn wb_sync(&mut self, wb_active: bool) {
        let Some(h) = handles() else { return };
        if wb_active != WB_ON.with(|c| c.get()) {
            if wb_active {
                if let Err(e) = wb::ensure(h.host, &config_dir().join("webview-wb")) {
                    self.flash = Some(format!("Whiteboard unavailable: {e}"));
                }
            }
            WB_ON.with(|c| c.set(wb_active));
            wb::set_visible(wb_active);
            layout();
            if wb_active {
                self.wb_shown = None;
            }
        }
        if wb_active && self.wb_ready {
            let dark = self.ws.theme.dark;
            if self.wb_dark != dark {
                self.wb_dark = dark;
                wb::set_theme(dark);
            }
            let t = &self.ws.tabs[self.ws.active];
            if self.wb_shown != Some(t.id) {
                self.wb_shown = Some(t.id);
                wb::load(&t.content);
                if wb::is_created() {
                    wb::focus();
                }
            }
        }
    }

    /// Bring chrome, title, menu check marks and minimap in line with the workspace.
    fn refresh(&mut self) {
        let Some(h) = handles() else { return };
        let wb_active = self.ws.is_whiteboard();
        if !wb_active {
            self.ws.update_smart();
            self.ws.sync_minimap();
        }
        self.wb_sync(wb_active);
        chrome::set_tabs(&h.chrome, self.ws.tab_infos(), self.ws.active);
        let s = self.ws.status();
        let position = self.flash.take().unwrap_or(s.position);
        chrome::set_status(
            &h.chrome,
            StatusText { position, lines: s.lines, length: s.length, enc: s.enc, eol: s.eol, lang: s.lang, ins: "INS".into() },
        );
        if self.ws.recent != self.recent_shown {
            self.rebuild_recent_menu();
        }
        chrome::set_checked(&h.chrome, ID_WRAP, self.ws.wrap);
        chrome::set_checked(&h.chrome, ID_THEME, self.ws.theme.dark);
        chrome::set_theme(&h.chrome, self.ws.theme.dark, self.ws.theme.bg, self.ws.theme.fg);

        // Mermaid files open their live preview automatically; the preview follows the theme.
        let doc = self.ws.tabs[self.ws.active].doc;
        if doc != self.preview_doc {
            self.preview_doc = doc;
            if !PREVIEW_ON.with(|c| c.get()) && self.ws.preview_kind() == Some(pcore::Kind::Mermaid) {
                self.set_preview(true);
            }
        }
        let preview_on = PREVIEW_ON.with(|c| c.get());
        if preview_on {
            if self.preview_dark != self.ws.theme.dark {
                self.preview_dark = self.ws.theme.dark;
                pview::set_theme(self.preview_dark);
                self.preview_key = None;
            }
            // Debounce: re-render shortly after the last change.
            unsafe {
                SetTimer(Some(h.host), TIMER_PREVIEW, 350, None);
            }
        }
        chrome::set_checked(&h.chrome, ID_PREVIEW, preview_on);

        let title = wide(&self.ws.window_title());
        unsafe {
            let _ = SetWindowTextW(h.host, PCWSTR(title.as_ptr()));
            let on = |b: bool| if b { MF_CHECKED } else { MF_UNCHECKED };
            let m = self.menus;
            CheckMenuItem(m.view, ID_WRAP as u32, (MF_BYCOMMAND | on(self.ws.wrap)).0);
            CheckMenuItem(m.view, ID_MINIMAP as u32, (MF_BYCOMMAND | on(self.ws.minimap)).0);
            CheckMenuItem(m.view, ID_THEME as u32, (MF_BYCOMMAND | on(self.ws.theme.dark)).0);
            CheckMenuItem(m.view, ID_PREVIEW as u32, (MF_BYCOMMAND | on(preview_on)).0);

            let t = &self.ws.tabs[self.ws.active];
            let enc = ENCODINGS.iter().position(|e| *e == t.enc).unwrap_or(0);
            let _ = CheckMenuRadioItem(m.enc, ID_ENC_BASE as u32, (ID_ENC_BASE + ENCODINGS.len() - 1) as u32, (ID_ENC_BASE + enc) as u32, MF_BYCOMMAND.0);
            let lang = LANGUAGES.iter().position(|l| l.1 == t.lang).unwrap_or(0);
            let _ = CheckMenuRadioItem(m.lang, ID_LANG_BASE as u32, (ID_LANG_BASE + LANGUAGES.len() - 1) as u32, (ID_LANG_BASE + lang) as u32, MF_BYCOMMAND.0);
            let eol = match t.eol {
                Eol::Crlf => ID_EOL_CRLF,
                Eol::Lf => ID_EOL_LF,
                Eol::Cr => ID_EOL_CR,
            };
            let _ = CheckMenuRadioItem(m.eol, ID_EOL_CRLF as u32, ID_EOL_CR as u32, eol as u32, MF_BYCOMMAND.0);
        }
    }

    /// Show or hide the live-preview pane.
    fn set_preview(&mut self, on: bool) {
        let Some(h) = handles() else { return };
        if on {
            if self.ws.preview_kind().is_none() {
                self.flash = Some("Preview works for Markdown, HTML and Mermaid files".into());
                return;
            }
            if let Err(e) = pview::ensure(h.host, &config_dir().join("webview")) {
                self.flash = Some(format!("Preview unavailable (WebView2): {e}"));
                return;
            }
            pview::set_theme(self.ws.theme.dark);
            self.preview_dark = self.ws.theme.dark;
        }
        PREVIEW_ON.with(|c| c.set(on));
        if !on {
            PREVIEW_MAX.with(|c| c.set(false));
        }
        pview::set_visible(on);
        self.preview_key = None;
        layout();
        if on {
            pview::set_max(false);
            self.preview_update(true);
        }
    }

    /// Re-render the preview if the active document changed (or `force`).
    fn preview_update(&mut self, force: bool) {
        if !PREVIEW_ON.with(|c| c.get()) {
            return;
        }
        let key = self.ws.preview_key();
        if !force && self.preview_key == Some(key) {
            return;
        }
        self.preview_key = Some(key);
        let Some(kind) = self.ws.preview_kind() else {
            return pview::render("message", "Preview is available for Markdown, HTML and Mermaid files.");
        };
        let text = sci::get_text(&self.ws.sci);
        match kind {
            pcore::Kind::Markdown if pcore::is_bare_mermaid(&text) => pview::render("mermaid", &text),
            pcore::Kind::Markdown => pview::render("markdown", &pcore::markdown_to_html(&text)),
            pcore::Kind::Html => pview::render("html", &text),
            pcore::Kind::Mermaid => pview::render("mermaid", &text),
        }
    }

    fn preview_message(&mut self, code: usize) {
        match code {
            pview::PV_CLOSE => self.set_preview(false),
            pview::PV_MAX => {
                let on = !PREVIEW_MAX.with(|c| c.get());
                PREVIEW_MAX.with(|c| c.set(on));
                pview::set_max(on);
                layout();
            }
            pview::PV_NEED_MERMAID => pview::load_mermaid(),
            pview::PV_REFRESH => self.preview_update(true),
            _ => {}
        }
    }

    fn rebuild_recent_menu(&mut self) {
        let m = self.menus.recent;
        unsafe {
            while GetMenuItemCount(Some(m)) > 0 {
                let _ = DeleteMenu(m, 0, MF_BYPOSITION);
            }
        }
        if self.ws.recent.is_empty() {
            let t = wide("(empty)");
            unsafe {
                let _ = AppendMenuW(m, MF_STRING | MF_GRAYED, 0, PCWSTR(t.as_ptr()));
            }
        } else {
            for (i, p) in self.ws.recent.iter().enumerate() {
                MenuBuilder::add(m, ID_RECENT_BASE + i, &format!("&{}  {}", i + 1, p.display().to_string().replace('&', "&&")));
            }
            MenuBuilder::sep(m);
            MenuBuilder::add(m, ID_RECENT_CLEAR, "Clear Recent Files List");
        }
        self.recent_shown = self.ws.recent.clone();
    }

    fn save_session(&mut self) {
        let s = self.ws.snapshot();
        let _ = s.save(&session_path());
    }
}

// ---- layout ------------------------------------------------------------------

fn layout() {
    let Some(h) = handles() else { return };
    unsafe {
        let mut rc = RECT::default();
        let _ = GetClientRect(h.host, &mut rc);
        let (w, total_h) = (rc.right, rc.bottom);
        let mv = |win: HWND, x: i32, y: i32, ww: i32, hh: i32| {
            let _ = MoveWindow(win, x, y, ww.max(0), hh.max(0), true);
        };
        let c = h.chrome;
        mv(c.menu, 0, 0, w, chrome::MENU_H);
        mv(c.tools, 0, chrome::MENU_H, w, chrome::TOOL_H);
        let tabs_y = chrome::MENU_H + chrome::TOOL_H;
        mv(c.tabs, 0, tabs_y, w, chrome::TAB_H);
        mv(c.status, 0, total_h - chrome::STATUS_H, w, chrome::STATUS_H);

        let top = tabs_y + chrome::TAB_H;
        let panel_h = findpanel::height();
        let panel_y = total_h - chrome::STATUS_H - panel_h;
        mv(h.panel, 0, panel_y, w, panel_h);
        findpanel::arrange(w);

        let area_h = panel_y - top;
        let mini_on = MINI_ON.with(|c| c.get());
        let preview_on = PREVIEW_ON.with(|c| c.get());
        let maximised = preview_on && PREVIEW_MAX.with(|c| c.get());
        let show = |win: HWND, on: bool| {
            let _ = ShowWindow(win, if on { SW_SHOW } else { SW_HIDE });
        };

        let wb_on = WB_ON.with(|c| c.get());

        if wb_on {
            // A whiteboard tab: the Excalidraw view replaces the editor row.
            show(h.edit, false);
            show(h.mini, false);
            show(h.split, false);
            pview::set_visible(false);
            wb::set_bounds(0, top, w, area_h);
        } else if maximised {
            pview::set_visible(true);
            // The preview takes the whole editor row.
            show(h.edit, false);
            show(h.mini, false);
            show(h.split, false);
            pview::set_bounds(0, top, w, area_h);
        } else {
            // Editor (with its minimap) on the left, optional splitter + preview on the right.
            let preview_w = if preview_on {
                ((w as f32) * PREVIEW_RATIO.with(|c| c.get())) as i32
            } else {
                0
            }
            .clamp(0, (w - 320).max(0));
            let preview_w = if preview_on { preview_w.max(240.min(w / 2)) } else { 0 };
            let split_w = if preview_on { SPLIT_W } else { 0 };
            let editor_w = w - preview_w - split_w;
            let mini_w = if mini_on && editor_w > 520 { MINI_W } else { 0 };

            show(h.edit, true);
            pview::set_visible(preview_on);
            mv(h.edit, 0, top, editor_w - mini_w, area_h);
            mv(h.mini, editor_w - mini_w, top, mini_w, area_h);
            show(h.mini, mini_w > 0);
            show(h.split, preview_on);
            if preview_on {
                mv(h.split, editor_w, top, split_w, area_h);
                pview::set_bounds(editor_w + split_w, top, preview_w, area_h);
            }
        }
    }
    with_app(|a| a.ws.sync_minimap());
}

/// The draggable bar between editor and preview: reports the cursor's x in
/// host client coordinates while dragging.
unsafe extern "system" fn split_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SETCURSOR => {
            unsafe { SetCursor(LoadCursorW(None, IDC_SIZEWE).ok()) };
            LRESULT(1)
        }
        WM_LBUTTONDOWN => {
            unsafe { SetCapture(hwnd) };
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = unsafe { ReleaseCapture() };
            LRESULT(0)
        }
        WM_MOUSEMOVE if unsafe { GetCapture() } == hwnd => {
            if let Some(h) = handles() {
                let mut pt = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut pt);
                    let _ = ScreenToClient(h.host, &mut pt);
                    let _ = PostMessageW(Some(h.host), WM_SPLIT, WPARAM(0), LPARAM(pt.x as isize));
                }
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

/// Minimap subclass: clicks and drags scroll the main view; the minimap itself
/// is never focused or edited.
unsafe extern "system" fn mini_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let prev: WNDPROC = unsafe { std::mem::transmute(MINI_PREV.load(Ordering::Relaxed)) };
    match msg {
        WM_LBUTTONDOWN | WM_MOUSEMOVE if msg == WM_LBUTTONDOWN || w.0 & 0x0001 != 0 => {
            let (x, y) = ((l.0 & 0xFFFF) as i16 as usize, ((l.0 >> 16) & 0xFFFF) as i16 as isize);
            let pos = unsafe { SendMessageW(hwnd, sci::SCI_POSITIONFROMPOINT, Some(WPARAM(x)), Some(LPARAM(y))).0 };
            let line = unsafe { SendMessageW(hwnd, sci::SCI_LINEFROMPOSITION, Some(WPARAM(pos.max(0) as usize)), None).0 };
            if let Some(h) = handles() {
                unsafe {
                    let _ = PostMessageW(Some(h.host), WM_MINIMAP, WPARAM(0), LPARAM(line));
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP | WM_LBUTTONDBLCLK | WM_RBUTTONDOWN | WM_RBUTTONUP | WM_CHAR | WM_KEYDOWN | WM_KEYUP | WM_SETFOCUS | WM_CONTEXTMENU => LRESULT(0),
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_SETCURSOR => {
            unsafe { SetCursor(LoadCursorW(None, IDC_HAND).ok()) };
            LRESULT(1)
        }
        WM_MOUSEWHEEL => {
            if let Some(h) = handles() {
                return unsafe { SendMessageW(h.edit, msg, Some(w), Some(l)) };
            }
            LRESULT(0)
        }
        _ => unsafe { CallWindowProcW(prev, hwnd, msg, w, l) },
    }
}

/// Leading part of Scintilla's `SCNotification` (64-bit layout), enough to read
/// the position and margin of a margin click.
#[repr(C)]
struct SciNotify {
    hwnd_from: HWND,
    id_from: usize,
    code: u32,
    position: isize,
    ch: i32,
    modifiers: i32,
    modification_type: i32,
    text: *const u8,
    length: isize,
    lines_added: isize,
    message: i32,
    wparam: usize,
    lparam: isize,
    line: isize,
    fold_level_now: i32,
    fold_level_prev: i32,
    margin: i32,
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            layout();
            LRESULT(0)
        }
        WM_GETMINMAXINFO => {
            let mmi = unsafe { &mut *(l.0 as *mut MINMAXINFO) };
            mmi.ptMinTrackSize = POINT { x: 760, y: 460 };
            LRESULT(0)
        }
        WM_SETFOCUS => {
            if let Some(h) = handles() {
                if WB_ON.with(|c| c.get()) && wb::is_created() {
                    wb::focus();
                } else {
                    unsafe {
                        let _ = SetFocus(Some(h.edit));
                    }
                }
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = w.0 & 0xFFFF;
            with_app(|a| a.command(id));
            LRESULT(0)
        }
        chrome::WM_BAR => {
            let arg = l.0;
            with_app(|a| {
                match w.0 {
                    chrome::BAR_TAB_SELECT => a.ws.activate(arg as usize),
                    chrome::BAR_TAB_CLOSE => {
                        let App { ws, ui, .. } = a;
                        ws.close(ui, arg as usize);
                    }
                    chrome::BAR_TAB_NEW => {
                        a.ws.new_tab();
                    }
                    chrome::BAR_STATUS => a.status_click(arg),
                    _ => {}
                }
                a.refresh();
                unsafe {
                    let _ = SetFocus(Some(a.ws.sci.0));
                }
            });
            LRESULT(0)
        }
        findpanel::WM_PANEL => {
            let (id, arg) = (w.0, l.0);
            with_app(|a| a.panel_command(id, arg));
            LRESULT(0)
        }
        palette::WM_PALETTE => {
            let (kind, id) = (w.0, l.0 as usize);
            with_app(|a| {
                if kind == Kind::Files as usize {
                    if let Some(p) = a.quick_files.get(id).cloned() {
                        let App { ws, ui, .. } = a;
                        ws.open_path(ui, &p);
                    }
                    a.refresh();
                    unsafe {
                        let _ = SetFocus(Some(a.ws.sci.0));
                    }
                } else {
                    a.command(id);
                }
            });
            LRESULT(0)
        }
        WM_MINIMAP => {
            with_app(|a| {
                a.ws.scroll_to_line(l.0.max(0) as usize);
            });
            LRESULT(0)
        }
        WM_TIMER if w.0 == TIMER_PREVIEW => {
            unsafe {
                let _ = KillTimer(Some(hwnd), TIMER_PREVIEW);
            }
            with_app(|a| a.preview_update(false));
            LRESULT(0)
        }
        WM_SPLIT => {
            let mut rc = RECT::default();
            unsafe {
                let _ = GetClientRect(hwnd, &mut rc);
            }
            if rc.right > 0 {
                let ratio = ((rc.right as f32 - l.0 as f32) / rc.right as f32).clamp(0.2, 0.8);
                PREVIEW_RATIO.with(|c| c.set(ratio));
                layout();
            }
            LRESULT(0)
        }
        pview::WM_PREVIEW => {
            let code = w.0;
            with_app(|a| a.preview_message(code));
            LRESULT(0)
        }
        wb::WM_WB => {
            with_app(|a| a.wb_events());
            LRESULT(0)
        }
        WM_NOTIFY => {
            let hdr = unsafe { &*(l.0 as *const windows::Win32::UI::Controls::NMHDR) };
            if let Some(h) = handles() {
                if hdr.hwndFrom == h.edit && hdr.code == sci::SCN_MARGINCLICK {
                    let n = unsafe { &*(l.0 as *const SciNotify) };
                    if n.margin == 0 {
                        let pos = n.position.max(0) as usize;
                        with_app(|a| a.ws.toggle_bookmark_at(pos));
                    }
                }
                if hdr.hwndFrom == h.edit && matches!(hdr.code, sci::SCN_SAVEPOINTLEFT | sci::SCN_SAVEPOINTREACHED | sci::SCN_UPDATEUI) {
                    // Deferred: this arrives inside a SendMessage the app itself made.
                    unsafe {
                        let _ = PostMessageW(Some(hwnd), WM_REFRESH, WPARAM(0), LPARAM(0));
                    }
                }
            }
            LRESULT(0)
        }
        WM_REFRESH => {
            with_app(|a| a.refresh());
            LRESULT(0)
        }
        WM_CLOSE => {
            with_app(|a| a.save_session());
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, w, l) },
    }
}

// ---- menus, toolbar, accelerators ------------------------------------------------

struct MenuBuilder;

impl MenuBuilder {
    fn add(m: HMENU, id: usize, text: &str) {
        let t = wide(text);
        unsafe {
            let _ = AppendMenuW(m, MF_STRING, id, PCWSTR(t.as_ptr()));
        }
    }
    fn sep(m: HMENU) {
        unsafe {
            let _ = AppendMenuW(m, MF_SEPARATOR, 0, PCWSTR::null());
        }
    }
    fn popup() -> HMENU {
        unsafe { CreatePopupMenu().unwrap() }
    }
    fn submenu(parent: HMENU, label: &str, child: HMENU) {
        let t = wide(label);
        unsafe {
            let _ = AppendMenuW(parent, MF_POPUP, child.0 as usize, PCWSTR(t.as_ptr()));
        }
    }
}

fn eol_menu() -> HMENU {
    let m = MenuBuilder::popup();
    MenuBuilder::add(m, ID_EOL_CRLF, "Windows (CR LF)");
    MenuBuilder::add(m, ID_EOL_LF, "Unix (LF)");
    MenuBuilder::add(m, ID_EOL_CR, "Macintosh (CR)");
    m
}

fn build_menus() -> (Vec<(String, HMENU)>, Menus) {
    use MenuBuilder as B;
    let file = B::popup();
    B::add(file, ID_NEW, "&New\tCtrl+N");
    B::add(file, ID_NEW_WB, "New &Whiteboard");
    B::add(file, ID_OPEN, "&Open...\tCtrl+O");
    let recent = B::popup();
    B::submenu(file, "Recent &Files", recent);
    B::sep(file);
    B::add(file, ID_SAVE, "&Save\tCtrl+S");
    B::add(file, ID_SAVEAS, "Save &As...\tCtrl+Shift+S");
    B::add(file, ID_SAVEALL, "Save A&ll\tCtrl+Alt+S");
    B::sep(file);
    B::add(file, ID_CLOSE, "&Close\tCtrl+W");
    B::add(file, ID_EXIT, "E&xit\tAlt+F4");

    let edit = B::popup();
    B::add(edit, ID_UNDO, "&Undo\tCtrl+Z");
    B::add(edit, ID_REDO, "&Redo\tCtrl+Y");
    B::sep(edit);
    B::add(edit, ID_CUT, "Cu&t\tCtrl+X");
    B::add(edit, ID_COPY, "&Copy\tCtrl+C");
    B::add(edit, ID_PASTE, "&Paste\tCtrl+V");
    B::add(edit, ID_SELALL, "Select &All\tCtrl+A");
    B::sep(edit);
    let lines = B::popup();
    B::add(lines, ID_L_DUP, "&Duplicate Line\tCtrl+D");
    B::add(lines, ID_L_DEL, "D&elete Line\tCtrl+L");
    B::add(lines, ID_L_UP, "Move Line &Up\tCtrl+Shift+Up");
    B::add(lines, ID_L_DOWN, "Move Line D&own\tCtrl+Shift+Down");
    B::sep(lines);
    B::add(lines, ID_L_SORT_ASC, "Sort Lines &Ascending");
    B::add(lines, ID_L_SORT_DESC, "Sort Lines D&escending");
    B::add(lines, ID_L_DEDUP, "Remove Duplicate &Lines");
    B::add(lines, ID_L_NOEMPTY, "Remove &Empty Lines");
    B::add(lines, ID_L_TRIM, "&Trim Trailing Space");
    B::submenu(edit, "&Line Operations", lines);
    let case = B::popup();
    B::add(case, ID_C_UPPER, "&UPPERCASE\tCtrl+Shift+U");
    B::add(case, ID_C_LOWER, "&lowercase\tCtrl+U");
    B::submenu(edit, "Convert &Case To", case);
    B::add(edit, ID_COMMENT, "Toggle Line C&omment\tCtrl+/");
    B::sep(edit);
    B::submenu(edit, "EOL Conversion", eol_menu());

    let search = B::popup();
    B::add(search, ID_FIND, "&Find...\tCtrl+F");
    B::add(search, ID_FINDNEXT, "Find &Next\tF3");
    B::add(search, ID_FINDPREV, "Find &Previous\tShift+F3");
    B::add(search, ID_REPLACE, "&Replace...\tCtrl+H");
    B::add(search, ID_FIF, "Find in &Files...\tCtrl+Shift+F");
    B::add(search, ID_MARK, "&Mark...\tCtrl+M");
    B::sep(search);
    B::add(search, ID_GOTO, "&Go to...\tCtrl+G");
    let bm = B::popup();
    B::add(bm, ID_BM_TOGGLE, "&Toggle Bookmark\tCtrl+F2");
    B::add(bm, ID_BM_NEXT, "&Next Bookmark\tF2");
    B::add(bm, ID_BM_PREV, "&Previous Bookmark\tShift+F2");
    B::add(bm, ID_BM_CLEAR, "&Clear All Bookmarks");
    B::submenu(search, "&Bookmark", bm);

    let view = B::popup();
    B::add(view, ID_WRAP, "&Word wrap");
    B::add(view, ID_PREVIEW, "&Live Preview\tCtrl+Shift+V");
    B::add(view, ID_MINIMAP, "&Minimap");
    B::add(view, ID_THEME, "&Dark mode");
    B::sep(view);
    B::add(view, ID_PALETTE, "&Command Palette...\tCtrl+Shift+P");
    B::add(view, ID_QUICKOPEN, "&Quick Open...\tCtrl+P");
    B::sep(view);
    B::add(view, ID_ZOOMIN, "Zoom &in\tCtrl+Num +");
    B::add(view, ID_ZOOMOUT, "Zoom &out\tCtrl+Num -");
    B::add(view, ID_ZOOMRESET, "&Restore default zoom");
    B::sep(view);
    B::add(view, ID_NEXT_TAB, "&Next tab\tCtrl+Tab");
    B::add(view, ID_PREV_TAB, "&Previous tab\tCtrl+Shift+Tab");

    let enc = B::popup();
    for (i, e) in ENCODINGS.iter().enumerate() {
        B::add(enc, ID_ENC_BASE + i, e.label());
    }

    let lang = B::popup();
    for (i, (label, _)) in LANGUAGES.iter().enumerate() {
        B::add(lang, ID_LANG_BASE + i, label);
    }

    let help = B::popup();
    B::add(help, ID_ABOUT, "&About Note++");

    let bar = vec![
        ("File".to_string(), file),
        ("Edit".to_string(), edit),
        ("Search".to_string(), search),
        ("View".to_string(), view),
        ("Encoding".to_string(), enc),
        ("Language".to_string(), lang),
        ("?".to_string(), help),
    ];
    (bar, Menus { view, enc, eol: eol_menu(), lang, recent })
}

fn toolbar_items() -> Vec<ToolItem> {
    macro_rules! btn {
        ($cmd:expr, $file:literal, $tip:literal) => {
            ToolItem::Btn { cmd: $cmd, svg: include_bytes!(concat!("../../assets/", $file)), tip: $tip }
        };
    }
    vec![
        btn!(ID_NEW, "new.svg", "New (Ctrl+N)"),
        btn!(ID_NEW_WB, "whiteboard.svg", "New Whiteboard"),
        btn!(ID_OPEN, "open.svg", "Open (Ctrl+O)"),
        btn!(ID_SAVE, "save.svg", "Save (Ctrl+S)"),
        btn!(ID_SAVEALL, "saveall.svg", "Save All (Ctrl+Alt+S)"),
        btn!(ID_CLOSE, "close.svg", "Close (Ctrl+W)"),
        ToolItem::Sep,
        btn!(ID_CUT, "cut.svg", "Cut (Ctrl+X)"),
        btn!(ID_COPY, "copy.svg", "Copy (Ctrl+C)"),
        btn!(ID_PASTE, "paste.svg", "Paste (Ctrl+V)"),
        ToolItem::Sep,
        btn!(ID_UNDO, "undo.svg", "Undo (Ctrl+Z)"),
        btn!(ID_REDO, "redo.svg", "Redo (Ctrl+Y)"),
        ToolItem::Sep,
        btn!(ID_FIND, "find.svg", "Find (Ctrl+F)"),
        btn!(ID_REPLACE, "replace.svg", "Replace (Ctrl+H)"),
        ToolItem::Sep,
        btn!(ID_ZOOMIN, "zoomin.svg", "Zoom In"),
        btn!(ID_ZOOMOUT, "zoomout.svg", "Zoom Out"),
        ToolItem::Sep,
        btn!(ID_WRAP, "wordwrap.svg", "Word Wrap"),
        btn!(ID_PREVIEW, "preview.svg", "Live Preview (Ctrl+Shift+V)"),
        btn!(ID_THEME, "darkmode.svg", "Dark Mode"),
    ]
}

fn accelerators() -> HACCEL {
    // FVIRTKEY 0x01, FSHIFT 0x04, FCONTROL 0x08, FALT 0x10
    const NONE: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x01);
    const SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x05);
    const CTRL: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x09);
    const CTRL_SHIFT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x0D);
    const CTRL_ALT: ACCEL_VIRT_FLAGS = ACCEL_VIRT_FLAGS(0x19);
    let k = |f, key: char, cmd: usize| ACCEL { fVirt: f, key: key as u16, cmd: cmd as u16 };
    let table = [
        k(CTRL, 'N', ID_NEW),
        k(CTRL, 'O', ID_OPEN),
        k(CTRL, 'S', ID_SAVE),
        k(CTRL_SHIFT, 'S', ID_SAVEAS),
        k(CTRL_ALT, 'S', ID_SAVEALL),
        k(CTRL, 'W', ID_CLOSE),
        k(CTRL, 'F', ID_FIND),
        k(CTRL, 'H', ID_REPLACE),
        k(CTRL, 'G', ID_GOTO),
        k(CTRL, 'M', ID_MARK),
        k(CTRL_SHIFT, 'F', ID_FIF),
        ACCEL { fVirt: NONE, key: 0x72, cmd: ID_FINDNEXT as u16 }, // F3
        ACCEL { fVirt: SHIFT, key: 0x72, cmd: ID_FINDPREV as u16 }, // Shift+F3
        ACCEL { fVirt: NONE, key: 0x71, cmd: ID_BM_NEXT as u16 },   // F2
        ACCEL { fVirt: SHIFT, key: 0x71, cmd: ID_BM_PREV as u16 },  // Shift+F2
        ACCEL { fVirt: CTRL, key: 0x71, cmd: ID_BM_TOGGLE as u16 }, // Ctrl+F2
        k(CTRL, 'D', ID_L_DUP),
        k(CTRL, 'P', ID_QUICKOPEN),
        k(CTRL_SHIFT, 'V', ID_PREVIEW),
        k(CTRL_SHIFT, 'P', ID_PALETTE),
        k(CTRL, 'L', ID_L_DEL),
        k(CTRL, 'U', ID_C_LOWER),
        k(CTRL_SHIFT, 'U', ID_C_UPPER),
        ACCEL { fVirt: CTRL_SHIFT, key: 0x26, cmd: ID_L_UP as u16 },   // Ctrl+Shift+Up
        ACCEL { fVirt: CTRL_SHIFT, key: 0x28, cmd: ID_L_DOWN as u16 }, // Ctrl+Shift+Down
        ACCEL { fVirt: CTRL, key: 0xBF, cmd: ID_COMMENT as u16 },      // Ctrl+/
        ACCEL { fVirt: CTRL, key: 0x09, cmd: ID_NEXT_TAB as u16 },
        ACCEL { fVirt: CTRL_SHIFT, key: 0x09, cmd: ID_PREV_TAB as u16 },
    ];
    unsafe { CreateAcceleratorTableW(&table).unwrap() }
}

// ---- startup -----------------------------------------------------------------------

/// Create the host window, chrome, Scintilla editor + minimap and the Find panel.
pub fn create_window(title: &str, w: i32, h: i32) -> windows::core::Result<(Handles, Menus)> {
    unsafe {
        // Loading the DLL is what registers Scintilla's window class.
        LoadLibraryW(w!("Scintilla.dll"))?;
        let hinstance = GetModuleHandleW(None)?;
        let class = w!("NoteppHost");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        });

        let title_w = wide(title);
        let host = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(title_w.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            w,
            h,
            None,
            None,
            Some(hinstance.into()),
            None,
        )?;

        // chrome::create also sets up the Mica backdrop (title bar, menu, toolbar, tabs).
        let (menu_bar, menus) = build_menus();
        let chrome = chrome::create(host, menu_bar, &toolbar_items());

        let scintilla = |extra: WINDOW_STYLE| {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("Scintilla"),
                PCWSTR::null(),
                WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | extra,
                0,
                0,
                10,
                10,
                Some(host),
                None,
                Some(hinstance.into()),
                None,
            )
        };
        let edit = scintilla(WS_TABSTOP)?;
        let mini = scintilla(WINDOW_STYLE(0))?;
        MINI_PREV.store(SetWindowLongPtrW(mini, GWLP_WNDPROC, mini_proc as *const () as isize), Ordering::Relaxed);

        findpanel::create(host);
        let panel = findpanel::hwnd().unwrap();

        // Splitter between editor and preview (hidden until the preview opens).
        let split_class = w!("NoteppSplit");
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(split_proc),
            hInstance: hinstance.into(),
            lpszClassName: split_class,
            hCursor: LoadCursorW(None, IDC_SIZEWE)?,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(17 as *mut core::ffi::c_void), // COLOR_BTNSHADOW + 1
            ..Default::default()
        });
        let split = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            split_class,
            PCWSTR::null(),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            SPLIT_W,
            10,
            Some(host),
            None,
            Some(hinstance.into()),
            None,
        )?;

        let hs = Handles { host, edit, mini, split, chrome, panel };
        HANDLES.with(|c| c.set(Some(hs)));
        Ok((hs, menus))
    }
}

/// Build the workspace, restore the last session, open `files`, and run the message loop.
pub fn run(hs: Handles, menus: Menus, files: Vec<PathBuf>) {
    let ui = Win32Ui { host: hs.host };
    let mut ws = Workspace::new(Win32Sci(hs.edit));
    ws.attach_minimap(Win32Sci(hs.mini));
    if let Some(s) = Session::load(&session_path()) {
        ws.restore(&s);
    }
    for f in &files {
        ws.open_path(&ui, f);
    }
    if ws.tabs.is_empty() {
        ws.new_tab();
    }
    MINI_ON.with(|c| c.set(ws.minimap));
    APP.with(|c| {
        *c.borrow_mut() = Some(App { ws, ui, menus, hits: Vec::new(), hits_root: PathBuf::new(), flash: None, recent_shown: vec![PathBuf::from("\u{0}")], quick_files: Vec::new(), preview_key: None, preview_doc: 0, preview_dark: false, wb_ready: false, wb_shown: None, wb_dark: false, wb_pending: None })
    });

    unsafe {
        layout();
        with_app(|a| a.refresh());
        let _ = ShowWindow(hs.host, SW_SHOW);
        let _ = SetFocus(Some(hs.edit));

        let accel = accelerators();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            if TranslateAcceleratorW(hs.host, accel, &msg) != 0 {
                continue;
            }
            // Tab / Enter / Esc navigation inside the Find panel and Go To dialog.
            let in_dialog = [findpanel::hwnd(), findpanel::goto_hwnd()]
                .into_iter()
                .flatten()
                .any(|d| IsDialogMessageW(d, &msg).as_bool());
            if !in_dialog {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
