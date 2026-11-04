//! Picture tabs.

use super::*;

const PNG: [u8; 24] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 0, 40, 0, 0, 0, 30];

fn workspace() -> Workspace<FakeSci> {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws
}

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
fn a_pasted_picture_is_a_new_unsaved_tab_named_image_n() {
    let mut ws = workspace();
    let first = ws.new_image(crate::image::to_content(&PNG));
    assert_eq!(ws.tabs[first].title, "image-1");
    assert!(ws.is_image() && ws.is_page() && !ws.is_whiteboard());
    assert!(ws.is_dirty(first), "closing it asks about saving");
    let second = ws.new_image(crate::image::to_content(&PNG));
    assert_eq!(ws.tabs[second].title, "image-2");
    assert_eq!(ws.tab_infos()[first].badge, Some("img"));
}

#[test]
fn the_status_bar_and_tool_tip_name_the_size() {
    let mut ws = workspace();
    let i = ws.new_image(crate::image::to_content(&PNG));
    let s = ws.status();
    assert_eq!((s.position.as_str(), s.lines.as_str(), s.lang.as_str()), ("Picture", "40 x 30", "PNG"));
    assert!(ws.tab_preview(i, 3, 20).contains("Picture, 40 x 30"));
}

#[test]
fn a_picture_is_saved_as_its_bytes_and_always_as_png() {
    let dir = std::env::temp_dir().join(format!("npp_img_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut ws = workspace();
    let i = ws.new_image(crate::image::to_content(&PNG));
    // Asked for a .jpg, it is written as .png: the page produces PNGs.
    assert!(ws.save_as(&SaveTo(dir.join("shot.jpg"))));
    let written = dir.join("shot.png");
    assert_eq!(std::fs::read(&written).unwrap(), PNG);
    assert_eq!(ws.tabs[i].path.as_deref(), Some(written.as_path()));
    assert!(!ws.is_dirty(i));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_unsaved_picture_survives_a_restart_and_a_saved_one_comes_from_its_file() {
    let dir = std::env::temp_dir().join(format!("npp_img_s_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut ws = workspace();
    ws.new_image(crate::image::to_content(&PNG));
    let snap = ws.snapshot();
    let saved = snap.tabs.iter().find(|t| t.kind == "image").expect("the picture is in the session");
    assert_eq!(saved.text.as_deref(), Some(crate::image::to_content(&PNG).as_str()));
    let mut again = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    again.restore(&NoUi, &snap);
    let back = again.tabs.iter().find(|t| t.kind == TabKind::Image).expect("the picture is restored");
    assert_eq!((back.content.as_str(), back.dirty), (crate::image::to_content(&PNG).as_str(), true));
    // Once it is a file and not edited since, only the path is kept.
    let file = dir.join("kept.png");
    std::fs::write(&file, PNG).unwrap();
    let mut third = workspace();
    third.open_path(&NoUi, &file);
    let snap = third.snapshot();
    let entry = snap.tabs.iter().find(|t| t.kind == "image").expect("the opened picture is in the session");
    assert_eq!((entry.text.as_deref(), entry.path.as_deref()), (None, file.to_str()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn opening_a_picture_file_makes_a_picture_tab_and_only_a_png_is_edited_in_place() {
    let dir = std::env::temp_dir().join(format!("npp_img_o_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let (png, gif, fake) = (dir.join("a.png"), dir.join("b.gif"), dir.join("c.png"));
    std::fs::write(&png, PNG).unwrap();
    std::fs::write(&gif, b"GIF89a\x10\0\x10\0rest").unwrap();
    std::fs::write(&fake, "not really a picture").unwrap();
    let mut ws = workspace();
    ws.open_path(&NoUi, &png);
    ws.open_path(&NoUi, &gif);
    ws.open_path(&NoUi, &fake);
    let kinds: Vec<(TabKind, bool)> = ws.tabs.iter().map(|t| (t.kind, t.path.is_some())).collect();
    // new 1 (text), a.png (picture, in place), b.gif (picture, no path: it saves as a PNG), c.png (read as text)
    assert_eq!(kinds, vec![(TabKind::Text, false), (TabKind::Image, true), (TabKind::Image, false), (TabKind::Text, true)]);
    assert_eq!(ws.tabs[2].title, "b.gif");
    let _ = std::fs::remove_dir_all(&dir);
}
