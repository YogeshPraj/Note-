//! The whiteboard: Excalidraw running in its own WebView2 child window.
//!
//! The page is the same bundle the Electron app uses (`assets/whiteboard/`),
//! served over a custom `http://wb.localhost/` protocol so its relative font and
//! asset URLs resolve. It speaks the original `postMessage` protocol
//! (`wb-load`, `wb-theme`, `wb-get-data` in; `wb-ready`, `wb-state`, `wb-data`,
//! `wb-save-request` out). Because the page is top-level here rather than in an
//! iframe, `window.parent === window`; a small init script forwards the
//! outgoing messages to the host over `window.ipc`.
//!
//! Incoming events are queued (they can be megabytes of JSON) and the host is
//! poked with `WM_WB`; it drains the queue with `take_events()`.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::num::NonZeroIsize;
use std::path::{Path, PathBuf};

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::http::{Request, Response};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

pub const WM_WB: u32 = WM_APP + 70;

#[derive(Debug, PartialEq)]
pub enum Event {
    /// Excalidraw mounted and can receive a scene.
    Ready,
    /// The scene changed (debounced by the page); carries the envelope JSON.
    State(String),
    /// Reply to `request_data`.
    Data(String),
    /// Ctrl+S pressed inside the page.
    SaveRequest,
}

/// Forward the page's outgoing protocol messages to the host.
const BRIDGE: &str = r#"
window.addEventListener('message', function (e) {
  var d = e.data;
  if (d && typeof d === 'object' && (d.type === 'wb-ready' || d.type === 'wb-state' || d.type === 'wb-data' || d.type === 'wb-save-request')) {
    window.ipc.postMessage(JSON.stringify(d));
  }
});
"#;

struct Parent(isize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

thread_local! {
    static CTX: RefCell<Option<WebContext>> = const { RefCell::new(None) };
    static VIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    static EVENTS: RefCell<VecDeque<Event>> = const { RefCell::new(VecDeque::new()) };
}

pub fn is_created() -> bool {
    VIEW.with(|v| v.borrow().is_some())
}

/// Where the bundled page lives: next to the exe (installed) or in the source tree (dev).
pub fn asset_dir() -> Option<PathBuf> {
    let beside_exe = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("whiteboard")));
    let in_tree = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("whiteboard");
    [beside_exe, Some(in_tree)].into_iter().flatten().find(|d| d.join("whiteboard.html").exists())
}

pub fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => "application/octet-stream",
    }
}

/// Resolve a request path under `root`, refusing anything that escapes it.
pub fn resolve(root: &Path, request_path: &str) -> Option<PathBuf> {
    let rel = request_path.trim_start_matches('/');
    if rel.split(['/', '\\']).any(|seg| seg == ".." || seg.contains(':')) {
        return None;
    }
    let p = root.join(rel);
    p.is_file().then_some(p)
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect { position: PhysicalPosition::new(x, y).into(), size: PhysicalSize::new(w.max(1) as u32, h.max(1) as u32).into() }
}

/// Create the WebView2 child of `host` (once).
pub fn ensure(host: HWND, data_dir: &Path) -> Result<(), String> {
    if is_created() {
        return Ok(());
    }
    let root = asset_dir().ok_or("whiteboard assets not found (assets/whiteboard)")?;
    let _ = std::fs::create_dir_all(data_dir);
    let host_raw = host.0 as isize;
    let mut ctx = WebContext::new(Some(data_dir.to_path_buf()));
    let view = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_initialization_script(BRIDGE)
        .with_custom_protocol("wb".to_string(), move |_id, req: Request<Vec<u8>>| {
            let path = req.uri().path().to_string();
            match resolve(&root, &path) {
                Some(file) => match std::fs::read(&file) {
                    Ok(bytes) => Response::builder()
                        .status(200)
                        .header("Content-Type", mime_for(&path))
                        .header("Access-Control-Allow-Origin", "*")
                        .body(Cow::Owned(bytes))
                        .unwrap(),
                    Err(_) => Response::builder().status(500).body(Cow::Borrowed(&b""[..])).unwrap(),
                },
                None => Response::builder().status(404).body(Cow::Borrowed(&b"not found"[..])).unwrap(),
            }
        })
        .with_url("http://wb.localhost/whiteboard.html")
        .with_bounds(rect(0, 0, 400, 300))
        .with_ipc_handler(move |req| {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(req.body()) else { return };
            let content = || v.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
            let ev = match v.get("type").and_then(|t| t.as_str()) {
                Some("wb-ready") => Event::Ready,
                Some("wb-state") => Event::State(content()),
                Some("wb-data") => Event::Data(content()),
                Some("wb-save-request") => Event::SaveRequest,
                _ => return,
            };
            EVENTS.with(|q| q.borrow_mut().push_back(ev));
            unsafe {
                let _ = PostMessageW(Some(HWND(host_raw as *mut _)), WM_WB, WPARAM(0), LPARAM(0));
            }
        })
        .with_navigation_handler(|url| url.starts_with("http://wb.localhost/") || url.starts_with("about:"))
        .build_as_child(&Parent(host_raw))
        .map_err(|e| e.to_string())?;
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
    VIEW.with(|v| *v.borrow_mut() = Some(view));
    Ok(())
}

/// Events received from the page since the last call.
pub fn take_events() -> Vec<Event> {
    EVENTS.with(|q| q.borrow_mut().drain(..).collect())
}

fn eval(js: &str) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.evaluate_script(js);
        }
    });
}

fn js_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

pub fn set_bounds(x: i32, y: i32, w: i32, h: i32) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.set_bounds(rect(x, y, w, h));
        }
    });
}

pub fn set_visible(on: bool) {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.set_visible(on);
        }
    });
}

pub fn focus() {
    VIEW.with(|v| {
        if let Some(view) = v.borrow().as_ref() {
            let _ = view.focus();
        }
    });
}

/// Show `scene` (the envelope JSON) in the editor.
pub fn load(scene: &str) {
    eval(&format!("window.postMessage({{type:'wb-load',content:{}}},'*')", js_string(scene)));
}

pub fn set_theme(dark: bool) {
    eval(&format!("window.postMessage({{type:'wb-theme',dark:{dark}}},'*')"));
    // The page's own background before React paints.
    eval(&format!("document.body.classList.toggle('dark',{dark})"));
}

/// Ask the page for its current scene; answered with `Event::Data`.
pub fn request_data() {
    eval("window.postMessage({type:'wb-get-data'},'*')");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_types() {
        assert_eq!(mime_for("/a/b.woff2"), "font/woff2");
        assert_eq!(mime_for("whiteboard.bundle.js"), "text/javascript; charset=utf-8");
        assert_eq!(mime_for("noext"), "application/octet-stream");
    }

    #[test]
    fn path_traversal_is_refused() {
        let d = std::env::temp_dir().join("notepp_wb_resolve");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub").join("a.txt"), "x").unwrap();
        assert!(resolve(&d, "/sub/a.txt").is_some());
        assert!(resolve(&d, "/sub/missing.txt").is_none());
        assert!(resolve(&d, "/../a.txt").is_none());
        assert!(resolve(&d, "/sub/../../etc").is_none());
        assert!(resolve(&d, "/C:/windows/win.ini").is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
