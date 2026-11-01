//! Text operations on the active document: lines, case, comments, columns, completion and bookmarks.

use super::*;

mod bookmarks_ops;

impl<S: Send> Workspace<S> {
    /// Is the active tab a Markdown document (so the Markdown toolbar applies)?
    pub fn is_markdown(&self) -> bool {
        !self.is_whiteboard() && self.preview_kind() == Some(preview::Kind::Markdown)
    }

    /// Apply a Markdown formatting command to the selection as one undo step.
    /// Returns a message when it could not be done.
    pub fn md_format(&mut self, fmt: mdformat::Fmt) -> Option<String> {
        if !self.is_markdown() {
            return Some("Not a Markdown file".into());
        }
        if self.send(sci::SCI_GETSELECTIONS, 0, 0) != 1 || self.send(sci::SCI_SELECTIONISRECTANGLE, 0, 0) != 0 {
            return Some("Select a single block of text first".into());
        }
        let text = sci::get_text(&self.sci);
        let sel =
            (crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0)), crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0)));
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let edit = mdformat::apply(fmt, &text, sel, eol);
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETSTART, edit.start, 0);
        self.send(sci::SCI_SETTARGETEND, edit.end, 0);
        self.send(sci::SCI_REPLACETARGET, edit.text.len(), edit.text.as_ptr() as isize);
        self.send(sci::SCI_SETSEL, edit.sel.0, crate::num::to_isize(edit.sel.1));
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        None
    }

    /// Apply a line operation to the selection (or whole document / current line,
    /// depending on the operation). Returns a message to show the user when
    /// nothing could be done.
    pub fn line_op(&mut self, op: LineOp) -> Option<String> {
        use edit::Scope::{CurrentLine, WholeDocument};
        let t = &self.tabs[self.active];
        let eol = match t.eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let tab = crate::num::to_usize(self.send(sci::SCI_GETTABWIDTH, 0, 0).max(1));
        let edge = crate::num::to_usize(self.settings.edge_column.max(0));
        let s = &self.sci;
        let has_selection = self.send(sci::SCI_GETSELECTIONSTART, 0, 0) != self.send(sci::SCI_GETSELECTIONEND, 0, 0);
        let lines = |scope, f: &dyn Fn(&str) -> String| edit::apply_lines(s, scope, f);
        let changed = match op {
            LineOp::Sort(kind, desc) => lines(WholeDocument, &|x| edit::sort_lines_by(x, eol, kind, desc)),
            LineOp::RemoveDuplicates => lines(WholeDocument, &|x| edit::remove_duplicate_lines(x, eol)),
            LineOp::RemoveConsecutiveDuplicates => lines(WholeDocument, &|x| edit::remove_consecutive_duplicates(x, eol)),
            LineOp::RemoveEmpty => lines(WholeDocument, &|x| edit::remove_empty_lines(x, eol)),
            LineOp::RemoveBlank => lines(WholeDocument, &|x| edit::remove_blank_lines(x, eol)),
            LineOp::Reverse => lines(WholeDocument, &|x| edit::reverse_lines(x, eol)),
            LineOp::Randomize => {
                let seed = edit::seed_now();
                lines(WholeDocument, &|x| edit::randomize_lines(x, eol, seed))
            }
            LineOp::Join | LineOp::Split if !has_selection => return Some("Select the lines first".into()),
            LineOp::Join => lines(CurrentLine, &|x| edit::join_lines(x)),
            // At the edge column when one is set (Preferences), else at 80.
            LineOp::Split => {
                let width = if edge > 0 { edge } else { 80 };
                lines(CurrentLine, &|x| edit::split_lines(x, eol, width))
            }
            LineOp::TrimTrailing => lines(WholeDocument, &|x| edit::trim_trailing(x, eol)),
            LineOp::TrimLeading => lines(WholeDocument, &|x| edit::trim_leading(x, eol)),
            LineOp::TrimBoth => lines(WholeDocument, &|x| edit::trim_both(x, eol)),
            LineOp::EolToSpace => lines(WholeDocument, &|x| edit::join_all(x)),
            LineOp::RemoveUnnecessaryBlanks => lines(WholeDocument, &|x| edit::remove_unnecessary_blanks(x, eol)),
            LineOp::TabsToSpaces => lines(WholeDocument, &|x| edit::tabs_to_spaces(x, eol, tab)),
            LineOp::SpacesToTabsAll => lines(WholeDocument, &|x| edit::spaces_to_tabs(x, eol, tab, false)),
            LineOp::SpacesToTabsLeading => lines(WholeDocument, &|x| edit::spaces_to_tabs(x, eol, tab, true)),
            LineOp::ToggleComment | LineOp::Comment | LineOp::Uncomment | LineOp::BlockComment | LineOp::BlockUncomment => match self.comment_op(op, eol) {
                Ok(changed) => changed,
                Err(message) => return Some(message),
            },
        };
        (!changed).then(|| "Nothing to change".into())
    }

    /// The comment operations of [`Workspace::line_op`]; `Err` carries the message to show.
    pub(super) fn comment_op(&mut self, op: LineOp, eol: &str) -> Result<bool, String> {
        use edit::Scope::CurrentLine;
        let t = &self.tabs[self.active];
        let (lang, label) = (t.lang, t.lang_label);
        let s = &self.sci;
        let lines = |f: &dyn Fn(&str) -> String| edit::apply_lines(s, CurrentLine, f);
        let none = || "This language has no comment syntax".to_string();
        let syn = edit::comment_syntax(label, lang);
        match op {
            LineOp::ToggleComment | LineOp::Comment | LineOp::Uncomment => {
                let mode = match op {
                    LineOp::ToggleComment => edit::CommentMode::Toggle,
                    LineOp::Comment => edit::CommentMode::Comment,
                    _ => edit::CommentMode::Uncomment,
                };
                let column_zero = label == "Fortran (fixed form)";
                if edit::comment_single("x", eol, syn, mode, column_zero).is_none() {
                    return Err(none());
                }
                Ok(lines(&|x| edit::comment_single(x, eol, syn, mode, column_zero).unwrap_or_else(|| x.to_string())))
            }
            _ if syn.start.is_empty() || syn.end.is_empty() => {
                // Without stream-comment symbols Notepad++ falls back on single-line comments.
                if syn.line.is_empty() {
                    return Err(none());
                }
                let mode = if op == LineOp::BlockComment { edit::CommentMode::Comment } else { edit::CommentMode::Uncomment };
                Ok(lines(&|x| edit::comment_single(x, eol, syn, mode, false).unwrap_or_else(|| x.to_string())))
            }
            LineOp::BlockComment => Ok(self.stream_comment(syn)),
            _ => Ok(self.stream_uncomment(syn)),
        }
    }

    /// "Block Comment": wrap the selection (or, with none, the current line from its first
    /// non-blank character) in the language's start and end symbols.
    pub(super) fn stream_comment(&mut self, syn: edit::Comments) -> bool {
        let (mut a, mut b) =
            (crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0)), crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0)));
        if a == b {
            let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, a, 0));
            a = crate::num::to_usize(self.send(sci::SCI_GETLINEINDENTPOSITION, line, 0).max(0));
            b = crate::num::to_usize(self.send(sci::SCI_GETLINEENDPOSITION, line, 0).max(0));
            if b <= a {
                return false;
            }
        }
        let text = sci::get_text(&self.sci);
        let wrapped = edit::stream_comment(&text[a..b], syn.start, syn.end);
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETRANGE, a, crate::num::to_isize(b));
        self.send(sci::SCI_REPLACETARGET, wrapped.len(), wrapped.as_ptr() as isize);
        self.send(sci::SCI_SETSEL, a + syn.start.len() + 1, crate::num::to_isize(a + wrapped.len() - syn.end.len() - 1));
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        true
    }

    /// "Block Uncomment": remove the comment around or inside the selection.
    pub(super) fn stream_uncomment(&mut self, syn: edit::Comments) -> bool {
        let (a, b) =
            (crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0)), crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0)));
        let text = sci::get_text(&self.sci);
        let Some((s, e, inner)) = edit::undo_stream_comment(&text, (a, b), syn.start, syn.end) else { return false };
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETRANGE, s, crate::num::to_isize(e));
        self.send(sci::SCI_REPLACETARGET, inner.len(), inner.as_ptr() as isize);
        self.send(sci::SCI_SETSEL, s, crate::num::to_isize(s + inner.len()));
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        true
    }

    /// Convert the selection (or the whole document) with a text tool.
    /// Returns a message when it could not be done.
    pub fn text_tool(&mut self, op: TextTool) -> Option<String> {
        let selected = search::selected_text(&self.sci);
        let whole = selected.is_empty();
        let input = if whole { sci::get_text(&self.sci) } else { selected };
        if input.trim().is_empty() {
            return Some("Nothing to convert".into());
        }
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let result: Result<String, crate::error::Error> = match op {
            TextTool::Base64Encode => Ok(textools::base64_encode(input.as_bytes())),
            TextTool::Base64Decode => textools::base64_decode(&input).map(|b| String::from_utf8_lossy(&b).into_owned()),
            TextTool::UrlEncode => Ok(textools::url_encode(&input)),
            TextTool::UrlDecode => Ok(textools::url_decode(&input)),
            TextTool::JsonPretty => textools::json_pretty(&input).map(|s| s.replace('\n', eol)),
            TextTool::JsonMinify => textools::json_minify(&input),
            TextTool::XmlPretty => textools::xml_pretty(&input).map(|s| s.replace('\n', eol)),
            TextTool::XmlMinify => textools::xml_minify(&input),
        };
        let out = match result {
            Ok(o) => o,
            Err(e) => return Some(format!("Cannot convert: {e}")),
        };
        let (start, end) = if whole {
            (0, crate::num::to_usize(sci::length(&self.sci).max(0)))
        } else {
            (crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0)), crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0)))
        };
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETSTART, start, 0);
        self.send(sci::SCI_SETTARGETEND, end, 0);
        self.send(sci::SCI_REPLACETARGET, out.len(), out.as_ptr() as isize);
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        None
    }

    /// Change the case of the selected text. Column selections are left alone.
    pub fn case_op(&mut self, op: CaseOp) -> Option<String> {
        if self.send(sci::SCI_GETSELECTIONS, 0, 0) != 1 || self.send(sci::SCI_SELECTIONISRECTANGLE, 0, 0) != 0 {
            return Some("Select a single block of text first".into());
        }
        let seed = edit::seed_now();
        let changed = edit::apply_range(&self.sci, false, &|x| match op {
            CaseOp::Proper => edit::proper_case(x, false),
            CaseOp::ProperBlend => edit::proper_case(x, true),
            CaseOp::Sentence => edit::sentence_case(x, false),
            CaseOp::SentenceBlend => edit::sentence_case(x, true),
            CaseOp::Invert => edit::invert_case(x),
            CaseOp::Random => edit::random_case(x, seed),
        });
        (!changed).then(|| "Select some text first".into())
    }

    /// First call remembers the caret; the second selects from there to the new
    /// caret as a stream or (for `column`) a rectangle.
    pub fn begin_end_select(&mut self, column: bool) -> Option<String> {
        let doc = self.tabs[self.active].doc;
        let caret = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
        match self.select_anchor.take() {
            Some((d, anchor)) if d == doc => {
                if column {
                    self.send(sci::SCI_SETRECTANGULARSELECTIONANCHOR, anchor, 0);
                    self.send(sci::SCI_SETRECTANGULARSELECTIONCARET, caret, 0);
                } else {
                    self.send(sci::SCI_SETSEL, anchor, crate::num::to_isize(caret));
                }
                None
            }
            _ => {
                self.select_anchor = Some((doc, caret));
                Some("Selection start marked: move the caret and repeat to select".into())
            }
        }
    }

    /// The lines the column editor works on, and the character column it inserts at.
    pub(super) fn column_target(&self) -> (usize, usize, usize) {
        let text = sci::get_text(&self.sci);
        let a = crate::num::to_usize(self.send(sci::SCI_GETANCHOR, 0, 0).max(0));
        let c = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
        let col_of = |pos: usize| {
            let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, pos, 0));
            let start = crate::num::to_usize(self.send(sci::SCI_POSITIONFROMLINE, line, 0));
            text.get(start..pos).map_or(0, |s| s.chars().count())
        };
        let (la, lc) = (crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, a, 0)), crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, c, 0)));
        (la.min(lc), la.max(lc), col_of(a).min(col_of(c)))
    }

    /// Insert text or a number sequence at the caret column of every selected line.
    pub fn column_edit(&mut self, text: Option<&str>, numbers: Option<&edit::ColumnNumbers>) -> Option<String> {
        let (first, last, col) = self.column_target();
        let count = last - first + 1;
        let insert: Vec<String> = match (numbers, text) {
            (Some(n), _) => edit::number_sequence(n, count),
            (None, Some(t)) => vec![t.to_string(); count],
            (None, None) => return Some("Nothing to insert".into()),
        };
        let doc = sci::get_text(&self.sci);
        let start = crate::num::to_usize(self.send(sci::SCI_POSITIONFROMLINE, first, 0));
        let end = crate::num::to_usize(self.send(sci::SCI_GETLINEENDPOSITION, last, 0));
        let old = &doc[start..end];
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let src: Vec<&str> = old.split(['\n']).map(|l| l.trim_end_matches('\r')).collect();
        let new = edit::insert_at_column(&src, col, &insert).join(eol);
        if new == old {
            return Some("Nothing to change".into());
        }
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_SETTARGETSTART, start, 0);
        self.send(sci::SCI_SETTARGETEND, end, 0);
        self.send(sci::SCI_REPLACETARGET, new.len(), new.as_ptr() as isize);
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
        self.send(sci::SCI_SETSEL, start, crate::num::to_isize(start + new.len()));
        None
    }

    /// Replace the selection (or insert at the caret) with `text`.
    pub fn insert_text(&mut self, text: &str) {
        self.insert_bytes(text.as_bytes());
    }

    /// Like `insert_text` for raw bytes (binary paste may hold NULs).
    pub fn insert_bytes(&mut self, bytes: &[u8]) {
        self.send(sci::SCI_BEGINUNDOACTION, 0, 0);
        self.send(sci::SCI_CLEAR, 0, 0);
        // The document is UTF-8; anything else on the clipboard is repaired rather than stored raw
        // (which would make every byte offset we compute from the text wrong).
        let text = String::from_utf8_lossy(bytes);
        self.send(sci::SCI_ADDTEXT, text.len(), text.as_ptr() as isize);
        self.send(sci::SCI_ENDUNDOACTION, 0, 0);
    }

    /// Add an empty line above or below the caret's line.
    pub fn insert_blank_line(&mut self, above: bool) {
        let eol = match self.tabs[self.active].eol {
            Eol::Crlf => "\r\n",
            Eol::Lf => "\n",
            Eol::Cr => "\r",
        };
        let caret = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
        let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, caret, 0));
        let at = crate::num::to_usize(if above { self.send(sci::SCI_POSITIONFROMLINE, line, 0) } else { self.send(sci::SCI_GETLINEENDPOSITION, line, 0) });
        let text = std::ffi::CString::new(eol).unwrap_or_default();
        self.send(sci::SCI_INSERTTEXT, at, text.as_ptr() as isize);
        // The caret lands on the new blank line either way (as in Notepad++).
        let target = if above { crate::num::to_isize(at) } else { self.send(sci::SCI_POSITIONFROMLINE, line + 1, 0) };
        self.send(sci::SCI_GOTOPOS, crate::num::to_usize(target.max(0)), 0);
    }

    /// Indent or dedent by one level: the selected lines, or the caret's line.
    pub fn indent(&mut self, increase: bool) {
        let multi = {
            let a = crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0));
            let b = crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0));
            self.send(sci::SCI_LINEFROMPOSITION, a, 0) != self.send(sci::SCI_LINEFROMPOSITION, b, 0)
        };
        if !increase {
            self.send(sci::SCI_BACKTAB, 0, 0);
        } else if multi {
            self.send(sci::SCI_TAB, 0, 0);
        } else {
            let caret = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
            let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, caret, 0));
            let width = self.send(sci::SCI_GETTABWIDTH, 0, 0).max(1);
            let current = self.send(sci::SCI_GETLINEINDENTATION, line, 0);
            self.send(sci::SCI_SETLINEINDENTATION, line, current + width);
        }
    }
}
