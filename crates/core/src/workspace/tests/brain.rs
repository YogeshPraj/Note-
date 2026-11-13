//! Adding to the day's note while it is open in a tab: the text is the tab's, nothing the user wrote is lost, and a note
//! that cannot be added to is left as it was.

use super::NoUi;
use super::*;
use crate::brain;
use crate::brain::store::Store;
use crate::edit::DateTime;
use crate::error::Error;
use std::cell::RefCell;

/// A Scintilla stand-in that holds no text (reading it gives an empty document): it shows which edits reach the editor,
/// as one undo step. The text the edit makes is `brain::tail_edit`'s, tested on its own.
#[derive(Default)]
struct EditSpy {
    next: Cell<isize>,
    doc: Cell<isize>,
    read_only: Cell<bool>,
    calls: RefCell<Vec<u32>>,
}

impl sci::Send for EditSpy {
    fn send(&self, msg: u32, _w: usize, l: isize) -> isize {
        self.calls.borrow_mut().push(msg);
        match msg {
            sci::SCI_CREATEDOCUMENT => {
                self.next.set(self.next.get() + 1);
                self.next.get()
            }
            sci::SCI_SETDOCPOINTER => {
                self.doc.set(l);
                0
            }
            sci::SCI_GETDOCPOINTER => self.doc.get(),
            sci::SCI_GETREADONLY => isize::from(self.read_only.get()),
            _ => 0,
        }
    }
}

fn count(ws: &Workspace<EditSpy>, msg: u32) -> usize {
    ws.sci.calls.borrow().iter().filter(|m| **m == msg).count()
}
fn at(hour: u32) -> DateTime {
    DateTime { year: 2026, month: 11, day: 12, weekday: 4, hour, minute: 0, second: 0 }
}

fn store(name: &str) -> Store {
    let root = std::env::temp_dir().join(format!("notepp_wsbrain_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    Store::new(root)
}

/// A workspace with a first tab and a second one that is the day's note; the first tab is in front.
fn with_open_note(store: &Store) -> (Workspace<EditSpy>, PathBuf) {
    let mut ws = Workspace::new(EditSpy { next: Cell::new(100), ..EditSpy::default() });
    ws.new_tab();
    let path = brain::note_path(&store.root);
    let i = ws.new_tab();
    ws.tabs[i].path = Some(path.clone());
    ws.activate(0);
    (ws, path)
}

#[test]
fn a_note_that_is_not_open_is_written() {
    let store = store("closed");
    let mut ws = Workspace::new(EditSpy::default());
    ws.new_tab();
    let (path, _) = ws.brain_add_in(&NoUi, &store, &at(9), "one", None).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), "# 2026-11-12 Thursday\n\n## 09:00\none\n");
    assert_eq!(count(&ws, sci::SCI_REPLACETARGET), 0, "no tab was touched");
    let _ = std::fs::remove_dir_all(&store.root);
}

#[test]
fn an_open_note_is_changed_in_the_tab_as_one_undo_step_not_on_disk_first() {
    let store = store("dirty");
    let (mut ws, path) = with_open_note(&store);
    ws.tabs[1].dirty = true;
    ws.brain_add_in(&NoUi, &store, &at(10), "two", None).unwrap();
    assert_eq!(count(&ws, sci::SCI_REPLACETARGET), 1);
    assert_eq!((count(&ws, sci::SCI_BEGINUNDOACTION), count(&ws, sci::SCI_ENDUNDOACTION)), (1, 1));
    assert!(ws.tabs[1].dirty, "still unsaved");
    assert!(!path.exists(), "nothing was written behind the user's back");
}

#[test]
fn a_clean_open_note_is_saved_and_the_tab_in_front_stays_in_front() {
    let store = store("clean");
    let (mut ws, path) = with_open_note(&store);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let front = ws.tabs[ws.active].id;
    ws.brain_add_in(&NoUi, &store, &at(10), "two", None).unwrap();
    assert!(path.exists(), "saved straight away");
    assert!(!ws.tabs[1].dirty && !ws.tabs[1].forced_dirty);
    assert_eq!(ws.tabs[ws.active].id, front);
    let _ = std::fs::remove_dir_all(&store.root);
}

#[test]
fn a_read_only_note_refuses_and_is_not_touched() {
    let store = store("readonly");
    let (mut ws, path) = with_open_note(&store);
    ws.sci.read_only.set(true);
    assert!(ws.brain_add_in(&NoUi, &store, &at(10), "two", None).is_err());
    assert_eq!(count(&ws, sci::SCI_REPLACETARGET), 0);
    assert!(!ws.tabs[1].dirty && !path.exists());
}

#[test]
fn an_encrypted_brain_that_is_locked_is_not_written() {
    let store = store("locked");
    let mut ws = Workspace::new(EditSpy::default());
    ws.new_tab();
    ws.settings.brain_encrypt = true;
    assert!(matches!(ws.brain_add_in(&NoUi, &store, &at(9), "secret", None), Err(Error::Locked)));
    assert!(!store.root.exists() || store.list().is_empty(), "nothing in the clear");
    let _ = std::fs::remove_dir_all(&store.root);
}

#[test]
fn encrypting_the_brain_rewrites_the_notes_and_turning_it_off_reads_them_back() {
    let store = store("convert");
    let mut ws = Workspace::new(EditSpy::default());
    ws.new_tab();
    store.add(&at(9), "my secret", None, None).unwrap();
    assert!(ws.brain_set_encrypted_in(&NoUi, &store, true).is_err(), "no profile, no encryption");
    assert!(!ws.settings.brain_encrypt);
    let (profile, dek, _) = crypto::create_profile("a test password").unwrap();
    ws.vault.profile = Some(profile);
    ws.vault.dek = Some(dek);
    assert_eq!(ws.brain_set_encrypted_in(&NoUi, &store, true).unwrap(), 1);
    assert!(ws.settings.brain_encrypt);
    let path = brain::note_path(&store.root);
    assert!(!std::fs::read_to_string(&path).is_ok_and(|t| t.contains("secret")), "nothing is left in the clear");
    assert!(brain::store::Index::default().notes(&store, ws.vault.dek.as_ref()).unwrap()[0].text.contains("my secret"), "unlocked, it reads");
    ws.brain_set_encrypted_in(&NoUi, &store, false).unwrap();
    assert!(!ws.settings.brain_encrypt);
    assert!(std::fs::read_to_string(&path).unwrap().contains("my secret"));
    let _ = std::fs::remove_dir_all(&store.root);
}

#[test]
fn agents_may_search_the_brain_at_read_level_but_adding_needs_edit_and_a_clock() {
    use crate::mcp::access::Level;
    let mut ws = Workspace::new(EditSpy::default());
    ws.new_tab();
    let add = serde_json::json!({ "text": "x" });
    assert!(ws.agent_call(Level::Read, "brain_add", &add).is_err(), "read level may not add");
    assert!(ws.agent_call(Level::Edit, "brain_add", &add).is_err(), "no clock, no entry");
    assert!(ws.agent_call(Level::Read, "brain_search", &serde_json::json!({ "query": "x" })).is_err(), "no clock, no search");
    assert!(ws.agent_call(Level::Read, "brain_search", &serde_json::json!({})).is_err(), "a query is required");
}

#[test]
fn agents_may_search_and_read_the_brain_by_default_but_not_add() {
    let ws = Workspace::new(EditSpy::default());
    assert_eq!(ws.settings.brain_agents, "read");
}

#[test]
fn agents_get_into_the_brain_only_as_far_as_the_user_allowed() {
    use crate::mcp::access::Level;
    let mut ws = Workspace::new(EditSpy::default());
    ws.new_tab();
    ws.settings.brain_agents = "off".into();
    let why = |ws: &mut Workspace<EditSpy>, tool: &str, args: serde_json::Value| ws.agent_call(Level::Full, tool, &args).unwrap_err().to_string();
    for tool in ["brain_overview", "brain_search", "brain_read", "brain_add"] {
        assert!(
            why(&mut ws, tool, serde_json::json!({ "query": "x", "note": "x", "text": "x" })).contains("not allowed"),
            "{tool} is closed when set to Nothing"
        );
    }
    ws.settings.brain_agents = "read".into();
    assert!(why(&mut ws, "brain_add", serde_json::json!({ "text": "x" })).contains("not add"), "reading is not adding");
    assert!(
        why(&mut ws, "brain_read", serde_json::json!({ "note": "C:\\Windows\\win.ini" })).contains("not a note of Quick Notes"),
        "only Quick Notes's own notes"
    );
}
