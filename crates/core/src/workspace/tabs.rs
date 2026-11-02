//! The tab list: pinning, colours, order, activation and what the window shows about it.

use super::*;

impl<S: Send> Workspace<S> {
    /// Drag-and-drop reorder: move tab `from` to index `to`; the active tab stays the same tab.
    /// How many tabs are pinned (they always come first).
    pub fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|t| t.pinned).count()
    }

    pub fn move_tab_to(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        // A tab stays inside its own group: pinned tabs left, the others right.
        let pinned = self.pinned_count();
        let to = if self.tabs[from].pinned { to.min(pinned.saturating_sub(1)) } else { to.max(pinned) };
        if from == to {
            return;
        }
        let active_id = self.tabs[self.active].id;
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(0);
    }

    /// Pin or unpin tab `i`: a pinned tab moves to the end of the pinned group, an unpinned one to its start.
    pub fn toggle_pin(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        let now = !self.tabs[i].pinned;
        self.tabs[i].pinned = now;
        let id = self.tabs[i].id;
        // The count of the others that are pinned decides where it lands.
        let others = self.tabs.iter().filter(|t| t.pinned && t.id != id).count();
        let active_id = self.tabs[self.active].id;
        let tab = self.tabs.remove(i);
        self.tabs.insert(others, tab);
        self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(0);
    }

    pub fn set_tab_color(&mut self, i: usize, color: Option<(u8, u8, u8)>) {
        if let Some(t) = self.tabs.get_mut(i) {
            t.color = color;
        }
    }

    /// Move the active tab one place (negative = left), keeping it active.
    pub fn move_tab(&mut self, delta: isize) -> bool {
        let to = crate::num::to_isize(self.active) + delta;
        if to < 0 || crate::num::to_usize(to) >= self.tabs.len() {
            return false;
        }
        let tab = self.tabs.remove(self.active);
        self.tabs.insert(crate::num::to_usize(to), tab);
        self.active = crate::num::to_usize(to);
        true
    }

    /// Point every view at `doc`.
    pub(super) fn set_doc(&self, doc: isize) {
        self.send(sci::SCI_SETDOCPOINTER, 0, doc);
        if let Some(m) = &self.mini {
            m.send(sci::SCI_SETDOCPOINTER, 0, doc);
        }
    }

    pub fn is_dirty(&self, i: usize) -> bool {
        let t = &self.tabs[i];
        if t.kind == TabKind::Whiteboard {
            return t.dirty || t.forced_dirty;
        }
        t.forced_dirty || if i == self.active { self.send(sci::SCI_GETMODIFY, 0, 0) != 0 } else { t.dirty }
    }

    /// Is the active tab a whiteboard (so the Scintilla view is not in use)?
    pub fn is_whiteboard(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|t| t.kind == TabKind::Whiteboard)
    }

    pub(super) fn take_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn new_tab(&mut self) -> usize {
        let n = (1..=self.tabs.len() + 1).find(|n| !self.tabs.iter().any(|t| t.path.is_none() && t.title == format!("new {n}"))).unwrap_or(1);
        let eol = match self.settings.new_doc_eol.as_str() {
            "crlf" => Eol::Crlf,
            "lf" => Eol::Lf,
            "cr" => Eol::Cr,
            _ => Eol::platform_default(),
        };
        let enc = Enc::from_key(&self.settings.new_doc_encoding);
        self.open_doc(format!("new {n}"), None, "", enc, eol, "null")
    }

    pub fn activate(&mut self, i: usize) {
        if i >= self.tabs.len() || i == self.active {
            return;
        }
        self.remember_view();
        self.active = i;
        if self.tabs[i].kind == TabKind::Text {
            self.set_doc(self.tabs[i].doc);
            self.apply_view();
        }
    }

    pub fn next_tab(&mut self, delta: isize) {
        let n = crate::num::to_isize(self.tabs.len());
        if n > 1 {
            self.activate((crate::num::to_isize(self.active) + delta).rem_euclid(n) as usize);
        }
    }

    /// What the tab strip draws for each tab, in order.
    pub fn tab_infos(&self) -> Vec<TabInfo> {
        (0..self.tabs.len())
            .map(|i| TabInfo {
                title: self.tabs[i].title.clone(),
                dirty: self.is_dirty(i),
                pinned: self.tabs[i].pinned,
                color: self.tabs[i].color,
                badge: if self.tabs[i].kind == TabKind::Whiteboard {
                    Some("wb")
                } else if self.tabs[i].encrypted {
                    Some("enc")
                } else {
                    None
                },
            })
            .collect()
    }

    /// What each tab is sorted and listed by.
    pub(super) fn tab_keys(&self) -> Vec<SortKeys> {
        (0..self.tabs.len())
            .map(|i| {
                let t = &self.tabs[i];
                let modified = t
                    .path
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .and_then(|m| m.modified().ok())
                    .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs());
                SortKeys {
                    name: t.title.clone(),
                    path: t.path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
                    kind: if t.kind == TabKind::Whiteboard { "Whiteboard".into() } else { t.lang_label.to_string() },
                    len: self.doc_length(i),
                    modified,
                }
            })
            .collect()
    }

    /// Size in bytes of tab `i`'s document.
    pub(super) fn doc_length(&self, i: usize) -> usize {
        let t = &self.tabs[i];
        if t.kind == TabKind::Whiteboard {
            return t.content.len();
        }
        if i == self.active {
            return crate::num::to_usize(sci::length(&self.sci).max(0));
        }
        // Put back whatever document the view was showing. The active tab's own pointer is no
        // use here: a whiteboard has none (0), and selecting 0 makes Scintilla create a stray
        // document that corrupts the view later.
        let shown = self.send(sci::SCI_GETDOCPOINTER, 0, 0);
        // Swapping documents resets the view's selection and scroll position; put them back too.
        let anchor = crate::num::to_usize(self.send(sci::SCI_GETANCHOR, 0, 0).max(0));
        let caret = self.send(sci::SCI_GETCURRENTPOS, 0, 0);
        let first = self.top_doc_line(); // a document line: display lines change meaning when word wrap is laid out again
        self.send(sci::SCI_SETDOCPOINTER, 0, t.doc);
        let n = crate::num::to_usize(sci::length(&self.sci).max(0));
        self.send(sci::SCI_SETDOCPOINTER, 0, shown);
        self.send(sci::SCI_SETSEL, anchor, caret);
        self.set_top_doc_line(first);
        n
    }

    /// Rows for Window > Windows...: name, folder, type, size.
    pub fn window_rows(&self) -> Vec<Vec<String>> {
        self.tab_keys()
            .into_iter()
            .enumerate()
            .map(|(i, k)| {
                let shown = if self.is_dirty(i) { format!("*{}", k.name) } else { k.name };
                vec![shown, k.path, k.kind, k.len.to_string()]
            })
            .collect()
    }

    /// Reorder the tabs by `field`; the active tab stays active.
    pub fn sort_tabs(&mut self, field: SortField, descending: bool) {
        let order = sort_order(&self.tab_keys(), field, descending);
        let active_id = self.tabs[self.active].id;
        let mut slots: Vec<Option<Tab>> = std::mem::take(&mut self.tabs).into_iter().map(Some).collect();
        self.tabs = order.iter().filter_map(|&i| slots[i].take()).collect();
        self.active = self.tabs.iter().position(|t| t.id == active_id).unwrap_or(0);
    }

    pub fn window_title(&self) -> String {
        let t = &self.tabs[self.active];
        let name = t.path.as_ref().map_or_else(|| t.title.clone(), |p| p.display().to_string());
        format!("{}{} - Note++", if self.is_dirty(self.active) { "* " } else { "" }, name)
    }

    pub fn status(&self) -> Status {
        let t = &self.tabs[self.active];
        if t.kind == TabKind::Whiteboard {
            return Status {
                position: "Whiteboard".into(),
                lines: String::new(),
                length: format!("elements: {}", scene_element_count(&t.content)),
                eol: String::new(),
                enc: "UTF-8".into(),
                lang: "Excalidraw".into(),
            };
        }
        let pos = self.send(sci::SCI_GETCURRENTPOS, 0, 0);
        let line = self.send(sci::SCI_LINEFROMPOSITION, crate::num::to_usize(pos), 0) + 1;
        let col = self.send(sci::SCI_GETCOLUMN, crate::num::to_usize(pos), 0) + 1;
        // Conventional wording: the caret's position, or what is selected.
        let tail = match crate::num::to_usize(self.send(sci::SCI_GETSELECTIONS, 0, 0).max(1)) {
            1 if self.send(sci::SCI_GETSELECTIONEMPTY, 0, 0) != 0 => format!("Pos: {}", commafy(crate::num::to_usize(pos.max(0)) + 1)),
            n => {
                let ranges: Vec<(usize, usize)> = (0..n)
                    .map(|i| {
                        (
                            crate::num::to_usize(self.send(sci::SCI_GETSELECTIONNSTART, i, 0).max(0)),
                            crate::num::to_usize(self.send(sci::SCI_GETSELECTIONNEND, i, 0).max(0)),
                        )
                    })
                    .collect();
                // Characters, not bytes: an accented letter or a CJK character counts once.
                let counts: Vec<usize> =
                    ranges.iter().map(|&(a, b)| crate::num::to_usize(self.send(sci::SCI_COUNTCHARACTERS, a, crate::num::to_isize(b)).max(0))).collect();
                let chars: usize = counts.iter().sum();
                let line_of = |p: usize| crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, p, 0).max(0));
                if n == 1 {
                    format!("Sel: {} | {}", commafy(chars), commafy(line_of(ranges[0].1) - line_of(ranges[0].0) + 1))
                } else if self.send(sci::SCI_SELECTIONISRECTANGLE, 0, 0) != 0 {
                    let widest = counts.iter().copied().max().unwrap_or(0);
                    let same = counts.iter().all(|&c| c == counts[0]);
                    format!("Sel: {}x{}{}{}", commafy(n), commafy(widest), if same { " = " } else { " -> " }, commafy(chars))
                } else {
                    let mut lines: Vec<(usize, usize)> = ranges.iter().map(|&(a, b)| (line_of(a), line_of(b))).collect();
                    lines.sort_unstable();
                    let (mut total, mut last) = (0usize, -1isize);
                    for (first, end) in lines {
                        let first = if crate::num::to_isize(first) <= last { crate::num::to_usize(last + 1) } else { first };
                        if end >= first {
                            total += end - first + 1;
                            last = crate::num::to_isize(end);
                        }
                    }
                    format!("Sel {} : {} | {}", commafy(n), commafy(chars), if n <= 99 { commafy(total) } else { "...".into() })
                }
            }
        };
        Status {
            position: format!("Ln: {line}    Col: {col}    {tail}"),
            lines: format!("lines: {}", commafy(crate::num::to_usize(sci::line_count(&self.sci).max(0)))),
            length: format!("length: {}", commafy(crate::num::to_usize(sci::length(&self.sci).max(0)))),
            eol: t.eol.label().into(),
            enc: t.enc.label().into(),
            lang: t.lang_label.to_string(),
        }
    }
}
