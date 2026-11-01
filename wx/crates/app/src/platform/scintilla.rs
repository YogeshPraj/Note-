//! The editor control: a wxStyledTextCtrl, driven through Scintilla's direct function and pointer
//! so the portable core can send it the same raw messages it sends everywhere else.
//!
//! This is the Scintilla adapter: the C interface speaks pointer-sized integers, so the cast lints are allowed here and nowhere else.
#![allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap, clippy::cast_sign_loss, clippy::cast_precision_loss)]

use std::ffi::{CString, c_char, c_void};

use notepp_core::sci;
use wxdragon::prelude::*;

type Direct = unsafe extern "C" fn(*mut c_void, u32, usize, isize) -> isize;

#[derive(Clone, Copy, Debug)]
pub struct WxSci {
    f: Direct,
    p: *mut c_void,
}

impl sci::Send for WxSci {
    fn send(&self, msg: u32, w: usize, l: isize) -> isize {
        unsafe { (self.f)(self.p, msg, w, l) }
    }
}

impl WxSci {
    /// The raw Scintilla access of a styled text control.
    pub fn of(stc: &StyledTextCtrl) -> WxSci {
        let win = stc.handle_ptr().cast::<wxdragon_sys::wxd_StyledTextCtrl_t>();
        let (f, p) = unsafe { (wxdragon_sys::wxd_StyledTextCtrl_GetDirectFunction(win), wxdragon_sys::wxd_StyledTextCtrl_GetDirectPointer(win)) };
        assert!(!f.is_null() && !p.is_null(), "Scintilla direct access is not available");
        WxSci { f: unsafe { std::mem::transmute::<*mut c_void, Direct>(f) }, p }
    }
}

unsafe extern "C" {
    // Lexilla, built into wxWidgets' STC.
    fn CreateLexer(name: *const c_char) -> *mut c_void;
}

fn create_lexer(name: &str) -> *mut c_void {
    match CString::new(name) {
        Ok(c) => unsafe { CreateLexer(c.as_ptr()) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Let the core build lexers with the copy of Lexilla inside wxWidgets.
pub fn register_lexilla() {
    notepp_core::lexer::register(create_lexer);
}
