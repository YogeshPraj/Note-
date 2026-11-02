//! A text entry box (single or multi line), themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let t = TextBox::new(&parent, "initial", true);
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::TextCtrl as WxTextCtrl;
use wxdragon::prelude::*;

themed!(
    /// A text entry box (single or multi line).
    TextBox, WxTextCtrl, Role::Input
);

impl TextBox {
    /// A single-line box holding `value`; `enter` makes Enter raise an event instead of being swallowed.
    pub fn new(parent: &impl WxWidget, value: &str, enter: bool) -> TextBox {
        let b = WxTextCtrl::builder(parent).with_value(value);
        let t = if enter { b.with_style(TextCtrlStyle::ProcessEnter).build() } else { b.build() };
        crate::theme::drop_border(&t);
        TextBox::adopt(t)
    }
}
