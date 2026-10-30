//! The Compare window: a separate window showing a side-by-side diff in its own WebView.
//!
//! The page is built by `core::compare` (self-contained HTML; the compared text is only ever
//! escaped into it). The window never navigates anywhere else.

use std::cell::{Cell, RefCell};
use std::num::NonZeroIsize;

use raw_window_handle::{HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebContext, WebView, WebViewBuilder};

struct Parent(isize);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let h = Win32WindowHandle::new(NonZeroIsize::new(self.0).ok_or(HandleError::Unavailable)?);
        Ok(unsafe { WindowHandle::borrow_raw(RawWindowHandle::Win32(h)) })
    }
}

thread_local! {
    static WIN: Cell<Option<HWND>> = const { Cell::new(None) };
    // The context must outlive the view built from it.
    static CTX: RefCell<Option<WebContext>> = const { RefCell::new(None) };
    static VIEW: RefCell<Option<WebView>> = const { RefCell::new(None) };
}

fn bounds(hwnd: HWND) -> Rect {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rc);
    }
    Rect {
        position: PhysicalPosition::new(0, 0).into(),
        size: PhysicalSize::new(rc.right.max(1) as u32, rc.bottom.max(1) as u32).into(),
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_SIZE => {
            VIEW.with(|v| {
                if let Some(view) = v.borrow().as_ref() {
                    let _ = view.set_bounds(bounds(hwnd));
                }
            });
            LRESULT(0)
        }
        WM_SETFOCUS => {
            VIEW.with(|v| {
                if let Some(view) = v.borrow().as_ref() {
                    let _ = view.focus();
                }
            });
            LRESULT(0)
        }
        WM_CLOSE => {
            // Drop the view before its window goes away.
            VIEW.with(|v| *v.borrow_mut() = None);
            WIN.with(|w| w.set(None));
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn register() {
    unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let mut wc = WNDCLASSW::default();
        if GetClassInfoW(Some(hinst.into()), w!("NoteppCompare"), &mut wc).is_ok() {
            return;
        }
        RegisterClassW(&WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: hinst.into(),
            lpszClassName: w!("NoteppCompare"),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
            ..Default::default()
        });
    }
}

/// Show `html` in a Compare window (replacing an earlier one). `data_dir` holds the WebView profile.
pub fn show(owner: HWND, data_dir: &std::path::Path, title: &str, html: &str) -> Result<(), String> {
    register();
    if let Some(old) = WIN.with(|w| w.get()) {
        unsafe {
            let _ = SendMessageW(old, WM_CLOSE, None, None);
        }
    }
    let _ = std::fs::create_dir_all(data_dir);
    let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut orc = RECT::default();
        let _ = GetWindowRect(owner, &mut orc);
        let (ww, wh) = (((orc.right - orc.left) * 9 / 10).max(900), ((orc.bottom - orc.top) * 9 / 10).max(600));
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("NoteppCompare"),
            PCWSTR(t.as_ptr()),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            orc.left + ((orc.right - orc.left) - ww) / 2,
            orc.top + ((orc.bottom - orc.top) - wh) / 4,
            ww,
            wh,
            Some(owner),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .map_err(|e| e.to_string())?;
        super::modern::frame(hwnd);
        let host = hwnd.0 as isize;
        let mut ctx = WebContext::new(Some(data_dir.to_path_buf()));
        let view = WebViewBuilder::new_with_web_context(&mut ctx)
            .with_html(html)
            .with_bounds(bounds(hwnd))
            .with_ipc_handler(move |req| {
                if req.body().as_str() == "close" {
                    unsafe {
                        let _ = PostMessageW(Some(HWND(host as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
                    }
                }
            })
            .with_navigation_handler(|url| url.starts_with("about:") || url.starts_with("data:"))
            .build_as_child(&Parent(host))
            .map_err(|e| {
                let _ = DestroyWindow(hwnd);
                e.to_string()
            })?;
        CTX.with(|c| *c.borrow_mut() = Some(ctx));
        VIEW.with(|v| *v.borrow_mut() = Some(view));
        WIN.with(|w| w.set(Some(hwnd)));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(hwnd));
    }
    Ok(())
}
