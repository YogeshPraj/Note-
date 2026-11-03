//! Per-language lexer set-up: keywords and colours.
//!
//! Lexilla lexers number their styles independently, so each language needs
//! its own palette. Palettes are applied to the *view* (styles are not stored
//! per document), so they are re-applied whenever the active tab changes.

use super::lexer;
use super::sci::{self, Send, Theme, rgb};
use std::ffi::CString;

type Rgb = (u8, u8, u8);

mod palette;
pub use palette::*;

const RUST_KW: &str = "as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while";
const RUST_TYPES: &str = "i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str String Vec Option Result Box Rc Arc HashMap HashSet";
const CPP_KW: &str = "alignas alignof auto bool break case catch char class const constexpr continue default delete do double else enum explicit export extern false final float for friend goto if inline int long mutable namespace new noexcept nullptr operator override private protected public register return short signed sizeof static struct switch template this throw true try typedef typename union unsigned using virtual void volatile while function var let async await import from yield of in instanceof typeof null undefined interface implements extends package func go defer chan fallthrough";
const CPP_TYPES: &str = "int8_t int16_t int32_t int64_t uint8_t uint16_t uint32_t uint64_t size_t string vector map set";
const PY_KW: &str = "False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield";
const SQL_KW: &str = "add all alter and any as asc backup begin between by case check column commit constraint create cross database default delete desc distinct drop else end exec exists foreign from full group having in index inner insert into is join key left like limit not null on or order outer primary procedure replace right rollback select set table top truncate union unique update values view when where with";
const SQL_TYPES: &str = "bigint binary bit blob boolean char date datetime decimal double float int integer money nchar number numeric nvarchar real smallint text time timestamp tinyint varchar";
const SH_KW: &str = "alias bg bind break builtin case cd command continue declare do done echo elif else esac eval exec exit export fc fg fi for function getopts hash help history if in jobs kill let local logout popd printf pushd pwd read readonly return select set shift shopt source test then times trap type typeset ulimit umask unalias unset until wait while";
const LUA_KW: &str = "and break do else elseif end false for function goto if in local nil not or repeat return then true until while";
const RUBY_KW: &str = "BEGIN END alias and begin break case class def defined? do else elsif end ensure false for if in module next nil not or redo rescue retry return self super then true undef unless until when while yield";
const PERL_KW: &str = "NULL __FILE__ __LINE__ __PACKAGE__ and cmp continue do else elsif eq eval exit for foreach ge goto gt if last le local lt my ne next no not or our package redo require return sub unless until use while xor print push pop shift unshift split join keys values defined delete exists";
const PS_KW: &str = "begin break catch class continue data do dynamicparam else elseif end exit filter finally for foreach from function if in param process return switch throw trap try until using var while workflow";
const PS_CMDLETS: &str = "get-childitem get-content set-content get-item set-item new-item remove-item copy-item move-item get-process stop-process start-process write-host write-output write-error select-object where-object foreach-object sort-object group-object measure-object invoke-command invoke-webrequest invoke-restmethod import-module export-csv import-csv convertto-json convertfrom-json test-path join-path split-path get-date get-help";
const BAT_KW: &str = "call defined do else equ exist for geq goto gtr if in leq lss neq not errorlevel";
const BAT_CMDS: &str = "assoc attrib cd chdir cls color copy date del dir echo endlocal erase exit ftype md mkdir mklink more move path pause popd prompt pushd rd rem ren rename rmdir set setlocal shift start time title type ver verify vol xcopy robocopy";
const VB_KW: &str = "addhandler as byref byval call case catch class const continue dim do each else elseif end enum erase error event exit false finally for friend function get goto handles if implements imports in inherits interface is let loop me module mustinherit mustoverride mybase myclass namespace new next not nothing notinheritable notoverridable of on option optional or overloads overridable overrides paramarray preserve private property protected public raiseevent readonly redim rem resume return select set shadows shared static step stop structure sub synclock then throw to true try typeof using wend when while with withevents writeonly";
const PAS_KW: &str = "and array as asm begin case class const constructor destructor div do downto else end except exports file finalization finally for function goto if implementation in inherited initialization inline interface is label library mod nil not object of on or out packed private procedure program property protected public raise record repeat resourcestring set shl shr string then threadvar to try type unit until uses var while with xor";
const PHP_KW: &str = "abstract and array as break callable case catch class clone const continue declare default die do echo else elseif empty enddeclare endfor endforeach endif endswitch endwhile eval exit extends false final finally fn for foreach function global goto if implements include include_once instanceof insteadof interface isset list match namespace new null or print private protected public readonly require require_once return static switch throw trait true try unset use var while xor yield";
const TCL_KW: &str = "append array break case catch continue else elseif eval exit expr for foreach format global if incr info join lappend lindex linsert list llength lrange lreplace lsearch lsort namespace proc puts regexp regsub rename return scan set split string subst switch then unset uplevel upvar variable while";
const R_KW: &str = "break else for function if in next repeat return while TRUE FALSE NULL NA Inf NaN library require";
const LISP_KW: &str = "and cond defmacro defun defvar do else if lambda let let* loop not nil or progn quote setq setf t unless when";
const HS_KW: &str = "case class data default deriving do else foreign if import in infix infixl infixr instance let module newtype of then type where";
const ERL_KW: &str = "after and andalso band begin bnot bor bsl bsr bxor case catch cond div end fun if let not of or orelse receive rem try when xor";
const JULIA_KW: &str = "abstract baremodule begin break catch const continue do else elseif end export finally for function global if import in let local macro module mutable primitive quote return struct try using while";
const MATLAB_KW: &str = "break case catch classdef continue else elseif end for function global if otherwise parfor persistent return spmd switch try while";
const NIM_KW: &str = "addr and as asm bind block break case cast concept const continue converter defer discard distinct div do elif else end enum except export finally for from func if import in include interface is isnot iterator let macro method mixin mod nil not notin object of or out proc ptr raise ref return shl shr static template try tuple type using var when while xor yield";
const D_KW: &str = "abstract alias align assert auto body bool break byte case cast catch char class const continue default delegate do double else enum export extern false final finally float for foreach function goto if immutable import in inout int interface is long module new nothrow null out override package pragma private protected public pure real return scope shared short static struct super switch synchronized template this throw true try typeid typeof ubyte uint ulong union unittest ushort version void while with";
const VERILOG_KW: &str = "always and assign begin buf case casex casez default defparam disable else end endcase endfunction endmodule endtask for force forever fork function if initial inout input integer join module nand negedge nor not or output parameter posedge reg release repeat supply0 supply1 task time tri wait while wire xnor xor";
const VHDL_KW: &str = "abs access after alias all and architecture array assert attribute begin block body buffer bus case component configuration constant disconnect downto else elsif end entity exit file for function generate generic group guarded if impure in inertial inout is label library linkage literal loop map mod nand new next nor not null of on open or others out package port postponed procedure process pure range record register reject rem report return rol ror select severity signal shared sla sll sra srl subtype then to transport type unaffected units until use variable wait when while with xnor xor";
const ADA_KW: &str = "abort abs abstract accept access aliased all and array at begin body case constant declare delay delta digits do else elsif end entry exception exit for function generic goto if in interface is limited loop mod new not null of or others out overriding package pragma private procedure protected raise range record rem renames requeue return reverse select separate some subtype synchronized tagged task terminate then type until use when while with xor";
const ASM_KW: &str = "mov push pop add sub mul div inc dec cmp jmp je jne jg jl jge jle call ret int nop lea and or xor not shl shr test loop";
const CMAKE_KW: &str = "add_custom_command add_custom_target add_definitions add_dependencies add_executable add_library add_subdirectory add_test cmake_minimum_required configure_file else elseif endforeach endfunction endif endmacro endwhile file find_library find_package find_path foreach function if include include_directories install link_directories link_libraries list macro message option project set string target_compile_definitions target_include_directories target_link_libraries while";
const MAKE_NONE: &str = "";

pub fn keyword_sets(lexer: &str) -> &'static [&'static str] {
    match lexer {
        "rust" => &[RUST_KW, RUST_TYPES],
        "cpp" | "coffeescript" => &[CPP_KW, CPP_TYPES],
        "python" => &[PY_KW],
        "json" => &["true false null"],
        "yaml" => &["true false yes no on off null"],
        "sql" | "mysql" | "mssql" => &[SQL_KW, SQL_TYPES],
        "bash" => &[SH_KW],
        "lua" => &[LUA_KW],
        "ruby" => &[RUBY_KW],
        "perl" => &[PERL_KW],
        "powershell" => &[PS_KW, PS_CMDLETS],
        "batch" => &[BAT_KW, BAT_CMDS],
        "vb" | "vbscript" | "freebasic" | "blitzbasic" | "purebasic" | "powerbasic" => &[VB_KW],
        "pascal" => &[PAS_KW],
        "phpscript" => &[MAKE_NONE, MAKE_NONE, MAKE_NONE, MAKE_NONE, PHP_KW],
        "tcl" => &[TCL_KW],
        "r" => &[R_KW],
        "lisp" => &[LISP_KW],
        "haskell" | "literatehaskell" => &[HS_KW],
        "erlang" => &[ERL_KW],
        "julia" => &[JULIA_KW],
        "matlab" | "octave" => &[MATLAB_KW],
        "nim" | "nimrod" => &[NIM_KW],
        "d" => &[D_KW],
        "verilog" => &[VERILOG_KW],
        "vhdl" => &[VHDL_KW],
        "ada" => &[ADA_KW],
        "asm" => &[ASM_KW, ""],
        "cmake" => &[CMAKE_KW],
        _ => &[],
    }
}

#[cfg(test)]
pub fn display_name(lexer: &str) -> String {
    match lexer {
        "null" => "Normal text".into(),
        "cpp" => "C / C++ / JS".into(),
        "json" => "JSON".into(),
        "yaml" => "YAML".into(),
        "xml" => "XML".into(),
        "hypertext" => "HTML".into(),
        "css" => "CSS".into(),
        "sql" => "SQL".into(),
        "toml" => "TOML".into(),
        "powershell" => "PowerShell".into(),
        other => {
            let mut c = other.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

/// Attach the lexer for `name` to the current document and enable folding.
pub fn apply_lexer<S: Send>(sci: &S, name: &str, theme: &Theme) {
    let lex = if name == "null" { std::ptr::null_mut() } else { lexer::create(name) };
    sci.send(sci::SCI_SETILEXER, 0, lex as isize);
    if name == "null" {
        // Plain text must not keep the colours of the lexer it replaces.
        sci.send(sci::SCI_CLEARDOCUMENTSTYLE, 0, 0);
    }
    for (i, kw) in keyword_sets(name).iter().enumerate() {
        let Ok(c) = CString::new(*kw) else { continue };
        sci.send(sci::SCI_SETKEYWORDS, i, c.as_ptr() as isize);
    }
    sci::setup_folding(sci, theme);
}

/// Colour the lexer styles for `name`, resetting every other style to plain.
pub fn apply_palette<S: Send>(sci: &S, name: &str, theme: &Theme) {
    // Up to 127 covers PHP, whose styles sit high in the HTML lexer's range.
    for style in 0..128usize {
        // The brace-match styles keep their own colours (see `sci::configure`).
        if style == sci::STYLE_BRACELIGHT || style == sci::STYLE_BRACEBAD || style == sci::STYLE_LINENUMBER {
            continue;
        }
        sci.send(sci::SCI_STYLESETFORE, style, rgb(theme.fg.0, theme.fg.1, theme.fg.2));
    }
    for (style, c) in palette(name, theme) {
        sci.send(sci::SCI_STYLESETFORE, style, rgb(c.0, c.1, c.2));
    }
}

/// Languages offered in the Language menu: (menu label, Lexilla name), the way
/// they are conventionally named. Several labels share one lexer (C, C++, Java, ...).
pub const LANGUAGES: &[(&str, &str)] = &[
    ("Normal text", "null"),
    ("ActionScript", "cpp"),
    ("Ada", "ada"),
    ("Assembly", "asm"),
    ("ASN.1", "asn1"),
    ("ASP", "hypertext"),
    ("AutoIt", "au3"),
    ("AviSynth", "avs"),
    ("BaanC", "baan"),
    ("Batch", "batch"),
    ("BlitzBasic", "blitzbasic"),
    ("C", "cpp"),
    ("C#", "cpp"),
    ("C++", "cpp"),
    ("Caml", "caml"),
    ("CMake", "cmake"),
    ("COBOL", "COBOL"),
    ("CoffeeScript", "coffeescript"),
    ("Csound", "csound"),
    ("CSS", "css"),
    ("D", "d"),
    ("Diff", "diff"),
    ("Erlang", "erlang"),
    ("ErrorList", "errorlist"),
    ("Escript", "escript"),
    ("F#", "fsharp"),
    ("Forth", "forth"),
    ("Fortran (free form)", "fortran"),
    ("Fortran (fixed form)", "f77"),
    ("FreeBasic", "freebasic"),
    ("GDScript", "gdscript"),
    ("Go", "cpp"),
    ("Gui4Cli", "gui4cli"),
    ("Haskell", "haskell"),
    ("Hollywood", "hollywood"),
    ("HTML", "hypertext"),
    ("ini file", "props"),
    ("Inno Setup", "inno"),
    ("Intel HEX", "ihex"),
    ("Java", "cpp"),
    ("JavaScript", "cpp"),
    ("JSON", "json"),
    ("JSP", "hypertext"),
    ("Julia", "julia"),
    ("KIXtart", "kix"),
    ("LaTeX", "latex"),
    ("LISP", "lisp"),
    ("Lua", "lua"),
    ("Makefile", "makefile"),
    ("Markdown", "markdown"),
    ("Matlab", "matlab"),
    ("MMIXAL", "mmixal"),
    ("Nim", "nim"),
    ("Nix", "nix"),
    ("NSIS", "nsis"),
    ("Objective-C", "cpp"),
    ("OScript", "oscript"),
    ("Pascal", "pascal"),
    ("Perl", "perl"),
    ("PHP", "phpscript"),
    ("PostScript", "ps"),
    ("PowerShell", "powershell"),
    ("Properties file", "props"),
    ("PureBasic", "purebasic"),
    ("Python", "python"),
    ("R", "r"),
    ("Raku", "raku"),
    ("REBOL", "rebol"),
    ("Registry", "registry"),
    ("Resource file", "cpp"),
    ("Ruby", "ruby"),
    ("Rust", "rust"),
    ("Scheme", "lisp"),
    ("Shell", "bash"),
    ("Smalltalk", "smalltalk"),
    ("Spice", "spice"),
    ("SQL", "sql"),
    ("S-Record", "srec"),
    ("Swift", "cpp"),
    ("TCL", "tcl"),
    ("Tektronix extended HEX", "tehex"),
    ("TeX", "tex"),
    ("TOML", "toml"),
    ("TypeScript", "cpp"),
    ("Verilog", "verilog"),
    ("VHDL", "vhdl"),
    ("Visual Basic", "vb"),
    ("Visual Prolog", "visualprolog"),
    ("XML", "xml"),
    ("YAML", "yaml"),
    ("Zig", "zig"),
];

/// The label shown for a lexer when nothing more specific is known.
const DEFAULT_LABELS: &[(&str, &str)] = &[
    ("cpp", "C++"),
    ("hypertext", "HTML"),
    ("props", "ini file"),
    ("lisp", "LISP"),
    ("fortran", "Fortran (free form)"),
    ("f77", "Fortran (fixed form)"),
    ("bash", "Shell"),
];

/// Extensions whose label differs from the lexer's default.
const EXTENSION_LABELS: &[(&str, &str)] = &[
    ("c", "C"),
    ("h", "C"),
    ("cs", "C#"),
    ("java", "Java"),
    ("js", "JavaScript"),
    ("mjs", "JavaScript"),
    ("cjs", "JavaScript"),
    ("jsx", "JavaScript"),
    ("ts", "TypeScript"),
    ("tsx", "TypeScript"),
    ("go", "Go"),
    ("swift", "Swift"),
    ("m", "Objective-C"),
    ("as", "ActionScript"),
    ("rc", "Resource file"),
    ("asp", "ASP"),
    ("jsp", "JSP"),
    ("scm", "Scheme"),
    ("ss", "Scheme"),
    ("properties", "Properties file"),
];

/// Lexer name for a menu label.
pub fn lexer_of_label(label: &str) -> &'static str {
    LANGUAGES.iter().find(|(l, _)| *l == label).map_or("null", |(_, x)| *x)
}

/// The menu label a file gets for its lexer and extension.
pub fn label_for(lexer: &str, path: Option<&std::path::Path>) -> &'static str {
    let ext = path.and_then(|p| p.extension()).and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    if let Some((_, l)) = EXTENSION_LABELS.iter().find(|(e, _)| *e == ext)
        && lexer_of_label(l) == lexer
    {
        return l;
    }
    DEFAULT_LABELS
        .iter()
        .find(|(x, _)| *x == lexer)
        .map(|(_, l)| *l)
        .or_else(|| LANGUAGES.iter().find(|(_, x)| *x == lexer).map(|(l, _)| *l))
        .unwrap_or("Normal text")
}

#[cfg(test)]
mod tests;
