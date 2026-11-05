//! The picture editor: its messages, pasting a picture from the clipboard, and showing it instead of the editor.

use super::*;
use crate::imageedit::{self as ime, Message};
use notepp_core::workspace::TabKind;

impl App {
    /// What the page said (its keys and copies are handled before this, with nothing borrowed).
    pub(crate) fn image_message(&mut self, msg: Message) {
        match msg {
            Message::Ready => {
                self.imgv.mark_ready();
                self.img.shown = None;
            }
            Message::State { edited, content } => self.image_changed(edited, content),
            Message::Save => {
                self.command(ID_SAVE);
                return;
            }
            // Beside a dock or a floating window, the click brings the user back to the main window.
            Message::Focus => self.ws.focus_pane(false),
            Message::Flushed | Message::CopyImage(_) | Message::Tab(_) | Message::Menu(_) => {}
        }
        self.refresh();
    }

    /// The main window's page sent the picture as it is now. Only the picture on screen counts: an echo for another one is
    /// ignored.
    fn image_changed(&mut self, edited: bool, content: String) {
        let main = self.ws.main_is_image().then(|| self.ws.tabs[self.ws.main_index()].id);
        if let (Some(id), true) = (main, main == self.img.shown) {
            self.picture_changed(id, edited, content);
        }
    }

    /// The page showing `tab` has this picture now. By tab, not the active one: the page need not be where the keyboard is.
    pub(super) fn picture_changed(&mut self, tab: u64, edited: bool, content: String) {
        let Some(t) = self.ws.tabs.iter_mut().find(|t| t.id == tab && t.kind == TabKind::Image) else { return };
        if t.content == content {
            return;
        }
        t.content = content;
        if edited {
            t.dirty = true;
        }
    }

    /// Ctrl+V with a picture on the clipboard (and no text): it opens in a new tab of its own. False when there is none,
    /// so the paste carries on as usual.
    pub(crate) fn paste_image(&mut self) -> bool {
        let Some((width, height, rgba)) = crate::services::clipboard::paste_image() else { return false };
        self.ws.new_image(String::new());
        self.img.pending = Some((width, height, notepp_core::textools::base64_encode(&rgba)));
        self.refresh();
        true
    }

    /// Show the picture page instead of the editor while a picture tab is active, and hand it the tab's picture when
    /// it has not got it yet.
    pub(crate) fn image_sync(&mut self) {
        let img = self.ws.main_is_image();
        if self.imgv.is_shown() != img {
            self.imgv.show(img);
            self.editor_pane.layout();
            if img {
                self.img.shown = None;
                if !self.imgv.is_loaded() {
                    self.imgv.load_html(ime::page_html());
                }
            }
        }
        if !img || !self.imgv.is_ready() {
            return;
        }
        let dark = self.ws.theme.dark;
        if self.imgv.sync_dark(dark) {
            ime::set_theme(&self.imgv, dark);
        }
        let main = self.ws.main_index();
        let id = self.ws.tabs[main].id;
        if self.img.shown != Some(id) {
            self.img.shown = Some(id);
            let content = self.ws.tabs[main].content.clone();
            match self.img.pending.take() {
                Some((w, h, pixels)) => ime::load_pixels(&self.imgv, w, h, &pixels),
                None if !content.is_empty() => ime::load(&self.imgv, &content),
                None => {}
            }
            if self.ws.current_float().is_none() {
                self.imgv.set_focus();
            }
        }
    }

    /// Give the keyboard to what the active tab shows: its picture or board page, or the editor.
    pub(crate) fn focus_shown(&self) {
        if self.ws.is_page() {
            self.current_page().set_focus();
        } else {
            self.stc().set_focus();
        }
    }
}
