//! The application's icon as pixels, read from the `.ico` the executable carries: the picture for the system tray.
//! An icon file here holds PNG pictures (8 bits per channel, with alpha, not interlaced), which is all this reads.

use std::io::Read;

/// Pictures larger than this are not read (a tray icon is tiny; this only guards a damaged file).
const MAX_SIDE: u32 = 512;

/// A picture: width, height and RGBA pixels.
pub type Rgba = (u32, u32, Vec<u8>);

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// The picture of the icon file `ico` whose width is nearest `want` (a larger one before a smaller one).
#[must_use]
pub fn ico_picture(ico: &[u8], want: u32) -> Option<Rgba> {
    let count = usize::from(u16::from_le_bytes(ico.get(4..6)?.try_into().ok()?));
    let mut best: Option<(u32, usize, usize)> = None;
    for i in 0..count {
        let entry = 6 + 16 * i;
        let width = match *ico.get(entry)? {
            0 => 256,
            w => u32::from(w),
        };
        let len = usize::try_from(le32(ico, entry + 8)?).ok()?;
        let at = usize::try_from(le32(ico, entry + 12)?).ok()?;
        // Smaller than wanted ranks after every larger one.
        let rank = if width >= want { width - want } else { 1000 + (want - width) };
        if best.is_none_or(|(r, ..)| rank < r) {
            best = Some((rank, at, len));
        }
    }
    let (_, at, len) = best?;
    decode_png(ico.get(at..at.checked_add(len)?)?)
}

/// The pixels of a PNG: 8 bits per channel with alpha, not interlaced. `None` for anything else.
#[must_use]
pub fn decode_png(png: &[u8]) -> Option<Rgba> {
    if png.get(..8)? != [137, 80, 78, 71, 13, 10, 26, 10] {
        return None;
    }
    let (mut at, mut size, mut data) = (8, None, Vec::new());
    while at + 8 <= png.len() {
        let len = usize::try_from(be32(png, at)?).ok()?;
        let kind = png.get(at + 4..at + 8)?;
        let body = png.get(at + 8..(at + 8).checked_add(len)?)?;
        match kind {
            b"IHDR" => {
                let (w, h) = (be32(body, 0)?, be32(body, 4)?);
                // 8 bits, colour type 6 (RGBA), the usual compression and filter, no interlacing.
                if body.get(8..13)? != [8, 6, 0, 0, 0] || w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
                    return None;
                }
                size = Some((w, h));
            }
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let (w, h) = size?;
    let mut raw = Vec::new();
    flate2::read::ZlibDecoder::new(data.as_slice()).read_to_end(&mut raw).ok()?;
    let stride = usize::try_from(w).ok()? * 4;
    let rows = usize::try_from(h).ok()?;
    if raw.len() != (stride + 1) * rows {
        return None;
    }
    let mut pixels = vec![0u8; stride * rows];
    for y in 0..rows {
        let line = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let (filter, line) = (line[0], &line[1..]);
        for x in 0..stride {
            let left = if x >= 4 { pixels[y * stride + x - 4] } else { 0 };
            let up = if y > 0 { pixels[(y - 1) * stride + x] } else { 0 };
            let up_left = if x >= 4 && y > 0 { pixels[(y - 1) * stride + x - 4] } else { 0 };
            let add = match filter {
                0 => 0,
                1 => left,
                2 => up,
                3 => left.midpoint(up),
                4 => paeth(left, up, up_left),
                _ => return None,
            };
            pixels[y * stride + x] = line[x].wrapping_add(add);
        }
    }
    Some((w, h, pixels))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i32::from(a), i32::from(b), i32::from(c));
    let p = ia + ib - ic;
    let (pa, pb, pc) = ((p - ia).abs(), (p - ib).abs(), (p - ic).abs());
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_applications_own_icon_gives_a_picture_of_the_wanted_size() {
        let ico = include_bytes!("../../../assets/app.ico");
        let picture = ico_picture(ico, 32);
        assert_eq!(picture.as_ref().map(|p| (p.0, p.1)), Some((32, 32)));
        let pixels = picture.map(|p| p.2).unwrap_or_default();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks(4).any(|p| p[3] > 0), "not all transparent");
    }

    #[test]
    fn a_size_nobody_has_takes_the_next_larger_picture() {
        let ico = include_bytes!("../../../assets/app.ico");
        assert_eq!(ico_picture(ico, 40).map(|p| p.0), Some(48));
        assert_eq!(ico_picture(ico, 1000).map(|p| p.0), Some(256));
    }

    #[test]
    fn anything_else_is_refused() {
        assert_eq!(ico_picture(b"not an icon", 32), None);
        assert_eq!(decode_png(&[137, 80, 78, 71, 13, 10, 26, 10]), None);
    }
}
