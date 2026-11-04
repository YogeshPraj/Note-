//! Picture tabs: which files open as pictures, their naming, and the payload they hold.
//!
//! A picture tab keeps the file's bytes as base64 in `Tab::content`, the way a whiteboard keeps its scene JSON:
//! the editor page decodes, edits and re-encodes it, and the tab only ever stores and writes the bytes.

use std::path::{Path, PathBuf};

use crate::textools::{base64_decode, base64_encode};

/// Largest picture file opened as a picture tab. A bigger one opens as text, as before.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

const EXTENSIONS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Does `path` name a picture the editor page can show?
pub fn is_image_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// The picture type of `bytes` by its signature, as a MIME type. None when it is none of the supported kinds.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'B', b'M', ..] => Some("image/bmp"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        _ => None,
    }
}

pub fn is_png(bytes: &[u8]) -> bool {
    sniff_mime(bytes) == Some("image/png")
}

pub fn to_content(bytes: &[u8]) -> String {
    base64_encode(bytes)
}

/// The bytes a tab's content stands for; empty when the content is not valid base64.
pub fn from_content(content: &str) -> Vec<u8> {
    base64_decode(content).unwrap_or_default()
}

/// The MIME type of a tab's content, for the page's data URL.
pub fn content_mime(content: &str) -> &'static str {
    // Only the first bytes matter: 16 base64 characters decode to 12 bytes.
    let head: String = content.chars().take(16).collect();
    sniff_mime(&from_content(&head)).unwrap_or("image/png")
}

/// The width and height of a picture from its header, when that is a PNG, GIF or BMP (read from the start of the
/// base64 content only, so it is cheap to ask for on every status update).
pub fn size_of_content(content: &str) -> Option<(u32, u32)> {
    let head: String = content.chars().take(36).collect();
    if !head.is_ascii() {
        return None;
    }
    let b = from_content(&head[..head.len() - head.len() % 4]);
    let be = |at: usize| b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let le32 = |at: usize| b.get(at..at + 4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
    let le16 = |at: usize| b.get(at..at + 2).map(|s| u32::from(u16::from_le_bytes([s[0], s[1]])));
    match sniff_mime(&b)? {
        "image/png" => Some((be(16)?, be(20)?)),
        "image/gif" => Some((le16(6)?, le16(8)?)),
        "image/bmp" => Some((le32(18)?, le32(22)?)),
        _ => None,
    }
}

/// The path a picture is written to: always a PNG, since that is what the page produces.
pub fn png_path(path: &Path) -> PathBuf {
    if path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("png")) { path.to_path_buf() } else { path.with_extension("png") }
}

/// Lowest unused `image-N` title (same gap-filling rule as `new N`).
pub fn next_title<'a>(existing: impl IntoIterator<Item = &'a str>) -> String {
    let existing: Vec<&str> = existing.into_iter().collect();
    let taken = |n: usize| existing.iter().any(|t| *t == format!("image-{n}") || t.starts_with(&format!("image-{n}.")));
    format!("image-{}", (1..=existing.len() + 1).find(|n| !taken(*n)).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    #[test]
    fn picture_files_are_told_by_extension() {
        for ok in ["a.png", "b.JPG", "c.jpeg", "d.Gif", "e.bmp", "f.webp"] {
            assert!(is_image_path(Path::new(ok)), "{ok}");
        }
        for no in ["a.txt", "b.svg", "png", "c.png.txt", ""] {
            assert!(!is_image_path(Path::new(no)), "{no}");
        }
    }

    #[test]
    fn signatures_name_the_type() {
        assert_eq!(sniff_mime(&PNG), Some("image/png"));
        assert_eq!(sniff_mime(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff_mime(b"GIF89a"), Some("image/gif"));
        assert_eq!(sniff_mime(b"BM\0\0"), Some("image/bmp"));
        assert_eq!(sniff_mime(b"RIFF\x01\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(sniff_mime(b"hello"), None);
        assert!(is_png(&PNG) && !is_png(b"GIF89a"));
    }

    #[test]
    fn content_round_trips_and_names_its_type() {
        let c = to_content(&PNG);
        assert_eq!(from_content(&c), PNG);
        assert_eq!(content_mime(&c), "image/png");
        assert_eq!(content_mime(&to_content(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10, b'J', b'F', b'I', b'F'])), "image/jpeg");
        assert!(from_content("not base64!").is_empty());
    }

    #[test]
    fn size_comes_from_the_header() {
        let mut png = PNG.to_vec();
        png.extend_from_slice(&[0, 0, 0, 13, b'I', b'H', b'D', b'R', 0, 0, 3, 0x20, 0, 0, 1, 0x2C]);
        assert_eq!(size_of_content(&to_content(&png)), Some((800, 300)));
        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&[0x40, 0x01, 0xF0, 0x00]);
        assert_eq!(size_of_content(&to_content(&gif)), Some((320, 240)));
        assert_eq!(size_of_content(&to_content(b"hello world, not a picture at all")), None);
        assert_eq!(size_of_content(""), None);
    }

    #[test]
    fn pictures_are_always_saved_as_png() {
        assert_eq!(png_path(Path::new("shot.png")), PathBuf::from("shot.png"));
        assert_eq!(png_path(Path::new("shot.PNG")), PathBuf::from("shot.PNG"));
        assert_eq!(png_path(Path::new("photo.jpg")), PathBuf::from("photo.png"));
        assert_eq!(png_path(Path::new("noext")), PathBuf::from("noext.png"));
    }

    #[test]
    fn titles_fill_gaps() {
        assert_eq!(next_title([]), "image-1");
        assert_eq!(next_title(["image-1", "image-2"]), "image-3");
        assert_eq!(next_title(["image-2", "new 1"]), "image-1");
        assert_eq!(next_title(["image-1.png"]), "image-2");
    }
}
