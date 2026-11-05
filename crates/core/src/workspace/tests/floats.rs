//! Floating tabs.

use super::*;
use crate::session::FloatRect;

/// Four text tabs, the last one active, with a second view for the dock.
fn four() -> Workspace<FakeSci> {
    let mut ws = Workspace::new(FakeSci::default());
    for _ in 0..4 {
        ws.new_tab();
    }
    ws.attach_second(FakeSci { next: Cell::new(800), shown: Cell::new(900), ..FakeSci::default() });
    ws
}

fn window() -> FakeSci {
    FakeSci { next: Cell::new(700), shown: Cell::new(901), ..FakeSci::default() }
}

/// What floating window `i` shows.
fn in_window(ws: &Workspace<FakeSci>, i: usize) -> isize {
    ws.floats[i].view.as_ref().map_or(0, |v| v.shown.get())
}

const RECT: FloatRect = FloatRect { x: 10, y: 20, w: 300, h: 200, maximized: false };

#[test]
fn a_floating_tab_leaves_the_strip_and_goes_to_the_end_of_the_list() {
    let mut ws = four();
    let (id, doc) = (ws.tabs[1].id, ws.tabs[1].doc);
    assert!(ws.float_tab(1, Some(window())));
    assert_eq!(ws.main_count(), 3);
    assert_eq!(ws.tabs[3].id, id, "floating tabs sit after the strip");
    assert_eq!(in_window(&ws, 0), doc);
    assert!(ws.is_floating(3) && !ws.is_floating(2));
    assert_eq!(ws.floating_tabs(), vec![id]);
    assert_eq!(ws.active, 2, "the active tab is still the same tab");
}

#[test]
fn floating_the_active_tab_shows_a_neighbour_in_the_main_view() {
    let mut ws = four();
    let doc = ws.tabs[3].doc;
    assert!(ws.float_tab(3, Some(window())));
    assert_eq!(in_window(&ws, 0), doc);
    assert_ne!(ws.sci.shown.get(), doc);
    assert_eq!(ws.current_float(), None, "the main window keeps the keyboard until the new window takes it");
}

#[test]
fn several_tabs_can_float() {
    let mut ws = four();
    assert!(ws.float_tab(0, Some(window())) && ws.float_tab(0, Some(window())));
    assert_eq!(ws.main_count(), 2);
    assert_eq!(ws.floats.len(), 2);
}

#[test]
fn the_last_text_tab_of_the_strip_stays() {
    let mut ws = four();
    assert!(ws.float_tab(0, Some(window())) && ws.float_tab(0, Some(window())) && ws.float_tab(0, Some(window())));
    assert!(!ws.can_float(0));
    assert!(!ws.float_tab(0, Some(window())));
    assert_eq!(ws.main_count(), 1);
}

#[test]
fn a_picture_floats_like_a_whiteboard_and_keeps_its_content() {
    let mut ws = four();
    let img = ws.new_image("AAAA".into());
    let id = ws.tabs[img].id;
    assert!(ws.can_float(img));
    assert!(!ws.float_tab(img, Some(window())), "a page has no editor view");
    assert!(ws.float_tab(img, None));
    assert_eq!(ws.floating_tabs(), vec![id]);
    ws.focus_float(id);
    assert!(ws.is_image());
    assert!(!ws.main_is_image(), "the main window shows its text");
    ws.tabs[ws.active].content = "BBBB".into();
    ws.dock_back(id);
    assert_eq!(ws.tabs[ws.active].id, id);
    assert_eq!(ws.tabs[ws.active].content, "BBBB");
    assert!(ws.is_image());
}

fn with_board() -> (Workspace<FakeSci>, u64) {
    let mut ws = four();
    let wb = ws.open_whiteboard("wb".into(), None, "{}".into(), false);
    let id = ws.tabs[wb].id;
    assert!(ws.can_float(wb));
    assert!(ws.float_tab(wb, None), "a whiteboard has a page, not a view");
    (ws, id)
}

#[test]
fn a_whiteboard_floats_with_no_editor_view() {
    let (ws, id) = with_board();
    assert_eq!(ws.floating_tabs(), vec![id]);
    assert_eq!(ws.main_count(), 4);
    assert_eq!(ws.side_views().count(), 1, "only the dock's view");
    assert!(!ws.main_is_page());
}

#[test]
fn a_text_tab_needs_its_view_and_a_whiteboard_must_not_have_one() {
    let mut ws = four();
    assert!(!ws.float_tab(0, None));
    let wb = ws.open_whiteboard("wb".into(), None, "{}".into(), false);
    assert!(!ws.float_tab(wb, Some(window())));
    assert!(ws.floats.is_empty());
}

#[test]
fn going_into_a_floating_whiteboard_leaves_the_main_view_alone() {
    let (mut ws, id) = with_board();
    let shown = ws.sci.shown.get();
    ws.focus_float(id);
    assert_eq!(ws.current_float(), Some(id));
    assert!(ws.is_whiteboard(), "commands see a whiteboard");
    assert!(!ws.main_is_page(), "the main window still shows its text");
    assert_eq!(ws.sci.shown.get(), shown);
    ws.focus_pane(false);
    assert_eq!(ws.current_float(), None);
    assert_eq!(ws.sci.shown.get(), shown);
    assert!(!ws.is_whiteboard());
}

#[test]
fn a_main_whiteboard_stays_on_screen_while_the_user_is_in_a_floating_window() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.new_whiteboard();
    ws.focus_float(id);
    assert!(!ws.is_page(), "the keyboard is in the text window");
    assert!(ws.main_is_whiteboard());
}

#[test]
fn a_floating_whiteboard_docks_back_last_with_its_scene() {
    let (mut ws, id) = with_board();
    ws.focus_float(id);
    ws.tabs[ws.active].content = "{\"drawn\":1}".into();
    ws.dock_back(id);
    assert!(ws.floats.is_empty());
    assert_eq!(ws.tabs[ws.active].id, id);
    assert_eq!(ws.active, 4);
    assert!(ws.is_whiteboard());
    assert_eq!(ws.tabs[4].content, "{\"drawn\":1}");
}

#[test]
fn closing_a_floating_whiteboard_leaves_the_strip_alone() {
    let (mut ws, id) = with_board();
    ws.focus_float(id);
    ws.remove_tab(ws.active, true);
    assert!(ws.floats.is_empty());
    assert_eq!(ws.tabs.len(), 4);
    assert_eq!(ws.current_float(), None);
}

#[test]
fn only_whiteboards_may_stay_in_the_strip() {
    let mut ws = Workspace::new(FakeSci::default());
    ws.new_tab();
    ws.new_whiteboard();
    assert!(ws.float_tab(0, Some(window())), "the whiteboard stays for the main window");
    assert_eq!(ws.main_count(), 1);
    assert!(ws.main_is_whiteboard());
    assert!(!ws.can_float(0), "the last tab of the strip stays");
}

#[test]
fn going_into_a_window_makes_its_tab_current_and_the_strip_keeps_its_tab() {
    let mut ws = four();
    let (id, doc, home) = (ws.tabs[0].id, ws.tabs[0].doc, ws.tabs[3].id);
    ws.float_tab(0, Some(window()));
    let main_doc = ws.sci.shown.get();
    ws.focus_float(id);
    assert_eq!(ws.current_float(), Some(id));
    assert_eq!(ws.tabs[ws.active].id, id);
    assert_eq!(ws.sci.shown.get(), doc, "commands act on the floating view");
    assert_eq!(ws.tabs[ws.main_index()].id, home, "the strip still marks the main window's tab");
    assert_eq!(in_window(&ws, 0), main_doc, "the main view waits in the slot");
    ws.focus_pane(false);
    assert_eq!(ws.current_float(), None);
    assert_eq!(ws.tabs[ws.active].id, home);
    assert_eq!(ws.sci.shown.get(), main_doc);
    assert_eq!(in_window(&ws, 0), doc);
}

#[test]
fn asking_for_a_floating_tab_brings_its_window_forward() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.activate(3);
    assert_eq!(ws.current_float(), Some(id));
    assert_eq!(ws.take_raise(), Some(id));
    assert_eq!(ws.take_raise(), None);
}

#[test]
fn choosing_a_strip_tab_from_a_floating_window_goes_back_to_the_main_window() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.focus_float(id);
    ws.activate(1);
    assert_eq!(ws.current_float(), None);
    assert_eq!(ws.active, 1);
}

#[test]
fn dock_back_puts_the_tab_last_in_the_strip_and_active() {
    let mut ws = four();
    let (id, doc) = (ws.tabs[0].id, ws.tabs[0].doc);
    ws.float_tab(0, Some(window()));
    ws.focus_float(id);
    ws.dock_back(id);
    assert_eq!(ws.main_count(), 4);
    assert!(ws.floats.is_empty());
    assert_eq!(ws.tabs[3].id, id);
    assert_eq!(ws.active, 3);
    assert_eq!(ws.sci.shown.get(), doc);
    assert_eq!(ws.current_float(), None);
}

#[test]
fn dock_back_goes_before_the_tabs_still_floating() {
    let mut ws = four();
    let (a, b) = (ws.tabs[0].id, ws.tabs[1].id);
    ws.float_tab(0, Some(window()));
    ws.float_tab(0, Some(window()));
    ws.dock_back(a);
    assert_eq!(ws.main_count(), 3);
    assert_eq!(ws.tabs[2].id, a);
    assert_eq!(ws.tabs[3].id, b);
    assert!(ws.is_floating(3));
}

#[test]
fn closing_a_floating_tab_ends_the_window_and_leaves_the_strip_alone() {
    let mut ws = four();
    let (id, home) = (ws.tabs[0].id, ws.tabs[3].id);
    ws.float_tab(0, Some(window()));
    ws.focus_float(id);
    ws.remove_tab(ws.active, true);
    assert!(ws.floats.is_empty());
    assert_eq!(ws.tabs.len(), 3);
    assert_eq!(ws.tabs[ws.active].id, home);
    assert_eq!(ws.current_float(), None);
}

#[test]
fn a_new_tab_opens_in_the_strip_before_the_floating_ones() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.focus_float(id);
    let i = ws.new_tab();
    assert_eq!(ws.current_float(), None);
    assert_eq!((i, ws.active), (3, 3));
    assert_eq!(ws.tabs[4].id, id);
    assert!(ws.is_floating(4) && !ws.is_floating(3));
}

#[test]
fn closing_the_last_tab_of_the_strip_makes_a_new_one_and_keeps_the_window() {
    let mut ws = four();
    ws.float_tab(0, Some(window()));
    ws.float_tab(0, Some(window()));
    ws.float_tab(0, Some(window()));
    let floating = ws.tabs[1].id;
    ws.remove_tab(0, false);
    assert_eq!(ws.main_count(), 1);
    assert_eq!(ws.active, 0);
    assert_eq!(ws.tabs[1].id, floating);
}

#[test]
fn next_tab_stays_in_the_strip_and_does_nothing_from_a_floating_window() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.activate(0);
    ws.next_tab(-1);
    assert_eq!(ws.active, 2, "wraps inside the strip");
    assert_eq!(ws.current_float(), None);
    ws.focus_float(id);
    ws.next_tab(1);
    assert_eq!(ws.current_float(), Some(id));
}

#[test]
fn sorting_and_moving_leave_the_floating_tabs_at_the_end() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.sort_tabs(SortField::Name, true);
    assert_eq!(ws.tabs[3].id, id);
    ws.move_tab_to(0, 3);
    assert_eq!(ws.tabs[3].id, id, "the strip ends at 2");
}

#[test]
fn close_all_is_about_the_strip() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    assert!(ws.close_many(&NoUi, CloseOp::All));
    assert_eq!(ws.floating_tabs(), vec![id]);
    assert_eq!(ws.main_count(), 1, "a fresh tab");
}

#[test]
fn the_unsaved_mark_follows_the_floating_view() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    if let Some(v) = &ws.floats[0].view {
        v.modified.set(true);
    }
    assert!(ws.is_dirty(3));
    ws.focus_float(id);
    assert!(ws.is_dirty(ws.active));
    assert!(ws.title_of(ws.active).starts_with("* "));
    assert!(!ws.window_title().starts_with("* "), "the main window's title is about its own tab");
}

#[test]
fn a_tab_docked_then_floated_ends_the_dock() {
    let mut ws = four();
    ws.dock_tab(1, DockSide::Right);
    assert!(ws.float_tab(1, Some(window())));
    assert_eq!(ws.dock_side(), None);
    assert_eq!(ws.floats.len(), 1);
}

#[test]
fn a_dock_and_a_floating_window_work_together() {
    let mut ws = four();
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    assert!(ws.dock_tab(0, DockSide::Right));
    let docked = ws.tabs[0].doc;
    ws.focus_float(id);
    assert_eq!(ws.docked_index(), Some(0));
    assert_eq!(ws.second.as_ref().map(|s| s.view.shown.get()), Some(docked));
    ws.focus_pane(true);
    assert_eq!(ws.current_float(), None);
    assert!(ws.in_dock());
}

#[test]
fn floating_windows_come_back_from_the_session() {
    let mut ws = four();
    ws.tabs[0].path = Some(PathBuf::from("a.txt"));
    ws.tabs[3].path = Some(PathBuf::from("d.txt"));
    let id = ws.tabs[0].id;
    ws.float_tab(0, Some(window()));
    ws.set_float_rect(id, RECT);
    let saved = ws.snapshot();
    assert_eq!(saved.floats.len(), 1);
    let f = saved.floats[0];
    assert_eq!(saved.tabs[f.tab].path.as_deref(), Some("a.txt"));
    assert_eq!(f.rect, RECT);
    ws.saved_floats.clone_from(&saved.floats);
    let back = ws.take_saved_floats();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].1, RECT);
    assert!(ws.take_saved_floats().is_empty());
}

#[test]
fn every_view_takes_new_settings() {
    let mut ws = four();
    ws.float_tab(0, Some(window()));
    ws.float_tab(0, Some(window()));
    assert_eq!(ws.side_views().count(), 3, "the dock's view and both windows'");
    assert!(ws.floats.iter().all(|f| f.view.is_some()));
}
