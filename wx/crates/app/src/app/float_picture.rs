//! A picture in a floating window or the dock: a page of its own, and the talk with it. The picture goes to its tab by id,
//! and a pending edit is only in the page, so whatever ends the window asks the page to flush first. (`image_pane` is
//! the same for the page in the main window.)

use super::float_pane::After;
use super::*;
use crate::imageedit::{self as ime, Message};

/// The page for a floating or docked picture, loading.
pub(super) fn page(parent: &Panel) -> WebPane {
    let page = WebPane::new(parent);
    no_border(&page);
    page.load_html(&ime::page_html());
    page
}

/// Messages from a picture's page. `tab` is the floating window's tab; `None` is the dock, whose tab changes.
pub(super) fn wire(a: &Shared, tab: Option<u64>, page: &WebPane) {
    let app = a.clone();
    page.on_script_message_received(move |e| {
        let msg = notepp_core::jsonpath::page_text(&e.get_string().unwrap_or_default());
        let Some(m) = ime::parse(&msg) else { return };
        match m {
            // In a floating window the other tabs are in the other window.
            Message::Tab(delta) => {
                if tab.is_none() {
                    switch_tab(&app, delta);
                }
            }
            Message::Menu(key) => open_menu_for_key(&app, &key),
            Message::CopyImage(copy) => {
                if let Some((w, h, rgba)) = preview::parse_image(&copy) {
                    crate::services::clipboard::copy_image(w, h, &rgba);
                }
            }
            m => {
                let flushed = m == Message::Flushed;
                let Some(tab) = tab.or_else(|| app.try_borrow().ok()?.ws.docked_tab()) else { return };
                let done = app.try_borrow_mut().ok().and_then(|mut a| a.picture_event(tab, m));
                float_board::finish_later(&app, tab, done);
                if flushed {
                    dock_board::scene_arrived(&app);
                }
            }
        }
    });
}

impl App {
    /// Something from the page showing `tab`. Returns what to do now the picture is in, if the window was waiting.
    fn picture_event(&mut self, tab: u64, m: Message) -> Option<After> {
        let page = self.page_of(tab)?.0;
        let mut done = None;
        match m {
            Message::Ready => {
                page.mark_ready();
                let dark = self.ws.theme.dark;
                page.sync_dark(dark);
                ime::set_theme(&page, dark);
                self.give_picture(tab, &page);
            }
            Message::State { edited, content } => self.picture_changed(tab, edited, content),
            Message::Flushed => done = self.take_waiting(tab),
            Message::AgentDone(done) => self.mcp_board_done(done),
            Message::Focus => self.go_to_page(tab),
            Message::Save => {
                self.go_to_page(tab);
                self.command(ID_SAVE);
                return None;
            }
            Message::CopyImage(_) | Message::Tab(_) | Message::Menu(_) => {}
        }
        self.refresh();
        done
    }

    /// Hand the page the tab's picture: its file, or the clipboard's pixels if it came from a paste not yet shown.
    pub(super) fn give_picture(&mut self, tab: u64, page: &WebPane) {
        let content = self.ws.tabs.iter().find(|t| t.id == tab).map(|t| t.content.clone()).unwrap_or_default();
        if !content.is_empty() {
            ime::load(page, &content);
        } else if let Some((w, h, pixels)) = self.img.pending.take() {
            ime::load_pixels(page, w, h, &pixels);
        }
    }
}
