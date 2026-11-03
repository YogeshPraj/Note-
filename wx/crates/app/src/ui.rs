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

    /// A large file is read on a worker thread with a progress window and Cancel; a small one at once.
    fn read_file(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        const LARGE: u64 = 16 * 1024 * 1024;
        let size = std::fs::metadata(path)?.len();
        if size < LARGE {
            return std::fs::read(path);
        }
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let (owned, title) = (path.to_path_buf(), format!("Opening {name}"));
        crate::widgets::progress::run(&self.frame, &title, "Reading the file...", move |p| read_in_chunks(&owned, size, p))
            .unwrap_or_else(|| Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled")))
    }
}

/// Read `path` (`size` bytes) a chunk at a time, reporting progress and stopping when cancelled.
fn read_in_chunks(path: &Path, size: u64, p: &crate::widgets::progress::Progress) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    const CHUNK: usize = 4 * 1024 * 1024;
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::with_capacity(crate::platform::wx::to_usize(size));
    let mut chunk = vec![0u8; CHUNK];
    loop {
        if p.cancelled() {
            return Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "cancelled"));
        }
        let n = file.read(&mut chunk)?;
        if n == 0 {
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..n]);
        p.set(
            crate::platform::wx::to_u32(crate::platform::wx::to_u64(bytes.len()).saturating_mul(100).checked_div(size).unwrap_or(100)),
            "Reading the file...",
        );
    }
}
