//! The docked tab.

use super::*;

/// Three text tabs, the last one active, and a second view resting on document 900.
fn docking_workspace() -> Workspace<FakeSci> {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws.new_tab();
    ws.new_tab();
    ws.attach_second(FakeSci { next: Cell::new(800), shown: Cell::new(900), ..FakeSci::default() });
    ws
}

fn second_shows(ws: &Workspace<FakeSci>) -> isize {
    ws.second.as_ref().map_or(0, |s| s.view.shown.get())
}

#[test]
fn docking_the_active_tab_puts_it_in_the_second_view_and_a_neighbour_in_the_main_one() {
    let mut ws = docking_workspace();
    let doc = ws.tabs[2].doc;
    assert!(ws.dock_tab(2, DockSide::Right));
    assert_eq!(second_shows(&ws), doc);
    assert_eq!(ws.docked_index(), Some(2));
    assert_eq!(ws.active, 1);
    assert_eq!(ws.sci.shown.get(), ws.tabs[1].doc);
    assert_eq!(ws.dock_side(), Some(DockSide::Right));
}

#[test]
fn choosing_the_docked_tab_goes_to_its_view_and_any_other_tab_opens_in_the_main_one() {
    let mut ws = docking_workspace();
    let docked = ws.tabs[2].doc;
    ws.dock_tab(2, DockSide::Left);
    ws.activate(2);
    assert!(ws.in_dock());
    assert_eq!(ws.active, 2);
    assert_eq!(ws.sci.shown.get(), docked, "the current view is the docked one");
    ws.activate(0);
    assert!(!ws.in_dock());
    assert_eq!(ws.sci.shown.get(), ws.tabs[0].doc);
    assert_eq!(second_shows(&ws), docked, "the docked view keeps its tab");
}

#[test]
fn a_new_tab_made_from_the_docked_view_opens_in_the_main_view() {
    let mut ws = docking_workspace();
    let docked = ws.tabs[2].doc;
    ws.dock_tab(2, DockSide::Left);
    ws.focus_pane(true);
    ws.new_tab();
    assert!(!ws.in_dock());
    assert_eq!(second_shows(&ws), docked);
    assert_eq!(ws.docked_index(), Some(2));
}

#[test]
fn focus_moves_between_the_views_and_back() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Top);
    let main = ws.active;
    ws.focus_other_pane();
    assert!(ws.in_dock());
    ws.focus_other_pane();
    assert!(!ws.in_dock());
    assert_eq!(ws.active, main);
}

#[test]
fn closing_the_docked_tab_ends_the_dock_and_rests_the_view() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    ws.remove_tab(2, false);
    assert_eq!(ws.dock_side(), None);
    assert_eq!(second_shows(&ws), 900, "the view must not hold the closed document");
    assert_eq!(ws.tabs.len(), 2);
}

#[test]
fn closing_a_main_tab_never_shows_the_docked_tab_in_the_main_view() {
    let mut ws = docking_workspace();
    ws.dock_tab(1, DockSide::Right);
    ws.activate(0);
    ws.remove_tab(0, false);
    assert_eq!(ws.dock_side(), Some(DockSide::Right));
    assert_ne!(ws.docked_index(), Some(ws.active));
    assert_ne!(ws.sci.shown.get(), second_shows(&ws));
}

#[test]
fn closing_the_only_tab_beside_the_dock_ends_the_dock() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    ws.remove_tab(0, false);
    ws.remove_tab(0, false);
    assert_eq!(ws.dock_side(), None);
    assert_eq!(ws.tabs.len(), 1);
    assert_eq!(ws.sci.shown.get(), ws.tabs[0].doc);
}

#[test]
fn docking_another_tab_replaces_the_docked_one() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    let doc = ws.tabs[0].doc;
    assert!(ws.dock_tab(0, DockSide::Bottom));
    assert_eq!(second_shows(&ws), doc);
    assert_eq!(ws.docked_index(), Some(0));
    assert_eq!(ws.dock_side(), Some(DockSide::Bottom));
}

#[test]
fn docking_the_docked_tab_again_only_moves_it() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    let main = ws.sci.shown.get();
    assert!(ws.dock_tab(2, DockSide::Left));
    assert_eq!(ws.dock_side(), Some(DockSide::Left));
    assert_eq!(ws.sci.shown.get(), main);
}

#[test]
fn a_docked_whiteboard_needs_a_text_tab_for_the_main_view() {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws.attach_second(FakeSci { next: Cell::new(800), ..FakeSci::default() });
    assert!(!ws.can_dock(0), "nothing would be left for the main view");
    let wb = ws.new_whiteboard();
    assert!(ws.can_dock(wb), "the text tab stays in the main view");
    assert!(!ws.can_dock(0), "a whiteboard cannot stand in for the main view");
    let img = ws.new_image("AAAA".into());
    assert!(ws.can_dock(img), "a picture docks like a whiteboard");
}

#[test]
fn a_docked_picture_leaves_the_second_view_on_its_empty_document() {
    let mut ws = docking_workspace();
    let img = ws.new_image("AAAA".into());
    let id = ws.tabs[img].id;
    assert!(ws.dock_tab(img, DockSide::Right));
    assert_eq!(second_shows(&ws), 900);
    assert_eq!(ws.dock_kind(), Some(TabKind::Image));
    assert!(ws.dock_is_page());
    let main = ws.sci.shown.get();
    ws.focus_pane(true);
    assert!(ws.is_image());
    assert_eq!(ws.docked_tab(), Some(id));
    assert_eq!(ws.sci.shown.get(), main, "the main editor keeps its document");
    assert!(!ws.main_is_page());
}

fn board_beside_text() -> (Workspace<FakeSci>, usize) {
    let mut ws = docking_workspace();
    let wb = ws.new_whiteboard();
    (ws, wb)
}

#[test]
fn a_docked_whiteboard_leaves_the_second_view_on_its_empty_document() {
    let (mut ws, wb) = board_beside_text();
    assert!(ws.dock_tab(wb, DockSide::Right));
    assert_eq!(second_shows(&ws), 900);
    assert!(!ws.sci.selected_zero.get() && ws.second.as_ref().is_some_and(|s| !s.view.selected_zero.get()), "a whiteboard has no document");
    assert_eq!(ws.docked_index(), Some(wb));
    assert!(ws.dock_is_page());
    assert_ne!(ws.active, wb, "a text tab comes forward in the main view");
    assert!(!ws.is_page());
}

#[test]
fn going_to_a_docked_whiteboard_swaps_no_views() {
    let (mut ws, wb) = board_beside_text();
    ws.dock_tab(wb, DockSide::Bottom);
    let main = ws.sci.shown.get();
    let home = ws.active;
    ws.focus_pane(true);
    assert!(ws.in_dock() && ws.is_whiteboard());
    assert_eq!(ws.active, wb);
    assert_eq!(ws.sci.shown.get(), main, "the main editor keeps its document");
    assert_eq!(second_shows(&ws), 900);
    assert_eq!(ws.main_index(), home);
    assert!(!ws.main_is_page(), "the main window shows its text beside the board");
    ws.focus_pane(false);
    assert_eq!(ws.active, home);
    assert_eq!(ws.sci.shown.get(), main);
}

#[test]
fn undocking_a_whiteboard_keeps_its_scene_and_the_main_view() {
    let (mut ws, wb) = board_beside_text();
    ws.dock_tab(wb, DockSide::Left);
    ws.focus_pane(true);
    ws.tabs[ws.active].content = "{\"drawn\":1}".into();
    let main = ws.sci.shown.get();
    ws.undock();
    assert!(ws.dock_side().is_none() && !ws.in_dock());
    assert!(!ws.is_page());
    assert_eq!(ws.sci.shown.get(), main);
    assert_eq!(ws.tabs[wb].content, "{\"drawn\":1}");
}

#[test]
fn a_docked_whiteboard_comes_back_from_the_session() {
    let (mut ws, wb) = board_beside_text();
    ws.tabs[0].path = Some(PathBuf::from("a.txt"));
    ws.tabs[wb].path = Some(PathBuf::from("b.excalidraw"));
    ws.dock_tab(wb, DockSide::Top);
    let saved = ws.snapshot();
    let dock = saved.dock.unwrap();
    assert_eq!(saved.tabs[dock.tab].path.as_deref(), Some("b.excalidraw"));
    assert_eq!(dock.side, DockSide::Top);
}

#[test]
fn closing_a_docked_whiteboard_rests_the_dock() {
    let (mut ws, wb) = board_beside_text();
    ws.dock_tab(wb, DockSide::Right);
    ws.focus_pane(true);
    ws.remove_tab(ws.active, true);
    assert!(ws.dock_side().is_none());
    assert!(!ws.is_page());
    assert!(!ws.sci.selected_zero.get());
}

#[test]
fn a_docked_whiteboard_can_float() {
    let (mut ws, wb) = board_beside_text();
    let id = ws.tabs[wb].id;
    ws.dock_tab(wb, DockSide::Right);
    assert!(ws.float_tab(wb, None));
    assert!(ws.dock_side().is_none());
    assert_eq!(ws.floating_tabs(), vec![id]);
}

#[test]
fn the_docked_view_comes_forward_from_a_whiteboard_with_text_beside_it() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    let wb = ws.new_whiteboard();
    assert_eq!(ws.active, wb);
    ws.activate(2);
    assert!(ws.in_dock());
    assert!(!ws.is_whiteboard());
    let main = ws.other_index().map(|i| ws.tabs[i].doc);
    assert_eq!(main, ws.second.as_ref().map(|s| s.view.shown.get()), "the main view shows the tab it is said to");
    assert!(!ws.sci.selected_zero.get());
}

#[test]
fn a_change_in_the_view_that_is_not_current_still_counts_as_unsaved() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    assert!(!ws.is_dirty(2));
    if let Some(s) = &ws.second {
        s.view.modified.set(true);
    }
    assert!(ws.is_dirty(2), "the unsaved dot must follow the docked view");
}

#[test]
fn the_session_remembers_the_dock_and_gives_it_back_once() {
    let mut ws = docking_workspace();
    ws.tabs[2].path = Some(PathBuf::from("docked.txt"));
    ws.tabs[0].path = Some(PathBuf::from("main.txt"));
    ws.dock_tab(2, DockSide::Bottom);
    ws.set_dock_share(300);
    let saved = ws.snapshot();
    let dock = saved.dock.expect("a dock in the session");
    assert_eq!((dock.side, dock.share), (DockSide::Bottom, 300));
    assert_eq!(saved.tabs[dock.tab].path.as_deref(), Some("docked.txt"));
    ws.saved_dock = saved.dock;
    assert_eq!(ws.take_saved_dock(), Some(dock));
    assert_eq!(ws.take_saved_dock(), None);
}

#[test]
fn undocking_keeps_the_tab_and_returns_to_the_main_view() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    ws.focus_pane(true);
    ws.undock();
    assert_eq!(ws.dock_side(), None);
    assert!(!ws.in_dock());
    assert_eq!(ws.tabs.len(), 3);
    assert_eq!(second_shows(&ws), 900);
}

#[test]
fn the_dock_share_stays_usable() {
    let mut ws = docking_workspace();
    ws.set_dock_share(5);
    assert_eq!(ws.dock_share(), 100);
    ws.set_dock_share(5000);
    assert_eq!(ws.dock_share(), 900);
}

#[test]
fn each_view_is_told_apart_for_the_headers_and_the_tab_strip() {
    let mut ws = docking_workspace();
    ws.dock_tab(2, DockSide::Right);
    assert_eq!(ws.main_index(), 1);
    assert!(ws.tab_infos()[2].beside, "the docked tab is on screen but not current");
    assert!(!ws.tab_infos()[1].beside);
    ws.focus_pane(true);
    assert_eq!(ws.main_index(), 1, "the main view keeps its tab when the keyboard leaves it");
    assert!(ws.tab_infos()[1].beside);
    assert!(!ws.tab_infos()[2].beside);
    assert!(!ws.tab_infos()[0].beside);
}

#[test]
fn the_dock_says_how_it_splits_and_keeps_where_the_divider_was_left() {
    let mut ws = docking_workspace();
    assert_eq!(ws.dock_layout(1000), None, "nothing docked");
    assert!(ws.dock_headings().is_none());
    ws.dock_tab(1, DockSide::Right);
    ws.set_dock_share(300);
    let split = ws.dock_layout(1000).unwrap();
    assert_eq!((split.side_by_side, split.dock_first, split.divider), (true, false, 700));
    ws.keep_dock_divider(600, 1000);
    assert_eq!(ws.dock_share(), 400);
    ws.keep_dock_divider(600, 0);
    assert_eq!(ws.dock_share(), 400, "a space with no size says nothing");
    ws.keep_dock_divider(990, 1000);
    assert_eq!(ws.dock_share(), 100, "never thinner than the smallest share");
}

#[test]
fn the_pane_headers_name_each_pane_and_where_the_user_is() {
    let mut ws = docking_workspace();
    ws.dock_tab(1, DockSide::Left);
    let (main, dock) = ws.dock_headings().unwrap();
    let docked = ws.docked_index().unwrap();
    assert_eq!((main.title.as_str(), dock.title.as_str()), (ws.tabs[ws.main_index()].title.as_str(), ws.tabs[docked].title.as_str()));
    assert_ne!(main.title, dock.title);
    assert_ne!(main.current, dock.current, "the user is in one pane");
    ws.focus_other_pane();
    let (main2, dock2) = ws.dock_headings().unwrap();
    assert_eq!((main2.current, dock2.current), (!main.current, !dock.current));
}
