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

mod messages;
pub use messages::*;

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
#[derive(Clone, Debug)]
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

fn luminance(c: Rgb) -> f64 {
    (0.299 * f64::from(c.0) + 0.587 * f64::from(c.1) + 0.114 * f64::from(c.2)) / 255.0
}

/// The selection colour, moved towards the text colour when the theme's own is too close to the background to see.
pub fn visible_selection(bg: Rgb, fg: Rgb, sel: Rgb) -> Rgb {
    const MIN_DIFFERENCE: f64 = 0.14;
    if (luminance(sel) - luminance(bg)).abs() >= MIN_DIFFERENCE {
        return sel;
    }
    // 30% of the way from the background to the text colour.
    let mix = |b: u8, f: u8| crate::num::to_u8(i32::from(b) + (i32::from(f) - i32::from(b)) * 3 / 10);
    (mix(bg.0, fg.0), mix(bg.1, fg.1), mix(bg.2, fg.2))
}

/// The usual defaults, applied through the portable API.
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

    // Matching braces are bold red, an unmatched one white on red (the classic defaults).
    let red = if theme.dark { rgb(0xFF, 0x6B, 0x6B) } else { rgb(0xFF, 0x00, 0x00) };
    sci.send(SCI_STYLESETFORE, STYLE_BRACELIGHT, red);
    sci.send(SCI_STYLESETBOLD, STYLE_BRACELIGHT, 1);
    sci.send(SCI_STYLESETFORE, STYLE_BRACEBAD, rgb(0xFF, 0xFF, 0xFF));
    sci.send(SCI_STYLESETBACK, STYLE_BRACEBAD, rgb(0xE0, 0x30, 0x30));

    sci.send(SCI_SETCARETFORE, crate::num::to_usize(rgb(theme.fg.0, theme.fg.1, theme.fg.2)), 0);
    let sel = visible_selection(theme.bg, theme.fg, theme.sel_bg);
    sci.send(SCI_SETSELBACK, 1, rgb(sel.0, sel.1, sel.2));
    // Scintilla's default for an unfocused selection is a light grey, which all but vanishes on a dark theme.
    sci.send(SCI_SETELEMENTCOLOUR, SC_ELEMENT_SELECTION_INACTIVE_BACK, rgb(sel.0, sel.1, sel.2) | 0xFF00_0000);
    sci.send(SCI_SETCARETLINEVISIBLE, 1, 0);
    sci.send(SCI_SETCARETLINEBACK, crate::num::to_usize(rgb(theme.caret_line.0, theme.caret_line.1, theme.caret_line.2)), 0);

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

/// Folding margin with the full connected-line marker set.
/// Call after the lexer is attached: properties are forwarded to the lexer and
/// are silently dropped if none is set yet.
pub fn setup_folding<S: Send>(sci: &S, theme: &Theme) {
    sci.send(SCI_SETPROPERTY, c"fold".as_ptr() as usize, c"1".as_ptr() as isize);
    sci.send(SCI_SETPROPERTY, c"fold.compact".as_ptr() as usize, c"0".as_ptr() as isize);
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
    let len = crate::num::to_usize(length(sci).max(0));
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

#[cfg(test)]
mod tests;
