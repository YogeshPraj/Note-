//! The application: window layout, event wiring and the command dispatcher.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use notepp_core::commands::{ID_PREVIEW, ID_IMPORT_THEME, ID_PREFS, ID_PRESET_BASE, ID_STYLE_CFG, ID_FIND, ID_FINDNEXT, ID_FINDPREV, ID_GOTO, ID_REPLACE, ID_ABOUT, ID_EOL_CR, ID_EOL_CRLF, ID_EOL_LF, ID_EXIT, ID_FULLSCREEN, ID_MINIMAP, ID_RELOAD, ID_THEME, ID_WRAP, PALETTE_COMMANDS};
use notepp_core::session::{self, Session};
use notepp_core::preview as pcore;
use notepp_core::sci::{self, Theme};
use notepp_core::settings::{self, Settings};
use notepp_core::shortcuts;
use notepp_core::workspace::Workspace;
use wxdragon::prelude::*;
use wxdragon::event::WebViewEvents;
use wxdragon::widgets::webview::WebView;

use crate::dispatch;
use crate::preview;
use crate::settings_ui;
use crate::tables;
use crate::menus::{self, Menus};
use crate::sci::WxSci;
use crate::ui::WxUi;

pub struct App {
    pub ws: Workspace<WxSci>,
    pub ui: WxUi,
    pub flash: Option<String>,
    pub frame: Frame,
    splitter: SplitterWindow,
    stc: StyledTextCtrl,
    view: WebView,
    preview_on: bool,
    preview_loaded: bool,
    preview_ready: bool,
    preview_dark: bool,
    preview_key: Option<(isize, usize, u64)>,
    tabs: Notebook,
    status: StatusBar,
    menubar: MenuBar,
    menus: Menus,
    /// (title, modified) of the tabs as last shown, to rebuild the strip only when it changed.
    tab_key: Vec<(String, bool, bool)>,
    /// Set while the shell itself changes the tab strip, so its events are ignored.
    syncing: bool,
    session_key: (usize, usize, usize, u64),
    recent_key: (Vec<PathBuf>, bool),
}

pub type Shared = Rc<RefCell<App>>;

fn shortcut_overrides(s: &Settings) -> HashMap<usize, String> {
    s.shortcuts.iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|id| (id, v.clone()))).collect()
}

/// Build the window and run. Files named on the command line open as tabs.
pub fn start(files: Vec<PathBuf>) {
    let settings = Settings::load();
    menus::set_shortcut_overrides(shortcut_overrides(&settings));

    let frame = Frame::builder().with_title("Note++").with_size(Size::new(1200, 800)).build();

    // Menu bar.
    let (bar, menu_set) = menus::build_menus();
    let mut mb = MenuBar::builder();
    for (title, m) in &bar {
        mb = mb.append(menus::realize(*m), title);
    }
    let menubar = mb.build();
    frame.set_menu_bar(menubar);

    // Tab strip (only its tab row is shown) above the editor.
    let tabs = Notebook::builder(&frame).build();
    tabs.set_min_size(Size::new(-1, 34));
    // The editor, with the preview pane beside it when that is switched on.
    let splitter = SplitterWindow::builder(&frame).build();
    let stc = StyledTextCtrl::builder(&splitter).build();
    let view = WebView::builder(&splitter).build();
    view.add_script_message_handler("ipc");
    view.show(false);
    splitter.initialize(&stc);
    splitter.set_minimum_pane_size(120);

    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    sizer.add(&tabs, 0, SizerFlag::Expand, 0);
    sizer.add(&splitter, 1, SizerFlag::Expand, 0);
    frame.set_sizer(sizer, true);

    let status = frame.create_status_bar(6, 0, -1, "status");
    status.set_status_widths(&[-5, -2, -2, -2, -2, 50]);

    let ui = WxUi { frame };
    let sci = WxSci::of(&stc);
    let mut ws = Workspace::new(sci);
    ws.set_settings(settings);
    if !session::standalone() {
        if let Some(s) = Session::load(&session::session_path()) {
            ws.restore(&ui, &s);
        }
    }
    for f in &files {
        ws.open_path(&ui, f);
    }
    if ws.tabs.is_empty() {
        ws.new_tab();
    }

    let app: Shared = Rc::new(RefCell::new(App {
        ws,
        ui,
        flash: None,
        frame,
        splitter,
        stc,
        view,
        preview_on: false,
        preview_loaded: false,
        preview_ready: false,
        preview_dark: false,
        preview_key: None,
        tabs,
        status,
        menubar: frame.get_menu_bar().expect("menu bar"),
        menus: menu_set,
        tab_key: Vec::new(),
        syncing: false,
        session_key: (usize::MAX, 0, 0, 0),
        recent_key: (vec![PathBuf::from("\u{0}")], false),
    }));

    wire(&app, frame, &stc, &tabs, &view);
    app.borrow_mut().refresh();
    frame.show(true);
    frame.centre();
    stc.set_focus();
}

fn wire(app: &Shared, frame: Frame, stc: &StyledTextCtrl, tabs: &Notebook, view: &WebView) {
    // The preview page finished loading: draw the current document in it.
    let a = app.clone();
    view.on_loaded(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.preview_ready = true;
            app.preview_dark = !app.ws.theme.dark;
            app.preview_update(true);
        }
    });
    // Messages from the page (close, refresh, lazy-loading of Mermaid / the JSON editor).
    let a = app.clone();
    let pv = *view;
    view.on_script_message_received(move |e| {
        let msg = e.get_string().unwrap_or_default();
        match msg.trim_matches('"') {
            "need-mermaid" => preview::load_mermaid(&pv),
            "need-jsoneditor" => preview::load_json_editor(&pv),
            "close" => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    app.set_preview(false);
                    app.refresh();
                }
            }
            "refresh" => {
                if let Ok(mut app) = a.try_borrow_mut() {
                    app.preview_update(true);
                }
            }
            _ => {}
        }
    });
    // Menu commands.
    let a = app.clone();
    frame.on_menu(move |e| {
        let id = e.get_id() as usize;
        // Modeless windows need the shared handle, so they are opened here, with nothing borrowed.
        match id {
            ID_FIND | ID_REPLACE => {
                let initial = a.try_borrow().map(|x| notepp_core::search::selected_text(&x.ws.sci)).unwrap_or_default();
                return crate::find::open(&a, id == ID_REPLACE, &initial);
            }
            ID_FINDNEXT | ID_FINDPREV => return crate::find::step(&a, id == ID_FINDNEXT),
            ID_GOTO => return crate::find::goto_line(&a),
            _ => {}
        }
        if let Ok(mut app) = a.try_borrow_mut() {
            app.command(id);
        }
    });

    // Caret moves, edits and selection changes update the status bar.
    let a = app.clone();
    stc.on_stc_update_ui(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.refresh();
        }
    });

    // Choosing a tab.
    let a = app.clone();
    tabs.on_page_changed(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            if !app.syncing {
                if let Some(i) = e.get_selection() {
                    app.ws.activate(i.max(0) as usize);
                    app.refresh();
                }
            }
        }
    });

    // Keep the session file current (a crash or power cut then loses at most a few seconds).
    let autosave = Timer::new(&frame);
    let a = app.clone();
    autosave.on_tick(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.autosave_session();
        }
    });
    autosave.start(10_000, false);
    std::mem::forget(autosave);

    // Closing the window saves the session; unsaved text comes back next time.
    let a = app.clone();
    frame.on_close(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.save_session();
        }
        e.skip(true);
    });
}

impl App {
    pub fn command(&mut self, id: usize) {
        if dispatch::command_core(&mut self.ws, &self.ui, &mut self.flash, id) {
            return self.refresh();
        }
        match id {
            ID_EXIT => self.frame.close(false),
            ID_RELOAD => {
                if self.ws.reload_from_disk(&self.ui) {
                    self.flash = Some("Reloaded from disk".into());
                }
            }
            ID_MINIMAP => self.ws.minimap = !self.ws.minimap,
            ID_PREVIEW => {
                let on = !self.preview_on;
                self.set_preview(on);
            }
            ID_FULLSCREEN => {
                let on = !self.frame.is_full_screen();
                self.frame.show_full_screen(on);
            }
            ID_ABOUT => {
                let text = format!("Note++\nVersion {} (wxWidgets build)\n\nExperience the native power.", notepp_core::update::VERSION);
                MessageDialog::builder(&self.frame, &text, "About Note++").with_style(MessageDialogStyle::OK | MessageDialogStyle::IconInformation).build().show_modal();
            }
            ID_PREFS => {
                if let Some(s) = settings_ui::preferences(&self.frame, &self.ws.settings) {
                    settings_ui::apply_settings(&mut self.ws, s);
                }
            }
            ID_STYLE_CFG => {
                if let Some(e) = settings_ui::style_configurator(&self.frame, &self.ws.theme) {
                    let dark = self.ws.theme.dark;
                    let base = if dark { Theme::dark() } else { Theme::light() };
                    let diff = if e.reset { Default::default() } else { settings::diff_from(&base, &e.entered) };
                    let bad = e.entered.iter().filter(|(_, h)| settings::parse_hex(h).is_none()).count();
                    let mut s = self.ws.settings.clone();
                    if dark {
                        s.dark_colors = diff;
                    } else {
                        s.light_colors = diff;
                    }
                    settings_ui::apply_settings(&mut self.ws, s);
                    if bad > 0 {
                        self.flash = Some(format!("{bad} colour(s) were not valid #RRGGBB and were ignored"));
                    }
                }
            }
            ID_IMPORT_THEME => {
                if let Some(p) = notepp_core::workspace::Ui::pick_open(&self.ui).into_iter().next() {
                    let imported = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| {
                        let mut s = self.ws.settings.clone();
                        tables::import_theme(&mut s, &t).map(|n| (s, n))
                    });
                    match imported {
                        Ok((s, n)) => {
                            settings_ui::apply_settings(&mut self.ws, s);
                            self.flash = Some(format!("Imported {n} colour(s)"));
                        }
                        Err(e) => notepp_core::workspace::Ui::error(&self.ui, &format!("Cannot import the theme\n\n{e}")),
                    }
                }
            }
            _ if (ID_PRESET_BASE..ID_PRESET_BASE + 2 + settings::PRESETS.len()).contains(&id) => {
                let i = id - ID_PRESET_BASE;
                let (dark, preset) = match i {
                    0 => (false, None),
                    1 => (true, None),
                    _ => (settings::PRESETS[i - 2].dark, Some(&settings::PRESETS[i - 2])),
                };
                let mut s = self.ws.settings.clone();
                s.apply_preset(dark, preset);
                settings_ui::apply_settings(&mut self.ws, s);
                self.ws.set_theme(if dark { Theme::dark() } else { Theme::light() });
                self.flash = Some(format!("Theme: {}", preset.map_or(if dark { "Default (dark)" } else { "Default (light)" }, |p| p.name)));
            }
            _ => self.flash = Some("This command is not available in this build yet".into()),
        }
        self.refresh();
    }

    /// Bring the tab strip, status bar, title and menu ticks in line with the workspace.
    pub fn refresh(&mut self) {
        if !self.ws.is_whiteboard() {
            self.ws.update_smart();
            self.ws.sync_minimap();
        }
        self.sync_tabs();
        let s = self.ws.status();
        let position = self.flash.take().unwrap_or_else(|| format!("{}", s.position));
        for (i, text) in [position, format!("{}", s.lines), format!("{}", s.length), format!("{}", s.enc), format!("{}", s.eol), format!("{}", s.lang)].iter().enumerate() {
            self.status.set_status_text(text, i);
        }
        self.frame.set_title(&self.ws.window_title());
        self.menubar.check_item(ID_WRAP as i32, self.ws.wrap);
        self.menubar.check_item(ID_THEME as i32, self.ws.theme.dark);
        self.menubar.check_item(ID_MINIMAP as i32, self.ws.minimap);
        self.menubar.check_item(ID_PREVIEW as i32, self.preview_on);
        self.preview_update(false);
        {
            use notepp_core::doc::Eol;
            let eol = self.ws.tabs[self.ws.active].eol;
            for (id, e) in [(ID_EOL_CRLF, Eol::Crlf), (ID_EOL_LF, Eol::Lf), (ID_EOL_CR, Eol::Cr)] {
                self.menubar.check_item(id as i32, eol == e);
            }
        }
        self.sync_file_menu();
    }


    /// Show or hide the preview pane beside the editor.
    pub fn set_preview(&mut self, on: bool) {
        if on == self.preview_on {
            return;
        }
        self.preview_on = on;
        if on {
            if !self.preview_loaded {
                self.preview_loaded = true;
                self.view.set_page(&preview::page_html(self.ws.settings.preview_remote_images), "about:blank");
            }
            self.view.show(true);
            let half = self.frame.get_size().width / 2;
            self.splitter.split_vertically(&self.stc, &self.view, half);
            self.preview_key = None;
        } else {
            self.splitter.unsplit(Some(&self.view));
            self.view.show(false);
        }
    }

    /// Re-render the preview when the document, its text or the theme changed.
    pub fn preview_update(&mut self, force: bool) {
        if !self.preview_on || !self.preview_ready {
            return;
        }
        let dark = self.ws.theme.dark;
        if self.preview_dark != dark {
            self.preview_dark = dark;
            preview::set_theme(&self.view, dark);
            self.preview_key = None;
        }
        let key = self.ws.preview_key();
        if !force && self.preview_key == Some(key) {
            return;
        }
        self.preview_key = Some(key);
        let view = self.view;
        let Some(kind) = self.ws.preview_kind() else {
            return preview::render(&view, "message", "Preview is available for Markdown, HTML, Mermaid and JSON files.");
        };
        let text = sci::get_text(&self.ws.sci);
        match kind {
            pcore::Kind::Markdown if pcore::is_bare_mermaid(&text) => preview::render(&view, "mermaid", &text),
            pcore::Kind::Markdown => {
                let base = self.ws.tabs[self.ws.active].path.as_deref().and_then(|p| p.parent());
                preview::render(&view, "markdown", &pcore::markdown_to_html_in(&text, base));
            }
            pcore::Kind::Html => preview::render(&view, "html", &text),
            pcore::Kind::Mermaid => preview::render(&view, "mermaid", &text),
            pcore::Kind::Json => preview::render(&view, "json", &text),
            pcore::Kind::Log | pcore::Kind::Http => preview::render(&view, "message", "This preview is not available in this build yet."),
        }
    }
    fn sync_tabs(&mut self) {
        let infos = self.ws.tab_infos();
        let key: Vec<(String, bool, bool)> = infos.iter().map(|t| (t.title.clone(), t.dirty, t.pinned)).collect();
        if key != self.tab_key || self.tabs.get_page_count() != infos.len() {
            self.syncing = true;
            // One empty page per tab; pages are added or dropped at the end and relabelled in place.
            while self.tabs.get_page_count() > infos.len() {
                let last = self.tabs.get_page_count() - 1;
                if let Some(w) = self.tabs.get_page(last) {
                    self.tabs.remove_page(last);
                    w.destroy();
                }
            }
            for (i, t) in infos.iter().enumerate() {
                let label = format!("{}{}{}", if t.pinned { "\u{1F4CC} " } else { "" }, if t.dirty { "\u{25CF} " } else { "" }, t.title);
                if i < self.tabs.get_page_count() {
                    self.tabs.set_page_text(i, &label);
                } else {
                    let page = Panel::builder(&self.tabs).build();
                    self.tabs.add_page(&page, &label, false, None);
                }
            }
            self.syncing = false;
            self.tab_key = key;
        }
        if self.tabs.selection() != self.ws.active as i32 {
            self.syncing = true;
            self.tabs.set_selection(self.ws.active);
            self.syncing = false;
        }
    }

    /// The File menu lists recent files and offers "Restore Recent Closed File": rebuild it when they change.
    fn sync_file_menu(&mut self) {
        let key = (self.ws.recent.clone(), !self.ws.closed.is_empty());
        if key == self.recent_key {
            return;
        }
        menus::fill_file_menu(&self.menus, &key.0, key.1);
        let new = menus::realize(self.menus.file);
        self.menubar.replace(0, new, "File");
        self.recent_key = key;
    }

    pub fn save_session(&mut self) {
        if !session::standalone() {
            let _ = self.ws.snapshot().save(&session::session_path());
        }
    }

    fn autosave_session(&mut self) {
        let key = self.ws.session_fingerprint();
        if key != self.session_key {
            self.session_key = key;
            self.save_session();
        }
    }
}

/// Default shortcut table for accelerators of items that are not in a menu.
#[allow(dead_code)]
pub fn default_shortcut(id: usize) -> Option<String> {
    PALETTE_COMMANDS.iter().find(|(_, i, _)| *i == id).map(|(_, _, k)| k.to_string()).filter(|k| shortcuts::parse(k).is_some())
}
