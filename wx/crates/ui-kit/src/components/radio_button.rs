//! A radio button, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let r = RadioButton::builder(&parent).with_label("Forward").build();
//! let r = RadioButton::adopt(r);
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::RadioButton as WxRadioButton;
use wxdragon::prelude::*;

themed!(
    /// A radio button.
    RadioButton, WxRadioButton, Role::Surface
);
