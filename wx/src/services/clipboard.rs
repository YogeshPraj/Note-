//! The clipboard: copying and pasting text, and keeping a copied password from lingering.
//!
//! A password copied from the vault is remembered only as a fingerprint (a digest). It is cleared from the
//! clipboard after [`SECRET_TTL`] if the clipboard still holds it, and kept out of the clipboard history. The rules
//! for that are in [`SecretGuard`], which has no toolkit in it and is unit tested; the functions below it talk to
//! the system clipboard.

use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};
use wxdragon::prelude::Clipboard;

/// How long a copied password stays on the clipboard.
pub const SECRET_TTL: Duration = Duration::from_secs(30);

/// A one-way fingerprint of a text; it can recognise the text again without keeping it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fingerprint(Vec<u8>);

impl Fingerprint {
    pub fn of(text: &str) -> Fingerprint {
        Fingerprint(Sha256::digest(text.as_bytes()).to_vec())
    }

    pub fn matches(&self, text: &str) -> bool {
        *self == Fingerprint::of(text)
    }
}

/// Remembers the password last copied, and when.
#[derive(Default)]
pub struct SecretGuard {
    armed: Option<(Fingerprint, Instant)>,
}

impl SecretGuard {
    /// A password was copied at `now`.
    pub fn arm(&mut self, secret: &str, now: Instant) {
        self.armed = Some((Fingerprint::of(secret), now));
    }

    /// Is `text` the password being guarded (so it must stay out of the history)?
    pub fn is_secret(&self, text: &str) -> bool {
        self.armed.as_ref().is_some_and(|(f, _)| f.matches(text))
    }

    pub fn is_armed(&self) -> bool {
        self.armed.is_some()
    }

    /// Once the password has been on the clipboard for `ttl`: its fingerprint, so the caller can clear the
    /// clipboard if it still holds it. The guard then disarms; before that, nothing.
    pub fn take_if_expired(&mut self, now: Instant, ttl: Duration) -> Option<Fingerprint> {
        match &self.armed {
            Some((_, at)) if now.duration_since(*at) >= ttl => self.armed.take().map(|(f, _)| f),
            _ => None,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fingerprint_recognises_its_text_and_nothing_else() {
        let f = Fingerprint::of("hunter2");
        assert!(f.matches("hunter2"));
        assert!(!f.matches("hunter3"));
        assert!(!f.matches(""));
    }

    #[test]
    fn a_fingerprint_does_not_contain_the_text() {
        let f = Fingerprint::of("hunter2");
        assert!(!format!("{f:?}").contains("hunter2"));
    }

    #[test]
    fn nothing_is_secret_until_armed() {
        let g = SecretGuard::default();
        assert!(!g.is_secret("x"));
        assert!(!g.is_armed());
    }

    #[test]
    fn an_armed_guard_recognises_only_the_password() {
        let mut g = SecretGuard::default();
        g.arm("pw", Instant::now());
        assert!(g.is_secret("pw") && !g.is_secret("other"));
    }

    #[test]
    fn it_expires_after_the_ttl_not_before() {
        let t0 = Instant::now();
        let mut g = SecretGuard::default();
        g.arm("pw", t0);
        assert!(g.take_if_expired(t0 + Duration::from_secs(29), SECRET_TTL).is_none());
        assert!(g.is_armed());
        let fp = g.take_if_expired(t0 + SECRET_TTL, SECRET_TTL).expect("expired");
        assert!(fp.matches("pw"));
    }

    #[test]
    fn expiry_happens_once_and_disarms() {
        let t0 = Instant::now();
        let mut g = SecretGuard::default();
        g.arm("pw", t0);
        assert!(g.take_if_expired(t0 + Duration::from_secs(60), SECRET_TTL).is_some());
        assert!(g.take_if_expired(t0 + Duration::from_secs(61), SECRET_TTL).is_none());
        assert!(!g.is_armed() && !g.is_secret("pw"));
    }

    #[test]
    fn arming_again_restarts_the_clock_and_replaces_the_password() {
        let t0 = Instant::now();
        let mut g = SecretGuard::default();
        g.arm("old", t0);
        g.arm("new", t0 + Duration::from_secs(20));
        assert!(g.take_if_expired(t0 + Duration::from_secs(40), SECRET_TTL).is_none());
        assert!(g.is_secret("new") && !g.is_secret("old"));
    }
}
