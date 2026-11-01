//! A simple list of strings, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let l = ListBox::new(&parent);
//! l.append("one");
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::ListBox as WxListBox;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A simple list of strings.
    ListBox, WxListBox, Role::Input
);

impl ListBox {
    pub fn new(parent: &impl WxWidget) -> ListBox {
        ListBox::adopt(WxListBox::builder(parent).build())
    }
}
