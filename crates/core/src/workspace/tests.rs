use super::*;
use std::cell::Cell;

/// A stand-in for Scintilla that tracks which document the view shows.
#[derive(Default)]
struct FakeSci {
    shown: Cell<isize>,
    next: Cell<isize>,
    selected_zero: Cell<bool>,
    swaps: Cell<usize>,
    modified: Cell<bool>,
}

impl sci::Send for FakeSci {
    fn send(&self, msg: u32, _w: usize, l: isize) -> isize {
        match msg {
            sci::SCI_CREATEDOCUMENT => {
                self.next.set(self.next.get() + 1);
                self.next.get()
            }
            sci::SCI_SETDOCPOINTER => {
                // Selecting document 0 makes the real Scintilla create a stray document.
                if l == 0 {
                    self.selected_zero.set(true);
                }
                self.swaps.set(self.swaps.get() + 1);
                self.shown.set(l);
                0
            }
            sci::SCI_GETDOCPOINTER => self.shown.get(),
            sci::SCI_GETMODIFY => isize::from(self.modified.get()),
            _ => 0,
        }
    }
}

#[test]
fn measuring_a_text_tab_while_a_whiteboard_is_active_leaves_the_view_alone() {
    // The session autosave measures every tab; with a whiteboard in front it used to put the
    // view back on the whiteboard's "document" (0), which crashed Scintilla when the tab closed.
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws.new_whiteboard();
    let before = ws.sci.shown.get();
    assert_ne!(before, 0);
    ws.session_fingerprint();
    assert!(!ws.sci.selected_zero.get(), "the view was pointed at document 0");
    assert_eq!(ws.sci.shown.get(), before, "the view must keep showing the same document");
}

/// A user who declines every question and every password prompt.
struct NoUi;

impl Ui for NoUi {
    fn pick_open(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn pick_save(&self, _: &str) -> Option<PathBuf> {
        None
    }
    fn confirm_save(&self, _: &str) -> Confirm {
        Confirm::Cancel
    }
    fn ask_yes_no(&self, _: &str) -> bool {
        false
    }
    fn recycle(&self, _: &Path) -> Result<(), crate::error::Error> {
        Ok(())
    }
    fn error(&self, _: &str) {}
    fn unlock(&self, _: &mut crypto::Vault, _: &str) -> bool {
        false
    }
    fn setup_encryption(&self, _: &mut crypto::Vault) -> bool {
        false
    }
}

#[test]
fn tabs_not_opened_this_time_stay_in_the_session() {
    // An encrypted tab whose password prompt is declined, and a tab of a kind this app cannot show, must be written
    // back by the next autosave: dropping them destroyed their unsaved text.
    let env = r#"{"_notepp_encrypted":true,"version":1,"alg":"AES-256-GCM","compression":"gzip","profile":"zz-test-profile","iv":"AAAA","ct":"AAAA"}"#;
    let tab = |title: &str, text: &str, encrypted: bool| SessionTab {
        title: title.into(),
        path: None,
        text: Some(text.into()),
        caret: 0,
        top: 0,
        enc: "utf8".into(),
        eol: "lf".into(),
        pinned: false,
        color: None,
        encrypted,
        kind: "text".into(),
        lang: None,
    };
    let locked = tab("secret.txt", env, true);
    let kept = vec![serde_json::json!({ "type": "drawio", "name": "flow.drawio", "content": "<mxfile>unsaved</mxfile>" })];
    let s = Session { tabs: vec![tab("new 1", "hello", false), locked.clone()], kept: kept.clone(), ..Session::default() };
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.restore(&NoUi, &s);
    assert_eq!(ws.tabs.len(), 1, "the locked tab is not opened");
    let snap = ws.snapshot();
    assert!(snap.tabs.contains(&locked), "the locked tab's entry, encrypted text and all, is written back");
    assert_eq!(snap.kept, kept);
}

#[test]
fn with_a_probe_the_editor_never_swaps_documents() {
    // Swapping a document into the editor costs it its multiple selections and horizontal scroll, so the session
    // autosave and the tab tool tips read hidden tabs through the probe view instead.
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws.new_tab();
    ws.attach_probe(FakeSci { shown: Cell::new(7), ..FakeSci::default() });
    let (shown, swaps) = (ws.sci.shown.get(), ws.sci.swaps.get());
    ws.session_fingerprint();
    ws.snapshot();
    ws.tab_preview(0, 3, 10);
    assert_eq!(ws.sci.swaps.get(), swaps, "the editor's document was swapped");
    assert_eq!(ws.sci.shown.get(), shown);
    let probe = ws.probe.as_ref().map(|(p, idle)| (p.shown.get(), *idle));
    assert_eq!(probe, Some((7, 7)), "the probe must rest on its own document again");
}

#[test]
fn moving_a_tab_keeps_it_in_its_group() {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    for _ in 0..3 {
        ws.new_tab();
    }
    ws.toggle_pin(0);
    ws.activate(1);
    assert!(!ws.move_tab(-1), "an unpinned tab moved into the pinned group");
    assert!(ws.move_tab(1));
    ws.activate(0);
    assert!(!ws.move_tab(1), "a pinned tab moved among the unpinned ones");
    assert!(ws.tabs[0].pinned && !ws.tabs[1].pinned && !ws.tabs[2].pinned);
}

#[test]
fn thousands_separators() {
    assert_eq!((commafy(0), commafy(999), commafy(1000), commafy(22_187_160)), ("0".into(), "999".into(), "1,000".into(), "22,187,160".into()));
}

#[test]
fn sort_order_by_each_field() {
    let key = |name: &str, path: &str, kind: &str, len, modified| SortKeys { name: name.into(), path: path.into(), kind: kind.into(), len, modified };
    let keys = vec![
        key("b.txt", "C:\\z\\b.txt", "Rust", 30, 5),
        key("A.txt", "C:\\y\\A.txt", "Normal Text", 10, 9),
        key("c.txt", "", "Normal Text", 20, 0),
        key("d.txt", "C:\\a\\d.txt", "Normal Text", 20, 7),
    ];
    assert_eq!(sort_order(&keys, SortField::Name, false), vec![1, 0, 2, 3]);
    assert_eq!(sort_order(&keys, SortField::Name, true), vec![3, 2, 0, 1]);
    assert_eq!(sort_order(&keys, SortField::Path, false), vec![2, 3, 1, 0]);
    assert_eq!(sort_order(&keys, SortField::Type, false), vec![2, 3, 1, 0]);
    // Equal lengths fall back to the path.
    assert_eq!(sort_order(&keys, SortField::Length, false), vec![1, 2, 3, 0]);
    assert_eq!(sort_order(&keys, SortField::Modified, true), vec![1, 3, 0, 2]);
}

#[test]
fn close_targets_cover_every_menu_option() {
    let dirty = [false, true, false, false, true];
    assert_eq!(close_targets(CloseOp::All, 2, &dirty), vec![0, 1, 2, 3, 4]);
    assert_eq!(close_targets(CloseOp::AllButActive, 2, &dirty), vec![0, 1, 3, 4]);
    assert_eq!(close_targets(CloseOp::ToTheLeft, 2, &dirty), vec![0, 1]);
    assert_eq!(close_targets(CloseOp::ToTheRight, 2, &dirty), vec![3, 4]);
    assert_eq!(close_targets(CloseOp::Unchanged, 2, &dirty), vec![0, 2, 3]);
}

#[test]
fn close_targets_edges() {
    assert!(close_targets(CloseOp::ToTheLeft, 0, &[false, false]).is_empty());
    assert!(close_targets(CloseOp::ToTheRight, 1, &[false, false]).is_empty());
    assert!(close_targets(CloseOp::AllButActive, 0, &[false]).is_empty());
    assert!(close_targets(CloseOp::All, 0, &[]).is_empty());
}

#[test]
fn recent_list_is_deduped_newest_first_and_capped() {
    let mut l = Vec::new();
    for n in 0..12 {
        push_recent(&mut l, Path::new(&format!("C:\\f{n}.txt")), 10);
    }
    assert_eq!(l.len(), 10);
    assert_eq!(l[0], PathBuf::from("C:\\f11.txt"));
    push_recent(&mut l, Path::new("C:\\f5.txt"), 10);
    assert_eq!(l[0], PathBuf::from("C:\\f5.txt"));
    assert_eq!(l.iter().filter(|p| p.as_path() == Path::new("C:\\f5.txt")).count(), 1);
    assert_eq!(l.len(), 10);
}

#[test]
fn margins_follow_the_display_scale() {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    let at_100 = ws.margin_widths();
    ws.set_ui_percent(150);
    let at_150 = ws.margin_widths();
    assert_eq!(at_100.map(|w| w * 3 / 2), at_150);
    ws.set_ui_percent(50);
    assert_eq!(ws.margin_widths(), at_100, "a scale under 100% is not a display scale");
}

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
fn a_whiteboard_or_the_only_text_tab_cannot_be_docked() {
    let mut ws = Workspace::new(FakeSci { next: Cell::new(100), ..FakeSci::default() });
    ws.new_tab();
    ws.attach_second(FakeSci { next: Cell::new(800), ..FakeSci::default() });
    assert!(!ws.can_dock(0), "nothing would be left for the main view");
    let wb = ws.new_whiteboard();
    assert!(!ws.can_dock(wb));
    assert!(!ws.can_dock(0), "a whiteboard cannot stand in for the main view");
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
