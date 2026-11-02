//! A modal or modeless dialog: themed background and title bar.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let d = Dialog::new(&frame, "Options");
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::Dialog as WxDialog;
use wxdragon::prelude::*;

themed!(
    /// A modal or modeless dialog: themed background and title bar.
    Dialog, WxDialog, Role::Surface
);

impl Dialog {
    pub fn new(parent: &dyn WxWidget, title: &str) -> Dialog {
        Dialog::adopt(WxDialog::builder(parent, title).build())
    }

    /// Make Esc end this dialog as cancelled. For modal dialogs, including ones without a Cancel button.
    pub fn close_on_escape(&self) {
        const KEY_ESCAPE: i32 = 27;
        let d = self.0;
        self.0.on_char_hook(move |e| match &e {
            WindowEventData::Keyboard(k) if k.get_key_code() == Some(KEY_ESCAPE) => d.end_modal(ID_CANCEL),
            _ => e.skip(true),
        });
    }
}
