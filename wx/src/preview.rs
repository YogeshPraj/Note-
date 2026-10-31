//! The live preview pane: a wxWebView showing the same page as the Win32 shell
//! (assets/preview.html), driven through `window.np.render(kind, payload)`.

use std::cell::Cell;

use wxdragon::prelude::*;
use wxdragon::widgets::webview::WebView;

const PAGE: &str = include_str!("../../assets/preview.html");
const MERMAID_JS: &str = include_str!("../../assets/mermaid.min.js");
const JSONEDITOR_JS: &str = include_str!("../../assets/jsoneditor.min.js");
const JSONEDITOR_CSS: &str = include_str!("../../assets/jsoneditor.min.css");
const JSONEDITOR_ICONS: &str = include_str!("../../assets/jsoneditor-icons.svg");
const JSON5_JS: &str = include_str!("../../assets/json5.min.js");

thread_local! {
    static MERMAID: Cell<bool> = const { Cell::new(false) };
    static JSON_EDITOR: Cell<bool> = const { Cell::new(false) };
}

/// The page with remote images allowed only when the user asked for them.
pub fn page_html(remote_images: bool) -> String {
    const OPEN: &str = "img-src data: https: http:;";
    PAGE.replacen(OPEN, if remote_images { OPEN } else { "img-src data:;" }, 1)
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

pub fn eval(view: &WebView, js: &str) {
    let _ = view.run_script(js);
}

/// Large libraries are handed over in pieces, then evaluated once in the page's global scope.
fn eval_big(view: &WebView, js: &str) {
    eval(view, "window.__big=[]");
    let mut start = 0;
    while start < js.len() {
        let mut end = (start + 200_000).min(js.len());
        while !js.is_char_boundary(end) {
            end += 1;
        }
        eval(view, &format!("window.__big.push({})", js_string(&js[start..end])));
        start = end;
    }
    eval(view, "(function(){var s=document.createElement('script');s.textContent=window.__big.join('');document.head.appendChild(s);window.__big=null})()");
}

/// Push content to the page. `kind` is "markdown" | "html" | "mermaid" | "json" | "message".
pub fn render(view: &WebView, kind: &str, payload: &str) {
    eval(view, &format!("window.np.render({},{})", js_string(kind), js_string(payload)));
}

pub fn set_theme(view: &WebView, dark: bool) {
    eval(view, &format!("window.np.setTheme({dark})"));
}

/// Scroll the rendered Markdown so source line `line` (0-based, fractional) is at the top.
pub fn scroll_to_line(view: &WebView, line: f64, total_lines: usize) {
    eval(view, &format!("window.np.scrollToLine({line:.3},{total_lines})"));
}

/// Inject Mermaid (once), then tell the page to draw the diagrams it was waiting on.
pub fn load_mermaid(view: &WebView) {
    if !MERMAID.with(|c| c.replace(true)) {
        eval_big(view, MERMAID_JS);
    }
    eval(view, "window.np.mermaidLoaded()");
}

/// The editor's stylesheet with its icon sprite inlined (the page has no files to load it from).
fn jsoneditor_css() -> String {
    let icons = format!("data:image/svg+xml;base64,{}", base64(JSONEDITOR_ICONS.as_bytes()));
    JSONEDITOR_CSS.replace("./img/jsoneditor-icons.svg", &icons).replace("img/jsoneditor-icons.svg", &icons)
}

/// Inject the JSON editor (once), then tell the page to draw the document it was waiting on.
pub fn load_json_editor(view: &WebView) {
    if !JSON_EDITOR.with(|c| c.replace(true)) {
        eval(
            view,
            &format!("(function(){{var s=document.createElement('style');s.textContent={};document.head.appendChild(s);}})();", js_string(&jsoneditor_css())),
        );
        eval_big(view, JSON5_JS);
        eval_big(view, JSONEDITOR_JS);
    }
    eval(view, "window.np.jsonLoaded()");
}
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
