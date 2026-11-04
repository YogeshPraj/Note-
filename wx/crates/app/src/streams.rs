//! Log Follow, the HTTP scratchpad and Compare: previews that are fed by something other than the document text.

use std::path::PathBuf;

use notepp_core::commands as c;
use notepp_core::logfollow::{self, Follower};
use notepp_core::preview as pcore;
use notepp_core::{http, sci};
use wxdragon::prelude::*;
use wxdragon::widgets::webview::WebView;

use crate::app::App;
use crate::command::CommandHandler;
use crate::preview;

impl App {
    /// Show the log or the last HTTP response in the preview pane.
    pub fn stream_preview(&mut self, kind: pcore::Kind, force: bool) {
        let view = self.view.clone();
        if kind == pcore::Kind::Log {
            let (id, path) = (self.ws.tabs[self.ws.active].id, self.ws.tabs[self.ws.active].path.clone());
            if force || self.streams.log_shown != Some(id) {
                self.log_start(id, path);
            }
        } else {
            let doc = self.ws.tabs[self.ws.active].doc;
            // Typing in the request must not rebuild the page and lose the tab you were reading.
            if force || self.streams.http_shown != Some(doc) {
                match &self.streams.http_last {
                    Some((d, json)) if *d == doc => preview::render(&view, "http", json),
                    _ => preview::render(&view, "message", "Press Ctrl+Enter to send the request under the cursor."),
                }
                self.streams.http_shown = Some(doc);
            }
        }
    }

    /// Begin following `path` in the preview pane, showing the end of the file.
    fn log_start(&mut self, id: u64, path: Option<PathBuf>) {
        let view = self.view.clone();
        self.streams.log = None;
        self.streams.log_shown = Some(id);
        let Some(p) = path else {
            preview::render(&view, "message", "Save the file to follow it.");
            return;
        };
        match Follower::open(&p, logfollow::TAIL_BYTES) {
            Ok((f, u)) => {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                preview::render(&view, "log", &serde_json::json!({ "name": name, "lines": u.lines, "skipped": u.skipped_bytes }).to_string());
                self.streams.log = Some((id, f));
            }
            Err(e) => preview::render(&view, "message", &format!("Cannot read {}: {e}", p.display())),
        }
    }

    /// Timer: send what was added to the followed log, and show a finished HTTP response.
    pub fn stream_tick(&mut self) {
        let view = self.view.clone();
        if let Some((doc, json)) = self.streams.http_result.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take() {
            self.streams.http_busy = false;
            self.streams.http_last = Some((doc, json.clone()));
            let showing = self.ws.tabs.get(self.ws.active).is_some_and(|t| t.doc == doc);
            if showing && self.preview.on && self.ws.preview_kind() == Some(pcore::Kind::Http) {
                preview::render(&view, "http", &json);
                self.streams.http_shown = Some(doc);
            }
        }
        let showing = self.preview.on && self.ws.preview_kind() == Some(pcore::Kind::Log);
        if !showing {
            self.streams.log = None;
            self.streams.log_shown = None;
            return;
        }
        let id = self.ws.tabs[self.ws.active].id;
        let Some((lid, f)) = self.streams.log.as_mut() else { return };
        if *lid != id {
            return;
        }
        // A file that is deleted or locked for a moment keeps what is shown; the next tick tries again.
        if let Ok(u) = f.poll()
            && (u.reset || !u.lines.is_empty())
        {
            preview::call(&view, "logAppend", &serde_json::json!({ "lines": u.lines, "reset": u.reset }).to_string());
        }
    }

    /// Send the request under the cursor (HTTP scratchpad, Ctrl+Enter in a .http / .rest file).
    pub fn http_send(&mut self) {
        if self.ws.preview_kind() != Some(pcore::Kind::Http) {
            self.flash = Some("Open a .http or .rest file to send requests".into());
            return;
        }
        if self.streams.http_busy {
            self.flash = Some("A request is already running".into());
            return;
        }
        let text = sci::get_text(&self.ws.sci);
        let line = self.ws.caret_line();
        let Some(req) = http::request_at(&text, line) else {
            self.flash = Some("No request found at the cursor".into());
            return;
        };
        if !self.vault_send_agreed(&req) {
            return;
        }
        if !self.preview.on {
            self.set_preview(true);
        }
        let doc = self.ws.tabs[self.ws.active].doc;
        let pending = http::pending_json(&req);
        let view = self.view.clone();
        preview::render(&view, "http", &pending);
        self.streams.http_last = Some((doc, pending));
        self.streams.http_shown = Some(doc);
        self.streams.http_busy = true;
        let slot = self.streams.http_result.clone();
        std::thread::spawn(move || {
            // {{vault:...}} is looked up here, at send time, and never leaves this thread.
            let json = match http::send(&req, &mut |r| notepp_core::bitwarden::resolve_reference(r).ok()) {
                Ok(res) => http::response_json(&req, &res),
                Err(e) => http::error_json(&req, &e.to_string()),
            };
            *slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((doc, json));
        });
    }

    /// Ask before a request sends vault values somewhere for the first time: its file can come from anyone.
    fn vault_send_agreed(&mut self, req: &http::Request) -> bool {
        let Some(consent) = http::vault_consent(req) else { return true };
        if self.streams.vault_origins.contains(&consent.origin) {
            return true;
        }
        let yes = crate::widgets::ask::yes_no(&self.frame, "Send vault values?", &consent.question, true);
        if yes {
            self.streams.vault_origins.insert(consent.origin);
        }
        yes
    }

    /// Compare the active document with `right` in a new window.
    fn show_compare(&mut self, right_name: &str, right: &str) {
        let name = self.ws.tabs[self.ws.active].title.clone();
        let left = sci::get_text(&self.ws.sci);
        let html = notepp_core::compare::diff_html(&name, &left, right_name, right, self.ws.theme.dark);
        let f = Frame::builder()
            .with_title(&format!("Compare - {name} / {right_name}"))
            .with_size(Size::new(crate::widgets::theme::current().px(1100), crate::widgets::theme::current().px(700)))
            .build();
        let view = WebView::builder(&f).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&view, 1, SizerFlag::Expand, 0);
        f.set_sizer(sizer, true);
        view.set_page(&html, "about:blank");
        f.centre();
        f.show(true);
    }

    fn compare_with_clipboard(&mut self) {
        if self.ws.is_page() {
            return;
        }
        match crate::services::clipboard::paste() {
            Some(t) if !t.is_empty() => self.show_compare("Clipboard", &t),
            _ => self.flash = Some("The clipboard has no text to compare".into()),
        }
    }

    fn compare_with_file(&mut self) {
        if self.ws.is_page() {
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

    /// Print the active document: its text as a page in a web view that opens the print panel itself. The window is the
    /// print preview and the user closes it. (A loaded-event handler on this short-lived web view crashed the app when
    /// the window closed, so the page calls `window.print()` on its own, which is all the toolkit's print does.)
    fn print_document(&mut self) {
        if self.ws.is_page() {
            self.flash = Some("Not available on a whiteboard".into());
            return;
        }
        let title = self.ws.tabs[self.ws.active].title.clone();
        let text = sci::get_text(&self.ws.sci);
        let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
        let font = self.ws.settings.font.clone();
        let html = format!(
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>@page{{margin:15mm}}body{{margin:0}}pre{{font-family:'{}',Consolas,monospace;font-size:10pt;white-space:pre-wrap;word-break:break-all;tab-size:4}}</style></head><body onload=\"window.print()\"><pre>{}</pre></body></html>",
            esc(&title),
            esc(&font),
            esc(&text)
        );
        let f = Frame::builder()
            .with_title(&format!("Print - {title}"))
            .with_size(Size::new(crate::widgets::theme::current().px(820), crate::widgets::theme::current().px(700)))
            .build();
        let view = WebView::builder(&f).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        sizer.add(&view, 1, SizerFlag::Expand, 0);
        f.set_sizer(sizer, true);
        // Printed pages carry the page's address in their footer: a file's name reads better than a data URL. A protected
        // document's text never goes to disk, so it keeps the in-memory page.
        match print_page_file(&title, &html).filter(|_| !self.ws.is_encrypted()) {
            Some(url) => view.load_url(&url),
            None => view.set_page(&html, "about:blank"),
        }
        f.show(true);
    }
}

/// Write the page to be printed to a file in the temp folder named after the document; its `file:` URL.
fn print_page_file(title: &str, html: &str) -> Option<String> {
    let dir = std::env::temp_dir().join("notepp-print");
    std::fs::create_dir_all(&dir).ok()?;
    let name: String = title.chars().map(|c| if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') { c } else { '_' }).collect();
    let path = dir.join(format!("{name}.html"));
    std::fs::write(&path, html).ok()?;
    let p = path.to_string_lossy().replace('\\', "/");
    Some(format!("file:///{}", p.trim_start_matches('/')))
}

/// Log Follow, the HTTP scratchpad, compare and print.
pub struct StreamCommands;

impl CommandHandler<App> for StreamCommands {
    fn name(&self) -> &'static str {
        "streams"
    }

    fn handle(&self, app: &mut App, id: usize) -> bool {
        match id {
            c::ID_HTTP_SEND => app.http_send(),
            c::ID_LOG_FOLLOW => {
                if app.ws.preview_kind() == Some(pcore::Kind::Log) {
                    let on = !app.preview.on;
                    app.set_preview(on);
                } else {
                    app.flash = Some("Log Follow works on .log, .out and .err files".into());
                }
            }
            c::ID_PRINT | c::ID_PRINT_NOW => app.print_document(),
            c::ID_PAGE_SETUP => app.flash = Some("Page margins are set in the print dialog".into()),
            c::ID_TAB_CMP_CLIP => app.compare_with_clipboard(),
            c::ID_TAB_CMP_FILE => app.compare_with_file(),
            _ => return false,
        }
        true
    }
}
