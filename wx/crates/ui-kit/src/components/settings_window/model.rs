//! The settings window's pages, rows and values: plain data, with no toolkit in it.

/// What a row lets the user set.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// A section heading between groups of rows (no value).
    Heading,
    Toggle(bool),
    Number {
        value: i64,
        min: i64,
        max: i64,
    },
    Text(String),
    /// One of a few options, shown as radio buttons.
    Choice {
        options: Vec<String>,
        selected: usize,
    },
    /// A push button with this label; pressing it reports the row's key.
    Button(String),
}

/// A value the user entered, as reported to the application.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Text(String),
    Choice(usize),
    Pressed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Identifies the row in change reports.
    pub key: &'static str,
    pub title: String,
    pub desc: String,
    pub control: Control,
}

impl Row {
    pub fn new(key: &'static str, title: &str, desc: &str, control: Control) -> Row {
        Row { key, title: title.into(), desc: desc.into(), control }
    }

    pub fn heading(title: &str) -> Row {
        Row::new("", title, "", Control::Heading)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub id: &'static str,
    pub title: String,
    /// Nesting in the navigation (0 = top level).
    pub level: u8,
    pub rows: Vec<Row>,
}

impl Page {
    pub fn new(id: &'static str, title: &str, level: u8, rows: Vec<Row>) -> Page {
        Page { id, title: title.into(), level, rows }
    }
}

/// The index of the page called `id`, or the first page.
pub fn page_index(pages: &[Page], id: &str) -> usize {
    pages.iter().position(|p| p.id == id).unwrap_or(0)
}

/// A number typed by the user: `None` when it is not a number, otherwise clamped to the row's range.
pub fn parse_number(text: &str, min: i64, max: i64) -> Option<i64> {
    text.trim().parse::<i64>().ok().map(|v| v.clamp(min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages() -> Vec<Page> {
        vec![Page::new("general", "General", 0, vec![]), Page::new("editor", "Editor", 0, vec![]), Page::new("fonts", "Fonts", 1, vec![])]
    }

    #[test]
    fn a_page_is_found_by_id_and_an_unknown_id_opens_the_first() {
        assert_eq!(page_index(&pages(), "fonts"), 2);
        assert_eq!(page_index(&pages(), "nope"), 0);
    }

    #[test]
    fn typed_numbers_are_clamped_and_text_is_rejected() {
        assert_eq!(parse_number(" 12 ", 1, 100), Some(12));
        assert_eq!(parse_number("500", 1, 100), Some(100));
        assert_eq!(parse_number("-3", 0, 10), Some(0));
        assert_eq!(parse_number("abc", 0, 10), None);
        assert_eq!(parse_number("", 0, 10), None);
    }

    #[test]
    fn a_heading_carries_no_key() {
        assert_eq!(Row::heading("Editor").key, "");
    }
}
