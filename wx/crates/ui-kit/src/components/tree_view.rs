//! A tree, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let t = TreeView::adopt(tree_ctrl);
//! ```

use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::TreeCtrl as WxTreeCtrl;

themed!(
    /// A tree.
    TreeView, WxTreeCtrl, Role::Input
);
