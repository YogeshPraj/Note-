//! Static text, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let l = Label::new(&parent, "Find what:");
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::StaticText as WxStaticText;
use wxdragon::prelude::*;

themed!(
    /// Static text.
    Label, WxStaticText, Role::Surface
);

impl Label {
    pub fn new(parent: &impl WxWidget, text: &str) -> Label {
        Label::adopt(WxStaticText::builder(parent).with_label(text).build())
    }

    /// The stock static text's own `get_label` (a String); the window-level one would hide it.
    pub fn get_label(&self) -> String {
        self.0.get_label()
    }
}
