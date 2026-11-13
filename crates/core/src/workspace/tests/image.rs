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

/// A real 4 x 3 picture, written as `format`.
fn picture(format: ::image::ImageFormat) -> Vec<u8> {
    let img = ::image::DynamicImage::ImageRgba8(::image::RgbaImage::from_pixel(4, 3, ::image::Rgba([200, 30, 30, 255])));
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

#[test]
fn a_picture_is_saved_as_the_type_its_file_name_asks_for() {
    let dir = std::env::temp_dir().join(format!("npp_img_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let png = picture(::image::ImageFormat::Png);
    let mut ws = workspace();
    let i = ws.new_image(crate::image::to_content(&png));
    // The same type is written untouched.
    assert!(ws.save_as(&SaveTo(dir.join("shot.png"))));
    assert_eq!(std::fs::read(dir.join("shot.png")).unwrap(), png);
    assert_eq!(ws.tabs[i].path.as_deref(), Some(dir.join("shot.png").as_path()));
    assert!(!ws.is_dirty(i));
    // Another type is converted: the bytes on disk are that format.
    for (name, format) in [
        ("a.jpg", ::image::ImageFormat::Jpeg),
        ("b.webp", ::image::ImageFormat::WebP),
        ("c.bmp", ::image::ImageFormat::Bmp),
        ("d.tiff", ::image::ImageFormat::Tiff),
        ("e.gif", ::image::ImageFormat::Gif),
        ("f.ico", ::image::ImageFormat::Ico),
        ("g.tga", ::image::ImageFormat::Tga),
        ("h.qoi", ::image::ImageFormat::Qoi),
    ] {
        assert!(ws.save_as(&SaveTo(dir.join(name))), "{name}");
        let bytes = std::fs::read(dir.join(name)).unwrap();
        if format != ::image::ImageFormat::Tga {
            assert_eq!(::image::guess_format(&bytes).ok(), Some(format), "{name}");
        }
        assert_eq!(::image::load_from_memory_with_format(&bytes, format).map(|p| (p.width(), p.height())).ok(), Some((4, 3)), "{name}");
    }
    // A type that cannot be written falls back to PNG.
    assert!(ws.save_as(&SaveTo(dir.join("z.dds"))));
    assert!(dir.join("z.png").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn any_picture_type_opens_as_a_picture_tab() {
    let dir = std::env::temp_dir().join(format!("npp_img_t_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let tiff = dir.join("scan.tif");
    std::fs::write(&tiff, picture(::image::ImageFormat::Tiff)).unwrap();
    let mut ws = workspace();
    ws.open_path(&NoUi, &tiff);
    let t = ws.tabs.last().unwrap();
    assert_eq!((t.kind, t.path.as_deref()), (TabKind::Image, Some(tiff.as_path())), "Save keeps the file's own type");
    assert_eq!(crate::image::sniff_mime(&crate::image::from_content(&t.content)), Some("image/png"));
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
fn opening_a_picture_file_makes_a_picture_tab_and_saves_in_its_own_type() {
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
    // new 1 (text), a.png (picture, in place), b.gif (picture, in place: Save writes a GIF), c.png (read as text)
    assert_eq!(kinds, vec![(TabKind::Text, false), (TabKind::Image, true), (TabKind::Image, true), (TabKind::Text, true)]);
    assert_eq!(ws.tabs[2].title, "b.gif");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_picture_can_be_saved_as_avif() {
    let dir = std::env::temp_dir().join(format!("npp_img_a_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut ws = workspace();
    ws.new_image(crate::image::to_content(&picture(::image::ImageFormat::Png)));
    assert!(ws.save_as(&SaveTo(dir.join("sky.avif"))));
    assert_eq!(crate::image::sniff_mime(&std::fs::read(dir.join("sky.avif")).unwrap()), Some("image/avif"));
    let _ = std::fs::remove_dir_all(&dir);
}
