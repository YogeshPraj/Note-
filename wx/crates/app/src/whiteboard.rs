//! The whiteboard: the Excalidraw page in a `WebView`, speaking the original `postMessage` protocol
//! (`wb-load`, `wb-theme`, `wb-get-data` in; `wb-ready`, `wb-state`, `wb-data`, `wb-save-request` out).
//!
//! The page is top-level here rather than in an iframe, so a small script forwards its outgoing
//! messages to the shell over `window.ipc`.

use std::path::{Path, PathBuf};

use crate::widgets::web_pane::{WebPane, script};

/// Forward the page's outgoing protocol messages to the shell.
pub const BRIDGE: &str = r"
window.addEventListener('message', function (e) {
  var d = e.data;
  if (d && typeof d === 'object' && (d.type === 'wb-ready' || d.type === 'wb-state' || d.type === 'wb-data' || d.type === 'wb-save-request')) {
    window.ipc.postMessage(JSON.stringify(d));
  }
});
";

#[derive(Debug, PartialEq)]
pub enum Event {
    Ready,
    State(String),
    Data(String),
    SaveRequest,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pending {
    Save,
    SaveAs,
    SaveCopy,
}

/// Where the bundled page lives: next to the exe (installed) or in the source tree (dev builds only).
pub fn asset_dir() -> Option<PathBuf> {
    let beside_exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("whiteboard")));
    let in_tree = cfg!(debug_assertions).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("assets").join("whiteboard"));
    [beside_exe, in_tree].into_iter().flatten().find(|d| d.join("whiteboard.html").exists())
}

pub fn page_url() -> Option<String> {
    let dir = asset_dir()?.canonicalize().ok()?;
    let p = dir.join("whiteboard.html").to_string_lossy().replace('\\', "/");
    let p = p.trim_start_matches("//?/");
    Some(format!("file:///{p}"))
}

pub fn parse(msg: &str) -> Option<Event> {
    let v: serde_json::Value = serde_json::from_str(msg).ok()?;
    let content = || v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
    Some(match v.get("type")?.as_str()? {
        "wb-ready" => Event::Ready,
        "wb-state" => Event::State(content()),
        "wb-data" => Event::Data(content()),
        "wb-save-request" => Event::SaveRequest,
        _ => return None,
    })
}

pub fn load(pane: &WebPane, scene: &str) {
    pane.eval(&format!("window.postMessage({{type:'wb-load',content:{}}},'*')", script::js_string(scene)));
}

pub fn set_theme(pane: &WebPane, dark: bool) {
    pane.eval(&format!("window.postMessage({{type:'wb-theme',dark:{dark}}},'*')"));
    pane.eval(&format!("document.body.classList.toggle('dark',{dark})"));
}

pub fn request_data(pane: &WebPane) {
    pane.eval("window.postMessage({type:'wb-get-data'},'*')");
}
