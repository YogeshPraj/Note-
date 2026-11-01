//! A check box, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let c = CheckBox::new(&parent, "Match case");
//! c.set_value(true);
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::CheckBox as WxCheckBox;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A check box.
    CheckBox, WxCheckBox, Role::Surface
);

impl CheckBox {
    pub fn new(parent: &impl WxWidget, label: &str) -> CheckBox {
        CheckBox::adopt(WxCheckBox::builder(parent).with_label(label).build())
    }
}
