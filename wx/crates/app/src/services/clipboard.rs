//! The clipboard: copying and pasting text, and keeping a copied password from lingering.
//!
//! A password copied from the vault is remembered only as a fingerprint (a digest). It is cleared from the
//! clipboard after [`SECRET_TTL`] if the clipboard still holds it, and kept out of the clipboard history. The rules
//! for that are in [`SecretGuard`], which has no toolkit in it and is unit tested; the functions below it talk to
//! the system clipboard.

use std::time::Instant;

use notepp_core::secret::{SECRET_TTL, SecretGuard};
use wxdragon::prelude::Clipboard;

/// How long a copied password stays on the clipboard.


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
        if let Some(fp) = self.secret.take_if_expired(Instant::now(), SECRET_TTL) {
            if paste().is_some_and(|t| fp.matches(&t)) {
                Clipboard::get().clear();
            }
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
