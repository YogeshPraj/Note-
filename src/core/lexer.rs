//! Lexilla — the same 125 language lexers Notepad++ ships (it bundles 128).
//!
//! Statically linked, so there is no second DLL to ship. Portable: the lexer
//! is handed to Scintilla through SCI_SETILEXER, which is platform-neutral.

use std::ffi::{c_char, c_void, CString};

#[link(name = "lexilla", kind = "static")]
unsafe extern "C" {
    fn CreateLexer(name: *const c_char) -> *mut c_void;
    #[cfg(test)]
    fn GetLexerCount() -> i32;
    #[cfg(test)]
    fn GetLexerName(index: u32, name: *mut c_char, buflength: i32);
}

/// Names of every lexer this build has.
#[cfg(test)]
pub fn names() -> Vec<String> {
    (0..unsafe { GetLexerCount() }.max(0) as u32)
        .map(|i| {
            let mut buf = [0 as c_char; 64];
            unsafe { GetLexerName(i, buf.as_mut_ptr(), buf.len() as i32) };
            let bytes: Vec<u8> = buf.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .collect()
}

/// Map a file extension to a Lexilla lexer name.
pub fn lexer_for_extension(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "rs" => "rust",
        "c" | "h" | "cpp" | "hpp" | "cc" | "cxx" | "hh" | "hxx" | "h++" | "c++" | "inl" | "ino" | "pde" | "cl" => "cpp",
        "cs" => "cpp",
        "js" | "mjs" | "cjs" | "ts" | "jsx" | "tsx" => "cpp",
        "py" | "pyw" | "pyi" | "pyx" => "python",
        "java" => "cpp",
        "go" => "cpp",
        "rb" | "rbw" | "rake" | "gemspec" => "ruby",
        "php" | "php3" | "php4" | "php5" | "phps" | "phtml" => "phpscript",
        "pl" | "pm" => "perl",
        "lua" => "lua",
        "sql" => "sql",
        "html" | "htm" | "xhtml" | "shtml" | "hta" => "hypertext",
        "xml" | "xsl" | "xslt" | "xsd" | "svg" | "xaml" | "plist" | "kml" | "wsdl" | "rss" | "atom" | "csproj" | "vbproj" | "vcxproj" | "fsproj" | "props" | "targets" | "resx" | "config" | "manifest" | "xlf" | "xliff" | "ui" => "xml",
        "css" | "scss" | "less" => "css",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "md" | "markdown" => "markdown",
        "sh" | "bash" => "bash",
        "ps1" | "psm1" | "psd1" => "powershell",
        "bat" | "cmd" => "batch",
        "ini" | "cfg" | "conf" | "inf" | "url" | "lnk" => "props",
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
        "adb" | "ads" | "ada" => "ada",
        "au3" => "au3",
        "cmake" => "cmake",
        "coffee" => "coffeescript",
        "d" => "d",
        "erl" | "hrl" => "erlang",
        "fs" | "fsx" | "fsi" => "fsharp",
        "hs" | "lhs" => "haskell",
        "ml" | "mli" => "caml",
        "jl" => "julia",
        "lisp" | "lsp" | "el" | "scm" | "ss" => "lisp",
        "mak" | "mk" => "makefile",
        "m" | "mm" => "cpp",
        "nsi" | "nsh" => "nsis",
        "tcl" => "tcl",
        "v" => "verilog",
        "vhd" | "vhdl" => "vhdl",
        "vbs" => "vb",
        "reg" => "registry",
        "nix" => "nix",
        "jsp" | "asp" | "aspx" => "hypertext",
        "gd" => "gdscript",
        "jsonc" | "json5" => "json",
        "zsh" | "ksh" => "bash",
        "properties" => "props",
        "rc" | "as" => "cpp",
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
