//! The live-preview pane: an embedded WebView2 (via `wry`) child window.
//!
//! The page itself (`assets/preview.html`) is loaded once; after that the host
//! only pushes content with `render()`. Mermaid (3 MB) is injected lazily the
//! first time a diagram needs drawing. Clicks on the page's header buttons and
//! "need Mermaid" requests come back as `WM_PREVIEW` messages to the host.

use std::cell::{Cell, RefCell};
use std::num::NonZeroIsize;

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SW_SHOWNORMAL, WM_APP};
use windows::core::{PCWSTR, w};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

pub const WM_PREVIEW: u32 = WM_APP + 50;
pub const PV_CLOSE: usize = 1;
pub const PV_MAX: usize = 2;
pub const PV_NEED_MERMAID: usize = 3;
pub const PV_REFRESH: usize = 4;
/// The preview was scrolled by the user; the message's lParam is the top source line times 100.
pub const PV_SCROLL: usize = 5;
pub const PV_NEED_JSON: usize = 6;
/// The JSON editor changed the document; fetch the text with `take_json_edit`.
pub const PV_JSON_EDIT: usize = 7;
/// A JSON tree node was clicked; fetch its path with `take_json_path`.
pub const PV_JSON_PATH: usize = 8;

const PAGE: &str = include_str!("../../assets/preview.html");
const MERMAID_JS: &str = include_str!("../../assets/mermaid.min.js");
// JSON editor: josdejong/jsoneditor (Apache-2.0) and JSON5 (MIT), loaded on the first JSON preview.
const JSONEDITOR_JS: &str = include_str!("../../assets/jsoneditor.min.js");
const JSONEDITOR_CSS: &str = include_str!("../../assets/jsoneditor.min.css");
const JSONEDITOR_ICONS: &str = include_str!("../../assets/jsoneditor-icons.svg");
const JSON5_JS: &str = include_str!("../../assets/json5.min.js");

struct Parent(isize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

thread_local! {
    // The context must outlive the view that was built from it.
    static CTX: RefCell<Option<WebContext>> = const { RefCell::new(None) };
    static VIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
    static MERMAID: Cell<bool> = const { Cell::new(false) };
    static JSON_EDITOR: Cell<bool> = const { Cell::new(false) };
    static JSON_EDIT: RefCell<Option<String>> = const { RefCell::new(None) };
    static JSON_PATH: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn is_created() -> bool {
    VIEW.with(|v| v.borrow().is_some())
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
    Rect {
        position: PhysicalPosition::new(x, y).into(),
        size: PhysicalSize::new(w.max(1) as u32, h.max(1) as u32).into(),
    }
}

/// Only web links leave the preview; they open in the default browser.
fn open_external(url: &str) {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:") {
        let u: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            ShellExecuteW(None, w!("open"), PCWSTR(u.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
        }
    }
}

/// The preview page with its Content-Security-Policy set for `remote_images`.
fn page_html(remote_images: bool) -> String {
    const OPEN: &str = "img-src data: https: http:;";
    debug_assert!(PAGE.contains(OPEN));
    PAGE.replacen(OPEN, if remote_images { OPEN } else { "img-src data:;" }, 1)
}

/// Create the WebView2 child of `host` (once). `data_dir` holds its profile.
/// Previewed documents may load web images only when `remote_images` is set.
pub fn ensure(host: HWND, data_dir: &std::path::Path, remote_images: bool) -> Result<(), String> {
    if is_created() {
        return Ok(());
    }
    let _ = std::fs::create_dir_all(data_dir);
    let host_raw = host.0 as isize;
    let mut ctx = WebContext::new(Some(data_dir.to_path_buf()));
    let view = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_html(page_html(remote_images))
        .with_bounds(rect(0, 0, 400, 300))
        .with_ipc_handler(move |req| {
            let body = req.body().as_str();
            // The page reports its scroll position as "line:<source line, fractional>".
            // The JSON editor sends its text ("json:" / "jsonfix:") or the clicked node's path ("jpath:").
            if let Some(text) = body.strip_prefix("json:") {
                JSON_EDIT.with(|j| *j.borrow_mut() = Some(text.to_string()));
                unsafe {
                    let _ = PostMessageW(Some(HWND(host_raw as *mut _)), WM_PREVIEW, WPARAM(PV_JSON_EDIT), LPARAM(0));
                }
                return;
            }
            if let Some(text) = body.strip_prefix("jsonfix:") {
                JSON_EDIT.with(|j| *j.borrow_mut() = Some(text.to_string()));
                unsafe {
                    let _ = PostMessageW(Some(HWND(host_raw as *mut _)), WM_PREVIEW, WPARAM(PV_JSON_EDIT), LPARAM(1));
                }
                return;
            }
            if let Some(path) = body.strip_prefix("jpath:") {
                JSON_PATH.with(|j| *j.borrow_mut() = Some(path.to_string()));
                unsafe {
                    let _ = PostMessageW(Some(HWND(host_raw as *mut _)), WM_PREVIEW, WPARAM(PV_JSON_PATH), LPARAM(0));
                }
                return;
            }
            let (code, arg) = if let Some(line) = body.strip_prefix("line:") {
                match line.trim().parse::<f64>() {
                    Ok(l) if l.is_finite() && (0.0..1e7).contains(&l) => (PV_SCROLL, (l * 100.0).round() as isize),
                    _ => return,
                }
            } else {
                let code = match body {
                    "close" => PV_CLOSE,
                    "max" => PV_MAX,
                    "need-mermaid" => PV_NEED_MERMAID,
                    "need-jsoneditor" => PV_NEED_JSON,
                    "refresh" => PV_REFRESH,
                    _ => return,
                };
                (code, 0)
            };
            unsafe {
                let _ = PostMessageW(Some(HWND(host_raw as *mut _)), WM_PREVIEW, WPARAM(code), LPARAM(arg));
            }
        })
        .with_navigation_handler(|url| {
            // The page is loaded from a string (about:/data:); links go to the browser, nothing else navigates.
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
                open_external(&url);
                false
            } else {
                url.starts_with("about:") || url.starts_with("data:")
            }
        })
        .build_as_child(&Parent(host_raw))
        .map_err(|e| e.to_string())?;
    CTX.with(|c| *c.borrow_mut() = Some(ctx));
    VIEW.with(|v| *v.borrow_mut() = Some(view));
    Ok(())
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

pub fn take_json_edit() -> Option<String> {
    JSON_EDIT.with(|j| j.borrow_mut().take())
}

pub fn take_json_path() -> Option<String> {
    JSON_PATH.with(|j| j.borrow_mut().take())
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

/// The editor's stylesheet with its icon sprite inlined (the page has no files to load it from).
fn jsoneditor_css() -> String {
    let icons = format!("data:image/svg+xml;base64,{}", base64(JSONEDITOR_ICONS.as_bytes()));
    JSONEDITOR_CSS.replace("./img/jsoneditor-icons.svg", &icons).replace("img/jsoneditor-icons.svg", &icons)
}

/// Inject the JSON editor (once), then tell the page to draw the document it was waiting on.
pub fn load_json_editor() {
    if !JSON_EDITOR.with(|c| c.replace(true)) {
        eval(&format!(
            "(function(){{var s=document.createElement('style');s.textContent={};document.head.appendChild(s);}})();",
            js_string(&jsoneditor_css())
        ));
        eval(JSON5_JS);
        eval(JSONEDITOR_JS);
    }
    eval("window.np.jsonLoaded()");
}

/// Push content to the page. `kind` is "markdown" | "html" | "mermaid" | "json" | "message".
pub fn render(kind: &str, payload: &str) {
    eval(&format!("window.np.render({},{})", js_string(kind), js_string(payload)));
}

pub fn set_theme(dark: bool) {
    eval(&format!("window.np.setTheme({dark})"));
}

pub fn set_max(on: bool) {
    eval(&format!("window.np.setMax({on})"));
}

/// Scroll the rendered Markdown so source line `line` (0-based, fractional) is at the top.
pub fn scroll_to_line(line: f64, total_lines: usize) {
    eval(&format!("window.np.scrollToLine({line:.3},{total_lines})"));
}

/// Inject Mermaid (once), then tell the page to draw the diagrams it was waiting on.
pub fn load_mermaid() {
    if !MERMAID.with(|c| c.replace(true)) {
        eval(MERMAID_JS);
    }
    eval("window.np.mermaidLoaded()");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn jsoneditor_icons_are_inlined() {
        assert!(!jsoneditor_css().contains("img/jsoneditor-icons.svg"));
        assert!(jsoneditor_css().contains("data:image/svg+xml;base64,"));
        assert!(!jsoneditor_css().contains("./data:"));
    }

    #[test]
    fn remote_images_are_blocked_unless_allowed() {
        let off = page_html(false);
        assert!(off.contains("img-src data:;") && !off.contains("img-src data: https"), "CSP still allows the web");
        let on = page_html(true);
        assert!(on.contains("img-src data: https: http:;"));
    }
}
