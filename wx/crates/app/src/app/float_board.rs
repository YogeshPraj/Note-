//! A whiteboard in a floating window: a page of its own, and the talk with it. The scene is applied to the window's tab
//! by id. The last strokes are only in the page, so whatever ends the window asks for the scene first and goes on when it
//! has arrived. (`whiteboard_pane` is the same for the page in the main window.)

use notepp_core::workspace::TabKind;

use super::float_pane::{After, Body};
use super::*;

/// The page for a floating whiteboard, loading.
pub(super) fn page(parent: &Panel) -> Option<WebPane> {
    let url = wbm::page_url()?;
    let page = WebPane::new(parent);
    page.add_user_script(wbm::BRIDGE, wxdragon::widgets::webview::WebViewUserScriptInjectionTime::AtDocumentStart);
    page.load_url(&url);
    Some(page)
}

/// Messages from a floating whiteboard's page.
pub(super) fn wire(a: &Shared, tab: u64, page: &WebPane) {
    let app = a.clone();
    page.on_script_message_received(move |e| {
        let Some(ev) = wbm::parse(&e.get_string().unwrap_or_default()) else { return };
        match ev {
            wbm::Event::Tab(_) => return,
            wbm::Event::Menu(key) => return open_menu_for_key(&app, &key),
            _ => {}
        }
        let scene_is_in = matches!(ev, wbm::Event::Data(_));
        message(&app, tab, ev);
        if scene_is_in {
            dock_board::scene_arrived(&app);
        }
    });
}

/// Something from the page showing `tab`.
pub(super) fn message(app: &Shared, tab: u64, ev: wbm::Event) {
    let done = app.try_borrow_mut().ok().and_then(|mut a| a.board_event(tab, ev));
    finish_later(app, tab, done);
}

/// The page is done, and `then` was waiting for it. Not from inside the page's own message: finishing can destroy the page.
pub(super) fn finish_later(app: &Shared, tab: u64, then: Option<After>) {
    let (Some(then), Some(main)) = (then, app.try_borrow().ok().map(|a| a.frame)) else { return };
    let app = app.clone();
    crate::widgets::defer::later(&main, move || {
        if let Ok(mut a) = app.try_borrow_mut() {
            a.finish_window(tab, then);
        }
    });
}

impl App {
    /// The page showing `tab`, in a floating window or the dock, and what it is a page of.
    pub(super) fn page_of(&self, tab: u64) -> Option<(WebPane, TabKind)> {
        if self.ws.docked_tab() == Some(tab) {
            let kind = self.ws.dock_kind()?;
            return Some((self.dock_panes.page(kind)?.page.clone(), kind));
        }
        match &self.window_of(tab)?.body {
            Body::Board(page) => Some((page.clone(), TabKind::Whiteboard)),
            Body::Picture(page) => Some((page.clone(), TabKind::Image)),
            Body::Editor(_) => None,
        }
    }

    /// The page the user is in: a floating window's, the dock's, or the main window's.
    pub(crate) fn current_page(&self) -> WebPane {
        let side = if self.ws.in_dock() { self.ws.docked_tab() } else { self.ws.current_float() };
        let page = side.and_then(|tab| self.page_of(tab)).map(|p| p.0);
        page.unwrap_or_else(|| if self.ws.is_image() { self.imgv.clone() } else { self.wbv.clone() })
    }

    /// Something from a floating whiteboard's page. Returns what to do now the scene is in, if the window was waiting.
    fn board_event(&mut self, tab: u64, ev: wbm::Event) -> Option<After> {
        let page = self.page_of(tab)?.0;
        let mut done = None;
        match ev {
            wbm::Event::Ready => {
                page.mark_ready();
                let dark = self.ws.theme.dark;
                page.sync_dark(dark);
                wbm::set_theme(&page, dark);
                if let Some(t) = self.ws.tabs.iter().find(|t| t.id == tab) {
                    wbm::load(&page, &t.content);
                }
            }
            wbm::Event::State(s) => self.scene_changed(tab, s),
            wbm::Event::Data(s) => {
                self.scene_changed(tab, s);
                done = self.take_waiting(tab);
                if done.is_none() && self.wb.then.is_none() {
                    self.wb_finish_pending();
                }
            }
            wbm::Event::Focus => self.go_to_page(tab),
            wbm::Event::SaveRequest => {
                self.go_to_page(tab);
                self.wb.pending = Some(Pending::Save);
                wbm::request_data(&page);
            }
            wbm::Event::Tab(_) | wbm::Event::Menu(_) => {}
        }
        self.refresh();
        done
    }

    /// Ask a whiteboard page for its latest scene before `then`. False when there is none to wait for.
    pub(super) fn ask_board_first(&mut self, tab: u64, then: After) -> bool {
        let Some((page, kind)) = self.page_of(tab).filter(|p| p.0.is_ready()) else { return false };
        if self.ws.docked_tab() == Some(tab) {
            if let Some(b) = self.dock_panes.page_mut(kind) {
                b.waiting = then == After::DockBack;
            }
        } else if let Some(w) = self.float_windows.iter_mut().find(|w| w.tab == tab) {
            w.after = Some(then);
        }
        dock_board::request_from_main_loop(&self.frame, page, kind);
        true
    }

    /// What `tab`'s page was asked for its scene for.
    pub(super) fn take_waiting(&mut self, tab: u64) -> Option<After> {
        if self.ws.docked_tab() == Some(tab) {
            let b = self.dock_panes.page_mut(self.ws.dock_kind()?)?;
            return std::mem::take(&mut b.waiting).then_some(After::DockBack);
        }
        self.float_windows.iter_mut().find(|w| w.tab == tab).and_then(|w| w.after.take())
    }

    /// The user is in `tab`'s page now.
    pub(super) fn go_to_page(&mut self, tab: u64) {
        if self.ws.docked_tab() == Some(tab) {
            self.ws.focus_pane(true);
        } else {
            self.ws.focus_float(tab);
        }
    }
}
