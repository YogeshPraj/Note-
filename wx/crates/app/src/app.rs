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
    menu_bar: isize,
    mini: StyledTextCtrl,
    pub(crate) stc: StyledTextCtrl,
    pub(crate) view: WebPane,
    pub(crate) chrome: Chrome,
    pub(crate) menubar: MenuBar,
    pub(crate) postit: bool,
    pub(crate) distraction: bool,
    pub(crate) chrome_vis: [bool; 4],
    pub(crate) menus: Menus,
    /// The menus of the menu bar, in order (their contents are current: the menu arena is rebuilt in place).
    pub(crate) bar_menus: Vec<menus::M>,
    /// What the menus and toolbar were last enabled for (see [`crate::command::App::sync_command_state`]).
    pub(crate) cmd_state: Option<notepp_core::registry::CommandState>,
    /// Defaults plus the user's overrides; what menus, tool-tips and the palette show.
    pub keymap: notepp_core::keymap::KeyMap,
    session_key: (usize, usize, usize, u64),
    recent_key: (Vec<PathBuf>, bool),
    window_key: (Vec<String>, usize),
}

pub type Shared = Rc<RefCell<App>>;

/// Build the window and run. Files named on the command line open as tabs.
/// Where a second launch leaves the files it was asked to open for the running one.
fn inbox() -> PathBuf {
    session::config_dir().join("inbox")
}

/// A second launch hands its files to the running Note++ and exits.
fn hand_off_to_running_instance(files: &[PathBuf]) {
    let checker = if session::standalone() {
        None
    } else {
        SingleInstanceChecker::new(&format!("Notepp-{}", std::env::var("USERNAME").or_else(|_| std::env::var("USER")).unwrap_or_default()), None)
    };
    if checker.as_ref().is_some_and(wxdragon::SingleInstanceChecker::is_another_running) {
        let dir = inbox();
        let _ = std::fs::create_dir_all(&dir);
        let list: Vec<String> = files
            .iter()
            .map(|f| std::fs::canonicalize(f).unwrap_or_else(|_| f.clone()).display().to_string().trim_start_matches("\\\\?\\").to_string())
            .collect();
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let _ = std::fs::write(dir.join(format!("{stamp}.txt")), list.join("\n"));
        // No window is created here, so the toolkit would wait forever.
        std::process::exit(0);
    }
    std::mem::forget(checker);
}

/// The widgets of the editor area and the window's vertical layout around them.
struct Widgets {
    splitter: SplitterWindow,
    editor_pane: Panel,
    stc: StyledTextCtrl,
    mini: StyledTextCtrl,
    panels: Panels,
    wbv: WebPane,
    view: WebPane,
    mdbar: crate::widgets::dock::Dock,
}

/// Drop the toolkit's themed border: it is drawn in the system colours, which show as a white frame in the dark theme.
fn no_border(w: &impl WxWidget) {
    const BORDERS: i64 = 0x1F00_0000; // wxBORDER_SIMPLE | SUNKEN | RAISED | STATIC | THEME
    const BORDER_NONE: i64 = 0x0020_0000;
    w.set_style_raw((w.get_style_raw() & !BORDERS) | BORDER_NONE);
    crate::widgets::native::current().remove_edge(w.get_handle() as isize);
}

fn build_widgets(frame: &Frame, chrome: &Chrome) -> Widgets {
    // The editor, with the preview pane beside it when that is switched on.
    // No 3-D border: it is drawn in the system colours, which show as a white frame around the editor in the dark theme.
    let splitter = SplitterWindow::builder(frame).with_style(wxdragon::widgets::splitter_window::SplitterWindowStyle::ThinSash).build();
    let editor_pane = Panel::builder(&splitter).build();
    let stc = StyledTextCtrl::builder(&editor_pane).build();
    let mini = StyledTextCtrl::builder(&editor_pane).build();
    mini.set_min_size(Size::new(110, -1));
    for editor in [&stc, &mini] {
        no_border(editor);
    }
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let panels = panels::create(&editor_pane);
    row.add(&panels.tree_box, 0, SizerFlag::Expand, 0);
    row.add(&stc, 1, SizerFlag::Expand, 0);
    row.add(&mini, 0, SizerFlag::Expand, 0);
    let wbv = WebPane::new(&editor_pane);
    wbv.add_user_script(wbm::BRIDGE, wxdragon::widgets::webview::WebViewUserScriptInjectionTime::AtDocumentStart);
    wbv.show(false);
    row.add(&wbv, 1, SizerFlag::Expand, 0);
    row.add(&panels.side, 0, SizerFlag::Expand, 0);
    editor_pane.set_sizer(row, true);
    let view = WebPane::new(&splitter);
    view.show(false);
    splitter.initialize(&editor_pane);
    splitter.set_minimum_pane_size(120);

    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    sizer.add(&chrome.menu, 0, SizerFlag::Expand, 0);
    sizer.add(&chrome.tools, 0, SizerFlag::Expand, 0);
    sizer.add(&chrome.tabs, 0, SizerFlag::Expand, 0);
    let mdbar = crate::mdbar::create(frame);
    sizer.add(&mdbar.panel(), 0, SizerFlag::Expand, 0);
    sizer.add(&splitter, 1, SizerFlag::Expand, 0);
    sizer.add(&findpanel::container(frame), 0, SizerFlag::Expand, 0);
    sizer.add(&chrome.status, 0, SizerFlag::Expand, 0);
    frame.set_sizer(sizer, true);
    Widgets { splitter, editor_pane, stc, mini, panels, wbv, view, mdbar }
}

/// The workspace: settings, the last session, then the files named on the command line.
fn load_workspace(ui: &WxUi, stc: &StyledTextCtrl, mini: &StyledTextCtrl, settings: Settings, files: &[PathBuf]) -> Workspace<WxSci> {
    let sci = WxSci::of(stc);
    let mut ws = Workspace::new(sci);
    ws.set_settings(settings);
    ws.attach_minimap(WxSci::of(mini));
    if !session::standalone()
        && let Some(s) = Session::load(&session::session_path())
    {
        ws.restore(ui, &s);
    }
    for f in files {
        ws.open_path(ui, f);
    }
    if ws.tabs.is_empty() {
        ws.new_tab();
    }
    ws
}

/// Files dropped on the window or the editor open as tabs; folders become the workspace.
fn attach_drop_targets(app: &Shared, frame: Frame, stc: StyledTextCtrl) {
    let drop = |a: Shared| {
        move |files: Vec<String>, _: i32, _: i32| {
            let paths: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
            if let Ok(mut app) = a.try_borrow_mut() {
                app.open_from_outside(&paths);
            }
            true
        }
    };
    let _ = FileDropTarget::builder(&frame).with_on_drop_files(drop(app.clone())).build();
    let _ = FileDropTarget::builder(&stc).with_on_drop_files(drop(app.clone())).build();
}

/// The editor's own popup is replaced by the configurable one.
fn replace_editor_popup(app: &Shared, stc: StyledTextCtrl) {
    use notepp_core::sci::Send;
    WxSci::of(&stc).send(2371, 0, 0); // SCI_USEPOPUP: never
    let (a, s) = (app.clone(), stc);
    stc.on_mouse_right_up(move |e| {
        if let wxdragon::event::WindowEventData::MouseButton(m) = &e
            && let Some(p) = m.get_position()
        {
            popups::editor_menu(&a, s.client_to_screen(p));
        }
    });
}

/// Everything that needs the finished `App`: tool-tips, key-map report, event wiring, then show the window.
#[allow(clippy::too_many_lines, reason = "start-up wiring, in order")]
fn finish_start(app: &Shared, frame: Frame, stc: StyledTextCtrl, mini: StyledTextCtrl, wbv: &WebPane, view: &WebPane, bar: &[(String, menus::M)]) {
    {
        let mut a = app.borrow_mut();
        a.chrome.refresh_tips(&a.keymap.clone());
        for p in a.keymap.problems() {
            trace(0, &format!("keymap: {p}"));
        }
        if let Some(p) = a.keymap.problems().first() {
            a.flash = Some(p.to_string());
        }
    }

    findpanel::wire(app);
    panels::wire(app);
    crate::mdbar::wire(app);
    app.borrow_mut().update_startup();
    crate::widgets::theme::subscribe(move |_| {
        // The title bar and native controls follow the theme.
        crate::widgets::theme::style_native(&frame);
    });
    attach_drop_targets(app, frame, stc);
    {
        let mut a = app.borrow_mut();
        if let Some(dir) = a.ws.workspace.clone() {
            a.open_workspace(dir);
        }
    }
    replace_editor_popup(app, stc);
    wire_mini(app, mini, stc);
    wire_whiteboard(app, wbv);
    wire_menu_keys(app, frame);
    wire(app, frame, &stc, view);
    app.borrow_mut().refresh();
    frame.show(true);
    frame.centre();
    {
        // Our own menu bar replaces the native one (which cannot follow the theme); the native menus stay
        // attached so shortcuts and ticks keep working.
        let hwnd = frame.get_handle() as isize;
        let mut a = app.borrow_mut();
        a.menu_bar = crate::widgets::native::current().detach_menu_bar(hwnd);
        a.chrome.set_menu_titles(bar.iter().map(|(t, _)| t.clone()).collect());
    }
    frame.layout();
    stc.set_focus();
}

pub fn start(files: &[PathBuf]) {
    hand_off_to_running_instance(files);
    let settings = Settings::load();
    let keymap = notepp_core::keymap::KeyMap::build(&settings.shortcuts);
    menus::set_keymap(&keymap);

    let frame = Frame::builder().with_title("Note++").with_size(Size::new(1200, 800)).build();

    // Menu bar.
    let (bar, menu_set) = menus::build_menus();
    let mut mb = MenuBar::builder();
    for (title, m) in &bar {
        mb = mb.append(menus::realize(*m), title);
    }
    let menubar = mb.build();
    frame.set_menu_bar(menubar);

    // Toolbar and tab strip above the editor, status bar below it.
    let chrome = Chrome::new(&frame);
    let w = build_widgets(&frame, &chrome);
    let Widgets { splitter, editor_pane, stc, mini, panels, wbv, view, mdbar } = w;

    let ui = WxUi { frame };
    let ws = load_workspace(&ui, &stc, &mini, settings, files);

    let Some(menubar) = frame.get_menu_bar() else { return };
    let app: Shared = Rc::new(RefCell::new(App {
        bus: CommandBus::standard(),
        results: crate::state::SearchResults::default(),
        clipboard: crate::services::clipboard::ClipboardService::default(),
        streams: crate::state::StreamState::default(),
        wb: crate::state::WhiteboardState::default(),
        preview: crate::state::PreviewState::default(),
        ws,
        ui,
        flash: None,
        frame,
        splitter,
        editor_pane,
        wbv: wbv.clone(),
        menu_bar: 0,
        panels,
        side: crate::side::Side::default(),
        side_key: None,
        mdbar,
        upd: crate::updates::State::default(),
        mini,
        stc,
        view: view.clone(),
        chrome,
        postit: false,
        distraction: false,
        chrome_vis: [true; 4],
        menubar,
        menus: menu_set,
        bar_menus: bar.iter().map(|(_, m)| *m).collect(),
        cmd_state: None,
        keymap,
        session_key: (usize::MAX, 0, 0, 0),
        recent_key: (vec![PathBuf::from("\u{0}")], false),
        window_key: (vec!["\u{0}".into()], 0),
    }));
    finish_start(&app, frame, stc, mini, &wbv, &view, &bar);
}

/// The minimap is a second view of the document: clicking or dragging in it scrolls the editor.
fn wire_mini(app: &Shared, mini: StyledTextCtrl, stc: StyledTextCtrl) {
    use notepp_core::sci::Send;
    let jump = |app: &Shared, mini: StyledTextCtrl, e: &wxdragon::event::WindowEventData| {
        let p = match e {
            wxdragon::event::WindowEventData::MouseButton(m) => m.get_position(),
            wxdragon::event::WindowEventData::MouseMotion(m) => m.get_position(),
            _ => None,
        };
        let Some(p) = p else { return };
        let sci = WxSci::of(&mini);
        let pos = sci.send(sci::SCI_POSITIONFROMPOINT, crate::platform::wx::to_usize(p.x), p.y as isize).max(0);
        let line = sci.send(sci::SCI_LINEFROMPOSITION, crate::platform::wx::to_usize(pos), 0).max(0);
        if let Ok(mut a) = app.try_borrow_mut() {
            a.ws.scroll_to_line(crate::platform::wx::to_usize(line));
            a.refresh();
        }
    };
    let down = Rc::new(std::cell::Cell::new(false));
    let (a, d) = (app.clone(), down.clone());
    mini.on_mouse_left_down(move |e| {
        d.set(true);
        jump(&a, mini, &e);
    });
    let d = down.clone();
    mini.on_mouse_left_up(move |_| d.set(false));
    let a = app.clone();
    mini.on_mouse_motion(move |e| {
        if down.get() {
            jump(&a, mini, &e);
        }
    });
    let a = app.clone();
    mini.on_mouse_right_up(move |e| {
        if let wxdragon::event::WindowEventData::MouseButton(m) = &e
            && let Some(p) = m.get_position()
        {
            popups::minimap_menu(&a, mini.client_to_screen(p));
        }
    });
    mini.on_set_focus(move |_| stc.set_focus());
}

/// Clicking a bar hands it the focus once the click has been handled; give it back to the editor (unless a command has put
/// it somewhere deliberate, such as the find box).
pub(crate) fn refocus_editor(app: &Shared) {
    let Ok(a) = app.try_borrow() else { return };
    let (frame, stc, bars) = (a.frame, a.stc, [a.chrome.menu, a.chrome.tools, a.chrome.tabs, a.chrome.status]);
    crate::widgets::defer::later(&frame, move || {
        if bars.iter().any(WxWidget::has_focus) {
            stc.set_focus();
        }
    });
}

/// Open menu `i` of the menu bar.
fn open_menu(a: &Shared, i: usize) {
    popups::bar_menu(a, i);
}

/// Alt+letter opens the menu whose title underlines that letter; F10 opens the first.
fn wire_menu_keys(app: &Shared, frame: Frame) {
    let a = app.clone();
    frame.on_char_hook(move |e| {
        let wxdragon::event::WindowEventData::Keyboard(k) = &e else {
            e.skip(true);
            return;
        };
        let code = k.get_key_code().unwrap_or(0);
        // An open menu takes every key.
        if crate::widgets::popup_menu::key(code, !k.control_down() && !k.alt_down()) {
            return;
        }
        let target = if k.alt_down() && !k.control_down() {
            u8::try_from(code).ok().filter(u8::is_ascii_graphic).and_then(|b| {
                let titles = a.try_borrow().ok()?.chrome.menu_titles();
                crate::widgets::menu_bar::menu_for_key(&titles, char::from(b))
            })
        } else if code == 344 && !k.alt_down() && !k.control_down() {
            Some(0) // WXK_F10
        } else {
            None
        };
        match target {
            Some(i) => open_menu(&a, i),
            None => e.skip(true),
        }
    });
    // Switching to another program closes an open menu.
    frame.on_activate(|e| {
        if let wxdragon::event::WindowEventData::Activate(a) = &e
            && !a.is_active()
        {
            crate::widgets::popup_menu::deactivated();
        }
        e.skip(true);
    });
}

/// One entry for menu items and toolbar buttons. Modeless windows need the shared handle, so they
/// are opened here, with nothing borrowed.
pub(crate) fn run_command(a: &Shared, id: usize) {
    trace(id, "event");
    match id {
        notepp_core::commands::ID_PREFS => return crate::settings_window::open(a, "general"),
        notepp_core::commands::ID_STYLE_CFG => return crate::settings_window::open(a, "colours"),
        notepp_core::commands::ID_SHORTCUTS => return crate::settings_window::open(a, "shortcuts"),
        notepp_core::commands::ID_EDIT_CONTEXT => return crate::settings_window::open(a, "context"),
        notepp_core::commands::ID_ENC_SETTINGS => return crate::settings_window::open(a, "security"),
        notepp_core::commands::ID_TAB_COLOR_CUSTOM => return crate::popups::custom_tab_colour(a),
        ID_FIND => return crate::find::open_panel(a, Mode::Find),
        ID_REPLACE => return crate::find::open_panel(a, Mode::Replace),
        ID_PALETTE => return crate::palette::open_commands(a),
        ID_QUICKOPEN => return crate::palette::open_files(a),
        ID_INCR => return crate::incr::open(a),
        ID_FIF => return crate::find::open_panel(a, Mode::InFiles),
        ID_MARK => return crate::find::open_panel(a, Mode::Mark),
        ID_FINDNEXT | ID_FINDPREV => return crate::find::step(a, id == ID_FINDNEXT),
        ID_GOTO => return crate::find::goto_line(a),
        _ => {}
    }
    if let Ok(mut app) = a.try_borrow_mut() {
        app.command(id);
    }
}
/// Messages from the Excalidraw page: readiness, scene changes, save requests.
fn wire_whiteboard(app: &Shared, wbv: &WebPane) {
    let a = app.clone();
    wbv.on_script_message_received(move |e| {
        let msg = e.get_string().unwrap_or_default();
        let Some(ev) = wbm::parse(&msg) else { return };
        if let Ok(mut app) = a.try_borrow_mut() {
            app.wb_event(ev);
        }
    });
}

fn wire(app: &Shared, frame: Frame, stc: &StyledTextCtrl, view: &WebPane) {
    wire_events(app, frame, stc, view);
    wire_timers(app, frame);
}

#[allow(clippy::too_many_lines, reason = "wires each window event to its action, one block per event")]
fn wire_events(app: &Shared, frame: Frame, stc: &StyledTextCtrl, view: &WebPane) {
    // The preview page finished loading: draw the current document in it.
    let a = app.clone();
    view.on_loaded(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.view.mark_ready();
            app.preview_update(true);
        }
    });
    // Messages from the page (close, refresh, lazy-loading of Mermaid / the JSON editor).
    let a = app.clone();
    let pv = view.clone();
    view.on_script_message_received(move |e| {
        let msg = e.get_string().unwrap_or_default();
        match msg.trim_matches('"') {
            // The libraries go in from the main loop: scripts run from inside the page's own message callback get dropped.
            "need-mermaid" => {
                let pv = pv.clone();
                crate::widgets::defer::later(&frame, move || preview::load_mermaid(&pv));
            }
            "need-jsoneditor" => {
                let pv = pv.clone();
                crate::widgets::defer::later(&frame, move || preview::load_json_editor(&pv));
            }
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
    frame.on_menu(move |e| run_command(&a, crate::platform::wx::to_usize(e.get_id())));

    // Caret moves, edits and selection changes update the status bar.
    let a = app.clone();
    stc.on_stc_update_ui(move |_| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.refresh();
        }
    });

    // Toolbar buttons and the tab strip.
    let a = app.clone();
    let handler: Rc<dyn Fn(Act)> = Rc::new(move |act| {
        refocus_editor(&a);
        if let Act::Menu(i) = act {
            return open_menu(&a, i);
        }
        // Popup menus are modal: nothing may be borrowed while they are open (their commands re-enter the app).
        match act {
            Act::Context(i, x, y) => {
                let at = {
                    let Ok(mut app) = a.try_borrow_mut() else { return };
                    app.ws.activate(i);
                    app.refresh();
                    app.chrome.tabs.client_to_screen(Point::new(x, y))
                };
                return popups::tab_menu(&a, at);
            }
            Act::Status(seg, x, y) => {
                let at = match a.try_borrow() {
                    Ok(app) => app.chrome.status.client_to_screen(Point::new(x, y)),
                    Err(_) => return,
                };
                return popups::status_menu(&a, seg, at);
            }
            _ => {}
        }
        let a2 = a.clone();
        let Ok(mut app) = a.try_borrow_mut() else { return };
        match act {
            Act::Cmd(id) => {
                drop(app);
                return run_command(&a2, id);
            }
            Act::Select(i) => app.ws.activate(i),
            Act::Close(i) => {
                let App { ws, ui, .. } = &mut *app;
                ws.close(ui, i);
            }
            Act::New => {
                app.ws.new_tab();
            }
            Act::Move(from, to) => app.ws.move_tab_to(from, to),
            Act::Menu(..) | Act::Context(..) | Act::Status(..) => {}
        }
        app.refresh();
    });
    app.borrow().chrome.set_handler(handler);
}

/// One timer for every periodic job, and saving the session when the window closes.
fn wire_timers(app: &Shared, frame: Frame) {
    // One timer drives every periodic job; the schedule says how often each runs.
    #[derive(Clone, Copy, PartialEq)]
    enum Job {
        /// Followed logs, HTTP responses, the update check, files dropped by a second launch.
        Stream,
        /// Clipboard History records what is copied, whichever program copies it; a copied password is cleared.
        Clipboard,
        /// Keep the session file current (a crash or power cut then loses at most a few seconds).
        Autosave,
    }
    let schedule = Schedule::new(500).every(Job::Stream, 500).every(Job::Clipboard, 1000).every(Job::Autosave, 10_000);
    let ticker = Timer::new(&frame);
    let a = app.clone();
    let beat = std::cell::Cell::new(0u64);
    let base = crate::platform::wx::to_i32(schedule.base_ms());
    ticker.on_tick(move |_| {
        beat.set(beat.get() + 1);
        let Ok(mut app) = a.try_borrow_mut() else { return };
        for job in schedule.due(beat.get()) {
            match job {
                Job::Stream => {
                    app.update_poll();
                    app.check_inbox();
                    app.monitor_tick();
                    app.stream_tick();
                }
                Job::Clipboard => {
                    app.clip_tick();
                    app.poll_clipboard();
                }
                Job::Autosave => app.autosave_session(),
            }
        }
    });
    ticker.start(base, false);
    std::mem::forget(ticker);
    // Closing the window saves the session; unsaved text comes back next time.
    let a = app.clone();
    frame.on_close(move |e| {
        if let Ok(mut app) = a.try_borrow_mut() {
            app.save_session();
        }
        e.skip(true);
    });
}

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
    /// Actions from the Find panel.
    pub fn panel_command(&mut self, cmd: Cmd) {
        use notepp_core::search::{self, Found};
        let sci = self.ws.sci;
        let mode = findpanel::mode();
        let opts = findpanel::read_opts();
        if !matches!(cmd, Cmd::Close | Cmd::Browse | Cmd::OpenHit(_) | Cmd::Clear) {
            self.ws.search = opts.clone();
        }
        let found = |f: Found| match f {
            Found::At { wrapped: true } => "Found (wrapped around the document)".to_string(),
            Found::At { wrapped: false } => String::new(),
            Found::NotFound => "Not found".to_string(),
            Found::BadPattern(e) => format!("Invalid regular expression: {e}"),
        };
        let outcome = |r: Result<usize, notepp_core::error::Error>, what: &str| match r {
            Ok(n) => format!("{n} {what}"),
            Err(e) => format!("Invalid regular expression: {e}"),
        };
        let mut note: Option<String> = None;
        match cmd {
            Cmd::Next => match mode {
                Mode::InFiles => return self.panel_command(Cmd::InFiles),
                Mode::Mark => return self.panel_command(Cmd::Mark),
                _ => note = Some(found(search::find(&sci, &opts, true))),
            },
            Cmd::Prev => note = Some(found(search::find(&sci, &opts, false))),
            Cmd::Replace => {
                let (replaced, f) = search::replace_current(&sci, &opts);
                let n = found(f);
                note = Some(if replaced && n.is_empty() { "Replaced".into() } else { n });
            }
            Cmd::ReplaceAll => note = Some(outcome(search::replace_all(&sci, &opts), "replaced")),
            Cmd::Count => note = Some(outcome(search::count(&sci, &opts), "matches")),
            Cmd::Mark => note = Some(outcome(search::mark_all(&sci, &opts, search::INDIC_FIND), "matches marked")),
            Cmd::Clear => {
                search::clear_marks(&sci, search::INDIC_FIND);
                note = Some("Marks cleared".into());
            }
            Cmd::FindAll => note = Some(self.find_all_note(&opts)),
            Cmd::InFiles => note = Some(self.in_files_note(&opts)),
            Cmd::Browse => {
                let d = DirDialog::builder(&self.frame, "Select a folder to search", "").build();
                if d.show_modal() == ID_OK
                    && let Some(p) = d.get_path()
                {
                    findpanel::set_dir(&p);
                }
            }
            Cmd::OpenHit(i) => {
                self.results.cursor = i;
                self.open_hit(i);
            }
            Cmd::Close => {
                findpanel::close();
                self.stc.set_focus();
            }
        }
        if let Some(n) = note {
            findpanel::set_status(&n);
        }
        self.refresh();
    }

    /// "Find All" in the current document: list the hits and say how many.
    fn find_all_note(&mut self, opts: &notepp_core::search::Opts) -> String {
        use notepp_core::search;
        match search::find_all(&self.ws.sci, opts) {
            Ok(mut hits) => {
                let total = hits.len();
                hits.truncate(search::MAX_LIST);
                let lines: Vec<String> = hits.iter().map(|h| format!("Line {}:  {}", h.line, h.text)).collect();
                let note = if total > hits.len() {
                    format!("{total} hits (listing the first {})", hits.len())
                } else {
                    format!("{total} hits in the current document")
                };
                self.results.hits = hits;
                findpanel::set_results(&lines);
                note
            }
            Err(e) => format!("Invalid regular expression: {e}"),
        }
    }

    /// "Find in Files": search the folder in the panel, list the hits and say how many.
    fn in_files_note(&mut self, opts: &notepp_core::search::Opts) -> String {
        use notepp_core::search;
        let (dir, filters) = findpanel::read_dir_filter();
        if dir.as_os_str().is_empty() || !dir.is_dir() {
            return "Choose an existing directory first".into();
        }
        match search::find_in_files(&dir, &filters, opts) {
            Ok(hits) => {
                let files: std::collections::HashSet<_> = hits.iter().filter_map(|h| h.path.as_ref()).collect();
                let note = format!("{} hits in {} files{}", hits.len(), files.len(), if hits.len() >= search::MAX_HITS { " (limit reached)" } else { "" });
                let lines: Vec<String> = hits
                    .iter()
                    .map(|h| {
                        let p = h.path.as_deref().unwrap_or(std::path::Path::new(""));
                        let rel = p.strip_prefix(&dir).unwrap_or(p);
                        format!("{} ({}):  {}", rel.display(), h.line, h.text)
                    })
                    .collect();
                self.results.hits = hits;
                self.results.root = dir;
                findpanel::set_results(&lines);
                note
            }
            Err(e) => format!("Invalid regular expression: {e}"),
        }
    }

    /// Open result `idx` of the last search in the editor.
    fn open_hit(&mut self, idx: usize) {
        use notepp_core::search;
        if let Some(h) = self.results.hits.get(idx).cloned() {
            let App { ws, ui, .. } = self;
            if let Some(p) = &h.path {
                ws.open_path(ui, p);
            }
            let a = ws.active;
            let sci = ws.sci;
            if ws.is_dirty(a) && h.path.is_some() {
                search::goto_line(&sci, h.line);
            } else {
                search::select_range(&sci, h.start, h.end);
            }
        }
    }

    /// F4 / Shift+F4: step through the results of the last Find All / Find in Files.
    fn step_result(&mut self, forward: bool) {
        let n = self.results.hits.len();
        if n == 0 {
            self.flash = Some("No search results yet".into());
        } else {
            self.results.cursor = if forward { (self.results.cursor + 1) % n } else { (self.results.cursor + n - 1) % n };
            self.open_hit(self.results.cursor);
            self.flash = Some(format!("Result {} of {n}", self.results.cursor + 1));
        }
        self.refresh();
    }

    fn wb_event(&mut self, ev: wbm::Event) {
        match ev {
            wbm::Event::Ready => {
                self.wbv.mark_ready();
                self.wb.shown = None;
            }
            wbm::Event::State(s) => self.wb_apply_scene(s),
            wbm::Event::Data(s) => {
                self.wb_apply_scene(s);
                self.wb_finish_pending();
            }
            wbm::Event::SaveRequest => {
                self.wb.pending = Some(Pending::Save);
                wbm::request_data(&self.wbv);
            }
        }
        self.refresh();
    }

    fn wb_apply_scene(&mut self, s: String) {
        let current = self.ws.is_whiteboard().then(|| self.ws.tabs[self.ws.active].id);
        // Ignore echoes for a scene that is no longer the one on screen.
        if current.is_some() && current == self.wb.shown {
            let t = &mut self.ws.tabs[self.ws.active];
            if t.content != s {
                // Pan/zoom and Excalidraw's own re-serialisation are not edits.
                let edited = !notepp_core::whiteboard::same_drawing(&t.content, &s);
                t.content = s;
                if edited {
                    t.dirty = true;
                }
            }
        }
    }

    fn wb_finish_pending(&mut self) {
        let Some(p) = self.wb.pending.take() else { return };
        if !self.ws.is_whiteboard() {
            return;
        }
        let App { ws, ui, .. } = self;
        match p {
            Pending::Save => ws.save(ui),
            Pending::SaveAs => ws.save_as(ui),
            Pending::SaveCopy => ws.save_copy_as(ui).is_some(),
        };
    }

    /// Show the whiteboard instead of the editor while a whiteboard tab is active, and push the
    /// tab's scene (and the theme) into the page when needed.
    fn wb_sync(&mut self) {
        let wb = self.ws.is_whiteboard();
        self.sync_preview_pane();
        let mut relayout = false;
        if self.stc.is_shown() == wb {
            self.stc.show(!wb);
            relayout = true;
        }
        let want_mini = self.ws.minimap && !wb;
        if self.mini.is_shown() != want_mini {
            self.mini.show(want_mini);
            relayout = true;
        }
        if self.wbv.is_shown() != wb {
            self.wbv.show(wb);
            relayout = true;
            if wb {
                self.wb.shown = None;
                if !self.wbv.is_loaded() {
                    match wbm::page_url() {
                        Some(url) => self.wbv.load_url(&url),
                        None => self.flash = Some("Whiteboard unavailable: assets/whiteboard not found".into()),
                    }
                }
            }
        }
        if relayout {
            self.editor_pane.layout();
        }
        if wb && self.wbv.is_ready() {
            let dark = self.ws.theme.dark;
            if self.wbv.sync_dark(dark) {
                wbm::set_theme(&self.wbv, dark);
            }
            let (id, content) = {
                let t = &self.ws.tabs[self.ws.active];
                (t.id, t.content.clone())
            };
            if self.wb.shown != Some(id) {
                self.wb.shown = Some(id);
                wbm::load(&self.wbv, &content);
                self.wbv.set_focus();
            }
        }
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
        self.stc.set_focus();
        if self.frame.is_iconized() {
            self.frame.iconize(false);
        }
        self.frame.raise();
    }

    /// Files a second launch left behind.
    pub fn check_inbox(&mut self) {
        let Ok(rd) = std::fs::read_dir(inbox()) else { return };
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut any = false;
        for e in rd.flatten() {
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

    /// Show or hide the preview pane beside the editor.
    pub fn set_preview(&mut self, on: bool) {
        if on == self.preview.on {
            return;
        }
        self.preview.on = on;
        self.sync_preview_pane();
    }

    /// The pane is only beside the editor while preview is on and the active tab has something to preview.
    fn sync_preview_pane(&mut self) {
        let previewable = !self.ws.is_whiteboard() && self.ws.preview_kind().is_some();
        self.show_preview_pane(self.preview.on && previewable);
    }

    /// Put the pane beside the editor or take it away (a whiteboard has nothing to preview).
    fn show_preview_pane(&mut self, want: bool) {
        if want == self.preview.split {
            return;
        }
        self.preview.split = want;
        if want {
            if !self.view.is_loaded() {
                self.view.load_html(&preview::page_html(self.ws.settings.preview_remote_images));
            }
            self.view.show(true);
            let half = self.frame.get_size().width / 2;
            self.splitter.split_vertically(&self.editor_pane, &self.view, half);
            self.preview.key = None;
        } else {
            self.splitter.unsplit(Some(&self.view));
            self.view.show(false);
        }
    }

    /// Re-render the preview when the document, its text or the theme changed.
    pub fn preview_update(&mut self, force: bool) {
        self.sync_preview_pane();
        if !self.preview.split || !self.view.is_ready() {
            return;
        }
        let dark = self.ws.theme.dark;
        if self.view.sync_dark(dark) {
            preview::set_theme(&self.view, dark);
            self.preview.key = None;
        }
        let key = self.ws.preview_key();
        if !force && self.preview.key == Some(key) {
            return;
        }
        self.preview.key = Some(key);
        let view = self.view.clone();
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
            pcore::Kind::Log | pcore::Kind::Http => self.stream_preview(kind, force),
        }
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
    notepp_core::registry::default_shortcut(id).map(str::to_string)
}
