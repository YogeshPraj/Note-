//! The workspace's window into the host: file pickers, questions and the encryption prompts,
//! as wxWidgets dialogs.

use std::path::{Path, PathBuf};

use notepp_core::crypto::{self, Vault};
use notepp_core::workspace::{Confirm, Ui};
use wxdragon::prelude::*;

pub struct WxUi {
    pub frame: Frame,
}

const ALL_FILES: &str = "All files (*.*)|*.*|Text (*.txt)|*.txt";

impl WxUi {
    fn message(&self, text: &str, caption: &str, style: MessageDialogStyle) -> i32 {
        MessageDialog::builder(&self.frame, text, caption).with_style(style).build().show_modal()
    }

    /// Ask for a password (hidden). None when cancelled.
    fn password(&self, message: &str, caption: &str) -> Option<String> {
        let d = TextEntryDialog::builder(&self.frame, message, caption).password().build();
        (d.show_modal() == ID_OK).then(|| d.get_value()).flatten()
    }
}

impl Ui for WxUi {
    fn pick_open(&self) -> Vec<PathBuf> {
        let d = FileDialog::builder(&self.frame)
            .with_message("Open")
            .with_wildcard(ALL_FILES)
            .with_style(FileDialogStyle::Open | FileDialogStyle::Multiple | FileDialogStyle::FileMustExist)
            .build();
        if d.show_modal() == ID_OK { d.get_paths().into_iter().map(PathBuf::from).collect() } else { Vec::new() }
    }

    fn pick_save(&self, suggested: &str) -> Option<PathBuf> {
        let d = FileDialog::builder(&self.frame)
            .with_message("Save As")
            .with_default_file(suggested)
            .with_wildcard(ALL_FILES)
            .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
            .build();
        (d.show_modal() == ID_OK).then(|| d.get_path()).flatten().map(PathBuf::from)
    }

    fn pick_session(&self, save: bool) -> Option<PathBuf> {
        let style = if save { FileDialogStyle::Save | FileDialogStyle::OverwritePrompt } else { FileDialogStyle::Open | FileDialogStyle::FileMustExist };
        let mut b = FileDialog::builder(&self.frame).with_message("Session").with_wildcard("Note++ session (*.json)|*.json|All files (*.*)|*.*").with_style(style);
        if save {
            b = b.with_default_file("session.json");
        }
        let d = b.build();
        (d.show_modal() == ID_OK).then(|| d.get_path()).flatten().map(PathBuf::from)
    }

    fn confirm_save(&self, name: &str) -> Confirm {
        let r = self.message(&format!("Save changes to \"{name}\"?"), "Note++", MessageDialogStyle::YesNo | MessageDialogStyle::Cancel | MessageDialogStyle::IconQuestion);
        match r {
            ID_YES => Confirm::Save,
            ID_NO => Confirm::Discard,
            _ => Confirm::Cancel,
        }
    }

    fn ask_yes_no(&self, msg: &str) -> bool {
        self.message(msg, "Note++", MessageDialogStyle::YesNo | MessageDialogStyle::IconQuestion) == ID_YES
    }

    fn recycle(&self, path: &Path) -> Result<(), String> {
        #[cfg(windows)]
        {
            // The shell's own "send to Recycle Bin", through the .NET file-system helper.
            let script = format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
                path.display().to_string().replace('\'', "''")
            );
            let out = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &script]).output().map_err(|e| e.to_string())?;
            if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
        }
        #[cfg(not(windows))]
        {
            let _ = path;
            Err("Moving files to the trash is not available here yet".into())
        }
    }

    fn error(&self, msg: &str) {
        self.message(msg, "Note++", MessageDialogStyle::OK | MessageDialogStyle::IconError);
    }

    fn unlock(&self, vault: &mut Vault, what: &str) -> bool {
        let Some(profile) = vault.profile.clone() else { return false };
        let mut note = String::new();
        loop {
            let msg = format!("{note}Enter your encryption password to open \"{what}\".\n(Type your recovery key instead if you forgot the password.)");
            let Some(typed) = self.password(&msg, "Unlock encryption") else { return false };
            let key = crypto::recovery_key_from_file(&typed).unwrap_or_else(|| typed.clone());
            match crypto::unlock_with_password(&profile, &typed) {
                Ok(dek) => {
                    vault.dek = Some(dek);
                    return true;
                }
                Err(_) => match crypto::unlock_with_recovery(&profile, &key) {
                    Ok(dek) => {
                        vault.dek = Some(dek);
                        return true;
                    }
                    Err(e) => note = format!("{e}\n\n"),
                },
            }
        }
    }

    fn setup_encryption(&self, vault: &mut Vault) -> bool {
        let Some(pw) = self.password(
            "Choose a password for your encrypted files.\n\nIf you forget your password AND lose your recovery key, your encrypted notes are gone forever. Note++ has no backdoor.",
            "Set up encryption",
        ) else {
            return false;
        };
        match self.password("Type the password again to confirm.", "Set up encryption") {
            Some(again) if again == pw => {}
            Some(_) => {
                self.error("The passwords do not match.");
                return false;
            }
            None => return false,
        }
        let (profile, dek, key) = match crypto::create_profile(&pw) {
            Ok(r) => r,
            Err(e) => {
                self.error(&format!("Cannot set up encryption\n\n{e}"));
                return false;
            }
        };
        let shown = format!("Your recovery key (profile {}):\n\n{key}\n\nKeep it somewhere safe. It is the only way back in if you forget your password.\n\nHave you saved it?", profile.fingerprint);
        if !self.ask_yes_no(&shown) {
            return false;
        }
        if let Err(e) = crypto::save_profile(&profile) {
            self.error(&format!("Cannot save the encryption profile\n\n{e}"));
            return false;
        }
        vault.profile = Some(profile);
        vault.dek = Some(dek);
        true
    }
}
