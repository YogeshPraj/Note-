//! The live preview pane: a wxWebView showing the same page as the Win32 shell
//! (assets/preview.html), driven through `window.np.render(kind, payload)`.

use crate::widgets::web_pane::{WebPane, script};

const PAGE: &str = include_str!("../../../../assets/preview.html");
const MERMAID_JS: &str = include_str!("../../../../assets/mermaid.min.js");
const JSONEDITOR_JS: &str = include_str!("../../../../assets/jsoneditor.min.js");
const JSONEDITOR_CSS: &str = include_str!("../../../../assets/jsoneditor.min.css");
const JSONEDITOR_ICONS: &str = include_str!("../../../../assets/jsoneditor-icons.svg");
const JSON5_JS: &str = include_str!("../../../../assets/json5.min.js");
const HIGHLIGHT_JS: &str = include_str!("../../../../assets/highlight.min.js");
const KATEX_JS: &str = include_str!("../../../../assets/katex.min.js");

/// The page with remote images allowed only when the user asked for them.
pub fn page_html(remote_images: bool) -> String {
    const OPEN: &str = "img-src data: https: http:;";
    PAGE.replacen(OPEN, if remote_images { OPEN } else { "img-src data:;" }, 1)
}

/// Push content to the page. `kind` is "markdown" | "html" | "mermaid" | "json" | "message".
pub fn render(pane: &WebPane, kind: &str, payload: &str) {
    pane.call("window.np.render", &[kind, payload]);
}

pub fn call(pane: &WebPane, name: &str, payload: &str) {
    pane.call(&format!("window.np.{name}"), &[payload]);
}

/// Hand the page a diagram from an agent. It goes as data, never as script.
pub fn agent(pane: &WebPane, id: u64, op: &serde_json::Value) {
    pane.call("window.np.agentDiagram", &[&id.to_string(), op["source"].as_str().unwrap_or_default()]);
}

pub fn set_theme(pane: &WebPane, dark: bool) {
    pane.eval(&format!("window.np.setTheme({dark})"));
}

/// Inject Mermaid (once), then tell the page to draw the diagrams it was waiting on.
pub fn load_mermaid(pane: &WebPane) {
    if pane.first_time("mermaid") {
        pane.eval_big(MERMAID_JS);
    }
    pane.eval("window.np.mermaidLoaded()");
}

/// Inject the code highlighter (once), then tell the page to colour the code blocks it was waiting on.
pub fn load_highlighter(pane: &WebPane) {
    if pane.first_time("highlight") {
        pane.eval_big(HIGHLIGHT_JS);
    }
    pane.eval("window.np.highlightLoaded()");
}

/// Inject `KaTeX` (once), then tell the page to typeset the formulas it was waiting on.
pub fn load_katex(pane: &WebPane) {
    if pane.first_time("katex") {
        pane.eval_big(KATEX_JS);
    }
    pane.eval("window.np.katexLoaded()");
}

/// The editor's stylesheet with its icon sprite inlined (the page has no files to load it from).
fn jsoneditor_css() -> String {
    let icons = format!("data:image/svg+xml;base64,{}", base64(JSONEDITOR_ICONS.as_bytes()));
    JSONEDITOR_CSS.replace("./img/jsoneditor-icons.svg", &icons).replace("img/jsoneditor-icons.svg", &icons)
}

/// Inject the JSON editor (once), then tell the page to draw the document it was waiting on.
pub fn load_json_editor(pane: &WebPane) {
    if pane.first_time("jsoneditor") {
        pane.eval(&format!(
            "(function(){{var s=document.createElement('style');s.textContent={};document.head.appendChild(s);}})();",
            script::js_string(&jsoneditor_css())
        ));
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

/// A picture sent by the page: `copy-image:<width>:<height>:<base64 RGBA pixels>`.
pub fn parse_image(msg: &str) -> Option<(u32, u32, Vec<u8>)> {
    let mut parts = msg.strip_prefix("copy-image:")?.splitn(3, ':');
    let width: u32 = parts.next()?.parse().ok()?;
    let height: u32 = parts.next()?.parse().ok()?;
    let pixels = notepp_core::textools::base64_decode(parts.next()?).ok()?;
    let expected = usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?.checked_mul(4)?;
    (width > 0 && height > 0 && pixels.len() == expected).then_some((width, height, pixels))
}

#[cfg(test)]
mod tests {
    use super::parse_image;

    #[test]
    fn a_picture_needs_four_bytes_for_each_pixel() {
        let rgba = notepp_core::textools::base64_encode(&[1, 2, 3, 255, 4, 5, 6, 255]);
        assert_eq!(parse_image(&format!("copy-image:2:1:{rgba}")), Some((2, 1, vec![1, 2, 3, 255, 4, 5, 6, 255])));
        assert_eq!(parse_image(&format!("copy-image:3:1:{rgba}")), None);
        assert_eq!(parse_image("copy-image:0:0:"), None);
        assert_eq!(parse_image("copy-image:x:1:AAAA"), None);
        assert_eq!(parse_image("tab:next"), None);
    }
}
