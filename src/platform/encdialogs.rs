//! Dialogs of the encrypted pad: unlock, first-time setup, recovery key, and Settings >
//! Encryption. The cryptography itself is in `core::crypto`; this file only asks and shows.
//!
//! Forms cannot be nested, so every step is its own form and the flows are plain sequences.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::*;
use windows::core::{PCWSTR, PWSTR};

use super::clip;
use super::dialogs;
use super::form::{self, Form, Kind, Spec, ctl};
use crate::core::crypto::{self, Dek, Profile, Vault};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn ask(host: HWND, text: &str) -> bool {
    dialogs::message(host, "Note++", text, dialogs::Tone::Question, &[("&Yes", dialogs::BTN_YES), ("&No", dialogs::BTN_NO)], dialogs::BTN_NO, dialogs::BTN_NO) == dialogs::BTN_YES
}

fn tell(host: HWND, text: &str) {
    dialogs::message(host, "Note++", text, dialogs::Tone::Info, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
}

fn fail(host: HWND, text: &str) {
    dialogs::message(host, "Note++", text, dialogs::Tone::Error, &[("OK", dialogs::BTN_OK)], dialogs::BTN_OK, dialogs::BTN_OK);
}

// ---- file pickers ---------------------------------------------------------------------------

fn pick(owner: HWND, save: bool, suggested: &str, filter: &[&str]) -> Option<PathBuf> {
    let mut buf = vec![0u16; 32768];
    for (i, c) in suggested.encode_utf16().take(buf.len() - 1).enumerate() {
        buf[i] = c;
    }
    let mut f: Vec<u16> = Vec::new();
    for p in filter {
        f.extend(p.encode_utf16());
        f.push(0);
    }
    f.push(0);
    let ext = wide("json");
    let mut ofn = OPENFILENAMEW {
        lStructSize: size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(f.as_ptr()),
        lpstrFile: PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrDefExt: PCWSTR(ext.as_ptr()),
        Flags: if save { OFN_EXPLORER | OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST } else { OFN_EXPLORER | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_HIDEREADONLY },
        ..Default::default()
    };
    let ok = unsafe { if save { GetSaveFileNameW(&mut ofn) } else { GetOpenFileNameW(&mut ofn) } };
    if !ok.as_bool() {
        return None;
    }
    let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(PathBuf::from(String::from_utf16_lossy(&buf[..end])))
}

// ---- unlock ---------------------------------------------------------------------------------

const U_INTRO: usize = 10;
const U_PW: usize = 11;
const U_STATUS: usize = 12;
const U_RECOVERY: usize = 13;
const U_HINT: usize = 14;

fn unlock_click(f: &Form, id: usize) {
    if id == U_RECOVERY {
        let rec = f.checked(U_RECOVERY);
        f.set_masked(U_PW, !rec);
        f.set_text(U_PW, "");
        f.set_text(U_STATUS, "");
        f.set_text(
            U_HINT,
            if rec { "Type or paste your recovery key (or the contents of recovery.json). You will then choose a new password." } else { "" },
        );
        f.focus(U_PW);
    }
}

pub struct Unlocked {
    pub dek: Dek,
    pub via_recovery: bool,
}

/// Ask for the password (or the recovery key) of `profile`. None when the user cancels.
pub fn unlock(host: HWND, profile: &Profile, what: &str) -> Option<Unlocked> {
    let out: Rc<RefCell<Option<Unlocked>>> = Rc::new(RefCell::new(None));
    let result = out.clone();
    let profile = profile.clone();
    let apply = move |v: &form::Values, f: &Form| {
        let typed = v.text(U_PW).to_string();
        let recovery = v.checked(U_RECOVERY);
        let attempt = if recovery {
            let key = crypto::recovery_key_from_file(&typed).unwrap_or(typed);
            crypto::unlock_with_recovery(&profile, &key)
        } else {
            crypto::unlock_with_password(&profile, &typed)
        };
        match attempt {
            Ok(dek) => {
                *result.borrow_mut() = Some(Unlocked { dek, via_recovery: recovery });
                false
            }
            Err(e) => {
                f.set_text(U_STATUS, &e);
                f.set_text(U_PW, "");
                f.focus(U_PW);
                true
            }
        }
    };
    let spec = Spec {
        title: "Unlock encryption",
        size: (460, 236),
        ctls: vec![
            ctl(U_INTRO, Kind::Label, &format!("Enter your encryption password to open \"{what}\"."), 18, 14, 424, 40),
            ctl(U_PW, Kind::Password, "", 18, 58, 424, 24),
            ctl(U_STATUS, Kind::Label, "", 18, 90, 424, 20),
            ctl(U_RECOVERY, Kind::Check, "I forgot my password - use my recovery key instead", 18, 120, 424, 20),
            ctl(U_HINT, Kind::Label, "", 18, 146, 424, 40),
        ],
        ok: "Unlock",
        on_change: Some(unlock_click),
        on_apply: Some(&apply),
    };
    form::run(host, spec, |_| {})?;
    let r = out.borrow_mut().take();
    r
}

// ---- choosing a password --------------------------------------------------------------------

const P_WARN: usize = 20;
const P_PW: usize = 22;
const P_CONFIRM: usize = 24;
const P_STRENGTH: usize = 25;
const P_STATUS: usize = 26;

fn password_changed(f: &Form, id: usize) {
    if id == P_PW {
        let pw = f.text(P_PW);
        f.set_text(P_STRENGTH, &if pw.is_empty() { String::new() } else { crypto::strength(&pw).1.to_string() });
    }
}

/// Ask for a new password twice, with a strength hint. None when cancelled.
pub fn new_password(host: HWND, title: &str, intro: &str) -> Option<String> {
    let out: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let result = out.clone();
    let apply = move |v: &form::Values, f: &Form| {
        let (a, b) = (v.text(P_PW), v.text(P_CONFIRM));
        let problem = if a.chars().count() < 8 {
            Some("The password must have at least 8 characters.")
        } else if a != b {
            Some("The two passwords are not the same.")
        } else {
            None
        };
        match problem {
            Some(p) => {
                f.set_text(P_STATUS, p);
                f.focus(P_PW);
                true
            }
            None => {
                *result.borrow_mut() = Some(a.to_string());
                false
            }
        }
    };
    form::on_edit(password_changed);
    let spec = Spec {
        title,
        size: (500, 300),
        ctls: vec![
            ctl(P_WARN, Kind::Label, intro, 18, 12, 464, 78),
            ctl(21, Kind::Label, "Password:", 18, 106, 100, 18),
            ctl(P_PW, Kind::Password, "", 130, 102, 352, 24),
            ctl(23, Kind::Label, "Confirm:", 18, 140, 100, 18),
            ctl(P_CONFIRM, Kind::Password, "", 130, 136, 352, 24),
            ctl(P_STRENGTH, Kind::Label, "", 130, 168, 352, 20),
            ctl(P_STATUS, Kind::Label, "", 130, 192, 352, 36),
        ],
        ok: "OK",
        on_change: None,
        on_apply: Some(&apply),
    };
    form::run(host, spec, |_| {})?;
    let r = out.borrow_mut().take();
    r
}

// ---- the recovery key -----------------------------------------------------------------------

const R_TEXT: usize = 30;
const R_COPY: usize = 31;
const R_SAVE: usize = 32;
const R_SAVED: usize = 33;

thread_local! {
    static RECOVERY: RefCell<(String, String)> = const { RefCell::new((String::new(), String::new())) };
    static ACTION: Cell<usize> = const { Cell::new(0) };
}

fn recovery_click(f: &Form, id: usize) {
    let (fp, key) = RECOVERY.with(|r| r.borrow().clone());
    match id {
        R_SAVED => f.enable(1, f.checked(R_SAVED)),
        R_COPY => {
            clip::set_text(f.hwnd(), &key);
        }
        R_SAVE => {
            if let Some(p) = pick(f.hwnd(), true, "recovery.json", &["JSON (*.json)", "*.json", "All files (*.*)", "*.*"]) {
                if let Err(e) = std::fs::write(&p, crypto::recovery_file(&fp, &key)) {
                    fail(f.hwnd(), &format!("Cannot save {}\n\n{e}", p.display()));
                }
            }
        }
        _ => {}
    }
}

/// Show the recovery key once. True when the user confirmed they saved it.
pub fn show_recovery(host: HWND, fingerprint: &str, key: &str) -> bool {
    RECOVERY.with(|r| *r.borrow_mut() = (fingerprint.to_string(), key.to_string()));
    let line = "=".repeat(41);
    let text = format!(
        "Note++ Recovery Key\r\nProfile fingerprint: {}\r\n{line}\r\n{key}\r\n{line}\r\n\r\nAnyone with this code can decrypt your protected notes.\r\nStore it in a password manager or a safe place.\r\nNote++ cannot show it again without your password.",
        crypto::display_fingerprint(fingerprint)
    );
    let spec = Spec {
        title: "Your recovery key",
        size: (540, 372),
        ctls: vec![
            ctl(R_TEXT, Kind::MultiEdit(true), &text, 16, 14, 508, 190),
            ctl(R_COPY, Kind::Button, "Copy key", 16, 214, 110, 26),
            ctl(R_SAVE, Kind::Button, "Save recovery.json...", 134, 214, 170, 26),
            ctl(R_SAVED, Kind::Check, "I have saved my recovery key", 16, 258, 508, 20),
            ctl(34, Kind::Label, "If you forget your password and lose this key, your encrypted files cannot be recovered.", 16, 286, 508, 36),
        ],
        ok: "Done",
        on_change: Some(recovery_click),
        on_apply: None,
    };
    form::run(host, spec, |f| f.enable(1, false)).is_some()
}

// ---- flows ----------------------------------------------------------------------------------

/// First-time setup: choose a password, create the profile, show the recovery key. Saves the
/// profile and leaves `vault` unlocked. False when cancelled or on error.
pub fn setup(host: HWND, vault: &mut Vault) -> bool {
    let intro = "Choose a password for your encrypted files.\r\n\r\nIf you forget your password AND lose your recovery key, your encrypted notes are gone forever. Note++ has no backdoor.";
    let Some(pw) = new_password(host, "Set up encryption", intro) else { return false };
    let (profile, dek, key) = match crypto::create_profile(&pw) {
        Ok(r) => r,
        Err(e) => {
            fail(host, &format!("Cannot set up encryption\n\n{e}"));
            return false;
        }
    };
    if !show_recovery(host, &profile.fingerprint, &key) {
        return false;
    }
    if let Err(e) = crypto::save_profile(&profile) {
        fail(host, &format!("Cannot save the encryption profile\n\n{e}"));
        return false;
    }
    vault.profile = Some(profile);
    vault.dek = Some(dek);
    true
}

/// Unlock `vault` (asking for the password, or the recovery key and then a new password).
pub fn unlock_vault(host: HWND, vault: &mut Vault, what: &str) -> bool {
    let Some(profile) = vault.profile.clone() else { return false };
    let Some(u) = unlock(host, &profile, what) else { return false };
    if u.via_recovery {
        // The recovery key is for emergencies: use it to choose a new password right away.
        let intro = "You unlocked with your recovery key.\r\n\r\nChoose a new password. Your files are not changed.";
        if let Some(pw) = new_password(host, "Choose a new password", intro) {
            let mut p = profile;
            match crypto::set_password(&mut p, &u.dek, &pw).and_then(|_| crypto::save_profile(&p)) {
                Ok(()) => vault.profile = Some(p),
                Err(e) => fail(host, &format!("Cannot save the new password\n\n{e}")),
            }
        }
    }
    vault.dek = Some(u.dek);
    true
}

// ---- Settings > Encryption ------------------------------------------------------------------

const S_STATUS: usize = 40;
const S_SETUP: usize = 41;
const S_LOCK: usize = 42;
const S_PASSWORD: usize = 43;
const S_RECOVERY: usize = 44;
const S_IMPORT: usize = 45;
const S_CLOSE: usize = 47;

fn settings_click(f: &Form, id: usize) {
    if id == S_CLOSE {
        ACTION.with(|a| a.set(0));
        f.close(true);
    } else if matches!(id, S_SETUP | S_LOCK | S_PASSWORD | S_RECOVERY | S_IMPORT) {
        ACTION.with(|a| a.set(id));
        f.close(true);
    }
}

fn status_text(v: &Vault) -> String {
    match &v.profile {
        None => "Encryption is not set up.\r\n\r\nSet it up to protect files with a password. One password protects all your encrypted files.".into(),
        Some(p) => format!(
            "Profile: {}\r\nCreated: {}\r\nStatus: {}\r\n\r\nFiles stay readable while unlocked; locking forgets the key until you enter the password again.",
            p.fingerprint_display(),
            p.created_at.replace('T', " ").trim_end_matches('Z'),
            if v.unlocked() { "Unlocked" } else { "Locked" }
        ),
    }
}

/// Settings > Encryption. Changes made here are written back to `vault`.
pub fn settings(host: HWND, vault: &mut Vault) {
    loop {
        ACTION.with(|a| a.set(0));
        let has = vault.profile.is_some();
        let unlocked = vault.unlocked();
        let spec = Spec {
            title: "Encryption",
            size: (470, 300),
            ctls: vec![
                ctl(S_STATUS, Kind::Label, &status_text(vault), 16, 12, 438, 104),
                ctl(S_SETUP, Kind::Button, "Set up / replace profile...", 16, 128, 212, 30),
                ctl(S_LOCK, Kind::Button, if unlocked { "Lock now" } else { "Unlock..." }, 242, 128, 212, 30),
                ctl(S_PASSWORD, Kind::Button, "Change password...", 16, 166, 212, 30),
                ctl(S_RECOVERY, Kind::Button, "New recovery key...", 242, 166, 212, 30),
                ctl(S_IMPORT, Kind::Button, "Import profile...", 16, 204, 212, 30),
                ctl(S_CLOSE, Kind::Button, "Close", 374, 258, 80, 28),
            ],
            ok: "",
            on_change: Some(settings_click),
            on_apply: None,
        };
        let shown = form::run(host, spec, |f| {
            f.enable(S_LOCK, has);
            f.enable(S_PASSWORD, has);
            f.enable(S_RECOVERY, has);
        });
        if shown.is_none() {
            return;
        }
        match ACTION.with(|a| a.get()) {
            S_SETUP => {
                if has && !ask(host, "Replace the encryption profile?\n\nFiles protected by the current profile can no longer be opened unless you restore it. Only do this if you have no encrypted files, or have a copy of the old profile.") {
                    continue;
                }
                setup(host, vault);
            }
            S_LOCK => {
                if unlocked {
                    vault.lock();
                } else {
                    unlock_vault(host, vault, "your encrypted files");
                }
            }
            S_PASSWORD => {
                if !vault.unlocked() && !unlock_vault(host, vault, "your encrypted files") {
                    continue;
                }
                let intro = "Choose a new password.\r\n\r\nYour encrypted files do not need to be touched: only the key's wrapping changes.";
                if let (Some(pw), Some(dek), Some(p)) = (new_password(host, "Change password", intro), vault.dek.clone(), vault.profile.clone()) {
                    let mut p = p;
                    match crypto::set_password(&mut p, &dek, &pw).and_then(|_| crypto::save_profile(&p)) {
                        Ok(()) => {
                            vault.profile = Some(p);
                            tell(host, "Your password was changed.");
                        }
                        Err(e) => fail(host, &format!("Cannot change the password\n\n{e}")),
                    }
                }
            }
            S_RECOVERY => {
                if !vault.unlocked() && !unlock_vault(host, vault, "your encrypted files") {
                    continue;
                }
                if !ask(host, "Create a new recovery key?\n\nThe old recovery key stops working.") {
                    continue;
                }
                if let (Some(dek), Some(p)) = (vault.dek.clone(), vault.profile.clone()) {
                    let mut p = p;
                    match crypto::regenerate_recovery(&mut p, &dek) {
                        Ok(key) => {
                            // Only replace the stored key once the user has seen and saved the new one.
                            if show_recovery(host, &p.fingerprint, &key) {
                                match crypto::save_profile(&p) {
                                    Ok(()) => vault.profile = Some(p),
                                    Err(e) => fail(host, &format!("Cannot save the profile\n\n{e}")),
                                }
                            }
                        }
                        Err(e) => fail(host, &format!("Cannot create a recovery key\n\n{e}")),
                    }
                }
            }
            S_IMPORT => {
                let Some(path) = pick(host, false, "", &["Encryption profile (profile.json)", "*.json", "All files (*.*)", "*.*"]) else { continue };
                let parsed = std::fs::read_to_string(&path).ok().and_then(|t| Profile::parse(&t));
                let Some(p) = parsed else {
                    fail(host, "That file is not a Note++ encryption profile.");
                    continue;
                };
                if has && !ask(host, "Replace the current encryption profile with the imported one?") {
                    continue;
                }
                match crypto::save_profile(&p) {
                    Ok(()) => {
                        vault.profile = Some(p);
                        vault.lock();
                    }
                    Err(e) => fail(host, &format!("Cannot save the profile\n\n{e}")),
                }
            }
            _ => return,
        }
    }
}
