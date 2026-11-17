//! Word files: opened as Markdown, and Markdown saved as a Word file.

use super::*;

/// What a Save As to a Word file made.
pub(super) enum Word {
    /// The target is not a Word file (or the tab is not text).
    Other,
    /// It is, and writing it failed or was refused (the user has been told).
    Failed,
    Bytes(Vec<u8>),
}

impl<S: Send> Workspace<S> {
    /// A Word file opens as Markdown in a new tab that is not a file: Save As then offers .md, and the Word file is never overwritten.
    pub(super) fn open_word_file(&mut self, ui: &dyn Ui, path: &Path, bytes: &[u8]) {
        let md = match crate::docx::to_markdown(bytes) {
            Ok(md) => md,
            Err(e) => return ui.error(&format!("Cannot open {}\n\n{e}", path.display())),
        };
        let title = format!("{}.md", path.file_stem().map_or_else(|| "document".into(), |s| s.to_string_lossy().into_owned()));
        let replace_pristine =
            self.tabs.len() == 1 && self.tabs[0].kind == TabKind::Text && self.tabs[0].path.is_none() && !self.is_dirty(0) && sci::length(&self.sci) == 0;
        self.open_doc(title, None, &md, Enc::from_key("utf8"), Eol::platform_default(), "markdown");
        self.note_recent(path);
        if replace_pristine {
            let old = self.tabs.remove(0);
            self.active -= 1;
            self.send(sci::SCI_RELEASEDOCUMENT, 0, old.doc);
        }
    }

    /// The active Markdown tab as a Word file when path names one:
    pub(super) fn word_bytes(&mut self, ui: &dyn Ui, path: &Path) -> Word {
        if !crate::docx::is_docx_path(path) || self.tabs[self.active].kind != TabKind::Text {
            return Word::Other;
        }
        if self.tabs[self.active].encrypted {
            ui.error("This pad is encrypted, so it cannot be saved as a Word file.\n\nDecrypt it first.");
            return Word::Failed;
        }
        match crate::docx::from_markdown(&sci::get_text(&self.sci)) {
            Ok(b) => Word::Bytes(b),
            Err(e) => {
                ui.error(&format!("Cannot save {}\n\n{e}", path.display()));
                Word::Failed
            }
        }
    }
}
