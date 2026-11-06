//! The agent tools that a whiteboard's, a picture's or the preview's page carries out: what is checked before the page is
//! asked, and that a refused call moves nobody.

use super::agent::{failure, workspace};
use super::*;
use crate::mcp::access::Level;
use serde_json::json;

#[test]
fn a_whiteboard_call_is_checked_and_brings_the_board_to_the_front() {
    let mut ws = workspace();
    let text = ws.tabs[0].id;
    ws.new_whiteboard();
    let board = ws.tabs[1].id;
    ws.activate(0);
    assert_eq!(ws.agent_board(Level::Read, "list_tabs", &json!({})).unwrap(), None, "not for a page");
    assert!(failure(ws.agent_board(Level::Read, "add_shapes", &json!({ "tab": board })).map(|_| json!(0))).contains("Edit"));
    assert!(failure(ws.agent_board(Level::Edit, "add_shapes", &json!({ "tab": text })).map(|_| json!(0))).contains("not a whiteboard"));
    assert_eq!(ws.active, 0, "a refused call moves nobody");
    let request = ws.agent_board(Level::Edit, "add_shapes", &json!({ "tab": board, "shapes": [] })).unwrap().unwrap();
    assert_eq!((request.tab, request.op["tool"].as_str(), request.save_after), (board, Some("add_shapes"), false));
    assert_eq!(ws.active, 1);
    ws.tabs[1].encrypted = true;
    assert!(failure(ws.agent_board(Level::Edit, "read_whiteboard", &json!({})).map(|_| json!(0))).contains("encrypted"));
}

#[test]
fn a_new_whiteboard_can_be_made_from_a_text_tab() {
    let mut ws = workspace();
    assert_eq!(ws.agent_board(Level::Edit, "new_whiteboard", &json!({})).unwrap(), None, "no page is needed");
    let made = ws.agent_call(Level::Edit, "new_whiteboard", &json!({})).unwrap();
    assert_eq!(made["tab"], ws.tabs[1].id);
    assert_eq!(ws.tabs[1].kind, TabKind::Whiteboard);
}

#[test]
fn every_page_tool_is_refused_on_a_text_tab_except_the_ones_that_make_a_tab() {
    let makes_a_tab = ["new_whiteboard", "new_picture"];
    for tool in crate::mcp::page_tools::TOOLS.iter().filter(|t| !makes_a_tab.contains(&t.name)) {
        let mut ws = workspace();
        assert!(ws.agent_board(Level::Full, tool.name, &json!({})).is_err(), "{}", tool.name);
    }
}

#[test]
fn a_diagram_goes_to_the_preview_page_and_moves_nothing() {
    let mut ws = workspace();
    ws.new_tab();
    let source = "graph TD\n A-->B";
    let request = ws.agent_board(Level::Read, "preview_diagram", &json!({ "text": source })).unwrap().unwrap();
    assert_eq!((request.tab, request.op["source"].as_str(), request.save_after), (crate::workspace::PREVIEW_PAGE, Some(source), false));
    assert_eq!(ws.active, 1);
    assert!(failure(ws.agent_board(Level::Read, "preview_diagram", &json!({ "text": "  " })).map(|_| json!(0))).contains("no diagram"));
    assert!(failure(ws.agent_board(Level::Read, "preview_diagram", &json!({ "text": "x".repeat(50_001) })).map(|_| json!(0))).contains("too long"));
    assert!(failure(ws.agent_board(Level::Read, "preview_diagram", &json!({ "tab": 9999 })).map(|_| json!(0))).contains("9999"));
}

#[test]
fn a_diagram_in_an_encrypted_tab_needs_the_users_answer() {
    let mut ws = workspace();
    ws.tabs[0].encrypted = true;
    assert!(failure(ws.agent_board(Level::Read, "preview_diagram", &json!({})).map(|_| json!(0))).contains("asking the user"));
}

#[test]
fn saving_a_whiteboard_asks_the_page_for_its_scene_first() {
    let mut ws = workspace();
    ws.new_whiteboard();
    assert!(failure(ws.agent_board(Level::Edit, "save_document", &json!({})).map(|_| json!(0))).contains("no file yet"));
    ws.tabs[1].path = Some(std::env::temp_dir().join("notepp-agent-no-such-board.excalidraw"));
    let request = ws.agent_board(Level::Edit, "save_document", &json!({})).unwrap().unwrap();
    assert_eq!((request.op["tool"].as_str(), request.save_after), (Some("sync"), true));
    assert_eq!(ws.agent_board(Level::Edit, "save_document", &json!({ "tab": ws.tabs[0].id })).unwrap(), None, "text saves here");
}

#[test]
fn a_picture_call_is_checked_like_a_whiteboard_one() {
    let mut ws = workspace();
    let text = ws.tabs[0].id;
    ws.new_image(String::new());
    let picture = ws.tabs[1].id;
    ws.activate(0);
    assert!(failure(ws.agent_board(Level::Read, "add_annotations", &json!({ "tab": picture })).map(|_| json!(0))).contains("Edit"));
    assert!(failure(ws.agent_board(Level::Edit, "add_annotations", &json!({ "tab": text })).map(|_| json!(0))).contains("not a picture"));
    assert!(failure(ws.agent_board(Level::Edit, "add_shapes", &json!({ "tab": picture })).map(|_| json!(0))).contains("not a whiteboard"));
    let read = ws.agent_board(Level::Read, "read_picture", &json!({ "tab": picture })).unwrap().unwrap();
    assert_eq!((read.tab, read.op["tool"].as_str()), (picture, Some("read_picture")));
    assert_eq!(ws.active, 1, "it came to the front");
}

#[test]
fn saving_a_picture_asks_its_page_first_and_makes_a_png() {
    let dir = std::env::temp_dir().join(format!("notepp-agent-png-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut ws = workspace();
    ws.workspace = Some(dir.clone());
    ws.new_image(String::new());
    let asked = dir.join("shot.jpg");
    let request = ws.agent_board(Level::Edit, "save_document", &json!({ "path": asked })).unwrap().unwrap();
    assert_eq!((request.op["tool"].as_str(), request.save_after), (Some("sync"), true));
    ws.agent_call(Level::Edit, "save_document", &json!({ "path": asked })).unwrap();
    assert!(dir.join("shot.png").exists() && !asked.exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_new_picture_with_a_wrong_size_leaves_no_empty_tab() {
    let mut ws = workspace();
    for args in [json!({ "width": 0, "height": 5 }), json!({ "width": 20000, "height": 5 }), json!({ "width": 16000, "height": 16000 }), json!({ "width": 5 })]
    {
        assert!(ws.agent_board(Level::Edit, "new_picture", &args).is_err(), "{args}");
    }
    assert_eq!(ws.tabs.len(), 1);
    assert!(ws.agent_board(Level::Edit, "new_picture", &json!({ "width": 10, "height": 10 })).unwrap().is_some());
    assert_eq!(ws.tabs.len(), 2);
}
