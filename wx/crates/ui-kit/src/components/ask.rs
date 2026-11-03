//! Asking the user: messages and questions (in the themed [`super::message_box`]) and the system pickers (files, a
//! folder, one of a list, a colour, a font). The shell asks through these and never builds a dialog of this kind
//! itself, so how a question looks is decided here once.

use std::path::PathBuf;

use wxdragon::prelude::*;

use super::message_box::{self, Choice};
use crate::paint::{Rgb, colour};

/// The icon beside a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Info,
    Question,
    Warning,
    Error,
}

/// The answer to a Yes / No / Cancel question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reply {
    Yes,
    No,
    Cancel,
}

const OK: Choice = Choice { code: ID_OK, label: "OK" };
const YES: Choice = Choice { code: ID_YES, label: "&Yes" };
const NO: Choice = Choice { code: ID_NO, label: "&No" };
const CANCEL: Choice = Choice { code: ID_CANCEL, label: "Cancel" };

/// Tell the user something; they press OK (or Enter, or Esc).
pub fn tell(parent: &impl WxWidget, title: &str, text: &str, icon: Icon) {
    message_box::show(parent, title, text, Some(icon), &[OK], ID_OK, ID_OK);
}

/// A Yes / No question; Esc is No. `safe_no` makes No the default button, for questions whose Yes is hard to undo.
pub fn yes_no(parent: &impl WxWidget, title: &str, text: &str, safe_no: bool) -> bool {
    let default = if safe_no { ID_NO } else { ID_YES };
    message_box::show(parent, title, text, Some(Icon::Question), &[NO, YES], default, ID_NO) == ID_YES
}

/// A Yes / No / Cancel question; Esc and closing the box are Cancel.
pub fn yes_no_cancel(parent: &impl WxWidget, title: &str, text: &str) -> Reply {
    match message_box::show(parent, title, text, Some(Icon::Question), &[CANCEL, NO, YES], ID_YES, ID_CANCEL) {
        ID_YES => Reply::Yes,
        ID_NO => Reply::No,
        _ => Reply::Cancel,
    }
}

/// Pick existing files to open (`many`: more than one). `wildcard` is wxWidgets' "Label|*.ext|..." form. Empty when cancelled.
pub fn open_files(parent: &impl WxWidget, title: &str, wildcard: &str, many: bool) -> Vec<PathBuf> {
    let mut style = FileDialogStyle::Open | FileDialogStyle::FileMustExist;
    if many {
        style |= FileDialogStyle::Multiple;
    }
    let d = FileDialog::builder(parent).with_message(title).with_wildcard(wildcard).with_style(style).build();
    if d.show_modal() != ID_OK {
        return Vec::new();
    }
    if many { d.get_paths().into_iter().map(PathBuf::from).collect() } else { d.get_path().map(PathBuf::from).into_iter().collect() }
}

/// Pick where to save, starting at `suggested`; an existing file is confirmed before it is chosen.
pub fn save_file(parent: &impl WxWidget, title: &str, suggested: &str, wildcard: &str) -> Option<PathBuf> {
    let d = FileDialog::builder(parent)
        .with_message(title)
        .with_default_file(suggested)
        .with_wildcard(wildcard)
        .with_style(FileDialogStyle::Save | FileDialogStyle::OverwritePrompt)
        .build();
    (d.show_modal() == ID_OK).then(|| d.get_path()).flatten().map(PathBuf::from)
}

/// Pick a folder.
pub fn folder(parent: &impl WxWidget, title: &str) -> Option<PathBuf> {
    let d = DirDialog::builder(parent, title, "").build();
    (d.show_modal() == ID_OK).then(|| d.get_path()).flatten().map(PathBuf::from)
}

/// Pick one of `choices`; the index of the one picked.
pub fn choice(parent: &impl WxWidget, title: &str, prompt: &str, choices: &[&str]) -> Option<usize> {
    let d = SingleChoiceDialog::builder(parent, prompt, title, choices).build();
    if d.show_modal() != ID_OK {
        return None;
    }
    usize::try_from(d.get_selection()).ok()
}

/// Pick a colour, starting at `start`.
pub fn colour_of(parent: &impl WxWidget, start: Rgb) -> Option<Rgb> {
    let d = ColourDialog::builder(parent).with_initial_colour(colour(start)).build();
    if d.show_modal() != ID_OK {
        return None;
    }
    d.get_colour().map(|c| (c.r, c.g, c.b))
}

/// Pick a font, starting at `face` in `size` points; the face name and point size picked.
pub fn font(parent: &impl WxWidget, face: &str, size: i32) -> Option<(String, i32)> {
    let mut data = FontData::new();
    if let Some(start) = Font::builder().with_face_name(face).with_point_size(size).build() {
        data.set_initial_font(&start);
    }
    let d = FontDialog::builder(parent).with_font_data(&data).build();
    if d.show_modal() != ID_OK {
        return None;
    }
    let f = d.get_font()?;
    Some((f.get_face_name(), f.get_point_size()))
}
