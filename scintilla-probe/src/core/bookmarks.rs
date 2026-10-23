//! Bookmarks: a marker in its own gutter margin, toggled by click or key.

use super::sci::{self, Send, Theme, rgb};

const MARKER: usize = 1;
const MASK: usize = 1 << MARKER;

/// Left-most margin (0) holds the bookmark glyph; clicking it toggles a bookmark.
pub fn setup<S: Send>(s: &S, theme: &Theme) {
    s.send(sci::SCI_SETMARGINTYPEN, 0, sci::SC_MARGIN_SYMBOL);
    s.send(sci::SCI_SETMARGINMASKN, 0, MASK as isize);
    s.send(sci::SCI_SETMARGINWIDTHN, 0, 16);
    s.send(sci::SCI_SETMARGINSENSITIVEN, 0, 1);
    s.send(sci::SCI_MARKERDEFINE, MARKER, sci::SC_MARK_BOOKMARK);
    let (f, b) = if theme.dark { ((0x1E, 0x1E, 0x1E), (0x56, 0x9C, 0xD6)) } else { ((0xFF, 0xFF, 0xFF), (0x1F, 0x6F, 0xE0)) };
    s.send(sci::SCI_MARKERSETFORE, MARKER, rgb(f.0, f.1, f.2));
    s.send(sci::SCI_MARKERSETBACK, MARKER, rgb(b.0, b.1, b.2));
}

pub fn current_line<S: Send>(s: &S) -> usize {
    let pos = s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
    s.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0) as usize
}

pub fn is_set<S: Send>(s: &S, line: usize) -> bool {
    s.send(sci::SCI_MARKERGET, line, 0) as usize & MASK != 0
}

pub fn toggle<S: Send>(s: &S, line: usize) {
    if is_set(s, line) {
        s.send(sci::SCI_MARKERDELETE, line, MARKER as isize);
    } else {
        s.send(sci::SCI_MARKERADD, line, MARKER as isize);
    }
}

/// Jump to the next/previous bookmark, wrapping around. Returns false if none.
pub fn goto<S: Send>(s: &S, forward: bool) -> bool {
    let line = current_line(s);
    let total = sci::line_count(s).max(1) as usize;
    let found = if forward {
        let n = s.send(sci::SCI_MARKERNEXT, line + 1, MASK as isize);
        if n >= 0 { n } else { s.send(sci::SCI_MARKERNEXT, 0, MASK as isize) }
    } else {
        let p = if line == 0 { -1 } else { s.send(sci::SCI_MARKERPREVIOUS, line - 1, MASK as isize) };
        if p >= 0 { p } else { s.send(sci::SCI_MARKERPREVIOUS, total - 1, MASK as isize) }
    };
    if found < 0 {
        return false;
    }
    s.send(sci::SCI_ENSUREVISIBLE, found as usize, 0);
    s.send(sci::SCI_GOTOLINE, found as usize, 0);
    s.send(sci::SCI_SCROLLCARET, 0, 0);
    true
}

pub fn clear_all<S: Send>(s: &S) {
    s.send(sci::SCI_MARKERDELETEALL, MARKER, 0);
}

/// All bookmarked lines (0-based), in order.
pub fn lines<S: Send>(s: &S) -> Vec<usize> {
    let mut out = Vec::new();
    let mut line = 0usize;
    loop {
        let n = s.send(sci::SCI_MARKERNEXT, line, MASK as isize);
        if n < 0 {
            return out;
        }
        out.push(n as usize);
        line = n as usize + 1;
    }
}
