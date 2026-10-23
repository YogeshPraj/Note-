//! Lexilla — the same 125 language lexers Notepad++ ships (it bundles 128).
//!
//! Statically linked, so there is no second DLL to ship. Portable: the lexer
//! is handed to Scintilla through SCI_SETILEXER, which is platform-neutral.

use std::ffi::{c_char, c_void, CString};

#[link(name = "lexilla", kind = "static")]
unsafe extern "C" {
    fn CreateLexer(name: *const c_char) -> *mut c_void;
    fn GetLexerCount() -> i32;
}

/// How many lexers this build has available.
pub fn count() -> i32 {
    unsafe { GetLexerCount() }
}

/// Map a file extension to a Lexilla lexer name.
pub fn lexer_for_extension(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" => "cpp",
        "cs" => "cpp",
        "js" | "mjs" | "cjs" | "ts" | "jsx" | "tsx" => "cpp",
        "py" | "pyw" => "python",
        "java" => "cpp",
        "go" => "cpp",
        "rb" => "ruby",
        "php" => "phpscript",
        "pl" | "pm" => "perl",
        "lua" => "lua",
        "sql" => "sql",
        "html" | "htm" | "xhtml" => "hypertext",
        "xml" | "xsl" | "svg" => "xml",
        "css" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "sh" | "bash" => "bash",
        "ps1" => "powershell",
        "bat" | "cmd" => "batch",
        "ini" | "cfg" | "conf" => "props",
        "diff" | "patch" => "diff",
        "vb" => "vb",
        "pas" => "pascal",
        "f" | "f90" | "f95" => "fortran",
        "asm" | "s" => "asm",
        "tex" => "latex",
        "r" => "r",
        "kt" | "kts" => "cpp",
        "swift" => "cpp",
        "dart" => "dart",
        "zig" => "zig",
        "nim" => "nim",
        _ => "null",
    }
}

/// Create a lexer instance to hand to SCI_SETILEXER. Null means "plain text".
pub fn create(name: &str) -> *mut c_void {
    let c = match CString::new(name) {
        Ok(c) => c,
        Err(_) => return std::ptr::null_mut(),
    };
    unsafe { CreateLexer(c.as_ptr()) }
}
