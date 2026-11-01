//! A list with columns, themed.
//!
//! Use it instead of the stock widget: it follows the theme.
//!
//! ```ignore
//! let v = ListView::adopt(list_ctrl);
//! v.set_columns(&[("Name", 200), ("Size", 80)]);
//! v.fill(&[vec!["a.txt".into(), "1 KB".into()]]);
//! ```

use super::data_list;
use super::themed::themed;
use crate::theme::Role;
use wxdragon::prelude::ListCtrl as WxListCtrl;
use wxdragon::prelude::*;

themed!(
    /// A list with columns.
    ListView, WxListCtrl, Role::Input
);

impl ListView {
    /// Set the columns, replacing any it had (the stock list cannot remove a single column).
    pub fn set_columns(&self, columns: &[(&str, i32)]) {
        for op in data_list::column_ops(crate::num::to_usize(self.0.get_column_count().max(0)), columns) {
            match op {
                data_list::ColumnOp::ClearAll => {
                    self.0.clear_all();
                }
                data_list::ColumnOp::Insert { index, name, width } => {
                    self.0.insert_column(crate::num::to_i64(index), &name, ListColumnFormat::Left, width);
                }
            }
        }
    }

    /// Replace all rows with `rows` (cells past the last column are ignored).
    pub fn fill(&self, rows: &[Vec<String>]) {
        self.0.delete_all_items();
        let columns = crate::num::to_usize(self.0.get_column_count().max(0));
        for op in data_list::cell_ops(rows, columns) {
            match op {
                data_list::CellOp::Insert { row, text } => {
                    self.0.insert_item(crate::num::to_i64(row), &text, None);
                }
                data_list::CellOp::Set { row, col, text } => self.0.set_item_text_by_column(crate::num::to_i64(row), crate::num::to_i32(col), &text),
            }
        }
    }

    /// The first selected row.
    pub fn selected(&self) -> Option<usize> {
        usize::try_from(self.0.get_first_selected_item()).ok()
    }

    /// Every selected row, in order.
    pub fn selected_all(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = -1i64;
        loop {
            let n = self.0.get_next_item(i, ListNextItemFlag::All, ListItemState::Selected);
            if n < 0 {
                break;
            }
            out.push(crate::num::to_usize(n));
            i = i64::from(n);
        }
        out
    }
}
