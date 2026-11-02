//! Lexilla — 125 language lexers.
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

/// Lexilla lexer name for each group of file extensions.
const EXTENSIONS: &[(&str, &[&str])] = &[
    ("rust", &["rs"]),
    (
        "cpp",
        &[
            "c", "h", "cpp", "hpp", "cc", "cxx", "hh", "hxx", "h++", "c++", "inl", "ino", "pde", "cl", "cs", "js", "mjs", "cjs", "ts", "jsx", "tsx", "java",
            "go", "kt", "kts", "swift", "m", "mm", "rc", "as",
        ],
    ),
    ("python", &["py", "pyw", "pyi", "pyx"]),
    ("ruby", &["rb", "rbw", "rake", "gemspec"]),
    ("phpscript", &["php", "php3", "php4", "php5", "phps", "phtml"]),
    ("perl", &["pl", "pm"]),
    ("lua", &["lua"]),
    ("sql", &["sql"]),
    ("hypertext", &["html", "htm", "xhtml", "shtml", "hta", "jsp", "asp", "aspx"]),
    (
        "xml",
        &[
            "xml", "xsl", "xslt", "xsd", "svg", "xaml", "plist", "kml", "wsdl", "rss", "atom", "csproj", "vbproj", "vcxproj", "fsproj", "props", "targets",
            "resx", "config", "manifest", "xlf", "xliff", "ui",
        ],
    ),
    ("css", &["css", "scss", "less"]),
    ("json", &["json", "jsonc", "json5"]),
    ("yaml", &["yaml", "yml"]),
    ("toml", &["toml"]),
    ("markdown", &["md", "markdown"]),
    ("bash", &["sh", "bash", "zsh", "ksh"]),
    ("powershell", &["ps1", "psm1", "psd1"]),
    ("batch", &["bat", "cmd"]),
    ("props", &["ini", "cfg", "conf", "inf", "url", "lnk", "properties"]),
    ("diff", &["diff", "patch"]),
    ("vb", &["vb", "vbs"]),
    ("pascal", &["pas"]),
    ("fortran", &["f", "f90", "f95"]),
    ("asm", &["asm", "s"]),
    ("latex", &["tex"]),
    ("r", &["r"]),
    ("dart", &["dart"]),
    ("zig", &["zig"]),
    ("nim", &["nim"]),
    ("ada", &["adb", "ads", "ada"]),
    ("au3", &["au3"]),
    ("cmake", &["cmake"]),
    ("coffeescript", &["coffee"]),
    ("d", &["d"]),
    ("erlang", &["erl", "hrl"]),
    ("fsharp", &["fs", "fsx", "fsi"]),
    ("haskell", &["hs", "lhs"]),
    ("caml", &["ml", "mli"]),
    ("julia", &["jl"]),
    ("lisp", &["lisp", "lsp", "el", "scm", "ss"]),
    ("makefile", &["mak", "mk"]),
    ("nsis", &["nsi", "nsh"]),
    ("tcl", &["tcl"]),
    ("verilog", &["v"]),
    ("vhdl", &["vhd", "vhdl"]),
    ("registry", &["reg"]),
    ("nix", &["nix"]),
    ("gdscript", &["gd"]),
];

/// Map a file extension to a Lexilla lexer name.
pub fn lexer_for_extension(ext: &str) -> &'static str {
    let ext = ext.to_ascii_lowercase();
    EXTENSIONS.iter().find(|(_, exts)| exts.contains(&ext.as_str())).map_or("null", |(lexer, _)| lexer)
}

/// Create a lexer instance to hand to `SCI_SETILEXER`. Null means "plain text".
pub fn create(name: &str) -> *mut c_void {
    FACTORY.get().map_or(std::ptr::null_mut(), |f| f(name))
}
