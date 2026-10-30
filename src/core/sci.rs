//! Portable Scintilla layer.
//!
//! Nothing in this file is Windows-specific. Scintilla's API is a flat message
//! interface that is byte-identical on Win32, GTK and Cocoa, so every function
//! here will work unchanged on all three once their shells exist. The only
//! platform-dependent thing is how the message is *delivered*, which is the
//! `Send` trait below — one small implementation per platform.
//!
//! This is the whole reason the three-platform plan is affordable: the editor
//! lives here and is written once.

#![allow(dead_code)]

use std::ffi::CString;

// ── Messages (scintilla/include/Scintilla.h) ───────────────────────────────
pub const SCI_SETTEXT: u32 = 2181;
pub const SCI_GETTEXT: u32 = 2182;
pub const SCI_GETLENGTH: u32 = 2006;
pub const SCI_GETLINECOUNT: u32 = 2154;
pub const SCI_SETILEXER: u32 = 4033;
pub const SCI_STYLESETFONT: u32 = 2056;
pub const SCI_STYLESETSIZE: u32 = 2055;
pub const SCI_STYLESETFORE: u32 = 2051;
pub const SCI_STYLESETBACK: u32 = 2052;
pub const SCI_STYLESETBOLD: u32 = 2053;
pub const SCI_STYLECLEARALL: u32 = 2050;
pub const SCI_SETMARGINTYPEN: u32 = 2240;
pub const SCI_SETMARGINWIDTHN: u32 = 2242;
pub const SCI_SETMARGINMASKN: u32 = 2244;
pub const SCI_SETMARGINSENSITIVEN: u32 = 2246;
pub const SCI_SETCARETLINEVISIBLE: u32 = 2096;
pub const SCI_SETCARETLINEBACK: u32 = 2098;
pub const SCI_SETCARETFORE: u32 = 2069;
pub const SCI_SETWRAPMODE: u32 = 2268;
pub const SCI_MARKERDEFINE: u32 = 2040;
pub const SCI_MARKERSETFORE: u32 = 2041;
pub const SCI_MARKERSETBACK: u32 = 2042;
pub const SCI_SETFOLDFLAGS: u32 = 2233;
pub const SCI_SETFOLDMARGINCOLOUR: u32 = 2290;
pub const SCI_SETFOLDMARGINHICOLOUR: u32 = 2291;
pub const SCI_SETAUTOMATICFOLD: u32 = 2663;
pub const SCI_SETPROPERTY: u32 = 4004;
pub const SCI_SETKEYWORDS: u32 = 4005;
pub const SCI_SETTABWIDTH: u32 = 2036;
pub const SCI_SETUSETABS: u32 = 2124;
pub const SCI_SETSELFORE: u32 = 2067;
pub const SCI_SETSELBACK: u32 = 2068;
pub const SCI_SETCODEPAGE: u32 = 2037;
pub const SCI_SETEOLMODE: u32 = 2031;
pub const SCI_COLOURISE: u32 = 4003;
pub const SCI_ADDTEXT: u32 = 2001;
pub const SCI_CLEARALL: u32 = 2004;
pub const SCI_SETSAVEPOINT: u32 = 2014;
pub const SCI_GETMODIFY: u32 = 2159;
pub const SCI_EMPTYUNDOBUFFER: u32 = 2175;
pub const SCI_GOTOPOS: u32 = 2025;
pub const SCI_GETCURRENTPOS: u32 = 2008;
pub const SCI_LINEFROMPOSITION: u32 = 2166;
pub const SCI_GETCOLUMN: u32 = 2129;
pub const SCI_GETSELECTIONSTART: u32 = 2143;
pub const SCI_GETSELECTIONEND: u32 = 2145;
pub const SCI_GETFIRSTVISIBLELINE: u32 = 2152;
pub const SCI_SETFIRSTVISIBLELINE: u32 = 2613;
pub const SCI_VISIBLEFROMDOCLINE: u32 = 2220;
pub const SCI_DOCLINEFROMVISIBLE: u32 = 2221;
pub const SCI_CREATEDOCUMENT: u32 = 2375;
pub const SCI_RELEASEDOCUMENT: u32 = 2377;
pub const SCI_SETDOCPOINTER: u32 = 2358;
pub const SCI_GETEOLMODE: u32 = 2030;
pub const SCI_CONVERTEOLS: u32 = 2029;
pub const SCI_UNDO: u32 = 2176;
pub const SCI_REDO: u32 = 2011;
pub const SCI_CUT: u32 = 2177;
pub const SCI_COPY: u32 = 2178;
pub const SCI_PASTE: u32 = 2179;
pub const SCI_SELECTALL: u32 = 2013;
pub const SCI_ZOOMIN: u32 = 2333;
pub const SCI_ZOOMOUT: u32 = 2334;
pub const SCI_SETZOOM: u32 = 2373;
pub const SCI_GETDOCPOINTER: u32 = 2357;
pub const SCI_GETSELECTIONS: u32 = 2570;
pub const SCI_COUNTCHARACTERS: u32 = 2633;
pub const SCI_GETSELECTIONNSTART: u32 = 2585;
pub const SCI_GETSELECTIONNEND: u32 = 2587;
pub const SCI_GETSELECTIONEMPTY: u32 = 2650;
pub const SCI_GETLINEINDENTPOSITION: u32 = 2128;
pub const SCI_SELECTIONISRECTANGLE: u32 = 2372;
pub const SCI_SETTARGETRANGE: u32 = 2686;
pub const SCI_GETTARGETTEXT: u32 = 2687;
pub const SCI_LINEDUPLICATE: u32 = 2404;
pub const SCI_SELECTIONDUPLICATE: u32 = 2469;
pub const SCI_GETTABWIDTH: u32 = 2121;
pub const SCI_GETLINEINDENTATION: u32 = 2127;
pub const SCI_SETLINEINDENTATION: u32 = 2126;
pub const SCI_CLEAR: u32 = 2180;
pub const SCI_TAB: u32 = 2327;
pub const SCI_BACKTAB: u32 = 2328;
pub const SCI_SETREADONLY: u32 = 2171;
pub const SCI_GETREADONLY: u32 = 2140;
pub const SCI_CANUNDO: u32 = 2174;
pub const SCI_CANREDO: u32 = 2016;
pub const SCI_CANPASTE: u32 = 2173;
pub const SCI_LINECUT: u32 = 2337;
pub const SCI_LINECOPY: u32 = 2455;
pub const SCI_LINETRANSPOSE: u32 = 2339;
pub const SCI_GETANCHOR: u32 = 2009;
pub const SCI_SETRECTANGULARSELECTIONANCHOR: u32 = 2590;
pub const SCI_SETRECTANGULARSELECTIONCARET: u32 = 2588;
pub const SCI_SETSELECTIONMODE: u32 = 2422;
pub const SCI_INSERTTEXT: u32 = 2003;
pub const SCI_AUTOCSHOW: u32 = 2100;
pub const SCI_AUTOCSETIGNORECASE: u32 = 2115;
pub const SCI_AUTOCSETCHOOSESINGLE: u32 = 2113;
pub const SCI_AUTOCSETSEPARATOR: u32 = 2106;
pub const SCI_AUTOCSETAUTOHIDE: u32 = 2118;
pub const SCI_CALLTIPSHOW: u32 = 2200;
pub const SCI_SETEDGEMODE: u32 = 2363;
pub const SCI_SETEDGECOLUMN: u32 = 2361;
pub const SCI_SETEDGECOLOUR: u32 = 2365;
pub const SCI_USEPOPUP: u32 = 2371;
pub const SCI_CLEARDOCUMENTSTYLE: u32 = 2005;
pub const SCI_SETVIEWWS: u32 = 2021;
pub const SCI_SETVIEWEOL: u32 = 2356;
pub const SCI_SETINDENTATIONGUIDES: u32 = 2132;
pub const SCI_SETWRAPVISUALFLAGS: u32 = 2460;
pub const SCI_SETREPRESENTATION: u32 = 2665;
pub const SCI_CLEARREPRESENTATION: u32 = 2667;
pub const SCI_FOLDALL: u32 = 2662;
pub const SCI_GETFOLDLEVEL: u32 = 2223;
pub const SCI_GETFOLDPARENT: u32 = 2225;
pub const SCI_FOLDLINE: u32 = 2237;
pub const SCI_HIDELINES: u32 = 2227;
pub const SCI_SHOWLINES: u32 = 2226;
pub const SCI_GETLINEVISIBLE: u32 = 2228;
pub const SCI_INDICATOREND: u32 = 2509;
pub const SCI_INDICATORVALUEAT: u32 = 2507;
pub const SCI_BRACEMATCH: u32 = 2353;
pub const SCI_BRACEHIGHLIGHT: u32 = 2351;
pub const SCI_BRACEBADLIGHT: u32 = 2352;
pub const SCI_GETCHARAT: u32 = 2007;
pub const SCI_WORDSTARTPOSITION: u32 = 2266;
pub const SCI_WORDENDPOSITION: u32 = 2267;
pub const SCI_DELETERANGE: u32 = 2645;
pub const SCI_LINEDELETE: u32 = 2338;
pub const SCI_MOVESELECTEDLINESUP: u32 = 2620;
pub const SCI_MOVESELECTEDLINESDOWN: u32 = 2621;
pub const SCI_UPPERCASE: u32 = 2341;
pub const SCI_LOWERCASE: u32 = 2340;
pub const SCI_POSITIONFROMLINE: u32 = 2167;
pub const SCI_GETLINEENDPOSITION: u32 = 2136;
pub const SCI_MARKERADD: u32 = 2043;
pub const SCI_MARKERDELETE: u32 = 2044;
pub const SCI_MARKERDELETEALL: u32 = 2045;
pub const SCI_MARKERGET: u32 = 2046;
pub const SCI_MARKERNEXT: u32 = 2047;
pub const SCI_MARKERPREVIOUS: u32 = 2048;
pub const SCN_MARGINCLICK: u32 = 2010;
pub const SC_MARK_BOOKMARK: isize = 31;
pub const SCI_SETCARETSTYLE: u32 = 2512;
pub const SCI_SETHSCROLLBAR: u32 = 2130;
pub const SCI_SETVSCROLLBAR: u32 = 2280;
pub const SCI_LINESONSCREEN: u32 = 2370;
pub const SCI_POSITIONFROMPOINT: u32 = 2022;
pub const SCI_TEXTHEIGHT: u32 = 2279;
pub const SCI_SETWRAPMODE_: u32 = 2268;
pub const SC_WRAP_CHAR: usize = 2;

pub const SCN_SAVEPOINTREACHED: u32 = 2002;

pub const SCI_SETSEARCHFLAGS: u32 = 2198;
pub const SCI_SETTARGETSTART: u32 = 2190;
pub const SCI_SETTARGETEND: u32 = 2192;
pub const SCI_GETTARGETEND: u32 = 2193;
pub const SCI_SEARCHINTARGET: u32 = 2197;
pub const SCI_REPLACETARGET: u32 = 2194;
pub const SCI_REPLACETARGETRE: u32 = 2195;
pub const SCI_SETSEL: u32 = 2160;
pub const SCI_SCROLLCARET: u32 = 2169;
pub const SCI_ENSUREVISIBLE: u32 = 2232;
pub const SCI_GOTOLINE: u32 = 2024;
pub const SCI_BEGINUNDOACTION: u32 = 2078;
pub const SCI_ENDUNDOACTION: u32 = 2079;
pub const SCI_INDICSETSTYLE: u32 = 2080;
pub const SCI_INDICSETFORE: u32 = 2082;
pub const SCI_INDICSETALPHA: u32 = 2523;
pub const SCI_INDICSETOUTLINEALPHA: u32 = 2558;
pub const SCI_SETINDICATORCURRENT: u32 = 2500;
pub const SCI_INDICATORFILLRANGE: u32 = 2504;
pub const SCI_INDICATORCLEARRANGE: u32 = 2505;
pub const INDIC_STRAIGHTBOX: usize = 8;
pub const SCN_SAVEPOINTLEFT: u32 = 2003;
pub const SCN_UPDATEUI: u32 = 2007;

pub const STYLE_DEFAULT: usize = 32;
pub const STYLE_LINENUMBER: usize = 33;
pub const STYLE_BRACELIGHT: usize = 34;
pub const STYLE_BRACEBAD: usize = 35;
pub const SC_MARGIN_NUMBER: isize = 1;
pub const SC_MARGIN_SYMBOL: isize = 0;
pub const SC_MASK_FOLDERS: isize = 0xFE00_0000_u32 as i32 as isize;
pub const SC_WRAP_WORD: isize = 1;
pub const SC_WRAP_NONE: isize = 0;
pub const SC_CP_UTF8: isize = 65001;

pub const SC_MARKNUM_FOLDEREND: usize = 25;
pub const SC_MARKNUM_FOLDEROPENMID: usize = 26;
pub const SC_MARKNUM_FOLDERMIDTAIL: usize = 27;
pub const SC_MARKNUM_FOLDERTAIL: usize = 28;
pub const SC_MARKNUM_FOLDERSUB: usize = 29;
pub const SC_MARKNUM_FOLDER: usize = 30;
pub const SC_MARKNUM_FOLDEROPEN: usize = 31;
pub const SC_MARK_BOXPLUS: isize = 12;
pub const SC_MARK_BOXMINUS: isize = 14;
pub const SC_MARK_VLINE: isize = 9;
pub const SC_MARK_LCORNER: isize = 10;
pub const SC_MARK_BOXPLUSCONNECTED: isize = 13;
pub const SC_MARK_BOXMINUSCONNECTED: isize = 15;
pub const SC_MARK_TCORNER: isize = 11;

/// The single platform seam. Win32 implements this with `SendMessage`, GTK
/// with `scintilla_send_message`, Cocoa with `[ScintillaView message:]`.
/// Everything else in this crate is written against the trait.
pub trait Send {
    fn send(&self, msg: u32, w: usize, l: isize) -> isize;
}

pub fn rgb(r: u8, g: u8, b: u8) -> isize {
    (r as isize) | ((g as isize) << 8) | ((b as isize) << 16)
}

pub type Rgb = (u8, u8, u8);

/// Editor colours. `light()` matches the Electron app's default look.
#[derive(Clone)]
pub struct Theme {
    pub dark: bool,
    pub fg: Rgb,
    pub bg: Rgb,
    pub line_no_fg: Rgb,
    pub line_no_bg: Rgb,
    pub fold_fg: Rgb,
    pub caret_line: Rgb,
    pub sel_bg: Rgb,
    pub comment: Rgb,
    pub number: Rgb,
    pub keyword: Rgb,
    pub string: Rgb,
    pub ty: Rgb,
    pub mac: Rgb,
    pub func: Rgb,
    pub prop: Rgb,
}

impl Theme {
    pub fn light() -> Self {
        Self {
            dark: false,
            fg: (0x1F, 0x2D, 0x4D),
            bg: (0xFF, 0xFF, 0xFF),
            line_no_fg: (0x23, 0x78, 0x93),
            line_no_bg: (0xFF, 0xFF, 0xFF),
            fold_fg: (0x80, 0x80, 0x80),
            caret_line: (0xEE, 0xF3, 0xFB),
            sel_bg: (0xAD, 0xD6, 0xFF),
            comment: (0x00, 0x80, 0x00),
            number: (0x09, 0x86, 0x58),
            keyword: (0x00, 0x00, 0xFF),
            string: (0xA3, 0x15, 0x15),
            ty: (0x26, 0x7F, 0x99),
            mac: (0xAF, 0x00, 0xDB),
            func: (0x79, 0x5E, 0x26),
            prop: (0x04, 0x51, 0xA5),
        }
    }

    pub fn dark() -> Self {
        Self {
            dark: true,
            fg: (0xD4, 0xD4, 0xD4),
            bg: (0x1E, 0x1E, 0x1E),
            line_no_fg: (0x85, 0x85, 0x85),
            line_no_bg: (0x1E, 0x1E, 0x1E),
            fold_fg: (0x85, 0x85, 0x85),
            caret_line: (0x2A, 0x2A, 0x2B),
            sel_bg: (0x26, 0x4F, 0x78),
            comment: (0x6A, 0x99, 0x55),
            number: (0xB5, 0xCE, 0xA8),
            keyword: (0x56, 0x9C, 0xD6),
            string: (0xCE, 0x91, 0x78),
            ty: (0x4E, 0xC9, 0xB0),
            mac: (0xC5, 0x86, 0xC0),
            func: (0xDC, 0xDC, 0xAA),
            prop: (0x9C, 0xDC, 0xFE),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

/// Everything Notepad++ shows by default, applied through the portable API.
pub fn configure<S: Send>(sci: &S, font: &str, size: isize, theme: &Theme) {
    // A NUL in a hand-edited settings file must not take the whole app down.
    let f = CString::new(font.replace('\0', "")).unwrap_or_default();

    sci.send(SCI_SETCODEPAGE, SC_CP_UTF8 as usize, 0);
    sci.send(SCI_STYLESETFONT, STYLE_DEFAULT, f.as_ptr() as isize);
    sci.send(SCI_STYLESETSIZE, STYLE_DEFAULT, size);
    sci.send(SCI_STYLESETFORE, STYLE_DEFAULT, rgb(theme.fg.0, theme.fg.1, theme.fg.2));
    sci.send(SCI_STYLESETBACK, STYLE_DEFAULT, rgb(theme.bg.0, theme.bg.1, theme.bg.2));
    // STYLECLEARALL propagates the default to every style slot; do it before
    // any per-style colours or they get wiped.
    sci.send(SCI_STYLECLEARALL, 0, 0);

    // Matching braces are bold red, an unmatched one white on red (Notepad++'s defaults).
    let red = if theme.dark { rgb(0xFF, 0x6B, 0x6B) } else { rgb(0xFF, 0x00, 0x00) };
    sci.send(SCI_STYLESETFORE, STYLE_BRACELIGHT, red);
    sci.send(SCI_STYLESETBOLD, STYLE_BRACELIGHT, 1);
    sci.send(SCI_STYLESETFORE, STYLE_BRACEBAD, rgb(0xFF, 0xFF, 0xFF));
    sci.send(SCI_STYLESETBACK, STYLE_BRACEBAD, rgb(0xE0, 0x30, 0x30));

    sci.send(SCI_SETCARETFORE, rgb(theme.fg.0, theme.fg.1, theme.fg.2) as usize, 0);
    sci.send(SCI_SETSELBACK, 1, rgb(theme.sel_bg.0, theme.sel_bg.1, theme.sel_bg.2));
    sci.send(SCI_SETCARETLINEVISIBLE, 1, 0);
    sci.send(SCI_SETCARETLINEBACK, rgb(theme.caret_line.0, theme.caret_line.1, theme.caret_line.2) as usize, 0);

    sci.send(SCI_SETTABWIDTH, 4, 0);
    sci.send(SCI_SETUSETABS, 0, 0);
}

/// Line-number margin. Scintilla maintains it, including under word wrap.
pub fn setup_line_numbers<S: Send>(sci: &S, theme: &Theme, width: isize) {
    sci.send(SCI_STYLESETFORE, STYLE_LINENUMBER, rgb(theme.line_no_fg.0, theme.line_no_fg.1, theme.line_no_fg.2));
    sci.send(SCI_STYLESETBACK, STYLE_LINENUMBER, rgb(theme.line_no_bg.0, theme.line_no_bg.1, theme.line_no_bg.2));
    sci.send(SCI_SETMARGINTYPEN, 1, SC_MARGIN_NUMBER);
    // The number margin must not draw markers (bookmarks live in margin 0).
    sci.send(SCI_SETMARGINMASKN, 1, 0);
    sci.send(SCI_SETMARGINWIDTHN, 1, width);
}

/// Folding margin with the full connected-line marker set Notepad++ uses.
/// Call after the lexer is attached: properties are forwarded to the lexer and
/// are silently dropped if none is set yet.
pub fn setup_folding<S: Send>(sci: &S, theme: &Theme) {
    sci.send(SCI_SETPROPERTY, b"fold\0".as_ptr() as usize, b"1\0".as_ptr() as isize);
    sci.send(SCI_SETPROPERTY, b"fold.compact\0".as_ptr() as usize, b"0\0".as_ptr() as isize);
    sci.send(SCI_SETMARGINTYPEN, 2, SC_MARGIN_SYMBOL);
    sci.send(SCI_SETMARGINMASKN, 2, SC_MASK_FOLDERS);
    sci.send(SCI_SETMARGINWIDTHN, 2, 16);
    sci.send(SCI_SETMARGINSENSITIVEN, 2, 1);
    let c = theme.line_no_bg;
    sci.send(SCI_SETFOLDMARGINCOLOUR, 1, rgb(c.0, c.1, c.2));
    sci.send(SCI_SETFOLDMARGINHICOLOUR, 1, rgb(c.0, c.1, c.2));

    let markers: [(usize, isize); 7] = [
        (SC_MARKNUM_FOLDER, SC_MARK_BOXPLUS),
        (SC_MARKNUM_FOLDEROPEN, SC_MARK_BOXMINUS),
        (SC_MARKNUM_FOLDERSUB, SC_MARK_VLINE),
        (SC_MARKNUM_FOLDERTAIL, SC_MARK_LCORNER),
        (SC_MARKNUM_FOLDEREND, SC_MARK_BOXPLUSCONNECTED),
        (SC_MARKNUM_FOLDEROPENMID, SC_MARK_BOXMINUSCONNECTED),
        (SC_MARKNUM_FOLDERMIDTAIL, SC_MARK_TCORNER),
    ];
    for (num, shape) in markers {
        sci.send(SCI_MARKERDEFINE, num, shape);
        sci.send(SCI_MARKERSETFORE, num, rgb(theme.bg.0, theme.bg.1, theme.bg.2));
        sci.send(SCI_MARKERSETBACK, num, rgb(theme.fold_fg.0, theme.fold_fg.1, theme.fold_fg.2));
    }
    sci.send(SCI_SETAUTOMATICFOLD, 7, 0); // show, click and change
    sci.send(SCI_SETFOLDFLAGS, 16, 0); // draw a line below a folded block
}

pub fn set_wrap<S: Send>(sci: &S, on: bool) {
    sci.send(SCI_SETWRAPMODE, if on { SC_WRAP_WORD as usize } else { SC_WRAP_NONE as usize }, 0);
}

pub fn set_text<S: Send>(sci: &S, text: &str) {
    sci.send(SCI_CLEARALL, 0, 0);
    add_text(sci, text);
}

/// Length-prefixed insert, so text containing NUL bytes is preserved.
pub fn add_text<S: Send>(sci: &S, text: &str) {
    sci.send(SCI_ADDTEXT, text.len(), text.as_ptr() as isize);
}

pub fn get_text<S: Send>(sci: &S) -> String {
    let len = length(sci).max(0) as usize;
    let mut buf = vec![0u8; len + 1];
    sci.send(SCI_GETTEXT, len + 1, buf.as_mut_ptr() as isize);
    buf.truncate(len);
    String::from_utf8_lossy(&buf).into_owned()
}

pub fn line_count<S: Send>(sci: &S) -> isize {
    sci.send(SCI_GETLINECOUNT, 0, 0)
}

pub fn length<S: Send>(sci: &S) -> isize {
    sci.send(SCI_GETLENGTH, 0, 0)
}
