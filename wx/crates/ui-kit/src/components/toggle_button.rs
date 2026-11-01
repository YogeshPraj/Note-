//! A button that stays pressed, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let t = ToggleButton::new(&parent, "Bold");
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::ToggleButton as WxToggleButton;
use wxdragon::prelude::*;

themed!(
    /// A button that stays pressed.
    ToggleButton, WxToggleButton, Role::Surface
);

impl ToggleButton {
    pub fn new(parent: &impl WxWidget, label: &str) -> ToggleButton {
        ToggleButton::adopt(WxToggleButton::builder(parent).with_label(label).build())
    }
}
