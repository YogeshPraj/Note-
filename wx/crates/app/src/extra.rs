//! Commands that need little more than the core and the operating system.

use std::path::{Path, PathBuf};

use notepp_core::commands as c;
use notepp_core::edit::{self, ColumnNumbers, NumFormat};
use notepp_core::workspace::{PathText, Ui as _};
use notepp_core::{bookmarks, sci, search, view};
use wxdragon::prelude::*;

use crate::app::App;
use crate::command::CommandHandler;
use crate::dialogs::{self, WinAction};
use crate::widgets::form::{Kind, Spec, ctl, run};

const DEFAULT_CONTEXT_MENU: &str = "# Right-click menu of the editor. One command per line, as named in the Command Palette;\n\
# a line with just - makes a separator. Save the file and the next right-click uses it.\n\
Edit: Cut\nEdit: Copy\nEdit: Paste\nEdit: Delete\nEdit: Select All\n-\n\
Edit: Open Selected File\nEdit: Search Selection on Internet\n-\n\
Edit: Toggle Line Comment\nEdit: Block Comment\nEdit: Block Uncomment\n-\n\
Edit: UPPERCASE\nEdit: lowercase\nEdit: Proper Case\n-\n\
Search: Style All Occurrences of Token (1st)\nSearch: Clear all Styles\n-\n\
View: Fold All\nView: Unfold All\n";

pub fn default_context_menu() -> &'static str {
    DEFAULT_CONTEXT_MENU
}

pub fn context_menu_path() -> PathBuf {
    notepp_core::session::config_dir().join("contextmenu.txt")
}

fn ensure_context_menu_file() -> PathBuf {
    let p = context_menu_path();
    if !p.exists() {
        let _ = std::fs::create_dir_all(notepp_core::session::config_dir());
        let _ = std::fs::write(&p, DEFAULT_CONTEXT_MENU);
    }
    p
}

/// Entries of the context menu file: Some(command) or None for a separator.
pub fn context_entries(text: &str) -> Vec<Option<(&'static str, usize, &'static str)>> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            if l == "-" {
                Some(None)
            } else {
                notepp_core::registry::palette().find(|s| s.palette.eq_ignore_ascii_case(l)).map(|s| Some((s.palette, s.id, s.hint())))
            }
        })
        .collect()
}

fn context_names(text: &str) -> Vec<String> {
    context_entries(text).into_iter().map(|e| e.map_or("-".to_string(), |(n, _, _)| n.to_string())).collect()
}

fn mapper_rows(s: &notepp_core::settings::Settings) -> Vec<dialogs::MapRow> {
    let native = [c::ID_UNDO, c::ID_REDO, c::ID_SELALL, c::ID_CUT, c::ID_COPY, c::ID_PASTE];
    let mut seen = std::collections::HashSet::new();
    notepp_core::registry::palette()
        .filter(|c| !native.contains(&c.id) && seen.insert(c.id))
        .map(|c| {
            let current = s.shortcuts.get(&c.id.to_string()).cloned().unwrap_or_else(|| c.hint().to_string());
            dialogs::MapRow { id: c.id, name: c.palette.to_string(), default: c.hint().to_string(), current }
        })
        .collect()
}

/// Files that run code when "opened" (programs, scripts, shortcuts, installers).
fn runs_code(path: &Path) -> bool {
    const RUNNABLE: [&str; 28] = [
        "exe",
        "com",
        "scr",
        "pif",
        "bat",
        "cmd",
        "ps1",
        "psm1",
        "vbs",
        "vbe",
        "js",
        "jse",
        "wsf",
        "wsh",
        "hta",
        "lnk",
        "msi",
        "msp",
        "reg",
        "cpl",
        "jar",
        "msc",
        "appx",
        "msix",
        "application",
        "gadget",
        "url",
        "dll",
    ];
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| RUNNABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// The file or folder named by selected text (quotes and extra lines ignored), looked up as given and then
/// relative to each of `bases`.
fn selected_path(selection: &str, bases: &[Option<&Path>]) -> Option<PathBuf> {
    let first = selection.lines().next()?.trim().trim_matches(['"', '\'']).trim();
    if first.is_empty() {
        return None;
    }
    let direct = PathBuf::from(first);
    if direct.exists() {
        return Some(direct);
    }
    bases.iter().flatten().map(|b| b.join(first)).find(|p| p.exists())
}

mod small_dialogs;

use small_dialogs::*;

// ---- the commands ---------------------------------------------------------------------------------------

impl App {
    /// Rebuild every menu (shortcut changes show up in their labels).
    pub fn reinstall_menus(&mut self) {
        self.keymap = notepp_core::keymap::KeyMap::build(&self.ws.settings.shortcuts);
        crate::menus::set_keymap(&self.keymap);
        self.chrome.refresh_tips(&self.keymap);
        if let Some(p) = self.keymap.problems().first() {
            self.flash = Some(p.to_string());
        }
        let (bar, set) = crate::menus::build_menus();
        for (i, (title, m)) in bar.iter().enumerate() {
            self.menubar.replace(i, crate::menus::realize(*m), title);
        }
        self.bar_menus = bar.iter().map(|(_, m)| *m).collect();
        self.menus = set;
        self.reset_recent_key();
    }

    /// The shortcuts as edited in the settings window.
    pub fn apply_shortcuts(&mut self, done: Vec<dialogs::MapRow>) {
        let mut s = self.ws.settings.clone();
        s.shortcuts.clear();
        for r in done {
            if r.current != r.default {
                s.shortcuts.insert(r.id.to_string(), r.current);
            }
        }
        let saved = s.save();
        self.ws.settings = s;
        self.reinstall_menus();
        self.flash = Some(match saved {
            Ok(()) => "Shortcuts updated".into(),
            Err(e) => format!("Shortcuts updated, but they could not be saved: {e}"),
        });
    }

    /// The rows of the shortcut mapper.
    pub fn shortcut_rows(&self) -> Vec<dialogs::MapRow> {
        mapper_rows(&self.ws.settings)
    }

    /// The editor's right-click menu: what can be offered, what it holds now and what it holds by default.
    pub fn context_menu_lists() -> (Vec<String>, Vec<String>, Vec<String>) {
        let p = ensure_context_menu_file();
        let current = context_names(&std::fs::read_to_string(&p).unwrap_or_else(|_| DEFAULT_CONTEXT_MENU.into()));
        let all = notepp_core::registry::palette().map(|c| c.palette.to_string()).collect();
        (all, current, context_names(DEFAULT_CONTEXT_MENU))
    }

    /// Save the right-click menu as edited in the settings window.
    pub fn save_context_menu(&mut self, menu: &[String]) {
        let header = DEFAULT_CONTEXT_MENU.lines().take(2).collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(ensure_context_menu_file(), format!("{header}\n{}\n", menu.join("\n")));
        self.flash = Some("Right-click menu saved".into());
    }

    /// Open the right-click menu file as text.
    pub fn open_context_menu_file(&mut self) {
        let p = ensure_context_menu_file();
        let App { ws, ui, .. } = self;
        ws.open_path(ui, &p);
    }
    /// Passwords (Bitwarden): pick an item and insert it at the cursor or copy it for 30 seconds.
    fn passwords(&mut self) {
        use crate::pwdialogs::{Outcome, run};
        match run(&self.frame, !self.ws.is_whiteboard()) {
            Outcome::Insert(secret) => {
                self.ws.insert_text(&secret);
                self.flash = Some("Inserted at the cursor (the clipboard was not used)".into());
            }
            Outcome::Copy(secret) => {
                if self.clipboard.copy_secret(&secret) {
                    self.flash = Some("Copied - the clipboard is cleared in 30 seconds".into());
                }
            }
            Outcome::Closed => {}
        }
    }

    /// Monitoring (tail -f): reload the active file when it has changed on disk.
    pub fn monitor_tick(&mut self) {
        let alive: Vec<u64> = self.ws.tabs.iter().map(|t| t.id).collect();
        self.streams.monitoring.retain(|id, _| alive.contains(id));
        if self.streams.monitoring.is_empty() {
            return;
        }
        let t = &self.ws.tabs[self.ws.active];
        let (id, path) = (t.id, t.path.clone());
        let (Some(last), Some(path)) = (self.streams.monitoring.get(&id).copied(), path) else { return };
        let Ok(md) = std::fs::metadata(&path) else { return };
        if md.len() == last || self.ws.is_dirty(self.ws.active) {
            return;
        }
        let App { ws, ui, .. } = self;
        if ws.reload_from_disk(ui) {
            let end = crate::platform::wx::to_usize(sci::length(&ws.sci).max(0));
            notepp_core::sci::Send::send(&ws.sci, sci::SCI_GOTOPOS, end, 0);
            notepp_core::sci::Send::send(&ws.sci, sci::SCI_SCROLLCARET, 0, 0);
        }
        self.streams.monitoring.insert(id, md.len());
        self.refresh();
    }

    /// Menu ticks for the switches handled here.
    pub fn extra_ticks(&self) {
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_UPDATE_AUTO), self.ws.settings.auto_update);
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_MD_TOOLBAR), self.ws.settings.md_toolbar);
        let monitored = self.streams.monitoring.contains_key(&self.ws.tabs[self.ws.active].id);
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_MONITOR), monitored);
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_ON_TOP), crate::widgets::native::current().is_topmost(self.frame.get_handle() as isize));
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_DISTRACTION), self.distraction);
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_POSTIT), self.postit);
        let side = self.side.mode;
        for (id, m) in [
            (c::ID_CHAR_PANEL, crate::side::Mode::Characters),
            (c::ID_CLIP_HISTORY, crate::side::Mode::Clipboard),
            (c::ID_DOC_LIST, crate::side::Mode::Documents),
            (c::ID_FUNC_LIST, crate::side::Mode::Functions),
        ] {
            self.menubar.check_item(crate::platform::wx::to_i32(id), side == Some(m));
        }
        self.menubar.check_item(crate::platform::wx::to_i32(c::ID_WORKSPACE_TOGGLE), self.tree_visible());
    }

    /// Show or hide the menu bar, toolbar, tab bar and status bar from the settings and the view modes.
    pub fn sync_chrome(&mut self) {
        let s = &self.ws.settings;
        let want = [
            !self.postit,
            !self.postit && !self.distraction && s.show_toolbar,
            !self.postit && !self.distraction && s.show_tabbar,
            !self.postit && !self.distraction && s.show_statusbar,
        ];
        let md = self.ws.is_markdown() && s.md_toolbar && !self.postit && !self.distraction;
        if md != self.mdbar.is_shown() {
            self.mdbar.set_height(if md { crate::mdbar::HEIGHT } else { 0 });
        }
        if want != self.chrome_vis {
            self.chrome_vis = want;
            self.chrome.menu.show(want[0]);
            self.chrome.tools.show(want[1]);
            self.chrome.tabs.show(want[2]);
            self.chrome.status.show(want[3]);
            self.frame.layout();
        }
    }
}

mod handlers;
pub use handlers::*;
