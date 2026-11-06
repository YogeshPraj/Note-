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
  if (d && typeof d === 'object' && (d.type === 'wb-ready' || d.type === 'wb-state' || d.type === 'wb-data' || d.type === 'wb-save-request' || d.type === 'wb-agent-done')) {
    window.ipc.postMessage(JSON.stringify(d));
  }
});
// The toolkit does not hear of a click in a page, and the app has to know which pane the user is in.
window.addEventListener('pointerdown', function () {
  window.ipc.postMessage(JSON.stringify({ type: 'wb-focus' }));
}, true);
// While the page has the keyboard the window never sees keys, so the window's own shortcuts are passed on:
// Ctrl+Tab and Ctrl+Shift+Tab (next and previous tab), Alt+letter (a menu) and F10 (the first menu).
window.addEventListener('keydown', function (e) {
  if (e.key === 'Tab' && e.ctrlKey && !e.altKey) {
    e.preventDefault();
    e.stopPropagation();
    window.ipc.postMessage(JSON.stringify({ type: 'wb-tab', delta: e.shiftKey ? -1 : 1 }));
  } else if ((e.altKey && !e.ctrlKey && /^[a-z0-9]$/i.test(e.key)) || (e.key === 'F10' && !e.altKey && !e.ctrlKey && !e.shiftKey)) {
    e.preventDefault();
    e.stopPropagation();
    window.ipc.postMessage(JSON.stringify({ type: 'wb-menu', key: e.key === 'F10' ? '' : e.key }));
  }
}, true);
";

/// A page's answer to an agent's request.
#[derive(Debug, PartialEq)]
pub struct AgentDone {
    pub id: u64,
    pub result: serde_json::Value,
    pub error: Option<String>,
    /// The scene as it is now.
    pub scene: Option<String>,
}

#[derive(Debug, PartialEq)]
pub enum Event {
    Ready,
    State(String),
    Data(String),
    SaveRequest,
    /// The user clicked in the page.
    Focus,
    /// Ctrl+Tab (1) or Ctrl+Shift+Tab (-1) pressed inside the page.
    Tab(i32),
    /// Alt+letter pressed inside the page: the letter, or empty for F10.
    Menu(String),
    AgentDone(AgentDone),
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
    let in_tree = cfg!(debug_assertions).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..").join("assets").join("whiteboard"));
    [beside_exe, in_tree].into_iter().flatten().find(|d| d.join("whiteboard.html").exists())
}

pub fn page_url() -> Option<String> {
    let dir = asset_dir()?.canonicalize().ok()?;
    let p = dir.join("whiteboard.html").to_string_lossy().replace('\\', "/");
    // "file:///" plus a path that already starts with "/" (macOS, Linux) must not become four slashes.
    let p = p.trim_start_matches("//?/");
    let p = if cfg!(windows) { p } else { p.trim_start_matches('/') };
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
        "wb-focus" => Event::Focus,
        "wb-agent-done" => Event::AgentDone(AgentDone {
            id: v.get("id")?.as_u64()?,
            result: v.get("result").cloned().unwrap_or_default(),
            error: v.get("error").and_then(|e| e.as_str()).map(str::to_string),
            scene: v.get("scene").and_then(|s| s.as_str()).map(str::to_string),
        }),
        "wb-menu" => Event::Menu(v.get("key").and_then(|k| k.as_str()).unwrap_or("").to_string()),
        "wb-tab" => Event::Tab(if v.get("delta").and_then(serde_json::Value::as_i64) == Some(-1) { -1 } else { 1 }),
        _ => return None,
    })
}

pub fn load(pane: &WebPane, scene: &str) {
    pane.eval(&format!("window.postMessage({{type:'wb-load',content:{}}},'*')", script::js_string(scene)));
}

/// Hand the page an agent's request. It is parsed in the page, so that nothing in it can be taken for script.
pub fn agent(pane: &WebPane, id: u64, op: &serde_json::Value) {
    pane.eval(&format!("window.postMessage({{type:'wb-agent',id:{id},op:JSON.parse({})}},'*')", script::js_string(&op.to_string())));
}

pub fn set_theme(pane: &WebPane, dark: bool) {
    pane.eval(&format!("window.postMessage({{type:'wb-theme',dark:{dark}}},'*')"));
    pane.eval(&format!("document.body.classList.toggle('dark',{dark})"));
}

pub fn request_data(pane: &WebPane) {
    pane.eval("window.postMessage({type:'wb-get-data'},'*')");
}
