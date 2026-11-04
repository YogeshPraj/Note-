//! The clipboard: copying and pasting text, and keeping a copied password from lingering.
//!
//! A password copied from the vault is remembered only as a fingerprint (a digest). It is cleared from the
//! clipboard after [`SECRET_TTL`] if the clipboard still holds it, and kept out of the clipboard history. The rules
//! for that are in [`SecretGuard`], which has no toolkit in it and is unit tested; the functions below it talk to
//! the system clipboard.

use std::time::Instant;

use notepp_core::secret::{SECRET_TTL, SecretGuard};
use wxdragon::prelude::{Bitmap, BitmapDataObject, Clipboard, DataFormat};

/// The clipboard service: the system clipboard plus the password guard.
#[derive(Default)]
pub struct ClipboardService {
    pub secret: SecretGuard,
}

impl ClipboardService {
    /// Copy a password: it will be cleared in [`SECRET_TTL`]. False when the clipboard is busy.
    pub fn copy_secret(&mut self, secret: &str) -> bool {
        if !copy(secret) {
            return false;
        }
        self.secret.arm(secret, Instant::now());
        true
    }

    /// Clear the clipboard if a guarded password has been on it long enough and is still its content (the user may
    /// have copied something else since, which must be left alone).
    pub fn expire_secret(&mut self) {
        if let Some(fp) = self.secret.take_if_expired(Instant::now(), SECRET_TTL)
            && paste().is_some_and(|t| fp.matches(&t))
        {
            Clipboard::get().clear();
        }
    }
}

/// Put `text` on the system clipboard. False when it is busy.
pub fn copy(text: &str) -> bool {
    Clipboard::get().set_text(text)
}

/// The text on the system clipboard, if any.
pub fn paste() -> Option<String> {
    Clipboard::get().get_text()
}

/// Put a picture (RGBA pixels, `width` by `height`) on the system clipboard. False when it cannot be made or the
/// clipboard is busy.
pub fn copy_image(width: u32, height: u32, rgba: &[u8]) -> bool {
    let Some(bitmap) = Bitmap::from_rgba(rgba, width, height) else { return false };
    Clipboard::get().set_data(&mut BitmapDataObject::new(&bitmap))
}

/// A picture on the system clipboard, when it holds no text (text wins, as with every editor): its width, height and
/// RGBA pixels.
pub fn paste_image() -> Option<(u32, u32, Vec<u8>)> {
    let clipboard = Clipboard::get();
    if clipboard.is_format_supported(DataFormat::TEXT) || !clipboard.is_format_supported(DataFormat::BITMAP) {
        return None;
    }
    let holder = BitmapDataObject::new(&Bitmap::new(1, 1)?);
    if !clipboard.get_data(&holder) {
        return None;
    }
    let bitmap = holder.get_bitmap()?;
    let (width, height) = (u32::try_from(bitmap.get_width()).ok()?, u32::try_from(bitmap.get_height()).ok()?);
    let rgba = bitmap.get_rgba_data()?;
    (width > 0 && height > 0 && rgba.len() == usize::try_from(width).ok()?.checked_mul(usize::try_from(height).ok()?)?.checked_mul(4)?)
        .then_some((width, height, rgba))
}
