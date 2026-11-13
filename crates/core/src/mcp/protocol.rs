//! The JSON-RPC side of MCP: what each message is answered with. No sockets and no documents here.

use serde_json::{Value, json};

use super::tools;
use crate::error::Error;

/// The protocol versions spoken, newest first. A client that asks for another gets the newest and decides for itself.
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
/// The body was not JSON.
pub const PARSE_ERROR: i32 = -32700;

/// The name a client gives in `initialize`.
pub fn client_name(msg: &Value) -> &str {
    msg["params"]["clientInfo"]["name"].as_str().unwrap_or_default()
}

const INSTRUCTIONS: &str = "Note++ is a text editor the user has open. These tools work on its tabs, and the user sees every change. \
Call list_tabs first; list_commands gives the editor's own commands, for run_command. Tabs are named by the id it gives; ids stay the same while a tab is open. Whiteboards have tools of their own: read_whiteboard, add_shapes, draw_elements (Excalidraw MCP's element format: read whiteboard_guide first), add_mermaid (Mermaid text: flowcharts and sequence diagrams become editable shapes, other types a picture; offline), add_tree and whiteboard_image, which shows the result. Pictures likewise: read_picture, add_annotations, edit_picture and picture_image. preview_diagram draws Mermaid and reports its errors. An encrypted document needs the user's permission: the tool says so, and Note++ asks them.";

/// A JSON-RPC error reply.
pub fn error_reply(id: &Value, code: i32, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn ok(id: &Value, result: &Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn initialized(id: &Value, msg: &Value) -> Value {
    let asked = msg["params"]["protocolVersion"].as_str();
    let version = asked.filter(|a| VERSIONS.contains(a)).unwrap_or(VERSIONS[0]);
    ok(
        id,
        &json!({
            "protocolVersion": version,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "notepp", "title": "Note++", "version": crate::update::VERSION },
            "instructions": INSTRUCTIONS,
        }),
    )
}

/// What an agent reads: an answer in plain text, anything else as compact JSON.
fn content(text: &str, failed: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": failed })
}

fn tool_call(id: &Value, msg: &Value, run: &dyn Fn(&str, &Value) -> Result<Value, Error>) -> Value {
    let Some(name) = msg["params"]["name"].as_str().filter(|n| tools::find(n).is_some()) else {
        return error_reply(id, INVALID_PARAMS, "Unknown tool");
    };
    let empty = json!({});
    let args = msg["params"].get("arguments").unwrap_or(&empty);
    ok(
        id,
        &match run(name, args) {
            Ok(Value::String(text)) => content(&text, false),
            Ok(value) if value.get("image_png").is_some() => json!({
                "content": [{ "type": "image", "data": value["image_png"], "mimeType": "image/png" }],
                "isError": false,
            }),
            Ok(value) => content(&value.to_string(), false),
            // A tool that fails is an answer the agent can act on, not a protocol error.
            Err(e) => content(&e.to_string(), true),
        },
    )
}

/// The answer to one message, None for a notification or a stray reply. `run` carries out a tool call.
pub fn answer(msg: &Value, run: &dyn Fn(&str, &Value) -> Result<Value, Error>) -> Option<Value> {
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        let stray_reply = msg.get("id").is_some() && (msg.get("result").is_some() || msg.get("error").is_some());
        return (!stray_reply).then(|| error_reply(msg.get("id").unwrap_or(&Value::Null), INVALID_REQUEST, "Not a JSON-RPC request"));
    };
    let id = msg.get("id")?;
    Some(match method {
        "initialize" => initialized(id, msg),
        "ping" => ok(id, &json!({})),
        "tools/list" => ok(id, &json!({ "tools": tools::listing() })),
        "tools/call" => tool_call(id, msg, run),
        _ => error_reply(id, METHOD_NOT_FOUND, "Method not found"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::needless_pass_by_value)]
    fn call(msg: Value) -> Value {
        answer(&msg, &never).unwrap()
    }

    fn never(_: &str, _: &Value) -> Result<Value, Error> {
        panic!("no tool should run")
    }

    #[test]
    fn initialize_keeps_a_known_version_and_offers_the_newest_otherwise() {
        let reply = |v: &str| call(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": v } }));
        assert_eq!(reply("2025-06-18")["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(reply("1999-01-01")["result"]["protocolVersion"], VERSIONS[0]);
        assert_eq!(reply("2025-11-25")["result"]["serverInfo"]["name"], "notepp");
    }

    #[test]
    fn a_newer_client_falls_back_when_its_discovery_is_not_known() {
        let r = call(json!({ "jsonrpc": "2.0", "id": 7, "method": "server/discover", "params": {} }));
        assert_eq!(r["error"]["code"], METHOD_NOT_FOUND);
        assert_eq!(r["id"], 7);
    }

    #[test]
    fn ping_and_the_tool_list_are_answered() {
        assert_eq!(call(json!({ "id": "a", "method": "ping" }))["result"], json!({}));
        assert!(!call(json!({ "id": 2, "method": "tools/list" }))["result"]["tools"].as_array().unwrap().is_empty());
    }

    #[test]
    fn notifications_and_stray_replies_get_no_answer() {
        assert!(answer(&json!({ "method": "notifications/initialized" }), &never).is_none());
        assert!(answer(&json!({ "id": 1, "result": {} }), &never).is_none());
    }

    #[test]
    fn anything_else_is_an_invalid_request() {
        assert_eq!(call(json!([1, 2]))["error"]["code"], INVALID_REQUEST);
        assert_eq!(call(json!({ "id": 1 }))["error"]["code"], INVALID_REQUEST);
    }

    #[test]
    fn a_tool_runs_and_its_failure_is_an_answer() {
        let run = |name: &str, args: &Value| -> Result<Value, Error> {
            assert_eq!(args, &json!({ "tab": 3 }));
            if name == "list_tabs" { Ok(json!({ "ok": 1 })) } else { Err(Error::Agent("no".into())) }
        };
        let msg = |name: &str| json!({ "id": 1, "method": "tools/call", "params": { "name": name, "arguments": { "tab": 3 } } });
        let good = answer(&msg("list_tabs"), &run).unwrap();
        assert_eq!((good["result"]["content"][0]["text"].as_str(), &good["result"]["isError"]), (Some("{\"ok\":1}"), &json!(false)));
        let bad = answer(&msg("read_document"), &run).unwrap();
        assert_eq!((bad["result"]["content"][0]["text"].as_str(), &bad["result"]["isError"]), (Some("no"), &json!(true)));
    }

    #[test]
    fn a_picture_is_an_image_answer() {
        let run = |_: &str, _: &Value| -> Result<Value, Error> { Ok(json!({ "image_png": "QUJD" })) };
        let r = answer(&json!({ "id": 1, "method": "tools/call", "params": { "name": "whiteboard_image" } }), &run).unwrap();
        let item = &r["result"]["content"][0];
        assert_eq!((item["type"].as_str(), item["data"].as_str(), item["mimeType"].as_str()), (Some("image"), Some("QUJD"), Some("image/png")));
    }

    #[test]
    fn an_unknown_tool_never_reaches_the_runner() {
        let r = call(json!({ "id": 1, "method": "tools/call", "params": { "name": "delete_everything" } }));
        assert_eq!(r["error"]["code"], INVALID_PARAMS);
    }

    #[test]
    fn a_tool_without_arguments_gets_an_empty_object() {
        let run = |_: &str, args: &Value| -> Result<Value, Error> {
            assert_eq!(args, &json!({}));
            Ok(json!("fine"))
        };
        let r = answer(&json!({ "id": 1, "method": "tools/call", "params": { "name": "list_tabs" } }), &run).unwrap();
        assert_eq!(r["result"]["content"][0]["text"], "fine");
    }
}
