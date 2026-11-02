//! Completion, path text and bookmarks of the active document.

use super::*;

impl<S: Send> Workspace<S> {
    /// Show an auto-completion list or call tip at the caret. Returns a message
    /// when there is nothing to offer.
    pub fn complete(&mut self, kind: Completion) -> Option<String> {
        let text = sci::get_text(&self.sci);
        let caret = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0)).min(text.len());
        let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, caret, 0));
        let line_start = crate::num::to_usize(self.send(sci::SCI_POSITIONFROMLINE, line, 0));
        let before_on_line = text.get(line_start..caret).unwrap_or("");
        let show = |sci: &S, entered: usize, items: &[String]| {
            let list = std::ffi::CString::new(items.join(" ")).unwrap_or_default();
            sci.send(sci::SCI_AUTOCSETIGNORECASE, 1, 0);
            sci.send(sci::SCI_AUTOCSETCHOOSESINGLE, 1, 0);
            sci.send(sci::SCI_AUTOCSHOW, entered, list.as_ptr() as isize);
        };
        match kind {
            Completion::Words | Completion::Functions => {
                let (at, prefix) = complete::word_before(&text, caret);
                let items =
                    if kind == Completion::Words { complete::words_with_prefix(&text, prefix, at) } else { complete::function_names(&text, prefix, at) };
                if items.is_empty() {
                    return Some("No suggestions".into());
                }
                show(&self.sci, prefix.len(), &items);
            }
            Completion::Path => {
                let base = self.tabs[self.active].path.as_ref().and_then(|p| p.parent()).map(Path::to_path_buf);
                let (typed, items) = complete::path_candidates(complete::path_fragment(before_on_line), base.as_deref());
                if items.is_empty() {
                    return Some("No matching paths".into());
                }
                show(&self.sci, typed, &items);
            }
            Completion::Hint => {
                // The call the caret is inside: the name before the nearest unmatched "(".
                let mut depth = 0;
                let open = before_on_line.char_indices().rev().find(|(_, c)| {
                    match c {
                        ')' => depth += 1,
                        '(' if depth == 0 => return true,
                        '(' => depth -= 1,
                        _ => {}
                    }
                    false
                });
                let name = open.map_or("", |(i, _)| complete::word_before(before_on_line, i).1);
                let Some(sig) = (!name.is_empty()).then(|| complete::signature_for(&text, name)).flatten() else {
                    return Some("No signature found".into());
                };
                let tip = std::ffi::CString::new(sig).unwrap_or_default();
                self.send(sci::SCI_CALLTIPSHOW, caret, tip.as_ptr() as isize);
            }
        }
        None
    }

    pub fn path_text(&self, kind: PathText) -> Option<String> {
        let one = |t: &Tab, f: &dyn Fn(&Path) -> String| Some(t.path.as_deref().map_or_else(|| t.title.clone(), f));
        let t = &self.tabs[self.active];
        match kind {
            PathText::FullPath => t.path.as_ref().map(|p| p.display().to_string()),
            PathText::FileName => one(t, &|p| file_title(p)),
            PathText::Directory => t.path.as_ref().and_then(|p| p.parent()).map(|d| d.display().to_string()),
            PathText::AllNames => Some(self.tabs.iter().map(|t| t.title.clone()).collect::<Vec<_>>().join("\r\n")),
            PathText::AllPaths => {
                Some(self.tabs.iter().map(|t| t.path.as_ref().map_or_else(|| t.title.clone(), |p| p.display().to_string())).collect::<Vec<_>>().join("\r\n"))
            }
        }
    }

    /// The JSON preview edited the document: take its text, as one undo step.
    pub fn replace_text(&self, text: &str) {
        let top = self.top_doc_line();
        edit::replace_whole(&self.sci, text);
        self.set_top_doc_line(top);
    }

    pub fn toggle_bookmark(&self) {
        bookmarks::toggle(&self.sci, bookmarks::current_line(&self.sci));
    }

    pub fn goto_bookmark(&self, forward: bool) -> bool {
        bookmarks::goto(&self.sci, forward)
    }

    pub fn clear_bookmarks(&self) {
        bookmarks::clear_all(&self.sci);
    }

    /// Toggle the bookmark on the line containing byte position `pos` (margin click).
    pub fn toggle_bookmark_at(&self, pos: usize) {
        let line = crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0));
        bookmarks::toggle(&self.sci, line);
    }
}
