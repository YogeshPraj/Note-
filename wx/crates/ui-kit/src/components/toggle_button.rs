//! A button that stays pressed, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let t = ToggleButton::new(&parent, "Bold");
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::ToggleButton as WxToggleButton;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A button that stays pressed.
    ToggleButton, WxToggleButton, Role::Surface
);

impl ToggleButton {
    pub fn new(parent: &impl WxWidget, label: &str) -> ToggleButton {
        ToggleButton::adopt(WxToggleButton::builder(parent).with_label(label).build())
    }
}
