//! What a form hands back: the text and check marks entered, read just before it closed.

use std::collections::HashMap;

use super::{Form, Kind};

/// What the user entered, read just before the form closed.
#[derive(Debug)]
pub struct Values {
    text: HashMap<usize, String>,
    checked: HashMap<usize, bool>,
}

impl Values {
    pub fn text(&self, id: usize) -> &str {
        self.text.get(&id).map_or("", String::as_str)
    }
    pub fn checked(&self, id: usize) -> bool {
        self.checked.get(&id).copied().unwrap_or(false)
    }
}

/// Read every edit box and check mark of the form.
pub(super) fn collect(kinds: &[(usize, Kind)], form: &Form) -> Values {
    let mut v = Values { text: HashMap::new(), checked: HashMap::new() };
    for (id, kind) in kinds {
        match kind {
            Kind::Edit | Kind::Password | Kind::MultiEdit(_) => {
                v.text.insert(*id, form.text(*id));
            }
            Kind::Check | Kind::Radio(_) => {
                v.checked.insert(*id, form.checked(*id));
            }
            _ => {}
        }
    }
    v
}
