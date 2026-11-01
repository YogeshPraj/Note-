//! How the document is shown: margins, themes, scrolling, wrap, read-only and the minimap.

use super::*;

impl<S: Send> Workspace<S> {
    /// Widths of the bookmark, line-number and fold margins under the current settings.
    pub(super) fn margin_widths(&self) -> [isize; 3] {
        let s = &self.settings;
        [if s.bookmark_margin { 16 } else { 0 }, if s.line_numbers { 54 } else { 0 }, if s.fold_margin { 16 } else { 0 }]
    }

    /// Show or hide the bookmark, line-number and fold margins (Distraction Free Mode).
    pub fn set_margins_visible(&self, on: bool) {
        for (margin, width) in self.margin_widths().into_iter().enumerate() {
            self.send(sci::SCI_SETMARGINWIDTHN, margin, if on { width } else { 0 });
        }
    }

    /// Take new preferences: fonts, tabs, margins, colours, new-document defaults.
    pub fn set_settings(&mut self, s: Settings) {
        self.settings = s;
        let base = if self.theme.dark { Theme::dark() } else { Theme::light() };
        self.set_theme(base);
        if !self.settings.smart_highlight {
            search::clear_marks(&self.sci, search::INDIC_SMART);
        }
        self.smart_key = (usize::MAX, 0, 0);
    }

    pub fn set_symbols(&mut self, v: view::Symbols) {
        self.symbols = v;
        view::apply_symbols(&self.sci, &self.symbols);
    }

    /// Colours, margins and indicators for one view. The minimap is the same
    /// document at a tiny zoom with no gutter, caret or scrollbars.
    pub(super) fn style_view(&self, v: &S, mini: bool) {
        let theme = &self.theme;
        sci::configure(v, &self.settings.font, self.settings.font_size as isize, theme);
        v.send(sci::SCI_SETTABWIDTH, crate::num::to_usize(self.settings.tab_width), 0);
        v.send(sci::SCI_SETUSETABS, usize::from(!self.settings.use_spaces), 0);
        v.send(sci::SCI_SETCARETLINEVISIBLE, usize::from(self.settings.caret_line), 0);
        if mini {
            for margin in 0..3 {
                v.send(sci::SCI_SETMARGINWIDTHN, margin, 0);
            }
            v.send(sci::SCI_SETZOOM, crate::num::to_usize(-9i32), 0);
            v.send(sci::SCI_SETWRAPMODE, sci::SC_WRAP_CHAR, 0);
            v.send(sci::SCI_SETCARETSTYLE, 0, 0);
            v.send(sci::SCI_SETCARETLINEVISIBLE, 0, 0);
            v.send(sci::SCI_SETVSCROLLBAR, 0, 0);
            v.send(sci::SCI_SETHSCROLLBAR, 0, 0);
        } else {
            sci::setup_line_numbers(v, theme, 54);
            sci::setup_folding(v, theme);
            bookmarks::setup(v, theme);
            for (margin, width) in self.margin_widths().into_iter().enumerate() {
                v.send(sci::SCI_SETMARGINWIDTHN, margin, width);
            }
            // Vertical guide line at the chosen column.
            v.send(sci::SCI_SETEDGEMODE, usize::from(self.settings.edge_column > 0), 0);
            v.send(sci::SCI_SETEDGECOLUMN, crate::num::to_usize(self.settings.edge_column.max(0)), 0);
            v.send(sci::SCI_SETEDGECOLOUR, crate::num::to_usize(sci::rgb(theme.line_no_fg.0, theme.line_no_fg.1, theme.line_no_fg.2)), 0);
        }
        // Find marks: amber box. Smart highlight: green box (Notepad++ style).
        for (ind, rgb, alpha) in [(search::INDIC_FIND, (0xFF, 0xB0, 0x00), 110), (search::INDIC_SMART, (0x4C, 0xC9, 0x5A), 90)]
            .into_iter()
            .chain((0..search::STYLE_SLOTS).map(|i| (search::style_indicator(i), search::STYLE_COLOURS[i], 100)))
        {
            v.send(sci::SCI_INDICSETSTYLE, ind, crate::num::to_isize(sci::INDIC_STRAIGHTBOX));
            v.send(sci::SCI_INDICSETFORE, ind, sci::rgb(rgb.0, rgb.1, rgb.2));
            v.send(sci::SCI_INDICSETALPHA, ind, alpha);
            v.send(sci::SCI_INDICSETOUTLINEALPHA, ind, 200);
        }
    }

    pub fn attach_minimap(&mut self, mini: S) {
        self.style_view(&mini, true);
        if let Some(t) = self.tabs.get(self.active) {
            mini.send(sci::SCI_SETDOCPOINTER, 0, t.doc);
            lang::apply_palette(&mini, t.lang, &self.theme);
        }
        self.mini = Some(mini);
    }

    pub fn set_theme(&mut self, theme: Theme) {
        let dark = theme.dark;
        self.theme = settings::themed(theme, self.settings.colors(dark));
        self.style_view(&self.sci, false);
        if let Some(m) = &self.mini {
            self.style_view(m, true);
        }
        sci::set_wrap(&self.sci, self.wrap);
        if !self.tabs.is_empty() {
            self.apply_palettes();
        }
    }

    /// Keep the minimap scrolled in proportion to the main view.
    pub fn sync_minimap(&self) {
        let Some(m) = &self.mini else { return };
        let total = sci::line_count(&self.sci).max(1);
        let main_first = self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0);
        let main_vis = self.send(sci::SCI_LINESONSCREEN, 0, 0).max(1);
        let mini_vis = m.send(sci::SCI_LINESONSCREEN, 0, 0).max(1);
        let first = if total <= mini_vis { 0 } else { main_first * (total - mini_vis) / (total - main_vis).max(1) };
        m.send(sci::SCI_SETFIRSTVISIBLELINE, crate::num::to_usize(first.max(0)), 0);
    }

    /// The document line at the top of the editor (wrapped lines count once).
    pub fn top_doc_line(&self) -> usize {
        let visible = crate::num::to_usize(self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0));
        crate::num::to_usize(self.send(sci::SCI_DOCLINEFROMVISIBLE, visible, 0).max(0))
    }

    /// Scroll the editor so document line `line` is at the top.
    pub fn set_top_doc_line(&self, line: usize) {
        let visible = crate::num::to_usize(self.send(sci::SCI_VISIBLEFROMDOCLINE, line, 0).max(0));
        self.send(sci::SCI_SETFIRSTVISIBLELINE, visible, 0);
        self.sync_minimap();
    }

    pub fn line_total(&self) -> usize {
        crate::num::to_usize(sci::line_count(&self.sci).max(1))
    }

    /// Scroll the main view so document line `line` (0-based) is centred.
    pub fn scroll_to_line(&self, line: usize) {
        let vis = crate::num::to_usize(self.send(sci::SCI_LINESONSCREEN, 0, 0).max(1));
        self.send(sci::SCI_SETFIRSTVISIBLELINE, line.saturating_sub(vis / 2), 0);
        self.sync_minimap();
    }

    /// Update the smart highlight if the selection or document size changed.
    pub fn update_smart(&mut self) {
        let a = crate::num::to_usize(self.send(sci::SCI_GETSELECTIONSTART, 0, 0).max(0));
        let b = crate::num::to_usize(self.send(sci::SCI_GETSELECTIONEND, 0, 0).max(0));
        let key = (a, b, crate::num::to_usize(sci::length(&self.sci).max(0)));
        if !self.is_whiteboard() {
            search::highlight_braces(&self.sci);
        }
        if key != self.smart_key {
            self.smart_key = key;
            if self.settings.smart_highlight {
                search::smart_highlight(&self.sci);
            }
        }
    }

    pub(super) fn remember_view(&mut self) {
        if self.tabs.is_empty() || self.is_whiteboard() {
            return;
        }
        let dirty = self.send(sci::SCI_GETMODIFY, 0, 0) != 0;
        let caret = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
        let top = crate::num::to_usize(self.send(sci::SCI_GETFIRSTVISIBLELINE, 0, 0).max(0));
        let t = &mut self.tabs[self.active];
        t.dirty = dirty;
        t.caret = caret;
        t.top = top;
    }

    /// Re-apply language colours to every view (does not touch caret or scroll).
    pub(super) fn apply_palettes(&self) {
        let t = &self.tabs[self.active];
        lang::apply_palette(&self.sci, t.lang, &self.theme);
        if let Some(m) = &self.mini {
            lang::apply_palette(m, t.lang, &self.theme);
        }
    }

    /// Show the active tab: colours, then its remembered caret and scroll position.
    pub(super) fn apply_view(&self) {
        let t = &self.tabs[self.active];
        self.apply_palettes();
        self.send(sci::SCI_GOTOPOS, t.caret, 0);
        self.send(sci::SCI_SETFIRSTVISIBLELINE, t.top, 0);
        self.sync_minimap();
    }

    pub fn is_read_only(&self) -> bool {
        self.send(sci::SCI_GETREADONLY, 0, 0) != 0
    }

    pub fn set_read_only(&mut self, on: bool) {
        self.send(sci::SCI_SETREADONLY, usize::from(on), 0);
    }

    /// What the active tab can be previewed as, if anything.
    pub fn preview_kind(&self) -> Option<preview::Kind> {
        let t = self.tabs.get(self.active)?;
        preview::kind_for(t.lang, t.path.as_deref())
    }

    /// Identity of the active document's content, to skip redundant re-renders.
    pub fn preview_key(&self) -> (isize, usize, u64) {
        let text = sci::get_text(&self.sci);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
        for b in text.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        (self.tabs.get(self.active).map_or(0, |t| t.doc), text.len(), h)
    }

    /// The 0-based line the caret is on.
    pub fn caret_line(&self) -> usize {
        let pos = crate::num::to_usize(self.send(sci::SCI_GETCURRENTPOS, 0, 0).max(0));
        crate::num::to_usize(self.send(sci::SCI_LINEFROMPOSITION, pos, 0).max(0))
    }

    /// Put the caret on 0-based `line` and select it, scrolling it into view.
    pub fn select_doc_line(&self, line: usize) {
        let total = self.line_total();
        let line = line.min(total.saturating_sub(1));
        self.send(sci::SCI_ENSUREVISIBLE, line, 0);
        let start = crate::num::to_usize(self.send(sci::SCI_POSITIONFROMLINE, line, 0).max(0));
        let end = crate::num::to_usize(self.send(sci::SCI_GETLINEENDPOSITION, line, 0).max(0));
        self.send(sci::SCI_SETSEL, start, crate::num::to_isize(end));
        self.send(sci::SCI_SCROLLCARET, 0, 0);
    }

    pub fn set_wrap(&mut self, on: bool) {
        self.wrap = on;
        sci::set_wrap(&self.sci, on);
    }
}
