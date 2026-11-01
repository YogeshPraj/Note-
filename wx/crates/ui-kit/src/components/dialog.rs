//! A modal or modeless dialog: themed background and title bar.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let d = Dialog::new(&frame, "Options");
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::Dialog as WxDialog;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A modal or modeless dialog: themed background and title bar.
    Dialog, WxDialog, Role::Surface
);

impl Dialog {
    pub fn new(parent: &dyn WxWidget, title: &str) -> Dialog {
        Dialog::adopt(WxDialog::builder(parent, title).build())
    }
}
