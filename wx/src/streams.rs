//! Log Follow, the HTTP scratchpad and Compare: previews that are fed by something other than the document text.

use std::path::PathBuf;
use std::sync::Mutex;

use notepp_core::commands as c;
use notepp_core::logfollow::{self, Follower};
use notepp_core::{http, sci};
use notepp_core::preview as pcore;
use wxdragon::prelude::*;
use wxdragon::widgets::webview::WebView;

use crate::app::App;
use crate::preview;

static HTTP_RESULT: Mutex<Option<(isize, String)>> = Mutex::new(None);

impl App {
    /// Show the log or the last HTTP response in the preview pane.
    pub fn stream_preview(&mut self, kind: pcore::Kind, force: bool) {
        let view = self.view;
        match kind {
            pcore::Kind::Log => {
                let (id, path) = (self.ws.tabs[self.ws.active].id, self.ws.tabs[self.ws.active].path.clone());
                if force || self.log_shown != Some(id) {
                    self.log_start(id, path);
                }
            }
            _ => {
                let doc = self.ws.tabs[self.ws.active].doc;
                // Typing in the request must not rebuild the page and lose the tab you were reading.
                if force || self.http_shown != Some(doc) {
                    match &self.http_last {
                        Some((d, json)) if *d == doc => preview::render(&view, "http", json),
                        _ => preview::render(&view, "message", "Press Ctrl+Enter to send the request under the cursor."),
                    }
                    self.http_shown = Some(doc);
                }
            }
        }
    }

    /// Begin following `path` in the preview pane, showing the end of the file.
    fn log_start(&mut self, id: u64, path: Option<PathBuf>) {
        let view = self.view;
        self.log = None;
        self.log_shown = Some(id);
        let Some(p) = path else {
            preview::render(&view, "message", "Save the file to follow it.");
            return;
        };
        match Follower::open(&p, logfollow::TAIL_BYTES) {
            Ok((f, u)) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                preview::render(&view, "log", &serde_json::json!({ "name": name, "lines": u.lines, "skipped": u.skipped_bytes }).to_string());
                self.log = Some((id, f));
            }
            Err(e) => preview::render(&view, "message", &format!("Cannot read {}: {e}", p.display())),
        }
    }

    /// Timer: send what was added to the followed log, and show a finished HTTP response.
    pub fn stream_tick(&mut self) {
        let view = self.view;
        if let Some((doc, json)) = HTTP_RESULT.lock().unwrap_or_else(|e| e.into_inner()).take() {
            self.http_busy = false;
            self.http_last = Some((doc, json.clone()));
            let showing = self.ws.tabs.get(self.ws.active).is_some_and(|t| t.doc == doc);
            if showing && self.preview_on && self.ws.preview_kind() == Some(pcore::Kind::Http) {
                preview::render(&view, "http", &json);
                self.http_shown = Some(doc);
            }
        }
        let showing = self.preview_on && self.ws.preview_kind() == Some(pcore::Kind::Log);
        if !showing {
            self.log = None;
            self.log_shown = None;
            return;
        }
        let id = self.ws.tabs[self.ws.active].id;
        let Some((lid, f)) = self.log.as_mut() else { return };
        if *lid != id {
            return;
        }
        // A file that is deleted or locked for a moment keeps what is shown; the next tick tries again.
        if let Ok(u) = f.poll() {
            if u.reset || !u.lines.is_empty() {
                preview::call(&view, "logAppend", &serde_json::json!({ "lines": u.lines, "reset": u.reset }).to_string());
            }
        }
    }

    /// Send the request under the cursor (HTTP scratchpad, Ctrl+Enter in a .http / .rest file).
    pub fn http_send(&mut self) {
        if self.ws.preview_kind() != Some(pcore::Kind::Http) {
            self.flash = Some("Open a .http or .rest file to send requests".into());
            return;
        }
        if self.http_busy {
            self.flash = Some("A request is already running".into());
            return;
        }
        let text = sci::get_text(&self.ws.sci);
        let line = self.ws.caret_line();
        let Some(req) = http::request_at(&text, line) else {
            self.flash = Some("No request found at the cursor".into());
            return;
        };
        if !self.preview_on {
            self.set_preview(true);
        }
        let doc = self.ws.tabs[self.ws.active].doc;
        let pending = http::pending_json(&req);
        let view = self.view;
        preview::render(&view, "http", &pending);
        self.http_last = Some((doc, pending));
        self.http_shown = Some(doc);
        self.http_busy = true;
        std::thread::spawn(move || {
            // {{vault:...}} is looked up here, at send time, and never leaves this thread.
            let json = match http::send(&req, &mut |r| notepp_core::bitwarden::resolve_reference(r).ok()) {
                Ok(res) => http::response_json(&req, &res),
                Err(e) => http::error_json(&req, &e),
            };
            *HTTP_RESULT.lock().unwrap_or_else(|e| e.into_inner()) = Some((doc, json));
        });
    }

    /// Compare the active document with `right` in a new window.
    fn show_compare(&mut self, right_name: &str, right: &str) {
        let name = self.ws.tabs[self.ws.active].title.clone();
        let left = sci::get_text(&self.ws.sci);
        let html = notepp_core::compare::diff_html(&name, &left, right_name, right, self.ws.theme.dark);
        let f = Frame::builder().with_title(&format!("Compare - {name} / {right_name}")).with_size(Size::new(1100, 700)).build();
        let view = WebView::builder(&f).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&view, 1, SizerFlag::Expand, 0);
        f.set_sizer(sizer, true);
        view.set_page(&html, "about:blank");
        f.centre();
        f.show(true);
    }

    fn compare_with_clipboard(&mut self) {
        if self.ws.is_whiteboard() {
            return;
        }
        match Clipboard::get().get_text() {
            Some(t) if !t.is_empty() => self.show_compare("Clipboard", &t),
            _ => self.flash = Some("The clipboard has no text to compare".into()),
        }
    }

    fn compare_with_file(&mut self) {
        if self.ws.is_whiteboard() {
            return;
        }
        use notepp_core::workspace::Ui as _;
        let Some(p) = self.ui.pick_open().into_iter().next() else { return };
        match std::fs::read(&p) {
            Ok(bytes) => {
                let (text, _, _) = notepp_core::doc::decode(&bytes);
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                self.show_compare(&name, &text);
            }
            Err(e) => self.ui.error(&format!("Cannot read {}\n\n{e}", p.display())),
        }
    }

    /// Print the active document: its text as a page in a hidden web view, through the system's print dialog.
    fn print_document(&mut self) {
        if self.ws.is_whiteboard() {
            self.flash = Some("Not available on a whiteboard".into());
            return;
        }
        let title = self.ws.tabs[self.ws.active].title.clone();
        let text = sci::get_text(&self.ws.sci);
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let font = self.ws.settings.font.clone();
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>@page{{margin:15mm}}body{{margin:0}}pre{{font-family:'{}',Consolas,monospace;font-size:10pt;white-space:pre-wrap;word-break:break-all;tab-size:4}}</style></head><body><pre>{}</pre></body></html>",
            esc(&title),
            esc(&font),
            esc(&text)
        );
        let f = Frame::builder().with_title(&format!("Print - {title}")).with_size(Size::new(820, 700)).build();
        let view = WebView::builder(&f).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&view, 1, SizerFlag::Expand, 0);
        f.set_sizer(sizer, true);
        {
            use wxdragon::event::WebViewEvents;
            let (v, fr) = (view, f);
            view.on_loaded(move |_| {
                v.print();
                fr.destroy();
            });
        }
        view.set_page(&html, "about:blank");
        f.show(true);
    }
    /// Commands of these features; true when `id` was one of them.
    pub fn stream_cmd(&mut self, id: usize) -> bool {
        match id {
            c::ID_HTTP_SEND => self.http_send(),
            c::ID_LOG_FOLLOW => {
                if self.ws.preview_kind() != Some(pcore::Kind::Log) {
                    self.flash = Some("Log Follow works on .log, .out and .err files".into());
                } else {
                    let on = !self.preview_on;
                    self.set_preview(on);
                }
            }
            c::ID_PRINT | c::ID_PRINT_NOW => self.print_document(),
            c::ID_PAGE_SETUP => self.flash = Some("Page margins are set in the print dialog".into()),
            c::ID_TAB_CMP_CLIP => self.compare_with_clipboard(),
            c::ID_TAB_CMP_FILE => self.compare_with_file(),
            _ => return false,
        }
        true
    }
}
