//! A tree, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let t = TreeView::adopt(tree_ctrl);
//! ```

use wxdragon::prelude::*;
use wxdragon::prelude::TreeCtrl as WxTreeCtrl;
use crate::theme::Role;
use super::themed::themed;

themed!(
    /// A tree.
    TreeView, WxTreeCtrl, Role::Input
);
