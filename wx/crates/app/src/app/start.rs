//! Starting up: the single-instance hand-off, building the window, loading the workspace.

use super::*;

/// Build the window and run. Files named on the command line open as tabs.
/// Where a second launch leaves the files it was asked to open for the running one.
pub(super) fn inbox() -> PathBuf {
    session::config_dir().join("inbox")
}

/// A second launch hands its files to the running Note++ and exits.
pub(super) fn hand_off_to_running_instance(files: &[PathBuf]) {
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
        // Written under another name and renamed: the running window polls the folder and must never read it half-written.
        let part = dir.join(format!("{stamp}.part"));
        if std::fs::write(&part, list.join("\n")).is_ok() {
            let _ = std::fs::rename(&part, dir.join(format!("{stamp}.txt")));
        }
        // No window is created here, so the toolkit would wait forever.
        std::process::exit(0);
    }
    std::mem::forget(checker);
}

/// The widgets of the editor area and the window's vertical layout around them.
pub(super) struct Widgets {
    splitter: SplitterWindow,
    editor_pane: Panel,
    dock_panes: DockPanes,
    stc: StyledTextCtrl,
    dock_stc: StyledTextCtrl,
    mini: StyledTextCtrl,
    probe: StyledTextCtrl,
    panels: Panels,
    wbv: WebPane,
    view: WebPane,
    mdbar: crate::widgets::dock::Dock,
}

/// Drop the toolkit's themed border: it is drawn in the system colours, which show as a white frame in the dark theme.
pub(super) fn no_border(w: &impl WxWidget) {
    const BORDERS: i64 = 0x1F00_0000; // wxBORDER_SIMPLE | SUNKEN | RAISED | STATIC | THEME
    const BORDER_NONE: i64 = 0x0020_0000;
    w.set_style_raw((w.get_style_raw() & !BORDERS) | BORDER_NONE);
    crate::widgets::native::current().remove_edge(w.get_handle() as isize);
}

pub(super) fn build_widgets(frame: &Frame, chrome: &Chrome) -> Widgets {
    // The editor, with the preview pane beside it when that is switched on.
    // No 3-D border: it is drawn in the system colours, which show as a white frame around the editor in the dark theme.
    let splitter = SplitterWindow::builder(frame).with_style(wxdragon::widgets::splitter_window::SplitterWindowStyle::ThinSash).build();
    let editor_pane = Panel::builder(&splitter).build();
    let (dock_panes, stc, dock_stc) = DockPanes::build(&editor_pane);
    let mini = StyledTextCtrl::builder(&editor_pane).build();
    mini.set_min_size(Size::new(crate::widgets::theme::current().px(110), -1));
    // Never shown: reads other tabs' documents for the session and tab tool tips, so `stc` never swaps documents.
    let probe = StyledTextCtrl::builder(&editor_pane).build();
    probe.show(false);
    for editor in [&stc, &dock_stc, &mini] {
        no_border(editor);
    }
    let row = BoxSizer::builder(Orientation::Horizontal).build();
    let panels = panels::create(&editor_pane);
    row.add(&panels.tree_box, 0, SizerFlag::Expand, 0);
    row.add(&dock_panes.split, 1, SizerFlag::Expand, 0);
    row.add(&mini, 0, SizerFlag::Expand, 0);
    let wbv = WebPane::new(&editor_pane);
    wbv.add_user_script(wbm::BRIDGE, wxdragon::widgets::webview::WebViewUserScriptInjectionTime::AtDocumentStart);
    wbv.show(false);
    row.add(&wbv, 1, SizerFlag::Expand, 0);
    row.add(&panels.side, 0, SizerFlag::Expand, 0);
    editor_pane.set_sizer(row, true);
    let view = WebPane::new(&splitter);
    view.show(false);
    crate::widgets::theme::own_sash(splitter, editor_pane);
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
    Widgets { splitter, editor_pane, dock_panes, stc, dock_stc, mini, probe, panels, wbv, view, mdbar }
}

/// The workspace: settings, the last session, then the files named on the command line.
pub(super) fn load_workspace(ui: &WxUi, views: [&StyledTextCtrl; 4], settings: Settings, files: &[PathBuf]) -> Workspace<WxSci> {
    let [stc, dock_stc, mini, probe] = views;
    let sci = WxSci::of(stc);
    let mut ws = Workspace::new(sci);
    ws.set_ui_percent(crate::platform::wx::to_isize(crate::widgets::theme::current().px(100)));
    ws.set_settings(settings);
    ws.attach_minimap(WxSci::of(mini));
    ws.attach_probe(WxSci::of(probe));
    ws.attach_second(WxSci::of(dock_stc));
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
pub(super) fn attach_drop_targets(app: &Shared, frame: Frame, views: [StyledTextCtrl; 2]) {
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
    for stc in views {
        let _ = FileDropTarget::builder(&stc).with_on_drop_files(drop(app.clone())).build();
    }
}

/// The editor's own popup is replaced by the configurable one.
pub(super) fn replace_editor_popup(app: &Shared, stc: StyledTextCtrl) {
    use notepp_core::sci::Send;
    WxSci::of(&stc).send(2371, 0, 0); // SCI_USEPOPUP: never
    let (a, s) = (app.clone(), stc);
    stc.on_mouse_right_up(move |e| {
        if let wxdragon::event::WindowEventData::MouseButton(m) = &e
            && let Some(p) = m.get_position()
        {
            // The menu acts on the view that was clicked.
            s.set_focus();
            popups::editor_menu(&a, s.client_to_screen(p));
        }
    });
}

/// Everything that needs the finished `App`: tool-tips, key-map report, event wiring, then show the window.
#[allow(clippy::too_many_lines, reason = "start-up wiring, in order")]
pub(super) fn finish_start(app: &Shared, frame: Frame, stc: StyledTextCtrl, mini: StyledTextCtrl, wbv: &WebPane, view: &WebPane, bar: &[(String, menus::M)]) {
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
    let dock_stc = app.borrow().dock_stc;
    attach_drop_targets(app, frame, [stc, dock_stc]);
    {
        let mut a = app.borrow_mut();
        if let Some(dir) = a.ws.workspace.clone() {
            a.open_workspace(dir);
        }
    }
    replace_editor_popup(app, stc);
    replace_editor_popup(app, dock_stc);
    dock_pane::wire_dock(app, frame, stc, dock_stc);
    wire_mini(app, mini);
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
        crate::widgets::native::current().set_app_icon(hwnd);
        a.menu_bar = crate::widgets::native::current().detach_menu_bar(hwnd);
        a.chrome.set_menu_titles(bar.iter().map(|(t, _)| t.clone()).collect());
    }
    frame.layout();
    stc.set_focus();
    app.borrow_mut().restore_dock();
}

/// The main window, sized for the display (the display scale is taken first: everything after is laid out with it).
fn main_frame() -> Frame {
    crate::widgets::theme::init_display_scale();
    let t = crate::widgets::theme::current();
    Frame::builder().with_title("Note++").with_size(Size::new(t.px(1200), t.px(800))).build()
}

/// Build the menus and give them to the window.
fn attach_menu_bar(frame: &Frame) -> (Vec<(String, menus::M)>, menus::Menus) {
    let (bar, menu_set) = menus::build_menus();
    let mut mb = MenuBar::builder();
    for (title, m) in &bar {
        mb = mb.append(menus::realize(*m), title);
    }
    frame.set_menu_bar(mb.build());
    (bar, menu_set)
}

pub fn start(files: &[PathBuf]) {
    hand_off_to_running_instance(files);
    // The only copy running: a downloaded update goes in now, before there is a window to flicker.
    crate::updates::apply_staged_at_start(files);
    let settings = Settings::load();
    let keymap = notepp_core::keymap::KeyMap::build(&settings.shortcuts);
    menus::set_keymap(&keymap);

    let frame = main_frame();

    let (bar, menu_set) = attach_menu_bar(&frame);

    // Toolbar and tab strip above the editor, status bar below it.
    let chrome = Chrome::new(&frame);
    let w = build_widgets(&frame, &chrome);
    let Widgets { splitter, editor_pane, dock_panes, stc, dock_stc, mini, probe, panels, wbv, view, mdbar } = w;

    let ui = WxUi::new(frame);
    let ws = load_workspace(&ui, [&stc, &dock_stc, &mini, &probe], settings, files);

    let Some(menubar) = frame.get_menu_bar() else { return };
    let app: Shared = Rc::new(RefCell::new(App {
        bus: CommandBus::standard(),
        results: crate::state::SearchResults::default(),
        file_index: notepp_core::files::FileIndex::default(),
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
        upd_plan: crate::updates::Plan::default(),
        incr: None,
        mini,
        main_stc: stc,
        dock_stc,
        dock_panes,
        view: view.clone(),
        chrome,
        postit: false,
        menu_took_focus: std::cell::Cell::new(false),
        distraction: false,
        chrome_vis: [true; 4],
        menubar,
        menus: menu_set,
        bar_menus: bar.iter().map(|(_, m)| *m).collect(),
        cmd_state: None,
        keymap,
        session_key: ((usize::MAX, 0, 0, 0), 0),
        recent_key: (vec![PathBuf::from("\u{0}")], false),
        window_key: (vec!["\u{0}".into()], 0),
    }));
    finish_start(&app, frame, stc, mini, &wbv, &view, &bar);
}
