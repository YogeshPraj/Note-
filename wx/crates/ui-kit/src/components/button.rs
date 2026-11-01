//! A push button, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let ok = Button::new(&parent, "OK");
//! ok.on_click(|_| println!("clicked"));
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::Button as WxButton;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A push button.
    Button, WxButton, Role::Surface
);

impl Button {
    pub fn new(parent: &impl WxWidget, label: &str) -> Button {
        Button::adopt(WxButton::builder(parent).with_label(label).build())
    }

    pub fn get_label(&self) -> String {
        self.0.get_label()
    }
}
