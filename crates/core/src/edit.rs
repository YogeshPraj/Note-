//! Text and line operations behind the Edit menu: case conversion, sorting,
//! blank handling, commenting, the column editor and date/time insertion.
//!
//! The transforms are pure functions over a block of text (and are unit
//! tested); `apply_lines` / `apply_range` apply one to the editor as a single
//! undo step.

use super::sci::{self, Send};

/// Split a block into lines, remembering whether it ended with a line break.
/// Only one trailing break is dropped, so a blank last line is preserved.
fn split(text: &str) -> (Vec<&str>, bool) {
    let (body, trailing) = if let Some(b) = text.strip_suffix("\r\n") {
        (b, true)
    } else if let Some(b) = text.strip_suffix(['\n', '\r']) {
        (b, true)
    } else {
        (text, false)
    };
    if body.is_empty() && !trailing {
        return (Vec::new(), false);
    }
    (lines_of(body), trailing)
}

/// The lines of `text` (no trailing-break handling). A block with a CR but no LF at all comes from a classic Mac
/// document, whose lines end in a lone CR; anything else breaks at LF (dropping the CR of a CR LF).
pub(crate) fn lines_of(text: &str) -> Vec<&str> {
    if !text.contains('\n') && text.contains('\r') {
        return text.split('\r').collect();
    }
    text.split('\n').map(|l| l.trim_end_matches('\r')).collect()
}

fn join(lines: &[String], eol: &str, trailing: bool) -> String {
    let mut s = lines.join(eol);
    if trailing {
        s.push_str(eol);
    }
    s
}

fn map_lines(text: &str, eol: &str, f: impl Fn(Vec<&str>) -> Vec<String>) -> String {
    let (lines, trailing) = split(text);
    if lines.is_empty() {
        return text.to_string();
    }
    join(&f(lines), eol, trailing)
}

mod lines;
pub use lines::*;

mod blanks;
pub use blanks::*;

mod case;
pub use case::*;

mod comments;
pub use comments::*;
mod inserts;
pub use inserts::*;

// ── applying to the editor ───────────────────────────────────────────────────

/// What a line operation covers when nothing is selected.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Scope {
    WholeDocument,
    CurrentLine,
}

fn replace_range<S: Send>(s: &S, start: usize, end: usize, new: &str, select: bool) {
    s.send(sci::SCI_BEGINUNDOACTION, 0, 0);
    s.send(sci::SCI_SETTARGETSTART, start, 0);
    s.send(sci::SCI_SETTARGETEND, end, 0);
    s.send(sci::SCI_REPLACETARGET, new.len(), new.as_ptr() as isize);
    s.send(sci::SCI_ENDUNDOACTION, 0, 0);
    if select {
        s.send(sci::SCI_SETSEL, start, crate::num::to_isize(start + new.len()));
    }
}

/// Replace the whole document as one undo step (the JSON preview writes its edits back this way).
pub fn replace_whole<S: Send>(s: &S, new: &str) {
    let len = crate::num::to_usize(sci::length(s).max(0));
    replace_range(s, 0, len, new, false);
}

/// Run `f` over the selected whole lines. With no selection it covers the whole
/// document or the caret's line, per `scope`. Returns true if the text changed.
pub fn apply_lines<S: Send>(s: &S, scope: Scope, f: &dyn Fn(&str) -> String) -> bool {
    let a = crate::num::to_usize(s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0));
    let b = crate::num::to_usize(s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0));
    let empty = a == b;
    let (start, end) = if empty && scope == Scope::WholeDocument {
        (0, crate::num::to_usize(sci::length(s).max(0)))
    } else {
        let la = crate::num::to_usize(s.send(sci::SCI_LINEFROMPOSITION, a, 0));
        let mut lb = crate::num::to_usize(s.send(sci::SCI_LINEFROMPOSITION, b, 0));
        // A selection that ends at column 0 does not include that line.
        if lb > la && crate::num::to_usize(s.send(sci::SCI_POSITIONFROMLINE, lb, 0)) == b {
            lb -= 1;
        }
        (crate::num::to_usize(s.send(sci::SCI_POSITIONFROMLINE, la, 0)), crate::num::to_usize(s.send(sci::SCI_GETLINEENDPOSITION, lb, 0)))
    };
    let text = sci::get_text(s);
    let old = &text[start..end];
    let new = f(old);
    if new == old {
        return false;
    }
    replace_range(s, start, end, &new, !empty);
    if empty && scope == Scope::CurrentLine {
        s.send(sci::SCI_GOTOPOS, start + new.len(), 0);
    }
    true
}

/// Run `f` over exactly the selected text. With no selection it does nothing,
/// unless `line_if_empty` asks for the caret's line instead.
pub fn apply_range<S: Send>(s: &S, line_if_empty: bool, f: &dyn Fn(&str) -> String) -> bool {
    let a = crate::num::to_usize(s.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0));
    let b = crate::num::to_usize(s.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0));
    let (start, end) = if a != b {
        (a, b)
    } else if line_if_empty {
        let l = crate::num::to_usize(s.send(sci::SCI_LINEFROMPOSITION, a, 0));
        (crate::num::to_usize(s.send(sci::SCI_POSITIONFROMLINE, l, 0)), crate::num::to_usize(s.send(sci::SCI_GETLINEENDPOSITION, l, 0)))
    } else {
        return false;
    };
    let text = sci::get_text(s);
    let old = &text[start..end];
    let new = f(old);
    if new == old {
        return false;
    }
    replace_range(s, start, end, &new, true);
    true
}

#[cfg(test)]
mod tests;
