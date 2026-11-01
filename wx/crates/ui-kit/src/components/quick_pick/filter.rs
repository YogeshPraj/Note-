//! Filtering and keyboard navigation for a pick list, with no toolkit in it.

/// One row of a pick list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// Right-hand detail: a shortcut, or the folder of a file.
    pub hint: String,
    /// What the caller gets back when this row is chosen.
    pub id: usize,
}

impl Item {
    pub fn new(label: impl Into<String>, hint: impl Into<String>, id: usize) -> Item {
        Item { label: label.into(), hint: hint.into(), id }
    }

    /// The text of the row as shown.
    pub fn row_text(&self) -> String {
        if self.hint.is_empty() { self.label.clone() } else { format!("{}   \u{2014} {}", self.label, self.hint) }
    }
}

/// Every word of `query` must occur in `text`; earlier hits score better (lower). An empty query matches everything.
pub fn matches(query: &str, text: &str) -> Option<usize> {
    let t = text.to_lowercase();
    let mut score = 0;
    for word in query.to_lowercase().split_whitespace() {
        score += t.find(word)?;
    }
    Some(score)
}

/// Indices of the items matching `query`, best first (ties keep the original order), at most `limit`.
pub fn rank(items: &[Item], query: &str, limit: usize) -> Vec<usize> {
    let mut scored: Vec<(usize, usize)> =
        items.iter().enumerate().filter_map(|(i, it)| matches(query, &format!("{} {}", it.label, it.hint)).map(|s| (s, i))).collect();
    scored.sort_by_key(|(s, i)| (*s, *i));
    scored.into_iter().take(limit).map(|(_, i)| i).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nav {
    Up,
    Down,
    PageUp,
    PageDown,
}

/// The selected row after `nav`, within `count` rows (`page` rows per page); -1 means nothing is selected yet.
/// Does not wrap: moving past either end stays on the first or last row. With no rows the result is -1.
pub fn navigate(sel: i32, count: i32, page: i32, nav: Nav) -> i32 {
    if count <= 0 {
        return -1;
    }
    let to = match nav {
        Nav::Up => sel - 1,
        Nav::Down => sel + 1,
        Nav::PageUp => sel - page,
        Nav::PageDown => sel + page,
    };
    to.clamp(0, count - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Item> {
        vec![Item::new("Save All", "Ctrl+Shift+S", 1), Item::new("Save", "Ctrl+S", 2), Item::new("Close All", "Ctrl+Shift+W", 3), Item::new("Open File", "", 4)]
    }

    #[test]
    fn every_word_must_match_in_any_order() {
        assert!(matches("all save", "Save All").is_some());
        assert!(matches("save zzz", "Save All").is_none());
    }

    #[test]
    fn matching_ignores_case_and_an_empty_query_matches_all() {
        assert!(matches("SAVE", "save all").is_some());
        assert_eq!(matches("", "anything"), Some(0));
        assert_eq!(matches("   ", "anything"), Some(0));
    }

    #[test]
    fn earlier_hits_rank_better() {
        assert!(matches("save", "Save All").unwrap() < matches("save", "Close and Save").unwrap());
    }

    #[test]
    fn ranking_puts_the_closest_match_first_and_drops_non_matches() {
        let r = rank(&items(), "all", 10);
        // "all" is earlier in "Save All" than in "Close All".
        assert_eq!(r, vec![0, 2]);
        assert!(!r.contains(&1) && !r.contains(&3));
    }

    #[test]
    fn an_empty_query_keeps_the_original_order() {
        assert_eq!(rank(&items(), "", 10), vec![0, 1, 2, 3]);
    }

    #[test]
    fn ranking_respects_the_limit() {
        assert_eq!(rank(&items(), "", 2), vec![0, 1]);
        assert!(rank(&items(), "", 0).is_empty());
    }

    #[test]
    fn the_hint_is_searchable_too() {
        // "Save" has it earliest; "Ctrl+Shift+..." rows also contain "ctrl+s".
        assert_eq!(rank(&items(), "ctrl+s", 10), vec![1, 0, 2]);
    }

    #[test]
    fn rows_show_the_hint_after_a_dash_only_when_there_is_one() {
        assert_eq!(Item::new("Open File", "", 4).row_text(), "Open File");
        assert!(Item::new("Save", "Ctrl+S", 2).row_text().ends_with("Ctrl+S"));
    }

    #[test]
    fn arrows_move_one_row_and_stop_at_the_ends() {
        assert_eq!(navigate(2, 5, 3, Nav::Up), 1);
        assert_eq!(navigate(2, 5, 3, Nav::Down), 3);
        assert_eq!(navigate(0, 5, 3, Nav::Up), 0);
        assert_eq!(navigate(4, 5, 3, Nav::Down), 4);
    }

    #[test]
    fn page_keys_jump_a_page_and_clamp() {
        assert_eq!(navigate(1, 20, 5, Nav::PageDown), 6);
        assert_eq!(navigate(18, 20, 5, Nav::PageDown), 19);
        assert_eq!(navigate(2, 20, 5, Nav::PageUp), 0);
    }

    #[test]
    fn with_nothing_selected_down_goes_to_the_first_row() {
        assert_eq!(navigate(-1, 5, 3, Nav::Down), 0);
    }

    #[test]
    fn an_empty_list_selects_nothing() {
        assert_eq!(navigate(0, 0, 3, Nav::Down), -1);
    }
}
