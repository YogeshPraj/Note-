//! Markdown to a Word file, and a Word file to Markdown, from the tabs.

use super::*;

/// A user who answers the save dialog with `path`.
struct SaveTo(PathBuf);

impl Ui for SaveTo {
    fn pick_open(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_save(&self, _: &str) -> Option<PathBuf> {
        Some(self.0.clone())
    }
    fn confirm_save(&self, _: &str) -> Confirm {
        Confirm::Cancel
    }
    fn ask_yes_no(&self, _: &str) -> bool {
        false
    }
    fn recycle(&self, _: &Path) -> Result<(), crate::error::Error> {
        Ok(())
    }
    fn error(&self, _: &str) {}
    fn unlock(&self, _: &mut crypto::Vault, _: &str) -> bool {
        false
    }
    fn setup_encryption(&self, _: &mut crypto::Vault) -> bool {
        false
    }
}

#[test]
fn a_markdown_tab_is_saved_as_a_word_file_and_the_word_file_opens_as_markdown() {
    let dir = std::env::temp_dir().join(format!("npp_word_t_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let docx = dir.join("notes.docx");
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    assert!(!ws.save_as(&SaveTo(docx.clone())), "an export is not a save: the tab stays as it was");
    assert_eq!(ws.exported.as_deref(), Some(docx.as_path()));
    assert!(ws.tabs[ws.active].path.is_none());
    assert_eq!(&std::fs::read(&docx).unwrap()[..2], b"PK");

    let before = ws.tabs.len();
    ws.open_path(&NoUi, &docx);
    assert_eq!(ws.tabs.len(), before, "the empty tab it was opened from is replaced");
    let t = &ws.tabs[ws.active];
    assert_eq!((t.title.as_str(), t.path.is_none(), t.lang), ("notes.md", true, "markdown"));

    // Save As on it offers Markdown and becomes the .md file.
    let md = dir.join("notes.md");
    assert!(ws.save_as(&SaveTo(md.clone())));
    assert!(md.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_that_is_not_a_word_file_does_not_open() {
    let dir = std::env::temp_dir().join(format!("npp_word_b_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let fake = dir.join("fake.docx");
    std::fs::write(&fake, "plain").unwrap();
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    let before = ws.tabs.len();
    ws.open_path(&NoUi, &fake);
    assert_eq!(ws.tabs.len(), before);
    let _ = std::fs::remove_dir_all(&dir);
}
