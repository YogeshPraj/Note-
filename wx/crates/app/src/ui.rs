//! The workspace's window into the host: file pickers, questions and the encryption prompts,
//! as wxWidgets dialogs.

use std::path::{Path, PathBuf};

use notepp_core::crypto::Vault;
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
        let mut b =
            FileDialog::builder(&self.frame).with_message("Session").with_wildcard("Note++ session (*.json)|*.json|All files (*.*)|*.*").with_style(style);
        if save {
            b = b.with_default_file("session.json");
        }
        let d = b.build();
        (d.show_modal() == ID_OK).then(|| d.get_path()).flatten().map(PathBuf::from)
    }

    fn confirm_save(&self, name: &str) -> Confirm {
        let r = self.message(
            &format!("Save changes to \"{name}\"?"),
            "Note++",
            MessageDialogStyle::YesNo | MessageDialogStyle::Cancel | MessageDialogStyle::IconQuestion,
        );
        match r {
            ID_YES => Confirm::Save,
            ID_NO => Confirm::Discard,
            _ => Confirm::Cancel,
        }
    }

    fn ask_yes_no(&self, msg: &str) -> bool {
        self.message(msg, "Note++", MessageDialogStyle::YesNo | MessageDialogStyle::IconQuestion) == ID_YES
    }

    fn recycle(&self, path: &Path) -> Result<(), notepp_core::error::Error> {
        crate::platform::current().recycle(path).map_err(|e| notepp_core::error::Error::Io(e.to_string()))
    }

    fn error(&self, msg: &str) {
        self.message(msg, "Note++", MessageDialogStyle::OK | MessageDialogStyle::IconError);
    }

    fn unlock(&self, vault: &mut Vault, what: &str) -> bool {
        crate::encdialogs::unlock_vault(&self.frame, vault, what)
    }

    fn setup_encryption(&self, vault: &mut Vault) -> bool {
        crate::encdialogs::setup(&self.frame, vault)
    }
}
