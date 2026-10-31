//! View menu behaviour that lives in the editor: visible symbols, folding by
//! level, hidden lines and the document summary.

use super::sci::{self, Send};

// ── show symbols ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Symbols {
    /// Spaces and tabs.
    pub whitespace: bool,
    pub eol: bool,
    /// Control characters such as ESC or SOH, shown as small labelled boxes.
    pub control: bool,
    pub indent_guides: bool,
    pub wrap_symbol: bool,
}

impl Symbols {
    pub fn non_printing(&self) -> bool {
        self.whitespace && self.eol
    }
    pub fn all(&self) -> bool {
        self.whitespace && self.eol && self.control
    }
}

pub fn apply_symbols<S: Send>(s: &S, v: &Symbols) {
    s.send(sci::SCI_SETVIEWWS, v.whitespace as usize, 0);
    s.send(sci::SCI_SETVIEWEOL, v.eol as usize, 0);
    s.send(sci::SCI_SETINDENTATIONGUIDES, if v.indent_guides { 3 } else { 0 }, 0); // SC_IV_LOOKBOTH
    s.send(sci::SCI_SETWRAPVISUALFLAGS, v.wrap_symbol as usize, 0); // SC_WRAPVISUALFLAG_END
    // Scintilla draws control characters as labelled boxes unless told otherwise;
    // an empty representation hides them. Tab, CR and LF are never touched.
    for c in 1u8..32 {
        if matches!(c, 9 | 10 | 13) {
            continue;
        }
        let key = [c, 0u8];
        if v.control {
            s.send(sci::SCI_CLEARREPRESENTATION, key.as_ptr() as usize, 0);
        } else {
            s.send(sci::SCI_SETREPRESENTATION, key.as_ptr() as usize, b"\0".as_ptr() as isize);
        }
    }
}

// ── folding ──────────────────────────────────────────────────────────────────

const FOLD_BASE: isize = 0x400;
const FOLD_HEADER: isize = 0x2000;
const FOLD_MASK: isize = 0xFFF;

fn line_count<S: Send>(s: &S) -> usize {
    sci::line_count(s).max(0) as usize
}

/// 1-based fold level of a header line, or None for other lines.
fn header_level<S: Send>(s: &S, line: usize) -> Option<usize> {
    let lv = s.send(sci::SCI_GETFOLDLEVEL, line, 0);
    (lv & FOLD_HEADER != 0).then(|| ((lv & FOLD_MASK) - FOLD_BASE + 1).max(1) as usize)
}

pub fn fold_all<S: Send>(s: &S, expand: bool) {
    s.send(sci::SCI_FOLDALL, expand as usize, 0);
}

/// Fold or unfold every block at nesting `level` (1 = outermost).
pub fn fold_level<S: Send>(s: &S, level: usize, expand: bool) -> usize {
    let mut n = 0;
    for l in 0..line_count(s) {
        if header_level(s, l) == Some(level) {
            s.send(sci::SCI_FOLDLINE, l, expand as isize);
            n += 1;
        }
    }
    n
}

/// Fold or unfold every block at the nesting level of the caret's block.
pub fn fold_current_level<S: Send>(s: &S, expand: bool) -> bool {
    let pos = s.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0) as usize;
    let line = s.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0) as usize;
    let header = if header_level(s, line).is_some() {
        Some(line)
    } else {
        let p = s.send(sci::SCI_GETFOLDPARENT, line, 0);
        (p >= 0).then_some(p as usize)
    };
    match header.and_then(|h| header_level(s, h)) {
        Some(level) => fold_level(s, level, expand) > 0,
        None => false,
    }
}

// ── hidden lines ─────────────────────────────────────────────────────────────

pub const MARKER_HIDDEN: usize = 2;

/// Hide the selected lines (or the caret's line). A marker on the line above
/// shows where they went; clicking it brings them back.
pub fn hide_selected_lines<S: Send>(s: &S) -> Result<usize, String> {
    let a = s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0) as usize;
    let b = s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0) as usize;
    let first = s.send(sci::SCI_LINEFROMPOSITION, a, 0).max(0) as usize;
    let mut last = s.send(sci::SCI_LINEFROMPOSITION, b, 0).max(0) as usize;
    if last > first && s.send(sci::SCI_POSITIONFROMLINE, last, 0) as usize == b {
        last -= 1;
    }
    if first == 0 {
        return Err("The first line cannot be hidden".into());
    }
    s.send(sci::SCI_HIDELINES, first, last as isize);
    s.send(sci::SCI_MARKERADD, first - 1, MARKER_HIDDEN as isize);
    // Keep the caret on a visible line.
    s.send(sci::SCI_GOTOLINE, first - 1, 0);
    Ok(last - first + 1)
}

/// Show the lines hidden just below `line` and remove its marker.
pub fn show_hidden_after<S: Send>(s: &S, line: usize) -> usize {
    let total = line_count(s);
    let mut l = line + 1;
    let mut shown = 0;
    while l < total && s.send(sci::SCI_GETLINEVISIBLE, l, 0) == 0 {
        s.send(sci::SCI_SHOWLINES, l, l as isize);
        l += 1;
        shown += 1;
    }
    s.send(sci::SCI_MARKERDELETE, line, MARKER_HIDDEN as isize);
    shown
}

pub fn has_hidden_marker<S: Send>(s: &S, line: usize) -> bool {
    s.send(sci::SCI_MARKERGET, line, 0) as usize & (1 << MARKER_HIDDEN) != 0
}

// ── summary ──────────────────────────────────────────────────────────────────

#[derive(Debug, PartialEq)]
pub struct Counts {
    pub bytes: usize,
    /// Characters, not counting line breaks.
    pub chars: usize,
    pub words: usize,
    pub lines: usize,
}

pub fn count_text(text: &str) -> Counts {
    let chars = text.chars().filter(|c| *c != '\r' && *c != '\n').count();
    let lines = if text.is_empty() {
        1
    } else {
        // A lone CR is a break too; CRLF counts once.
        1 + text.replace("\r\n", "\n").chars().filter(|c| *c == '\n' || *c == '\r').count()
    };
    Counts { bytes: text.len(), chars, words: text.split_whitespace().count(), lines }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_text() {
        let c = count_text("one two\r\nthree\n\nfour five six");
        assert_eq!(c, Counts { bytes: 29, chars: 25, words: 6, lines: 4 });
        assert_eq!(count_text(""), Counts { bytes: 0, chars: 0, words: 0, lines: 1 });
        assert_eq!(count_text("\u{e9}a").bytes, 3);
        assert_eq!(count_text("a\rb").lines, 2);
    }

    #[test]
    fn symbols_summary_flags() {
        let mut v = Symbols::default();
        assert!(!v.non_printing() && !v.all());
        v.whitespace = true;
        v.eol = true;
        assert!(v.non_printing() && !v.all());
        v.control = true;
        assert!(v.all());
    }
}
