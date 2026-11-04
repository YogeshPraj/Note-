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

mod dock;
mod image;
