//! A plain container, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let p = Panel::new(&frame);
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::Panel as WxPanel;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A plain container.
    Panel, WxPanel, Role::Surface
);

impl Panel {
    pub fn new(parent: &impl WxWidget) -> Panel {
        Panel::adopt(WxPanel::builder(parent).build())
    }
}
