//! The agent tools' rules, on a stand-in for Scintilla that holds no text: levels, refusals, undo steps and the modified
//! mark. The text work has its own tests in `mcp::text`, and a real agent exercises the whole of it against the real editor.

use super::*;
use crate::mcp::access::Level;
use serde_json::{Value, json};
use std::cell::RefCell;

#[derive(Default)]
pub(super) struct Recorder {
    doc: Cell<isize>,
    next: Cell<isize>,
    read_only: Cell<bool>,
    calls: RefCell<Vec<u32>>,
}

impl sci::Send for Recorder {
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
            sci::SCI_GETLINECOUNT => 1,
            _ => 0,
        }
    }
}

pub(super) fn workspace() -> Workspace<Recorder> {
    let mut ws = Workspace::new(Recorder { next: Cell::new(100), ..Recorder::default() });
    ws.new_tab();
    ws
}

pub(super) fn failure(r: Result<Value, crate::error::Error>) -> String {
    r.unwrap_err().to_string()
}

fn count(ws: &Workspace<Recorder>, msg: u32) -> usize {
    ws.sci.calls.borrow().iter().filter(|m| **m == msg).count()
}

#[test]
fn read_only_lets_agents_read_and_nothing_else() {
    let mut ws = workspace();
    assert!(ws.agent_call(Level::Read, "list_tabs", &json!({})).is_ok());
    assert!(ws.agent_call(Level::Read, "read_document", &json!({})).is_ok());
    for (tool, args) in [
        ("new_document", json!({})),
        ("insert_text", json!({ "text": "x" })),
        ("write_document", json!({ "text": "x" })),
        ("edit_text", json!({ "old_text": "a", "new_text": "b" })),
        ("replace_all", json!({ "find": "a", "replace": "b" })),
        ("save_document", json!({})),
    ] {
        let message = failure(ws.agent_call(Level::Read, tool, &args));
        assert!(message.contains("Settings > AI Agents"), "{tool}: {message}");
    }
    assert_eq!(ws.tabs.len(), 1, "nothing happened");
    assert!(!ws.is_dirty(0));
}

#[test]
fn commands_follow_the_level_and_forbidden_ones_stay_forbidden() {
    let mut ws = workspace();
    let sort = json!({ "command": "Edit: Sort Lines Ascending" });
    let wrap = json!({ "command": "View: Toggle Word Wrap" });
    assert!(failure(ws.agent_command(Level::Read, &sort).map(|i| json!(i))).contains("Edit"));
    assert_eq!(ws.agent_command(Level::Edit, &sort).unwrap(), crate::commands::ID_L_SORT_ASC);
    assert!(failure(ws.agent_command(Level::Edit, &wrap).map(|i| json!(i))).contains("Full"));
    assert_eq!(ws.agent_command(Level::Full, &wrap).unwrap(), crate::commands::ID_WRAP);
    for name in ["Edit: Paste", "File: Move to Recycle Bin", "Tools: Passwords (Bitwarden)...", "No such command"] {
        assert!(failure(ws.agent_command(Level::Full, &json!({ "command": name })).map(|i| json!(i))).contains("cannot run"), "{name}");
    }
}

#[test]
fn a_text_command_needs_a_tab_an_agent_may_change() {
    let mut ws = workspace();
    let plain = ws.tabs[0].id;
    let sort = |tab: u64| json!({ "command": "Edit: Sort Lines Ascending", "tab": tab });
    ws.new_whiteboard();
    let board = ws.tabs[1].id;
    assert!(failure(ws.agent_command(Level::Full, &sort(board)).map(|i| json!(i))).contains("whiteboard"));
    ws.tabs[0].encrypted = true;
    assert!(failure(ws.agent_command(Level::Full, &sort(plain)).map(|i| json!(i))).contains("encrypted"));
    assert_eq!(ws.active, 1, "a refused command moves nobody");
    ws.tabs[0].encrypted = false;
    ws.sci.read_only.set(true);
    assert!(failure(ws.agent_command(Level::Full, &sort(plain)).map(|i| json!(i))).contains("read-only"));
    ws.sci.read_only.set(false);
    assert!(ws.agent_command(Level::Full, &sort(plain)).is_ok());
    assert_eq!(ws.active, 0, "the tab it named comes to the front");
}

#[test]
fn a_modified_tab_is_not_closed_by_an_agent() {
    let mut ws = workspace();
    ws.tabs[0].forced_dirty = true;
    assert!(failure(ws.agent_command(Level::Full, &json!({ "command": "File: Close" })).map(|i| json!(i))).contains("unsaved"));
    ws.tabs[0].forced_dirty = false;
    assert!(ws.agent_command(Level::Full, &json!({ "command": "File: Close" })).is_ok());
}

#[test]
fn a_text_command_is_one_undo_step_and_undo_is_not_wrapped() {
    let ws = workspace();
    ws.sci.calls.borrow_mut().clear();
    ws.agent_undo_group(crate::commands::ID_L_SORT_ASC, true);
    ws.agent_undo_group(crate::commands::ID_L_SORT_ASC, false);
    ws.agent_undo_group(crate::commands::ID_UNDO, true);
    ws.agent_undo_group(crate::commands::ID_UNDO, false);
    assert_eq!((count(&ws, sci::SCI_BEGINUNDOACTION), count(&ws, sci::SCI_ENDUNDOACTION)), (1, 1));
}

#[test]
fn the_command_list_says_what_each_needs() {
    let mut ws = workspace();
    let listed = ws.agent_call(Level::Read, "list_commands", &json!({})).unwrap();
    let rows = listed["commands"].as_array().unwrap();
    assert_eq!(rows.len(), crate::mcp::commands::allowed().count());
    assert!(rows.iter().any(|r| r["name"] == "Edit: Undo" && r["needs"] == "edit"));
    assert!(!rows.iter().any(|r| r["name"] == "Edit: Paste"));
}

#[test]
fn the_user_is_asked_about_an_encrypted_tab_and_the_answer_holds() {
    let mut ws = workspace();
    ws.tabs[0].encrypted = true;
    let tab = ws.tabs[0].id;
    let read = json!({ "tab": tab });
    assert!(failure(ws.agent_call(Level::Read, "read_document", &read)).contains("asking the user"));
    assert_eq!(ws.agent_asking().map(|a| a.0), Some(tab));
    assert_eq!(ws.agent_asking(), None, "asked once");
    ws.agent_answer(tab, true);
    assert!(ws.agent_call(Level::Read, "read_document", &read).is_ok());
    ws.agent_forget();
    assert!(ws.agent_call(Level::Read, "read_document", &read).is_err(), "only until the agents stop");
    ws.agent_asking();
    ws.agent_answer(tab, false);
    assert!(failure(ws.agent_call(Level::Read, "read_document", &read)).contains("refused"));
    assert_eq!(ws.agent_asking(), None, "a refusal is not asked about again");
}

#[test]
fn the_log_names_what_a_call_was_about_and_no_text() {
    let mut ws = workspace();
    ws.tabs[0].title = "notes.txt".into();
    assert_eq!(ws.agent_target("read_document", &json!({})), "notes.txt");
    assert_eq!(ws.agent_target("write_document", &json!({ "tab": ws.tabs[0].id, "text": "secret words" })), "notes.txt");
    assert_eq!(ws.agent_target("open_file", &json!({ "path": "C:/a.txt", "text": "secret words" })), "C:/a.txt");
    assert_eq!(ws.agent_target("read_document", &json!({ "tab": 9999 })), "a tab now closed");
    assert_eq!(ws.agent_target("list_tabs", &json!({})), "", "not about a tab");
    assert_eq!(ws.agent_target("new_document", &json!({ "text": "secret words" })), "");
    assert_eq!(ws.agent_target("preview_diagram", &json!({ "text": "graph TD" })), "", "a diagram given as text is about no tab");
    assert_eq!(ws.agent_target("preview_diagram", &json!({})), "notes.txt");
}

#[test]
fn a_snapshot_tab_is_read_only_and_reused() {
    let mut ws = workspace();
    ws.show_snapshot("AI activity", "one");
    ws.new_tab();
    ws.show_snapshot("AI activity", "two");
    assert_eq!(ws.tabs.iter().filter(|t| t.title == "AI activity").count(), 1);
    assert_eq!(ws.tabs[ws.active].title, "AI activity");
}

#[test]
fn an_unknown_tool_is_refused() {
    let mut ws = workspace();
    assert!(ws.agent_call(Level::Edit, "delete_file", &json!({})).is_err());
}

#[test]
fn every_tool_in_the_table_is_carried_out() {
    let mut ws = workspace();
    for tool in crate::mcp::tools::all() {
        let message = ws.agent_call(Level::Edit, tool.name, &json!({})).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(!message.contains("no tool"), "{}: {message}", tool.name);
    }
}

#[test]
fn listing_gives_ids_and_kinds() {
    let mut ws = workspace();
    ws.new_whiteboard();
    let listed = ws.agent_call(Level::Read, "list_tabs", &json!({})).unwrap();
    assert_eq!(listed["tabs"][0]["kind"], "text");
    assert_eq!(listed["tabs"][1]["kind"], "whiteboard");
    assert_eq!(listed["tabs"][1]["active"], true);
    assert_eq!(listed["tabs"][0]["id"], ws.tabs[0].id);
    assert!(listed["tabs"][1].get("lines").is_none());
}

#[test]
fn a_tab_that_does_not_exist_is_said() {
    let mut ws = workspace();
    assert!(failure(ws.agent_call(Level::Read, "read_document", &json!({ "tab": 9999 }))).contains("9999"));
    assert!(failure(ws.agent_call(Level::Read, "switch_tab", &json!({}))).contains("required"));
}

#[test]
fn whiteboards_and_encrypted_documents_are_refused() {
    let mut ws = workspace();
    let plain = ws.tabs[0].id;
    ws.new_whiteboard();
    assert!(failure(ws.agent_call(Level::Edit, "read_document", &json!({}))).contains("whiteboard"));
    ws.tabs[0].encrypted = true;
    for (tool, args) in [
        ("read_document", json!({ "tab": plain })),
        ("find_text", json!({ "tab": plain, "find": "a" })),
        ("write_document", json!({ "tab": plain, "text": "x" })),
        ("save_document", json!({ "tab": plain })),
    ] {
        assert!(failure(ws.agent_call(Level::Edit, tool, &args)).contains("encrypted"), "{tool}");
    }
    assert_eq!(ws.agent_call(Level::Read, "list_tabs", &json!({})).unwrap()["tabs"][0]["encrypted"], true);
}

#[test]
fn an_edit_is_one_undo_step_and_marks_a_hidden_tab_modified() {
    let mut ws = workspace();
    let hidden = ws.tabs[0].id;
    ws.new_tab();
    ws.sci.calls.borrow_mut().clear();
    ws.agent_call(Level::Edit, "write_document", &json!({ "tab": hidden, "text": "x" })).unwrap();
    assert_eq!(count(&ws, sci::SCI_BEGINUNDOACTION), 1);
    assert_eq!(count(&ws, sci::SCI_ENDUNDOACTION), 1);
    assert_eq!(count(&ws, sci::SCI_REPLACETARGET), 1);
    assert!(ws.is_dirty(0), "the hidden tab shows as modified");
    assert_eq!(ws.active, 1, "the user stays where they are");
}

#[test]
fn a_read_only_document_is_not_touched() {
    let mut ws = workspace();
    ws.sci.read_only.set(true);
    ws.sci.calls.borrow_mut().clear();
    assert!(failure(ws.agent_call(Level::Edit, "insert_text", &json!({ "text": "x" }))).contains("read-only"));
    assert_eq!(count(&ws, sci::SCI_REPLACETARGET), 0);
}

#[test]
fn an_exact_edit_with_no_match_changes_nothing() {
    let mut ws = workspace();
    ws.sci.calls.borrow_mut().clear();
    assert!(ws.agent_call(Level::Edit, "edit_text", &json!({ "old_text": "gone", "new_text": "x" })).is_err());
    assert_eq!(count(&ws, sci::SCI_BEGINUNDOACTION), 0);
    assert!(!ws.tabs[0].dirty);
}

#[test]
fn saving_a_tab_with_no_file_needs_a_path_and_opens_no_dialog() {
    let mut ws = workspace();
    assert!(failure(ws.agent_call(Level::Edit, "save_document", &json!({}))).contains("no file yet"));
}

#[test]
fn a_path_is_needed_in_reach_to_open_a_file() {
    let mut ws = workspace();
    let message = failure(ws.agent_call(Level::Read, "open_file", &json!({ "path": std::env::temp_dir().join("secret.txt") })));
    assert!(message.contains("not open in Note++"), "{message}");
    assert_eq!(ws.tabs.len(), 1);
}

#[test]
fn saving_refuses_a_file_changed_on_disk_and_never_overwrites_another() {
    let dir = std::env::temp_dir().join(format!("notepp-agent-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (mine, other) = (dir.join("mine.txt"), dir.join("other.txt"));
    std::fs::write(&mine, "one").unwrap();
    std::fs::write(&other, "two").unwrap();
    let mut ws = workspace();
    ws.workspace = Some(dir.clone());
    ws.agent_call(Level::Read, "open_file", &json!({ "path": mine })).unwrap();
    assert!(failure(ws.agent_call(Level::Edit, "save_document", &json!({ "path": other }))).contains("already exists"));
    assert_eq!(std::fs::read_to_string(&other).unwrap(), "two");
    std::fs::write(&mine, "changed by another program").unwrap();
    assert!(failure(ws.agent_call(Level::Edit, "save_document", &json!({}))).contains("changed on disk"));
    assert_eq!(std::fs::read_to_string(&mine).unwrap(), "changed by another program");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_new_file_inside_the_workspace_can_be_saved() {
    let dir = std::env::temp_dir().join(format!("notepp-agent-new-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("made.txt");
    let mut ws = workspace();
    ws.workspace = Some(dir.clone());
    let saved = ws.agent_call(Level::Edit, "save_document", &json!({ "path": target })).unwrap();
    assert!(target.exists());
    assert_eq!(saved["tab"], ws.tabs[0].id);
    assert_eq!(ws.tabs[0].title, "made.txt");
    let _ = std::fs::remove_dir_all(dir);
}
