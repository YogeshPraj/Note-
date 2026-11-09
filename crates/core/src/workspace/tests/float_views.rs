//! What floating windows show, and Dock Back for either kind of tab out of the strip.

use super::floats::{four, window};

#[test]
fn each_floating_window_says_its_title_and_whether_it_is_current() {
    let mut ws = four();
    let tab = ws.tabs[1].id;
    assert!(ws.float_tab(1, Some(window())));
    let headings = ws.float_headings();
    assert_eq!(headings.len(), 1);
    let (id, window_title, heading) = &headings[0];
    assert_eq!(*id, tab);
    assert!(window_title.contains(&heading.title), "the title bar names the tab");
    assert_eq!(heading.current, ws.current_float() == Some(tab));
}

#[test]
fn dock_back_returns_a_floating_tab_and_undocks_a_docked_one() {
    let mut ws = four();
    let tab = ws.tabs[1].id;
    assert!(ws.float_tab(1, Some(window())));
    ws.send_back(tab);
    assert!(ws.floating_tabs().is_empty());
    let docked = ws.tabs[0].id;
    ws.dock_tab(0, crate::session::DockSide::Left);
    assert_eq!(ws.docked_tab(), Some(docked));
    ws.send_back(docked);
    assert_eq!(ws.dock_side(), None);
}
