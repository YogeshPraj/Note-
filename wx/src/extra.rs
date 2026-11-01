//! Commands that need little more than the core and the operating system.

use std::path::{Path, PathBuf};

use notepp_core::commands as c;
use notepp_core::edit::{self, ColumnNumbers, NumFormat};
use notepp_core::workspace::{PathText, Ui as _};
use notepp_core::{bookmarks, sci, search, view};
use wxdragon::prelude::*;

use crate::app::App;
use crate::form::{ctl, run, Kind, Spec};

fn clipboard_set(text: &str) -> bool {
    Clipboard::get().set_text(text)
}

fn clipboard_get() -> Option<String> {
    Clipboard::get().get_text()
}

// ---- operating system helpers ---------------------------------------------------------------------

#[cfg(windows)]
fn explorer_select(path: &Path) {
    use std::os::windows::process::CommandExt;
    // Explorer wants `/select,"path"` as one argument with its own quotes.
    let _ = std::process::Command::new("explorer").raw_arg(format!("/select,\"{}\"", path.display())).spawn();
}

#[cfg(target_os = "macos")]
fn explorer_select(path: &Path) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}

#[cfg(all(unix, not(target_os = "macos")))]
fn explorer_select(path: &Path) {
    let _ = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
}

#[cfg(windows)]
fn open_command_prompt(dir: Option<&Path>) {
    use std::os::windows::process::CommandExt;
    let mut cmd = std::process::Command::new("cmd");
    cmd.arg("/K");
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let _ = cmd.creation_flags(0x10).spawn(); // CREATE_NEW_CONSOLE
}

#[cfg(not(windows))]
fn open_command_prompt(dir: Option<&Path>) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-a").arg("Terminal").arg(dir.unwrap_or(Path::new("."))).spawn();
    #[cfg(not(target_os = "macos"))]
    {
        let mut cmd = std::process::Command::new("x-terminal-emulator");
        if let Some(d) = dir {
            cmd.current_dir(d);
        }
        let _ = cmd.spawn();
    }
}

/// Open a file or address with whatever program the system associates with it.
#[cfg(windows)]
fn shell_open(target: &str) {
    // Not `cmd /c start`: that would parse `&` and friends in addresses.
    let _ = std::process::Command::new("rundll32").arg("url.dll,FileProtocolHandler").arg(target).spawn();
}

#[cfg(target_os = "macos")]
fn shell_open(target: &str) {
    let _ = std::process::Command::new("open").arg(target).spawn();
}

#[cfg(all(unix, not(target_os = "macos")))]
fn shell_open(target: &str) {
    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
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

#[cfg(windows)]
mod sys {
    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    pub struct SystemTime {
        pub year: u16,
        pub month: u16,
        pub weekday: u16,
        pub day: u16,
        pub hour: u16,
        pub minute: u16,
        pub second: u16,
        pub ms: u16,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetLocalTime(t: *mut SystemTime);
        pub fn GetDateFormatEx(locale: *const u16, flags: u32, date: *const SystemTime, format: *const u16, out: *mut u16, cch: i32, calendar: *const u16) -> i32;
        pub fn GetTimeFormatEx(locale: *const u16, flags: u32, time: *const SystemTime, format: *const u16, out: *mut u16, cch: i32) -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        pub fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    }
}

#[cfg(windows)]
fn local_time() -> edit::DateTime {
    let mut t = sys::SystemTime::default();
    unsafe { sys::GetLocalTime(&mut t) };
    edit::DateTime { year: t.year as i32, month: t.month as u32, day: t.day as u32, weekday: t.weekday as u32, hour: t.hour as u32, minute: t.minute as u32, second: t.second as u32 }
}

#[cfg(not(windows))]
fn local_time() -> edit::DateTime {
    // No time-zone database here: UTC.
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    edit::DateTime { year, month, day, weekday: ((days + 4).rem_euclid(7)) as u32, hour: (rem / 3600) as u32, minute: (rem % 3600 / 60) as u32, second: (rem % 60) as u32 }
}

/// Time then date, in the user's regional format.
#[cfg(windows)]
fn system_date_time(long: bool) -> String {
    let mut now = sys::SystemTime::default();
    let (mut date, mut time) = ([0u16; 128], [0u16; 64]);
    unsafe {
        sys::GetLocalTime(&mut now);
        let d = sys::GetDateFormatEx(std::ptr::null(), if long { 2 } else { 1 }, &now, std::ptr::null(), date.as_mut_ptr(), date.len() as i32, std::ptr::null());
        let t = sys::GetTimeFormatEx(std::ptr::null(), 0, &now, std::ptr::null(), time.as_mut_ptr(), time.len() as i32);
        let part = |buf: &[u16], n: i32| String::from_utf16_lossy(&buf[..(n.max(1) as usize - 1).min(buf.len())]);
        format!("{} {}", part(&time, t), part(&date, d))
    }
}

#[cfg(not(windows))]
fn system_date_time(long: bool) -> String {
    edit::format_date_time(if long { "HH:mm:ss dddd, MMMM dd, yyyy" } else { "HH:mm yyyy-MM-dd" }, &local_time())
}

/// Right-to-left or left-to-right reading order for the editor.
fn set_text_direction(app: &App, rtl: bool) {
    #[cfg(windows)]
    unsafe {
        let hwnd = app.stc.get_handle() as isize;
        let bits: isize = 0x0000_1000 | 0x0000_2000 | 0x0000_4000; // WS_EX_RIGHT | WS_EX_RTLREADING | WS_EX_LEFTSCROLLBAR
        let ex = sys::GetWindowLongPtrW(hwnd, -20);
        sys::SetWindowLongPtrW(hwnd, -20, if rtl { ex | bits } else { ex & !bits });
        sys::SetWindowPos(hwnd, 0, 0, 0, 0, 0, 0x37); // NOMOVE | NOSIZE | NOZORDER | FRAMECHANGED | NOACTIVATE
    }
    // Scintilla's own bidirectional mode: 1 = right-to-left, 0 = off.
    notepp_core::sci::Send::send(&app.ws.sci, 2709, rtl as usize, 0);
}

fn is_topmost(app: &App) -> bool {
    #[cfg(windows)]
    unsafe {
        return sys::GetWindowLongPtrW(app.frame.get_handle() as isize, -20) & 0x8 != 0;
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        false
    }
}

fn set_topmost(app: &App, on: bool) {
    #[cfg(windows)]
    unsafe {
        sys::SetWindowPos(app.frame.get_handle() as isize, if on { -1 } else { -2 }, 0, 0, 0, 0, 0x13); // NOMOVE | NOSIZE | NOACTIVATE
    }
    #[cfg(not(windows))]
    let _ = (app, on);
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

// ---- the commands ---------------------------------------------------------------------------------------

impl App {
    /// Commands implemented here; true when `id` was one of them.
    pub fn extra_command(&mut self, id: usize) -> bool {
        let sci = self.ws.sci;
        match id {
            c::ID_OPEN_FOLDER_EXPLORER | c::ID_OPEN_FOLDER_CMD | c::ID_OPEN_FOLDER_WS | c::ID_OPEN_VIEWER => match self.ws.tabs[self.ws.active].path.clone() {
                None => self.flash = Some("Save the file first".into()),
                Some(p) => match id {
                    c::ID_OPEN_FOLDER_EXPLORER => explorer_select(&p),
                    c::ID_OPEN_FOLDER_CMD => open_command_prompt(p.parent()),
                    c::ID_OPEN_FOLDER_WS => self.flash = Some("The workspace tree is not available in this build yet".into()),
                    _ if runs_code(&p) && !self.ui.ask_yes_no(&format!("{} is a program or script. Opening it will run it.\n\nRun it?", p.display())) => {}
                    _ => shell_open(&p.display().to_string()),
                },
            },
            c::ID_INS_DATE_SHORT | c::ID_INS_DATE_LONG | c::ID_INS_DATE_CUSTOM => {
                let text = if id == c::ID_INS_DATE_CUSTOM {
                    let current = self.ws.date_format.clone();
                    ask_date_format(self, &current).map(|f| {
                        self.ws.date_format = f.clone();
                        edit::format_date_time(&f, &local_time())
                    })
                } else {
                    Some(system_date_time(id == c::ID_INS_DATE_LONG))
                };
                if let Some(t) = text {
                    self.ws.insert_text(&t);
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
                self.flash = Some(match self.ws.path_text(kind) {
                    Some(t) if clipboard_set(&t) => "Copied to the clipboard".into(),
                    Some(_) => "The clipboard is busy".into(),
                    None => "Save the file first".into(),
                });
            }
            c::ID_SEL_OPEN | c::ID_SEL_FOLDER => {
                let sel = search::selected_text(&sci);
                let tab = &self.ws.tabs[self.ws.active];
                let found = selected_path(&sel, &[tab.path.as_deref().and_then(|p| p.parent()), self.ws.workspace.as_deref()]);
                match found {
                    None => self.flash = Some("No file or folder with that name".into()),
                    Some(p) if id == c::ID_SEL_FOLDER => explorer_select(&p),
                    Some(p) if p.is_file() => {
                        let App { ws, ui, .. } = self;
                        ws.open_path(ui, &p);
                    }
                    Some(_) => self.flash = Some("The workspace tree is not available in this build yet".into()),
                }
            }
            c::ID_SEL_SEARCH => {
                let sel = search::selected_text(&sci);
                if sel.trim().is_empty() {
                    self.flash = Some("Select some text first".into());
                } else {
                    shell_open(&edit::search_url(self.ws.search_engine, &sel));
                }
            }
            c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV | c::ID_VOL_NEXT | c::ID_VOL_PREV => {
                let forward = matches!(id, c::ID_SEL_FIND_NEXT | c::ID_VOL_NEXT);
                search::select_word_at_caret(&sci);
                let token = search::selected_text(&sci);
                if token.is_empty() || token.contains(['\r', '\n']) {
                    self.flash = Some("Select a word to search for".into());
                } else {
                    let o = search::Opts { find: token, match_case: self.ws.search.match_case, wrap: true, ..Default::default() };
                    // "Volatile" searches leave the Find box and F3 untouched.
                    if matches!(id, c::ID_SEL_FIND_NEXT | c::ID_SEL_FIND_PREV) {
                        self.ws.search = o.clone();
                    }
                    if search::find(&sci, &o, forward) == search::Found::NotFound {
                        self.flash = Some("Not found".into());
                    }
                }
            }
            c::ID_BM_CUT | c::ID_BM_COPY => {
                use notepp_core::doc::Eol;
                let eol = match self.ws.tabs[self.ws.active].eol {
                    Eol::Crlf => "\r\n",
                    Eol::Lf => "\n",
                    Eol::Cr => "\r",
                };
                let text = bookmarks::marked_text(&sci, eol);
                if text.is_empty() && bookmarks::lines(&sci).is_empty() {
                    self.flash = Some("No bookmarks".into());
                } else if clipboard_set(&text) {
                    if id == c::ID_BM_CUT {
                        bookmarks::delete_lines(&sci, true);
                    }
                    self.flash = Some("Bookmarked lines copied".into());
                }
            }
            c::ID_BM_PASTE => match clipboard_get() {
                Some(t) => {
                    let t = t.trim_end_matches(['\r', '\n']).to_string();
                    self.flash = Some(format!("{} line(s) replaced", bookmarks::replace_lines(&sci, &t)));
                }
                None => self.flash = Some("Nothing to paste".into()),
            },
            _ if (c::ID_COPY_STYLED_BASE..c::ID_COPY_STYLED_BASE + 6).contains(&id) => {
                let slot = id - c::ID_COPY_STYLED_BASE;
                let slots: Vec<usize> = if slot < 5 { vec![slot] } else { (0..5).collect() };
                let text = search::styled_text(&sci, &slots);
                self.flash = Some(if text.is_empty() {
                    "Nothing is styled".into()
                } else if clipboard_set(&text) {
                    "Styled text copied".into()
                } else {
                    "The clipboard is busy".into()
                });
            }
            c::ID_RTL | c::ID_LTR => set_text_direction(self, id == c::ID_RTL),
            c::ID_ON_TOP => {
                let on = !is_topmost(self);
                set_topmost(self, on);
            }
            c::ID_POSTIT | c::ID_DISTRACTION => {
                if id == c::ID_POSTIT {
                    self.postit = !self.postit;
                    self.distraction = false;
                } else {
                    self.distraction = !self.distraction;
                    self.postit = false;
                }
                self.ws.set_margins_visible(!self.distraction);
                // Distraction free mode also goes full screen, as in Notepad++.
                self.frame.show_full_screen(self.distraction);
            }
            c::ID_COLUMN_MODE => {
                let t = "Column Mode selects a rectangle of text.\n\n\u{2022} Hold Alt and drag with the mouse, or\n\u{2022} press Alt+Shift with the arrow keys, or\n\u{2022} press Alt+Shift+B at one corner, move, and press it again.\n\nThen type to edit every line at once, or use Column Editor (Alt+C) to insert text or numbers.";
                info(self, "Column Mode", t);
            }
            c::ID_COLUMN_EDITOR => self.flash = column_editor(self),
            c::ID_SUMMARY => summary(self),
            _ => return false,
        }
        true
    }

    /// Menu ticks for the switches handled here.
    pub fn extra_ticks(&self) {
        self.menubar.check_item(c::ID_ON_TOP as i32, is_topmost(self));
        self.menubar.check_item(c::ID_DISTRACTION as i32, self.distraction);
        self.menubar.check_item(c::ID_POSTIT as i32, self.postit);
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
