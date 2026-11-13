//! Picture tabs: which files open as pictures, their naming, and the payload they hold.
//!
//! A picture tab keeps the file's bytes as base64 in `Tab::content`, the way a whiteboard keeps its scene JSON:
//! the editor page decodes, edits and re-encodes it, and the tab only ever stores and writes the bytes.

use std::path::{Path, PathBuf};

use crate::textools::{base64_decode, base64_encode};

/// Largest picture file opened as a picture tab. A bigger one opens as text, as before.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Every picture extension that is opened (the ones the editor page cannot show itself are converted to PNG on the way in).
const EXTENSIONS: [&str; 25] = [
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "bmp", "dib", "webp", "tif", "tiff", "ico", "tga", "qoi", "pbm", "pgm", "ppm", "pnm", "pam", "hdr", "exr",
    "dds", "apng", "icb", "avif",
];

/// The extensions a picture can be saved as (the DDS format can only be read).
pub const SAVE_EXTENSIONS: [&str; 21] =
    ["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "tga", "qoi", "pbm", "pgm", "ppm", "pnm", "pam", "hdr", "exr", "dib", "jfif", "avif"];

fn extension_of(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

/// Does `path` name a picture that opens as a picture tab?
pub fn is_image_path(path: &Path) -> bool {
    extension_of(path).is_some_and(|e| EXTENSIONS.contains(&e.as_str()))
}

/// Can a picture be written to `path` in the type its extension names?
pub fn can_write(path: &Path) -> bool {
    target_format(path).is_some()
}

/// The format a file name asks to be written in, when it is one that can be written.
fn target_format(path: &Path) -> Option<image::ImageFormat> {
    let e = extension_of(path)?;
    if !SAVE_EXTENSIONS.contains(&e.as_str()) {
        return None;
    }
    match e.as_str() {
        "dib" => Some(image::ImageFormat::Bmp),
        "jfif" => Some(image::ImageFormat::Jpeg),
        other => image::ImageFormat::from_extension(other),
    }
}

/// `bytes` as a picture the editor page can show: as they are when they already are a PNG, JPEG, GIF, BMP or WebP,
/// else decoded and written as a PNG. None when they are not a picture any supported format can read.
pub fn for_page(bytes: &[u8]) -> Option<Vec<u8>> {
    if sniff_mime(bytes).is_some() {
        return Some(bytes.to_vec());
    }
    let img = image::load_from_memory(bytes).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// The picture `bytes` as it is written to `path`: untouched when it is already in the format the extension asks
/// for, else converted to it.
pub fn encode_for(bytes: &[u8], path: &Path) -> Result<Vec<u8>, String> {
    let Some(format) = target_format(path) else { return Ok(bytes.to_vec()) };
    if image::guess_format(bytes).ok() == Some(format) {
        return Ok(bytes.to_vec());
    }
    let img = image::load_from_memory(bytes).map_err(|e| format!("The picture cannot be read: {e}"))?;
    let img = match format {
        // These have no transparency (or no 8-bit colour): flatten to what they can hold.
        image::ImageFormat::Jpeg | image::ImageFormat::Pnm => image::DynamicImage::ImageRgb8(img.to_rgb8()),
        image::ImageFormat::Hdr | image::ImageFormat::OpenExr => image::DynamicImage::ImageRgb32F(img.to_rgb32f()),
        _ => img,
    };
    let mut out = std::io::Cursor::new(Vec::new());
    let written = if format == image::ImageFormat::Avif {
        // The encoder's default speed is for patient batch jobs; a Save has to come back in seconds.
        let encoder = image::codecs::avif::AvifEncoder::new_with_speed_quality(&mut out, 10, 80);
        img.write_with_encoder(encoder)
    } else {
        img.write_to(&mut out, format)
    };
    written.map_err(|e| format!("The picture cannot be written as {}: {e}", extension_of(path).unwrap_or_default().to_uppercase()))?;
    Ok(out.into_inner())
}

/// The picture type of `bytes` by its signature, as a MIME type. None when it is none of the supported kinds.
pub fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [b'G', b'I', b'F', b'8', ..] => Some("image/gif"),
        [b'B', b'M', ..] => Some("image/bmp"),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some("image/webp"),
        [_, _, _, _, b'f', b't', b'y', b'p', b'a', b'v', b'i', b'f' | b's', ..] => Some("image/avif"),
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

/// The path a picture is written to: the one asked for when its extension is a picture type that can be written,
/// else the same name as a PNG.
pub fn png_path(path: &Path) -> PathBuf {
    if target_format(path).is_some() { path.to_path_buf() } else { path.with_extension("png") }
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
        assert_eq!(sniff_mime(b"\0\0\0\x1cftypavif\0\0\0\0"), Some("image/avif"));
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
        assert_eq!(png_path(Path::new("photo.jpg")), PathBuf::from("photo.jpg"));
        assert_eq!(png_path(Path::new("photo.TIFF")), PathBuf::from("photo.TIFF"));
        assert_eq!(png_path(Path::new("photo.dds")), PathBuf::from("photo.png"));
        assert_eq!(png_path(Path::new("photo.txt")), PathBuf::from("photo.png"));
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
