//! The live preview pane: a wxWebView showing the same page as the Win32 shell
//! (assets/preview.html), driven through `window.np.render(kind, payload)`.

use std::cell::Cell;

use crate::widgets::web_pane::{WebPane, script};

const PAGE: &str = include_str!("../../../../assets/preview.html");
const MERMAID_JS: &str = include_str!("../../../../assets/mermaid.min.js");
const JSONEDITOR_JS: &str = include_str!("../../../../assets/jsoneditor.min.js");
const JSONEDITOR_CSS: &str = include_str!("../../../../assets/jsoneditor.min.css");
const JSONEDITOR_ICONS: &str = include_str!("../../../../assets/jsoneditor-icons.svg");
const JSON5_JS: &str = include_str!("../../../../assets/json5.min.js");

thread_local! {
    static MERMAID: Cell<bool> = const { Cell::new(false) };
    static JSON_EDITOR: Cell<bool> = const { Cell::new(false) };
}

/// The page with remote images allowed only when the user asked for them.
pub fn page_html(remote_images: bool) -> String {
    const OPEN: &str = "img-src data: https: http:;";
    PAGE.replacen(OPEN, if remote_images { OPEN } else { "img-src data:;" }, 1)
}

/// Run `js` in the page (kept until the page is ready).
pub fn eval(pane: &WebPane, js: &str) {
    pane.eval(js);
}

/// Push content to the page. `kind` is "markdown" | "html" | "mermaid" | "json" | "message".
pub fn render(pane: &WebPane, kind: &str, payload: &str) {
    pane.call("window.np.render", &[kind, payload]);
}

pub fn call(pane: &WebPane, name: &str, payload: &str) {
    pane.call(&format!("window.np.{name}"), &[payload]);
}

pub fn set_theme(pane: &WebPane, dark: bool) {
    pane.eval(&format!("window.np.setTheme({dark})"));
}
/// Scroll the rendered Markdown so source line `line` (0-based, fractional) is at the top.
pub fn scroll_to_line(pane: &WebPane, line: f64, total_lines: usize) {
    pane.eval(&format!("window.np.scrollToLine({line:.3},{total_lines})"));
}

/// Inject Mermaid (once), then tell the page to draw the diagrams it was waiting on.
pub fn load_mermaid(pane: &WebPane) {
    if !MERMAID.with(|c| c.replace(true)) {
        pane.eval_big(MERMAID_JS);
    }
    pane.eval("window.np.mermaidLoaded()");
}

/// The editor's stylesheet with its icon sprite inlined (the page has no files to load it from).
fn jsoneditor_css() -> String {
    let icons = format!("data:image/svg+xml;base64,{}", base64(JSONEDITOR_ICONS.as_bytes()));
    JSONEDITOR_CSS.replace("./img/jsoneditor-icons.svg", &icons).replace("img/jsoneditor-icons.svg", &icons)
}

/// Inject the JSON editor (once), then tell the page to draw the document it was waiting on.
pub fn load_json_editor(pane: &WebPane) {
    if !JSON_EDITOR.with(|c| c.replace(true)) {
        pane.eval(&format!("(function(){{var s=document.createElement('style');s.textContent={};document.head.appendChild(s);}})();", script::js_string(&jsoneditor_css())));
        pane.eval_big(JSON5_JS);
        pane.eval_big(JSONEDITOR_JS);
    }
    pane.eval("window.np.jsonLoaded()");
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = u32::from(c[0]) << 16 | u32::from(*c.get(1).unwrap_or(&0)) << 8 | u32::from(*c.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}
