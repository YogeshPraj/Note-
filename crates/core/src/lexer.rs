//! Lexilla — the same 125 language lexers Notepad++ ships (it bundles 128).
//!
//! Statically linked, so there is no second DLL to ship. Portable: the lexer
//! is handed to Scintilla through `SCI_SETILEXER`, which is platform-neutral.

use std::ffi::c_void;
use std::sync::OnceLock;

/// Builds a Lexilla lexer by name (an `ILexer5*` for `SCI_SETILEXER`); null for an unknown name.
/// Each shell registers its own: the Win32 app links Lexilla itself, the wxWidgets shell uses the
/// copy inside wxWidgets. The core stays free of link-time dependencies.
pub type Factory = fn(&str) -> *mut c_void;

static FACTORY: OnceLock<Factory> = OnceLock::new();

pub fn register(f: Factory) {
    let _ = FACTORY.set(f);
}

/// Map a file extension to a Lexilla lexer name.
#[allow(clippy::match_same_arms, reason = "one arm per extension group keeps the table readable")]
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
        "xml" | "xsl" | "xslt" | "xsd" | "svg" | "xaml" | "plist" | "kml" | "wsdl" | "rss" | "atom" | "csproj" | "vbproj" | "vcxproj" | "fsproj" | "props"
        | "targets" | "resx" | "config" | "manifest" | "xlf" | "xliff" | "ui" => "xml",
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

/// Create a lexer instance to hand to `SCI_SETILEXER`. Null means "plain text".
pub fn create(name: &str) -> *mut c_void {
    FACTORY.get().map_or(std::ptr::null_mut(), |f| f(name))
}
