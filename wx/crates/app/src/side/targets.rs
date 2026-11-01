//! Lists whose rows each point at something: the open documents (a tab) and the functions of the file (a line).

use super::{Activate, Content};

/// What activating a row does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Document,
    Line,
}

pub struct Targets {
    kind: Kind,
    columns: Vec<(&'static str, i32)>,
    /// The cells of each row and the target it opens.
    rows: Vec<(Vec<String>, usize)>,
}

impl Targets {
    /// A list of open documents.
    pub fn new(columns: Vec<(&'static str, i32)>) -> Targets {
        let kind = if columns.first().map(|c| c.0) == Some("Function") { Kind::Line } else { Kind::Document };
        Targets { kind, columns, rows: Vec::new() }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Replace the rows. False (and nothing changes) when they are the same as now.
    pub fn set_rows(&mut self, rows: Vec<(Vec<String>, usize)>) -> bool {
        if self.rows == rows {
            return false;
        }
        self.rows = rows;
        true
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

impl Content for Targets {
    fn columns(&self) -> Vec<(&'static str, i32)> {
        self.columns.clone()
    }

    fn rows(&self) -> Vec<Vec<String>> {
        self.rows.iter().map(|(cells, _)| cells.clone()).collect()
    }

    fn activate(&self, row: usize) -> Option<Activate> {
        let target = self.rows.get(row)?.1;
        Some(match self.kind {
            Kind::Document => Activate::Document(target),
            Kind::Line => Activate::Goto(target),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn docs() -> Targets {
        Targets::new(vec![("Name", 100), ("Path", 100)])
    }

    fn funcs() -> Targets {
        Targets::new(vec![("Function", 100), ("Line", 50)])
    }

    #[test]
    fn the_kind_follows_the_first_column() {
        assert_eq!(docs().kind(), Kind::Document);
        assert_eq!(funcs().kind(), Kind::Line);
    }

    #[test]
    fn setting_the_same_rows_again_reports_no_change() {
        let mut t = docs();
        let rows = vec![(vec!["a.rs".to_string(), "C:/a.rs".to_string()], 0)];
        assert!(t.set_rows(rows.clone()));
        assert!(!t.set_rows(rows));
    }

    #[test]
    fn a_document_row_activates_its_tab() {
        let mut t = docs();
        t.set_rows(vec![(vec!["a".into(), "".into()], 3)]);
        assert_eq!(t.activate(0), Some(Activate::Document(3)));
    }

    #[test]
    fn a_function_row_goes_to_its_line() {
        let mut t = funcs();
        t.set_rows(vec![(vec!["main".into(), "12".into()], 11)]);
        assert_eq!(t.activate(0), Some(Activate::Goto(11)));
    }

    #[test]
    fn a_row_that_does_not_exist_does_nothing() {
        assert_eq!(funcs().activate(0), None);
    }

    #[test]
    fn rows_come_back_as_their_cells() {
        let mut t = funcs();
        t.set_rows(vec![(vec!["a".into(), "1".into()], 0), (vec!["b".into(), "2".into()], 1)]);
        assert_eq!(t.rows(), vec![vec!["a", "1"], vec!["b", "2"]]);
        assert_eq!(t.len(), 2);
    }
}
