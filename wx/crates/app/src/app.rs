//! The application: window layout, event wiring and the command dispatcher.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::command::AppError;
use crate::widgets::web_pane::WebPane;
use notepp_core::commands::{
    ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_FIF, ID_FIND, ID_FINDNEXT, ID_FINDPREV, ID_GOTO, ID_INCR, ID_MARK, ID_MINIMAP, ID_NEXT_RESULT, ID_PALETTE,
    ID_PREV_RESULT, ID_PREVIEW, ID_QUICKOPEN, ID_REPLACE, ID_SAVE, ID_SAVE_COPY, ID_SAVEAS, ID_THEME, ID_WRAP,
};
use notepp_core::preview as pcore;
use notepp_core::sci::{self};
use notepp_core::session::{self, Session};
use notepp_core::settings::Settings;
use notepp_core::workspace::Workspace;
use wxdragon::event::WebViewEvents;
use wxdragon::prelude::*;

use crate::chrome::{Act, Chrome, StatusText};
use crate::command::CommandBus;
use crate::findpanel::{self, Cmd, Mode};
use crate::menus::{self, Menus};
use crate::panels::{self, Panels};
use crate::popups;
use crate::preview;
use crate::sci::WxSci;
use crate::state::{PreviewState, SearchResults, StreamState, WhiteboardState};
use crate::ui::WxUi;
use crate::whiteboard::{self as wbm, Pending};
use notepp_core::schedule::Schedule;

pub struct App {
    pub(crate) results: SearchResults,
    /// The project folder's files for Quick Open, listed in the background.
    pub(crate) file_index: notepp_core::files::FileIndex,
    pub(crate) clipboard: crate::services::clipboard::ClipboardService,
    pub(crate) streams: StreamState,
    pub(crate) wb: WhiteboardState,
    pub(crate) preview: PreviewState,
    bus: Rc<CommandBus<App>>,
    pub ws: Workspace<WxSci>,
    pub ui: WxUi,
    pub flash: Option<String>,
    pub frame: Frame,
    splitter: SplitterWindow,
    pub(crate) editor_pane: Panel,
    wbv: WebPane,
    pub(crate) panels: Panels,
    pub(crate) side: crate::side::Side,
    pub(crate) side_key: Option<(isize, isize)>,
    pub(crate) mdbar: crate::widgets::dock::Dock,
    pub(crate) upd: crate::updates::State,
    pub(crate) upd_plan: crate::updates::Plan,
    /// The Incremental Search window, once it has been opened.
    pub(crate) incr: Option<crate::incr::Search>,
    menu_bar: isize,
    mini: StyledTextCtrl,
    /// The main editor view and the docked one; `stc()` says which the user is in.
    main_stc: StyledTextCtrl,
    dock_stc: StyledTextCtrl,
    /// Holds both views; the second appears when a tab is docked.
    dock_split: SplitterWindow,
    dock_shown: Option<notepp_core::session::DockSide>,
    pub(crate) view: WebPane,
    pub(crate) chrome: Chrome,
    pub(crate) menubar: MenuBar,
    pub(crate) postit: bool,
    /// A menu took the keyboard from a web page (see `take_focus_for_menu`): it goes back when the menu is done.
    pub(crate) menu_took_focus: std::cell::Cell<bool>,
    pub(crate) distraction: bool,
    pub(crate) chrome_vis: [bool; 4],
    pub(crate) menus: Menus,
    /// The menus of the menu bar, in order (their contents are current: the menu arena is rebuilt in place).
    pub(crate) bar_menus: Vec<menus::M>,
    /// What the menus and toolbar were last enabled for (see [`crate::command::App::sync_command_state`]).
    pub(crate) cmd_state: Option<notepp_core::registry::CommandState>,
    /// Defaults plus the user's overrides; what menus, tool-tips and the palette show.
    pub keymap: notepp_core::keymap::KeyMap,
    session_key: ((usize, usize, usize, u64), u64),
    recent_key: (Vec<PathBuf>, bool),
    window_key: (Vec<String>, usize),
}

pub type Shared = Rc<RefCell<App>>;

/// The Tab key's code (`WXK_TAB`).
const KEY_TAB: i32 = 9;

/// How much of another tab's text its tool tip shows: lines, and characters per line.
const TIP_LINES: usize = 12;
const TIP_WIDTH: usize = 90;

mod start;
pub use start::*;
mod timers;
mod wiring;
pub(crate) use wiring::*;
/// How many times the editor's text has changed. Bumped from the change notification, which also fires for edits made
/// by commands while the app is borrowed, so it lives outside `App`.
static EDITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

mod dock_pane;
pub(crate) use dock_pane::DockCommands;
mod notes;
pub(crate) use notes::*;
/// With `NPP_TRACE`=<file>, append "<id> <how it was handled>" for every command (used by the test sweep).
pub(crate) fn trace(id: usize, how: &str) {
    if let Some(p) = std::env::var_os("NPP_TRACE")
        && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p)
    {
        let _ = std::io::Write::write_all(&mut f, format!("{id} {how}\n").as_bytes());
    }
}

impl App {
    /// Run command `id`: the one entry point for menus, the toolbar, shortcuts and the palette.
    pub fn command(&mut self, id: usize) {
        match crate::command::execute(self, id) {
            Ok(()) | Err(AppError::Disabled(_)) => {}
            Err(e) => self.flash = Some(e.to_string()),
        }
    }

    /// The dispatch behind [`crate::command::execute`]: core commands first, then the registered handlers.
    pub(crate) fn dispatch_command(&mut self, id: usize) -> Result<(), AppError> {
        // A whiteboard is saved from the page's latest scene, so ask for it first.
        if matches!(id, ID_SAVE | ID_SAVEAS | ID_SAVE_COPY) && self.ws.is_whiteboard() && self.wbv.is_ready() {
            self.wb.pending = Some(match id {
                ID_SAVE => Pending::Save,
                ID_SAVEAS => Pending::SaveAs,
                _ => Pending::SaveCopy,
            });
            wbm::request_data(&self.wbv);
            return Ok(());
        }
        if notepp_core::dispatch::command_core(&mut self.ws, &self.ui, &mut self.flash, id) {
            trace(id, "core");
            self.refresh();
            return Ok(());
        }
        if matches!(id, ID_NEXT_RESULT | ID_PREV_RESULT) {
            self.step_result(id == ID_NEXT_RESULT);
            return Ok(());
        }
        let bus = self.bus.clone();
        let handled = bus.dispatch(self, id).is_some();
        if !handled {
            trace(id, "unported");
        }
        self.refresh();
        if handled { Ok(()) } else { Err(AppError::NotAvailable) }
    }

    /// Open files handed to this window: from a second launch or dropped onto it. Folders become the workspace.
    pub fn open_from_outside(&mut self, paths: &[PathBuf]) {
        for p in paths {
            if p.is_dir() {
                self.open_workspace(p.clone());
            } else {
                let App { ws, ui, .. } = self;
                ws.open_path(ui, p);
            }
        }
        self.refresh();
        self.stc().set_focus();
        if self.frame.is_iconized() {
            self.frame.iconize(false);
        }
        self.frame.raise();
    }

    /// Ask about open files that another program changed or deleted since they were read or saved.
    pub fn check_files_on_disk(&mut self) {
        let App { ws, ui, .. } = self;
        if ws.check_external(ui) {
            self.whiteboard_reloaded();
            self.refresh();
        }
    }

    /// A whiteboard's scene may have been read from disk again: the page must show it (it only loads a tab's scene when
    /// a different tab comes to the front).
    pub fn whiteboard_reloaded(&mut self) {
        self.wb.shown = None;
    }

    /// Files a second launch left behind.
    pub fn check_inbox(&mut self) {
        let Ok(rd) = std::fs::read_dir(inbox()) else { return };
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut any = false;
        // Only finished hand-offs (a second launch writes `.part` and renames it to `.txt`).
        for e in rd.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "txt")) {
            if let Ok(text) = std::fs::read_to_string(e.path()) {
                any = true;
                paths.extend(text.lines().filter(|l| !l.is_empty()).map(PathBuf::from));
            }
            let _ = std::fs::remove_file(e.path());
        }
        if any {
            self.open_from_outside(&paths);
        }
    }

    pub fn reset_recent_key(&mut self) {
        self.recent_key = (vec![PathBuf::from("\u{0}")], false);
        self.window_key = (vec!["\u{0}".into()], 0);
    }

    /// Bring the tab strip, status bar, title and menu ticks in line with the workspace.
    pub fn refresh(&mut self) {
        self.sync_dock();
        if !self.ws.is_whiteboard() {
            self.ws.update_smart();
            self.ws.auto_detect_language();
            self.ws.sync_current_line();
            self.ws.sync_minimap();
        }
        self.sync_tabs();
        let s = self.ws.status();
        let position = self.flash.take().unwrap_or_else(|| s.position.clone());
        self.chrome.set_status(StatusText {
            position,
            lines: s.lines.clone(),
            length: s.length.clone(),
            enc: s.enc.clone(),
            eol: s.eol.clone(),
            lang: s.lang.clone(),
            overtype: self.ws.overtype(),
        });
        let th = &self.ws.theme;
        let base = if th.dark { crate::widgets::theme::Theme::dark() } else { crate::widgets::theme::Theme::light() };
        crate::widgets::theme::set(base.with_editor(th.bg, th.fg));
        self.chrome.set_checked(ID_WRAP, self.ws.wrap);
        self.chrome.set_checked(ID_THEME, self.ws.theme.dark);
        self.chrome.set_checked(ID_PREVIEW, self.preview.on);
        self.frame.set_title(&self.ws.window_title());
        self.menubar.check_item(crate::platform::wx::to_i32(ID_WRAP), self.ws.wrap);
        self.menubar.check_item(crate::platform::wx::to_i32(ID_THEME), self.ws.theme.dark);
        self.menubar.check_item(crate::platform::wx::to_i32(ID_MINIMAP), self.ws.minimap);
        self.wb_sync();
        self.refresh_side();
        self.sync_chrome();
        self.extra_ticks();
        self.menubar.check_item(crate::platform::wx::to_i32(ID_PREVIEW), self.preview.on);
        self.preview_update(false);
        self.sync_preview_scroll();
        {
            use notepp_core::doc::Eol;
            let eol = self.ws.tabs[self.ws.active].eol;
            for (id, e) in [(ID_EOL_CRLF, Eol::Crlf), (ID_EOL_LF, Eol::Lf), (ID_EOL_CR, Eol::Cr)] {
                self.menubar.check_item(crate::platform::wx::to_i32(id), eol == e);
            }
        }
        self.sync_file_menu();
        self.sync_window_menu();
        self.sync_command_state();
    }

    fn sync_tabs(&mut self) {
        let infos = self.ws.tab_infos();
        self.chrome.set_tabs(infos, self.ws.active);
    }

    /// The File menu lists recent files and offers "Restore Recent Closed File": rebuild it when they change.
    fn sync_file_menu(&mut self) {
        let key = (self.ws.recent.clone(), !self.ws.closed.is_empty());
        if key == self.recent_key {
            return;
        }
        menus::fill_file_menu(&self.menus, &key.0, key.1);
        let new = menus::realize(self.menus.file);
        self.menubar.replace(0, new, "&File");
        self.recent_key = key;
    }

    /// The Window menu lists the open documents: rebuild it when the tabs or the active one change.
    fn sync_window_menu(&mut self) {
        let key = (self.ws.tab_infos().into_iter().map(|t| t.title).collect::<Vec<_>>(), self.ws.active);
        if key == self.window_key {
            return;
        }
        menus::fill_window_menu(self.menus.window, &key.0, key.1);
        self.menubar.replace(8, menus::realize(self.menus.window), "&Window");
        self.window_key = key;
    }

    /// Write the session now (the window is closing): this one waits for the write.
    pub fn save_session(&mut self) {
        if !session::standalone() {
            let ticket = session::ticket();
            self.keep_dock_share();
            let _ = self.ws.snapshot().save_in_order(&session::session_path(), ticket);
        }
    }

    fn autosave_session(&mut self) {
        // The tabs and their unsaved text, plus the edit count: an edit that keeps the length (a case change, overtyping,
        // a same-length replacement) must still reach the session file.
        let key = (self.ws.session_fingerprint(), EDITS.load(std::sync::atomic::Ordering::Relaxed));
        if key == self.session_key || session::standalone() {
            return;
        }
        self.session_key = key;
        self.keep_dock_share();
        // The snapshot is taken here; turning it into JSON and writing it (slow with large unsaved documents) happens
        // on a worker thread. The tickets keep a late write from replacing a newer one.
        let (snapshot, ticket) = (self.ws.snapshot(), session::ticket());
        std::thread::spawn(move || {
            let _ = snapshot.save_in_order(&session::session_path(), ticket);
        });
    }
}

/// Default shortcut table for accelerators of items that are not in a menu.
#[allow(dead_code)]
pub fn default_shortcut(id: usize) -> Option<String> {
    notepp_core::registry::default_shortcut(id).map(str::to_string)
}

mod preview_pane;

mod whiteboard_pane;

mod find_panel;
