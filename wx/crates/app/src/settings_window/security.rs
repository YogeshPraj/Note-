//! The Security page: the encryption profile and the password vault.

use super::{Env, button, number};
use crate::widgets::settings_window::{Control, Page, Row};

pub(super) fn page(env: &Env) -> Page {
    let s = &env.settings;
    Page::new(
        "security",
        "Security",
        0,
        vec![
            Row::new("", "Encryption", &env.encryption, Control::Heading),
            button("enc_setup", "Profile", "Set up the password that protects encrypted files, or replace the profile.", "Set up profile..."),
            button(
                "enc_lock",
                "Lock",
                "Forget the key until the password is entered again, or enter it now.",
                if env.unlocked { "Lock now" } else { "Unlock..." },
            ),
            button("enc_password", "Password", "Choose a new password. Encrypted files do not need to be touched.", "Change password..."),
            button("enc_recovery", "Recovery key", "Create a new recovery key. The old one stops working.", "New recovery key..."),
            button("enc_import", "Import", "Use an encryption profile saved elsewhere.", "Import profile..."),
            Row::heading("Passwords"),
            number(
                "lock",
                "Lock the vault when idle",
                "Lock Bitwarden after this many idle minutes. 0 means never.",
                i64::from(s.passwords_auto_lock_min),
                0,
                1440,
            ),
        ],
    )
}
