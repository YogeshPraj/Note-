//! The Win32 app's own copy of Lexilla, linked statically (see build.rs), registered with the core.

use std::ffi::{c_char, c_void, CString};

#[link(name = "lexilla", kind = "static")]
unsafe extern "C" {
    fn CreateLexer(name: *const c_char) -> *mut c_void;
    #[cfg(test)]
    fn GetLexerCount() -> i32;
    #[cfg(test)]
    fn GetLexerName(index: u32, name: *mut c_char, buflength: i32);
}

fn create(name: &str) -> *mut c_void {
    match CString::new(name) {
        Ok(c) => unsafe { CreateLexer(c.as_ptr()) },
        Err(_) => std::ptr::null_mut(),
    }
}

/// Let the core build lexers with this Lexilla. Call once at startup.
pub fn init() {
    notepp_core::lexer::register(create);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Names of every lexer this build has.
    fn names() -> Vec<String> {
        (0..unsafe { GetLexerCount() }.max(0) as u32)
            .map(|i| {
                let mut buf = [0 as c_char; 64];
                unsafe { GetLexerName(i, buf.as_mut_ptr(), buf.len() as i32) };
                let bytes: Vec<u8> = buf.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
                String::from_utf8_lossy(&bytes).into_owned()
            })
            .collect()
    }

    #[test]
    fn every_menu_language_has_a_lexer_in_this_build() {
        let have = names();
        for (label, l) in notepp_core::lang::LANGUAGES {
            assert!(*l == "null" || have.iter().any(|n| n == l), "{label} -> {l}");
        }
    }
}
