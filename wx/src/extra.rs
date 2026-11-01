//! Commands that need little more than the core and the operating system.

use std::path::{Path, PathBuf};

use notepp_core::commands as c;
use notepp_core::edit::{self, ColumnNumbers, NumFormat};
use notepp_core::workspace::{PathText, Ui as _};
use notepp_core::{bookmarks, sci, search, view};
use wxdragon::prelude::*;

use crate::command::CommandHandler;
use crate::app::App;
use crate::dialogs::{self, WinAction};
use crate::form::{ctl, run, Kind, Spec};

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
                c::PALETTE_COMMANDS.iter().find(|(name, _, _)| name.eq_ignore_ascii_case(l)).map(|(n, id, k)| Some((*n, *id, *k)))
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
    c::PALETTE_COMMANDS
        .iter()
        .filter(|(_, id, _)| !native.contains(id) && seen.insert(*id))
        .map(|(name, id, default)| {
            let current = s.shortcuts.get(&id.to_string()).cloned().unwrap_or_else(|| default.to_string());
            dialogs::MapRow { id: *id, name: name.to_string(), default: default.to_string(), current }
        })
        .collect()
}

fn clipboard_set(text: &str) -> bool {
    Clipboard::get().set_text(text)
}

fn clipboard_get() -> Option<String> {
    Clipboard::get().get_text()
}

/// Files that run code when "opened" (programs, scripts, shortcuts, installers).
fn runs_code(path: &Path) -> bool {
    const RUNNABLE: [&str; 28] = [
        "exe", "com", "scr", "pif", "bat", "cmd", "ps1", "psm1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "lnk", "msi", "msp", "reg", "cpl", "jar", "msc", "appx", "msix", "application", "gadget", "url", "dll",
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

// ---- dialogs ---------------------------------------------------------------------------------------------

fn info(app: &App, title: &str, text: &str) {
    MessageDialog::builder(&app.frame, text, title).with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation).build().show_modal();
}

/// The Summary box: file facts and text statistics.
fn summary(app: &App) {
    let ws = &app.ws;
    let counts = view::count_text(&sci::get_text(&ws.sci));
    let t = &ws.tabs[ws.active];
    let mut s = String::new();
    match &t.path {
        Some(p) => {
            s += &format!("Full file path: {}\n", p.display());
            if let Ok(md) = std::fs::metadata(p) {
                s += &format!("File size on disk: {} bytes\n", md.len());
            }
        }
        None => s += &format!("{} (not saved yet)\n", t.title),
    }
    s += &format!(
        "\nCharacters (without line endings): {}\nWords: {}\nLines: {}\nDocument length: {} bytes\nEncoding: {}\n",
        counts.chars,
        counts.words,
        counts.lines,
        counts.bytes,
        t.enc.label()
    );
    let sel = search::selected_text(&ws.sci);
    if !sel.is_empty() {
        let v = view::count_text(&sel);
        s += &format!("\nSelection: {} characters, {} words, {} lines", v.chars, v.words, v.lines);
    }
    info(app, "Summary", &s);
}

/// Ask for the picture used by Insert > Date Time (customized).
fn ask_date_format(app: &App, current: &str) -> Option<String> {
    const FMT: usize = 101;
    let spec = Spec {
        title: "Date Time (customized)",
        size: (420, 140),
        ctls: vec![
            ctl(100, Kind::Label, "Format: yyyy MM dd HH mm ss tt dddd MMMM  ('quoted text' is kept as is)", 14, 12, 392, 34),
            ctl(FMT, Kind::Edit, current, 14, 52, 392, 24),
        ],
        ok: "Insert",
        buttons: None,
        on_change: None,
        on_apply: None,
    };
    let v = run(&app.frame, spec, |_| {})?;
    let f = v.text(FMT).trim().to_string();
    (!f.is_empty()).then_some(f)
}

/// The Column Editor: insert the same text, or a counting sequence, on every selected line.
fn column_editor(app: &mut App) -> Option<String> {
    const TEXT_R: usize = 100;
    const TEXT: usize = 101;
    const NUM_R: usize = 102;
    const INIT: usize = 104;
    const STEP: usize = 106;
    const REPEAT: usize = 108;
    const ZEROS: usize = 109;
    const DEC: usize = 111;
    const HEX: usize = 112;
    const OCT: usize = 113;
    const BIN: usize = 114;
    fn react(f: &crate::form::Form, _id: usize) {
        let text = f.checked(TEXT_R);
        f.enable(TEXT, text);
        for id in [103, INIT, 105, STEP, 107, REPEAT, ZEROS, DEC, HEX, OCT, BIN] {
            f.enable(id, !text);
        }
    }
    let spec = Spec {
        title: "Column / Multi-Selection Editor",
        size: (380, 262),
        ctls: vec![
            ctl(TEXT_R, Kind::Radio(true), "&Text to Insert", 14, 12, 200, 20),
            ctl(TEXT, Kind::Edit, "", 30, 36, 330, 24),
            ctl(NUM_R, Kind::Radio(false), "&Number to Insert", 14, 72, 200, 20),
            ctl(103, Kind::Label, "Initial number:", 30, 98, 110, 20),
            ctl(INIT, Kind::Edit, "1", 150, 95, 100, 24),
            ctl(105, Kind::Label, "Increase by:", 30, 128, 110, 20),
            ctl(STEP, Kind::Edit, "1", 150, 125, 100, 24),
            ctl(107, Kind::Label, "Repeat:", 30, 158, 110, 20),
            ctl(REPEAT, Kind::Edit, "1", 150, 155, 100, 24),
            ctl(ZEROS, Kind::Check, "Leading &zeros", 30, 188, 200, 20),
            ctl(110, Kind::Group, "Format", 268, 86, 92, 122),
            ctl(DEC, Kind::Radio(true), "&Dec", 280, 106, 70, 20),
            ctl(HEX, Kind::Radio(false), "&Hex", 280, 130, 70, 20),
            ctl(OCT, Kind::Radio(false), "&Oct", 280, 154, 70, 20),
            ctl(BIN, Kind::Radio(false), "&Bin", 280, 178, 70, 20),
        ],
        ok: "OK",
        buttons: None,
        on_change: Some(react),
        on_apply: None,
    };
    let v = run(&app.frame, spec, |f| {
        f.set_checked(TEXT_R, true);
        f.set_checked(DEC, true);
        react(f, 0);
    })?;
    if v.checked(TEXT_R) {
        return app.ws.column_edit(Some(v.text(TEXT)), None);
    }
    let int = |id| v.text(id).trim().parse::<i64>().ok();
    let (Some(initial), Some(step), Some(repeat)) = (int(INIT), int(STEP), int(REPEAT)) else {
        return Some("The initial number, step and repeat must be whole numbers".into());
    };
    let format = if v.checked(HEX) {
        NumFormat::Hex
    } else if v.checked(OCT) {
        NumFormat::Oct
    } else if v.checked(BIN) {
        NumFormat::Bin
    } else {
        NumFormat::Dec
    };
    let numbers = ColumnNumbers { initial, step, repeat: repeat.max(1) as usize, format, leading_zeros: v.checked(ZEROS) };
    app.ws.column_edit(None, Some(&numbers))
}

/// Search > Find characters in range: stays open so Find can be pressed repeatedly.
fn find_chars_dialog(app: &App) {
    const NON_ASCII: usize = 100;
    const ASCII: usize = 101;
    const MY_RANGE: usize = 102;
    const FROM: usize = 103;
    const TO: usize = 104;
    const UP: usize = 121;
    const DOWN: usize = 122;
    const WRAP: usize = 123;
    const STATUS: usize = 130;
    fn react(f: &crate::form::Form, _id: usize) {
        let mine = f.checked(MY_RANGE);
        f.enable(FROM, mine);
        f.enable(TO, mine);
    }
    // Decimal, or hex with a 0x prefix.
    fn number(s: &str) -> Option<u32> {
        let s = s.trim();
        match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            Some(h) => u32::from_str_radix(h, 16).ok(),
            None => s.parse().ok(),
        }
    }
    let sci = app.ws.sci;
    let apply = std::rc::Rc::new(move |v: &crate::form::Values, f: &crate::form::Form| {
        let (lo, hi) = if v.checked(NON_ASCII) {
            (0x80, u32::MAX)
        } else if v.checked(ASCII) {
            (0x00, 0x7F)
        } else {
            match (number(v.text(FROM)), number(v.text(TO))) {
                (Some(a), Some(b)) if a <= b => (a, b),
                _ => {
                    f.set_text(STATUS, "Enter a valid range, e.g. 0 to 255");
                    return true;
                }
            }
        };
        let found = search::find_char(&sci, lo, hi, v.checked(DOWN), v.checked(WRAP));
        f.set_text(STATUS, if found { "Found" } else { "No character in that range" });
        true
    });
    let spec = Spec {
        title: "Find characters in range",
        size: (360, 270),
        ctls: vec![
            ctl(110, Kind::Group, "Search mode", 12, 8, 336, 118),
            ctl(NON_ASCII, Kind::Radio(true), "&Non-ASCII characters (128 - 255 and above)", 26, 28, 310, 20),
            ctl(ASCII, Kind::Radio(false), "&ASCII characters (0 - 127)", 26, 52, 310, 20),
            ctl(MY_RANGE, Kind::Radio(false), "&My range:", 26, 80, 90, 20),
            ctl(FROM, Kind::Edit, "0", 120, 78, 60, 22),
            ctl(111, Kind::Label, "to", 188, 81, 16, 18),
            ctl(TO, Kind::Edit, "255", 208, 78, 60, 22),
            ctl(112, Kind::Label, "(decimal, or 0x for hex)", 26, 102, 250, 18),
            ctl(120, Kind::Group, "Direction", 12, 132, 336, 48),
            ctl(UP, Kind::Radio(true), "&Up", 26, 150, 80, 20),
            ctl(DOWN, Kind::Radio(false), "&Down", 120, 150, 80, 20),
            ctl(WRAP, Kind::Check, "&Wrap around", 26, 188, 200, 20),
            ctl(STATUS, Kind::Label, "", 12, 214, 336, 18),
        ],
        ok: "Find",
        buttons: None,
        on_change: Some(react),
        on_apply: Some(apply),
    };
    let _ = run(&app.frame, spec, |f| {
        f.set_checked(NON_ASCII, true);
        f.set_checked(DOWN, true);
        f.set_checked(WRAP, true);
        react(f, 0);
    });
}

// ---- the commands ---------------------------------------------------------------------------------------

impl App {
    /// Commands implemented here; true when `id` was one of them.


    /// Rebuild every menu (shortcut changes show up in their labels).
    pub fn reinstall_menus(&mut self) {
        let overrides = self.ws.settings.shortcuts.iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|id| (id, v.clone()))).collect();
        crate::menus::set_shortcut_overrides(overrides);
        let (bar, set) = crate::menus::build_menus();
        for (i, (title, m)) in bar.iter().enumerate() {
            self.menubar.replace(i, crate::menus::realize(*m), title);
        }
        self.menus = set;
        self.reset_recent_key();
    }

    /// Passwords (Bitwarden): pick an item and insert it at the cursor or copy it for 30 seconds.
    fn passwords(&mut self) {
        use crate::pwdialogs::{run, Outcome};
        use sha2::{Digest, Sha256};
        match run(&self.frame, !self.ws.is_whiteboard()) {
            Outcome::Insert(secret) => {
                self.ws.insert_text(&secret);
                self.flash = Some("Inserted at the cursor (the clipboard was not used)".into());
            }
            Outcome::Copy(secret) => {
                if clipboard_set(&secret) {
                    self.clipboard.secret = Some((Sha256::digest(secret.as_bytes()).to_vec(), std::time::Instant::now()));
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
            let end = sci::length(&ws.sci).max(0) as usize;
            notepp_core::sci::Send::send(&ws.sci, sci::SCI_GOTOPOS, end, 0);
            notepp_core::sci::Send::send(&ws.sci, sci::SCI_SCROLLCARET, 0, 0);
        }
        self.streams.monitoring.insert(id, md.len());
        self.refresh();
    }

    /// Menu ticks for the switches handled here.
    pub fn extra_ticks(&self) {
        self.menubar.check_item(c::ID_UPDATE_AUTO as i32, self.ws.settings.auto_update);
        self.menubar.check_item(c::ID_MD_TOOLBAR as i32, self.ws.settings.md_toolbar);
        let monitored = self.streams.monitoring.contains_key(&self.ws.tabs[self.ws.active].id);
        self.menubar.check_item(c::ID_MONITOR as i32, monitored);
        self.menubar.check_item(c::ID_ON_TOP as i32, crate::platform::current().is_topmost(self.frame.get_handle() as isize));
        self.menubar.check_item(c::ID_DISTRACTION as i32, self.distraction);
        self.menubar.check_item(c::ID_POSTIT as i32, self.postit);
        let side = crate::panels::mode();
        for (id, m) in [(c::ID_CHAR_PANEL, crate::panels::Mode::Characters), (c::ID_CLIP_HISTORY, crate::panels::Mode::Clipboard), (c::ID_DOC_LIST, crate::panels::Mode::Documents), (c::ID_FUNC_LIST, crate::panels::Mode::Functions)] {
            self.menubar.check_item(id as i32, side == Some(m));
        }
        self.menubar.check_item(c::ID_WORKSPACE_TOGGLE as i32, self.tree_visible());
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

/// The long tail of editor commands: clipboard, dates, folders, dialogs, encryption, passwords.
pub struct ExtraCommands;

impl CommandHandler<App> for ExtraCommands {
    fn name(&self) -> &'static str {
        "extra"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        let sci = app.ws.sci;
        match id {
            c::ID_OPEN_FOLDER_EXPLORER | c::ID_OPEN_FOLDER_CMD | c::ID_OPEN_FOLDER_WS | c::ID_OPEN_VIEWER => match app.ws.tabs[app.ws.active].path.clone() {
                None => app.flash = Some("Save the file first".into()),
                Some(p) => match id {
                    c::ID_OPEN_FOLDER_EXPLORER => crate::platform::current().reveal_in_file_manager(&p),
                    c::ID_OPEN_FOLDER_CMD => crate::platform::current().open_terminal(p.parent()),
                    c::ID_OPEN_FOLDER_WS => {
                        if let Some(d) = p.parent() {
                            app.open_workspace(d.to_path_buf());
                        }
                    }
                    _ if runs_code(&p) && !app.ui.ask_yes_no(&format!("{} is a program or script. Opening it will run it.\n\nRun it?", p.display())) => {}
                    _ => crate::platform::current().open_default(&p.display().to_string()),
                },
            },
            c::ID_INS_DATE_SHORT | c::ID_INS_DATE_LONG | c::ID_INS_DATE_CUSTOM => {
                let text = if id == c::ID_INS_DATE_CUSTOM {
                    let current = app.ws.date_format.clone();
                    ask_date_format(app, &current).map(|f| {
                        app.ws.date_format = f.clone();
                        edit::format_date_time(&f, &crate::platform::current().local_time())
                    })
                } else {
                    Some(crate::platform::current().date_time_text(id == c::ID_INS_DATE_LONG))
                };
                if let Some(t) = text {
                    app.ws.insert_text(&t);
                }
            }
            c::ID_CP_FULLPATH | c::ID_CP_NAME | c::ID_CP_DIR | c::ID_CP_ALLNAMES | c::ID_CP_ALLPATHS => {
                let kind = match id {
                    c::ID_CP_FULLPATH => PathText::FullPath,
                    c::ID_CP_NAME => PathText::FileName,
                    c::ID_CP_DIR => PathText::Directory,
                    c::ID_CP_ALLNAMES => PathText::AllNames,
                    _ => PathText::AllPaths,
                };
                app.flash = Some(match app.ws.path_text(kind) {
                    Some(t) if clipboard_set(&t) => "Copied to the clipboard".into(),
                    Some(_) => "The clipboard is busy".into(),
                    None => "Save the file first".into(),
                });
            }
            c::ID_SEL_OPEN | c::ID_SEL_FOLDER => {
                let sel = search::selected_text(&sci);
                let tab = &app.ws.tabs[app.ws.active];
                let found = selected_path(&sel, &[tab.path.as_deref().and_then(|p| p.parent()), app.ws.workspace.as_deref()]);
                match found {
                    None => app.flash = Some("No file or folder with that name".into()),
                    Some(p) if id == c::ID_SEL_FOLDER => crate::platform::current().reveal_in_file_manager(&p),
                    Some(p) if p.is_file() => {
                        let App { ws, ui, .. } = app;
                        ws.open_path(ui, &p);
                    }
                    Some(p) => app.open_workspace(p),
                }
            }
            c::ID_SEL_SEARCH => {
                let sel = search::selected_text(&sci);
                if sel.trim().is_empty() {
                    app.flash = Some("Select some text first".into());
                } else {
                    crate::platform::current().open_default(&edit::search_url(app.ws.search_engine, &sel));
                }
            }
            c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV | c::ID_VOL_NEXT | c::ID_VOL_PREV => {
                let forward = matches!(id, c::ID_SEL_FIND_NEXT | c::ID_VOL_NEXT);
                search::select_word_at_caret(&sci);
                let token = search::selected_text(&sci);
                if token.is_empty() || token.contains(['\r', '\n']) {
                    app.flash = Some("Select a word to search for".into());
                } else {
                    let o = search::Opts { find: token, match_case: app.ws.search.match_case, wrap: true, ..Default::default() };
                    // "Volatile" searches leave the Find box and F3 untouched.
                    if matches!(id, c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV) {
                        app.ws.search = o.clone();
                    }
                    if search::find(&sci, &o, forward) == search::Found::NotFound {
                        app.flash = Some("Not found".into());
                    }
                }
            }
            c::ID_BM_CUT | c::ID_BM_COPY => {
                use notepp_core::doc::Eol;
                let eol = match app.ws.tabs[app.ws.active].eol {
                    Eol::Crlf => "\r\n",
                    Eol::Lf => "\n",
                    Eol::Cr => "\r",
                };
                let text = bookmarks::marked_text(&sci, eol);
                if text.is_empty() && bookmarks::lines(&sci).is_empty() {
                    app.flash = Some("No bookmarks".into());
                } else if clipboard_set(&text) {
                    if id == c::ID_BM_CUT {
                        bookmarks::delete_lines(&sci, true);
                    }
                    app.flash = Some("Bookmarked lines copied".into());
                }
            }
            c::ID_BM_PASTE => match clipboard_get() {
                Some(t) => {
                    let t = t.trim_end_matches(['\r', '\n']).to_string();
                    app.flash = Some(format!("{} line(s) replaced", bookmarks::replace_lines(&sci, &t)));
                }
                None => app.flash = Some("Nothing to paste".into()),
            },
            _ if (c::ID_COPY_STYLED_BASE..c::ID_COPY_STYLED_BASE + 6).contains(&id) => {
                let slot = id - c::ID_COPY_STYLED_BASE;
                let slots: Vec<usize> = if slot < 5 { vec![slot] } else { (0..5).collect() };
                let text = search::styled_text(&sci, &slots);
                app.flash = Some(if text.is_empty() {
                    "Nothing is styled".into()
                } else if clipboard_set(&text) {
                    "Styled text copied".into()
                } else {
                    "The clipboard is busy".into()
                });
            }
            c::ID_RTL | c::ID_LTR => {
                crate::platform::current().set_rtl(app.stc.get_handle() as isize, id == c::ID_RTL);
                // Scintilla's own bidirectional mode: 1 = right-to-left, 0 = off.
                notepp_core::sci::Send::send(&app.ws.sci, 2709, (id == c::ID_RTL) as usize, 0);
            }
            c::ID_ON_TOP => {
                let p = crate::platform::current();
                let h = app.frame.get_handle() as isize;
                let on = !p.is_topmost(h);
                p.set_topmost(h, on);
            }
            c::ID_POSTIT | c::ID_DISTRACTION => {
                if id == c::ID_POSTIT {
                    app.postit = !app.postit;
                    app.distraction = false;
                } else {
                    app.distraction = !app.distraction;
                    app.postit = false;
                }
                app.ws.set_margins_visible(!app.distraction);
                // Distraction free mode also goes full screen, as in Notepad++.
                app.frame.show_full_screen(app.distraction);
            }
            c::ID_COLUMN_MODE => {
                let t = "Column Mode selects a rectangle of text.\n\n\u{2022} Hold Alt and drag with the mouse, or\n\u{2022} press Alt+Shift with the arrow keys, or\n\u{2022} press Alt+Shift+B at one corner, move, and press it again.\n\nThen type to edit every line at once, or use Column Editor (Alt+C) to insert text or numbers.";
                info(app, "Column Mode", t);
            }
            c::ID_COLUMN_EDITOR => app.flash = column_editor(app),
            c::ID_SUMMARY => summary(app),
            c::ID_FIND_CHARS => find_chars_dialog(app),
            c::ID_MD_TOOLBAR => {
                let mut s = app.ws.settings.clone();
                s.md_toolbar = !s.md_toolbar;
                crate::settings_ui::apply_settings(&mut app.ws, s);
            }
            c::ID_RESULTS => {
                if crate::findpanel::visible() {
                    crate::findpanel::close();
                } else if app.results.hits.is_empty() {
                    app.flash = Some("No search results yet".into());
                } else {
                    let mode = if app.results.root.as_os_str().is_empty() { crate::findpanel::Mode::Find } else { crate::findpanel::Mode::InFiles };
                    let (last, dir) = (app.ws.search.find.clone(), app.results.root.display().to_string());
                    crate::findpanel::open(mode, "", &last, &dir);
                }
            }
            _ if (c::ID_BROWSER_BASE..c::ID_BROWSER_BASE + crate::tables::BROWSERS.len()).contains(&id) => match app.ws.tabs[app.ws.active].path.clone() {
                Some(p) => crate::platform::current().open_with(crate::tables::BROWSERS[id - c::ID_BROWSER_BASE].1, &p),
                None => app.flash = Some("Save the file first".into()),
            },
            c::ID_MONITOR => {
                let (tab_id, path) = (app.ws.tabs[app.ws.active].id, app.ws.tabs[app.ws.active].path.clone());
                match (path, app.streams.monitoring.contains_key(&tab_id)) {
                    (None, _) => app.flash = Some("Save the file first".into()),
                    (Some(_), true) => {
                        app.streams.monitoring.remove(&tab_id);
                        app.ws.set_read_only(false);
                    }
                    (Some(p), false) => {
                        let len = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                        app.streams.monitoring.insert(tab_id, len);
                        app.ws.set_read_only(true);
                        app.flash = Some("Monitoring: new lines are shown as the file grows".into());
                    }
                }
            }
            c::ID_PW_LOCK => {
                notepp_core::bitwarden::lock();
                app.flash = Some("Passwords locked".into());
            }
            c::ID_PASSWORDS | c::ID_PW_TOOLBAR => app.passwords(),
            c::ID_ENC_SETTINGS => {
                let mut vault = app.ws.vault.clone();
                crate::encdialogs::settings(&app.frame, &mut vault);
                app.ws.vault = vault;
            }
            c::ID_WINDOWS => {
                let mut sel = app.ws.active;
                loop {
                    let rows = app.ws.window_rows();
                    match dialogs::windows_dialog(&app.frame, &rows, sel.min(rows.len().saturating_sub(1))) {
                        None => break,
                        Some(WinAction::Activate(i)) => {
                            app.ws.activate(i);
                            break;
                        }
                        Some(WinAction::Save(rows)) => {
                            let App { ws, ui, .. } = app;
                            for &i in &rows {
                                // Saving goes through the active tab; whiteboards save from their own tab.
                                if i < ws.tabs.len() && ws.tabs[i].kind == notepp_core::workspace::TabKind::Text && ws.is_dirty(i) {
                                    ws.activate(i);
                                    ws.save(ui);
                                }
                            }
                            sel = ws.active;
                        }
                        Some(WinAction::Close(rows)) => {
                            let App { ws, ui, .. } = app;
                            // Highest first so the indices stay valid.
                            for &i in rows.iter().rev() {
                                if i < ws.tabs.len() && !ws.close(ui, i) {
                                    break;
                                }
                            }
                            sel = ws.active;
                        }
                        Some(WinAction::Sort(field, descending)) => {
                            app.ws.sort_tabs(field, descending);
                            sel = app.ws.active;
                        }
                    }
                }
            }
            _ if (c::ID_WINDOW_SORT_BASE..c::ID_WINDOW_SORT_BASE + dialogs::SORTS.len()).contains(&id) => {
                let (_, field, descending) = dialogs::SORTS[id - c::ID_WINDOW_SORT_BASE];
                app.ws.sort_tabs(field, descending);
            }
            c::ID_SHORTCUTS => {
                if let Some(done) = dialogs::shortcut_mapper(&app.frame, mapper_rows(&app.ws.settings)) {
                    let mut s = app.ws.settings.clone();
                    s.shortcuts.clear();
                    for r in done {
                        if r.current != r.default {
                            s.shortcuts.insert(r.id.to_string(), r.current);
                        }
                    }
                    let _ = s.save();
                    app.ws.settings = s;
                    app.reinstall_menus();
                    app.flash = Some("Shortcuts updated".into());
                }
            }
            c::ID_EDIT_CONTEXT => {
                let p = ensure_context_menu_file();
                let current = context_names(&std::fs::read_to_string(&p).unwrap_or_else(|_| DEFAULT_CONTEXT_MENU.into()));
                let all = c::PALETTE_COMMANDS.iter().map(|(n, _, _)| n.to_string()).collect();
                if let Some((menu, as_text)) = dialogs::context_menu_editor(&app.frame, all, current.clone(), context_names(DEFAULT_CONTEXT_MENU)) {
                    if menu != current {
                        let header = DEFAULT_CONTEXT_MENU.lines().take(2).collect::<Vec<_>>().join("\n");
                        let _ = std::fs::write(&p, format!("{header}\n{}\n", menu.join("\n")));
                        app.flash = Some("Right-click menu saved".into());
                    }
                    if as_text {
                        let App { ws, ui, .. } = app;
                        ws.open_path(ui, &p);
                    }
                }
            }
            _ if (c::ID_HASH_BASE..c::ID_HASH_BASE + notepp_core::hash::ALGOS.len() * 10).contains(&id) => {
                let algo = notepp_core::hash::ALGOS[(id - c::ID_HASH_BASE) / 10];
                match (id - c::ID_HASH_BASE) % 10 {
                    0 => dialogs::hash_text(&app.frame, algo),
                    1 => {
                        let files = app.ui.pick_open();
                        if !files.is_empty() {
                            let lines: Vec<String> = files
                                .iter()
                                .map(|p| match std::fs::File::open(p).and_then(|f| algo.hash_reader(f)) {
                                    Ok(h) => format!("{h}  {}", p.display()),
                                    Err(e) => format!("(error)  {}: {e}", p.display()),
                                })
                                .collect();
                            dialogs::text_result(&app.frame, &format!("{} of {} file(s)", algo.name(), files.len()), &lines.join("\r\n"));
                        }
                    }
                    _ => {
                        let sel = search::selected_text(&sci);
                        if sel.is_empty() {
                            app.flash = Some("Select some text first".into());
                        } else {
                            let digest = algo.hash_bytes(sel.as_bytes());
                            app.flash = Some(if clipboard_set(&digest) { format!("{} copied: {digest}", algo.name()) } else { "The clipboard is busy".into() });
                        }
                    }
                }
            }
            _ => return false,
        }
        true
    }
}
