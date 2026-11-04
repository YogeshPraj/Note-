//! Tool tips of the tabs: a preview of another tab, or the active tab's name when it is cut short.

use super::*;

/// The tab to show a tool tip for. The active tab's text is on screen, so it gets one only when its name is cut short;
/// there is none over a close button or in a vertical strip.
fn tip_target(hot: Option<(usize, bool)>, active: usize, vertical: bool, cut: bool) -> Option<usize> {
    hot.filter(|(i, on_close)| !vertical && !on_close && (*i != active || cut)).map(|(i, _)| i)
}

/// The tool tip for the tab under the pointer: a preview of its text, or the active tab's full name. `None` leaves the
/// tool tip as it is (already made for this tab).
pub(super) fn tab_tip(st: &Rc<RefCell<State>>) -> Option<String> {
    let (hot, active, vertical, preview, done, cut, title) = {
        let s = st.borrow();
        let cut = s.cut.borrow().get(&s.active).copied().unwrap_or(false);
        (s.hot, s.active, s.vertical, s.preview.clone(), s.tip_for, cut, s.tabs.get(s.active).map(|t| t.title.clone()))
    };
    let wanted = tip_target(hot, active, vertical, cut);
    if wanted == done {
        return None;
    }
    st.borrow_mut().tip_for = wanted;
    Some(match (wanted, preview) {
        (Some(i), _) if i == active => title.unwrap_or_default(),
        (Some(i), Some(f)) => f(i),
        _ => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::tip_target;

    #[test]
    fn the_active_tab_has_a_tip_only_when_its_name_is_cut_short() {
        assert_eq!(tip_target(Some((2, false)), 2, false, false), None);
        assert_eq!(tip_target(Some((2, false)), 2, false, true), Some(2));
    }

    #[test]
    fn other_tabs_always_have_one_except_over_the_close_mark() {
        assert_eq!(tip_target(Some((1, false)), 2, false, false), Some(1));
        assert_eq!(tip_target(Some((1, true)), 2, false, false), None);
        assert_eq!(tip_target(None, 2, false, true), None);
    }

    #[test]
    fn a_vertical_strip_has_none() {
        assert_eq!(tip_target(Some((1, false)), 2, true, true), None);
    }
}
