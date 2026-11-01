//! Planning how to fill a multi-column list, with no toolkit in it.
//!
//! The wx list cannot delete a single column, so a changed column set is rebuilt from scratch; rows are inserted
//! with their first cell and the other cells set afterwards. These plans say which calls to make.

/// A step in setting up columns.
#[derive(Debug, PartialEq, Eq)]
pub enum ColumnOp {
    /// Remove every column (and row) so the set can be rebuilt.
    ClearAll,
    Insert {
        index: usize,
        name: String,
        width: i32,
    },
}

/// What to do to get from a list with `have` columns to `want`.
pub fn column_ops(have: usize, want: &[(&str, i32)]) -> Vec<ColumnOp> {
    let mut ops = Vec::new();
    if have > 0 {
        ops.push(ColumnOp::ClearAll);
    }
    ops.extend(want.iter().enumerate().map(|(i, (name, width))| ColumnOp::Insert { index: i, name: (*name).to_string(), width: *width }));
    ops
}

/// A step in filling rows.
#[derive(Debug, PartialEq, Eq)]
pub enum CellOp {
    /// Add row `row` with its first cell.
    Insert {
        row: usize,
        text: String,
    },
    Set {
        row: usize,
        col: usize,
        text: String,
    },
}

/// The calls that put `rows` into a list of `columns` columns. Cells past the last column are dropped; a missing
/// first cell is empty.
pub fn cell_ops(rows: &[Vec<String>], columns: usize) -> Vec<CellOp> {
    let mut ops = Vec::new();
    for (row, cells) in rows.iter().enumerate() {
        ops.push(CellOp::Insert { row, text: cells.first().cloned().unwrap_or_default() });
        for (col, text) in cells.iter().enumerate().skip(1).take(columns.saturating_sub(1)) {
            ops.push(CellOp::Set { row, col, text: text.clone() });
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_list_only_needs_its_columns_inserted() {
        let ops = column_ops(0, &[("Name", 100), ("Path", 200)]);
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0], ColumnOp::Insert { index: 0, name: "Name".into(), width: 100 });
        assert_eq!(ops[1], ColumnOp::Insert { index: 1, name: "Path".into(), width: 200 });
    }

    #[test]
    fn a_list_that_has_columns_is_cleared_first() {
        let ops = column_ops(3, &[("A", 10)]);
        assert_eq!(ops[0], ColumnOp::ClearAll);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn no_wanted_columns_on_an_empty_list_does_nothing() {
        assert!(column_ops(0, &[]).is_empty());
    }

    #[test]
    fn rows_insert_the_first_cell_then_set_the_rest() {
        let rows = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
        let ops = cell_ops(&rows, 3);
        assert_eq!(ops[0], CellOp::Insert { row: 0, text: "a".into() });
        assert_eq!(ops[1], CellOp::Set { row: 0, col: 1, text: "b".into() });
        assert_eq!(ops[2], CellOp::Set { row: 0, col: 2, text: "c".into() });
    }

    #[test]
    fn cells_past_the_last_column_are_dropped() {
        let rows = vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]];
        assert_eq!(cell_ops(&rows, 2).len(), 2);
        assert_eq!(cell_ops(&rows, 1).len(), 1);
    }

    #[test]
    fn short_and_empty_rows_still_get_a_row() {
        let rows = vec![vec![], vec!["x".to_string()]];
        let ops = cell_ops(&rows, 3);
        assert_eq!(ops, vec![CellOp::Insert { row: 0, text: String::new() }, CellOp::Insert { row: 1, text: "x".into() }]);
    }

    #[test]
    fn rows_are_numbered_in_order() {
        let rows: Vec<Vec<String>> = (0..3).map(|i| vec![i.to_string()]).collect();
        let idx: Vec<usize> = cell_ops(&rows, 1).iter().map(|o| if let CellOp::Insert { row, .. } = o { *row } else { 99 }).collect();
        assert_eq!(idx, vec![0, 1, 2]);
    }
}
