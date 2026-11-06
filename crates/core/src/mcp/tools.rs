//! The tool table: what agents are told exists, and the level each tool needs. `Workspace::agent_call` carries them out.

use serde_json::{Value, json};

use super::access::Level;

/// One tool.
#[derive(Debug)]
pub struct Tool {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub level: Level,
    /// The properties of its input; `required` lists those that must be given.
    pub properties: fn() -> Value,
    pub required: &'static [&'static str],
}

fn tab() -> Value {
    json!({ "type": "integer", "description": "The tab's id from list_tabs. The active tab when left out." })
}

fn find_options() -> Value {
    json!({
        "tab": tab(),
        "find": { "type": "string", "description": "What to look for." },
        "regex": { "type": "boolean", "description": "Treat find as a regular expression." },
        "match_case": { "type": "boolean" },
        "whole_word": { "type": "boolean" },
    })
}

/// Every tool. All the tools that change something need [`Level::Edit`].
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "list_tabs",
        title: "List tabs",
        description: "Lists the tabs open in Note++: id, title, path, kind, whether modified or active, line count.",
        level: Level::Read,
        properties: || json!({}),
        required: &[],
    },
    Tool {
        name: "read_document",
        title: "Read a document",
        description: "Reads a text tab, including changes not saved yet. A large document comes in parts: go on from next_line.",
        level: Level::Read,
        properties: || {
            json!({
                "tab": tab(),
                "start_line": { "type": "integer", "description": "First line, from 1." },
                "line_count": { "type": "integer", "description": "How many lines. All when left out." },
            })
        },
        required: &[],
    },
    Tool {
        name: "find_text",
        title: "Find text",
        description: "Finds text in a tab and gives each match's line.",
        level: Level::Read,
        properties: find_options,
        required: &["find"],
    },
    Tool {
        name: "open_file",
        title: "Open a file",
        description: "Opens a file in a tab and makes it active. Only files in the workspace folder, or already open.",
        level: Level::Read,
        properties: || json!({ "path": { "type": "string", "description": "Full path." } }),
        required: &["path"],
    },
    Tool {
        name: "switch_tab",
        title: "Switch tab",
        description: "Brings a tab to the front.",
        level: Level::Read,
        properties: || json!({ "tab": { "type": "integer", "description": "The tab's id from list_tabs." } }),
        required: &["tab"],
    },
    Tool {
        name: "new_document",
        title: "New document",
        description: "Opens a new unsaved tab, optionally with text in it, and makes it active.",
        level: Level::Edit,
        properties: || json!({ "text": { "type": "string" } }),
        required: &[],
    },
    Tool {
        name: "edit_text",
        title: "Replace exact text",
        description: "Replaces exact text in a tab, as one undo step. It must match once, unless all is set. This is the safest way to change part of a document.",
        level: Level::Edit,
        properties: || {
            json!({
                "tab": tab(),
                "old_text": { "type": "string", "description": "The text to replace, exactly as it is in the document." },
                "new_text": { "type": "string" },
                "all": { "type": "boolean", "description": "Replace every match." },
            })
        },
        required: &["old_text", "new_text"],
    },
    Tool {
        name: "insert_text",
        title: "Insert text",
        description: "Inserts text before a line, or at the end of the document, as one undo step.",
        level: Level::Edit,
        properties: || {
            json!({
                "tab": tab(),
                "text": { "type": "string" },
                "line": { "type": "integer", "description": "Insert before this line, from 1. The end of the document when left out." },
            })
        },
        required: &["text"],
    },
    Tool {
        name: "write_document",
        title: "Replace the whole document",
        description: "Replaces everything in a tab, as one undo step. Prefer edit_text for changes to part of a document.",
        level: Level::Edit,
        properties: || json!({ "tab": tab(), "text": { "type": "string" } }),
        required: &["text"],
    },
    Tool {
        name: "replace_all",
        title: "Find and replace",
        description: "Replaces every match in a tab, as one undo step, and says how many. With regex, replace may use $1.",
        level: Level::Edit,
        properties: || {
            let mut options = find_options();
            options["replace"] = json!({ "type": "string" });
            options
        },
        required: &["find", "replace"],
    },
    Tool {
        name: "save_document",
        title: "Save a document",
        description: "Saves a text tab to its file. A tab with no file needs a path in the workspace folder, and an existing file is never overwritten that way.",
        level: Level::Edit,
        properties: || json!({ "tab": tab(), "path": { "type": "string", "description": "Full path, for a tab with no file." } }),
        required: &[],
    },
    Tool {
        name: "list_commands",
        title: "List commands",
        description: "Lists the Note++ commands run_command can run, and the level each needs.",
        level: Level::Read,
        properties: || json!({}),
        required: &[],
    },
    Tool {
        name: "select_lines",
        title: "Select lines",
        description: "Brings a tab to the front and selects whole lines. Commands such as sorting or changing case work on the selection.",
        level: Level::Edit,
        properties: || {
            json!({
                "tab": tab(),
                "start_line": { "type": "integer", "description": "First line, from 1." },
                "end_line": { "type": "integer", "description": "Last line. Only the first line when left out." },
            })
        },
        required: &["start_line"],
    },
    Tool {
        name: "run_command",
        title: "Run a command",
        description: "Runs a Note++ command by the name list_commands gives, on the active tab or the tab named, which comes to the front. A text command is one undo step.",
        level: Level::Edit,
        properties: || json!({ "command": { "type": "string", "description": "A name from list_commands." }, "tab": tab() }),
        required: &["command"],
    },
];

/// The tool called `name`.
pub fn find(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|t| t.name == name)
}

/// The tools as `tools/list` gives them. All of them, whatever the level: an agent that tries one is told what to ask the
/// user, which is more use than a tool it cannot know about, and the level can change while it is connected.
pub fn listing() -> Value {
    TOOLS
        .iter()
        .map(|t| {
            json!({
                "name": t.name,
                "title": t.title,
                "description": t.description,
                "inputSchema": { "type": "object", "properties": (t.properties)(), "required": t.required, "additionalProperties": false },
                "annotations": { "title": t.title, "readOnlyHint": t.level == Level::Read, "destructiveHint": false, "openWorldHint": false },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_every_required_input_is_described() {
        for (i, t) in TOOLS.iter().enumerate() {
            assert!(TOOLS[i + 1..].iter().all(|o| o.name != t.name), "{}", t.name);
            let properties = (t.properties)();
            assert!(t.required.iter().all(|r| properties.get(r).is_some()), "{}", t.name);
        }
    }

    #[test]
    fn the_listing_is_what_a_client_expects() {
        let listed = listing();
        let first = &listed[0];
        assert_eq!(first["name"], "list_tabs");
        assert_eq!(first["inputSchema"]["type"], "object");
        assert_eq!(first["annotations"]["readOnlyHint"], true);
        assert_eq!(listed.as_array().unwrap().len(), TOOLS.len());
    }

    #[test]
    fn nothing_that_changes_a_document_is_read_level() {
        let changing = ["new_document", "edit_text", "insert_text", "write_document", "replace_all", "save_document", "run_command", "select_lines"];
        for name in changing {
            assert_eq!(find(name).map(|t| t.level), Some(Level::Edit), "{name}");
        }
    }
}
