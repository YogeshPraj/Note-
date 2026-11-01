//! A simple list of strings, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let l = ListBox::new(&parent);
//! l.append("one");
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::ListBox as WxListBox;
use wxdragon::prelude::*;

themed!(
    /// A simple list of strings.
    ListBox, WxListBox, Role::Input
);

impl ListBox {
    pub fn new(parent: &impl WxWidget) -> ListBox {
        ListBox::adopt(WxListBox::builder(parent).build())
    }
}
