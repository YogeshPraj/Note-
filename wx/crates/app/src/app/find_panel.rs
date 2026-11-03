//! Actions from the Find panel: find, replace, count, mark, Find All, the results list.

use super::*;

impl App {
    /// Actions from the Find panel.
    pub fn panel_command(&mut self, cmd: Cmd) {
        use notepp_core::search;
        let sci = self.ws.sci;
        let mode = findpanel::mode();
        let opts = findpanel::read_opts();
        if !matches!(cmd, Cmd::Close | Cmd::Browse | Cmd::OpenHit(_) | Cmd::Clear) {
            self.ws.search = opts.clone();
        }
        // Searching the current document means nothing on a whiteboard: the editor behind it holds another tab's text.
        if searches_document(&cmd, mode) && self.ws.is_whiteboard() {
            return findpanel::set_status("Not available on a whiteboard: switch to a text tab");
        }
        let (found, outcome) = (found_note, count_note);
        let mut note: Option<String> = None;
        match cmd {
            Cmd::Next => match mode {
                Mode::InFiles => return self.panel_command(Cmd::InFiles),
                Mode::Mark => return self.panel_command(Cmd::Mark),
                _ => note = Some(found(search::find(&sci, &opts, true))),
            },
            Cmd::Prev => note = Some(found(search::find(&sci, &opts, false))),
            Cmd::Replace => {
                let (replaced, f) = search::replace_current(&sci, &opts);
                let n = found(f);
                note = Some(if replaced && n.is_empty() { "Replaced".into() } else { n });
            }
            Cmd::ReplaceAll => note = Some(outcome(search::replace_all(&sci, &opts), "replaced")),
            Cmd::Count => note = Some(outcome(search::count(&sci, &opts), "matches")),
            Cmd::Mark => note = Some(outcome(search::mark_all(&sci, &opts, search::INDIC_FIND), "matches marked")),
            Cmd::Clear => {
                search::clear_marks(&sci, search::INDIC_FIND);
                note = Some("Marks cleared".into());
            }
            Cmd::FindAll => note = Some(self.find_all_note(&opts)),
            Cmd::InFiles => note = Some(self.in_files_toggle(&opts)),
            Cmd::Browse => {
                let d = DirDialog::builder(&self.frame, "Select a folder to search", "").build();
                if d.show_modal() == ID_OK
                    && let Some(p) = d.get_path()
                {
                    findpanel::set_dir(&p);
                }
            }
            Cmd::OpenHit(i) => {
                self.results.cursor = i;
                self.open_hit(i);
            }
            Cmd::Close => {
                self.in_files_cancel();
                findpanel::close();
                self.stc.set_focus();
            }
        }
        if let Some(n) = note {
            findpanel::set_status(&n);
        }
        self.refresh();
    }

    /// "Find All" in the current document: list the hits and say how many.
    pub(crate) fn find_all_note(&mut self, opts: &notepp_core::search::Opts) -> String {
        use notepp_core::search;
        match search::find_all(&self.ws.sci, opts) {
            Ok(mut hits) => {
                let total = hits.len();
                hits.truncate(search::MAX_LIST);
                let lines: Vec<String> = hits.iter().map(|h| format!("Line {}:  {}", h.line, h.text)).collect();
                let note = if total > hits.len() {
                    format!("{total} hits (listing the first {})", hits.len())
                } else {
                    format!("{total} hits in the current document")
                };
                self.results.hits = hits;
                findpanel::set_results(lines);
                note
            }
            Err(e) => error_note(&e),
        }
    }

    /// Open result `idx` of the last search in the editor.
    pub(crate) fn open_hit(&mut self, idx: usize) {
        use notepp_core::search;
        if let Some(h) = self.results.hits.get(idx).cloned() {
            // A hit in "the current document" (Find All) has no file to open; on a whiteboard there is no text to select.
            if h.path.is_none() && self.ws.is_whiteboard() {
                return;
            }
            let App { ws, ui, .. } = self;
            if let Some(p) = &h.path {
                ws.open_path(ui, p);
            }
            let a = ws.active;
            let sci = ws.sci;
            if ws.is_dirty(a) && h.path.is_some() {
                search::goto_line(&sci, h.line);
            } else {
                search::select_range(&sci, h.start, h.end);
            }
        }
    }

    /// F4 / Shift+F4: step through the results of the last Find All / Find in Files.
    pub(crate) fn step_result(&mut self, forward: bool) {
        let n = self.results.hits.len();
        if n == 0 {
            self.flash = Some("No search results yet".into());
        } else {
            self.results.cursor = if forward { (self.results.cursor + 1) % n } else { (self.results.cursor + n - 1) % n };
            self.open_hit(self.results.cursor);
            self.flash = Some(format!("Result {} of {n}", self.results.cursor + 1));
        }
        self.refresh();
    }
}
