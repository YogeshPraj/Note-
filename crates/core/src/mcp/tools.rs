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

fn board() -> Value {
    json!({ "type": "integer", "description": "The whiteboard tab's id from list_tabs. The active tab when left out." })
}

fn shape() -> Value {
    json!({
        "type": "object",
        "properties": {
            "type": { "type": "string", "enum": ["rectangle", "ellipse", "diamond", "text", "arrow", "line"] },
            "id": { "type": "string", "description": "Your own name for it, so that arrows in this call can use it. Made up for you when left out." },
            "x": { "type": "number", "description": "Below what is on the board when left out, with y." },
            "y": { "type": "number" },
            "width": { "type": "number" },
            "height": { "type": "number" },
            "text": { "type": "string", "description": "The text of a text shape, or a label inside a shape or on an arrow." },
            "stroke": { "type": "string", "description": "Colour, like #1e1e1e." },
            "background": { "type": "string", "description": "Colour, or transparent." },
            "from": { "type": "string", "description": "Arrows: the id of the shape it starts at, from this call or already on the board." },
            "to": { "type": "string", "description": "Arrows: the id of the shape it ends at." },
            "font_size": { "type": "number" },
        },
        "required": ["type"],
    })
}

fn tree() -> Value {
    json!({
        "type": "object",
        "description": "A node: { title, icon, color, open, children: [nodes] }. Icons are chosen from the title when left out; agent, tools, skills, memory and the like get fitting ones.",
    })
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
    Tool {
        name: "read_whiteboard",
        title: "Read a whiteboard",
        description: "Brings a whiteboard to the front and lists what is on it: each shape's id, kind, place, text and colours, arrows' ends, and trees as one item with their nodes.",
        level: Level::Read,
        properties: || json!({ "tab": board() }),
        required: &[],
    },
    Tool {
        name: "whiteboard_image",
        title: "See a whiteboard",
        description: "Brings a whiteboard to the front and gives a PNG picture of it. Use it to check what you drew.",
        level: Level::Read,
        properties: || json!({ "tab": board() }),
        required: &[],
    },
    Tool {
        name: "new_whiteboard",
        title: "New whiteboard",
        description: "Opens a new empty whiteboard and makes it active.",
        level: Level::Edit,
        properties: || json!({}),
        required: &[],
    },
    Tool {
        name: "add_shapes",
        title: "Draw shapes",
        description: "Adds shapes to a whiteboard as one undo step: boxes, ellipses, diamonds, text, lines, and arrows that stay joined to the shapes they connect.",
        level: Level::Edit,
        properties: || json!({ "tab": board(), "shapes": { "type": "array", "items": shape() } }),
        required: &["shapes"],
    },
    Tool {
        name: "update_shape",
        title: "Change a shape",
        description: "Moves, resizes, relabels or recolours a shape, as one undo step. Arrows joined to it follow.",
        level: Level::Edit,
        properties: || {
            json!({
                "tab": board(),
                "id": { "type": "string" },
                "x": { "type": "number" },
                "y": { "type": "number" },
                "width": { "type": "number" },
                "height": { "type": "number" },
                "text": { "type": "string" },
                "stroke": { "type": "string" },
                "background": { "type": "string" },
            })
        },
        required: &["id"],
    },
    Tool {
        name: "delete_shapes",
        title: "Remove shapes",
        description: "Removes shapes or trees by id, as one undo step.",
        level: Level::Edit,
        properties: || json!({ "tab": board(), "ids": { "type": "array", "items": { "type": "string" } } }),
        required: &["ids"],
    },
    Tool {
        name: "add_tree",
        title: "Draw a tree",
        description: "Adds a compact tree to a whiteboard, as one undo step. Good for showing an agent: its instruction, tools, skills and sub-agents. Gives the tree's id.",
        level: Level::Edit,
        properties: || json!({ "tab": board(), "tree": tree(), "x": { "type": "number" }, "y": { "type": "number" } }),
        required: &["tree"],
    },
    Tool {
        name: "set_tree",
        title: "Change a tree",
        description: "Replaces a tree's nodes, as one undo step. Read it with read_whiteboard, change it, and give it back.",
        level: Level::Edit,
        properties: || json!({ "tab": board(), "id": { "type": "string", "description": "The tree's id." }, "tree": tree() }),
        required: &["id", "tree"],
    },
];

/// The tools a whiteboard's page carries out.
const BOARD: [&str; 7] = ["read_whiteboard", "whiteboard_image", "add_shapes", "update_shape", "delete_shapes", "add_tree", "set_tree"];

/// Is `tool` carried out by a whiteboard's page?
pub fn is_board(tool: &str) -> bool {
    BOARD.contains(&tool)
}

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
        let changing = [
            "new_document",
            "edit_text",
            "insert_text",
            "write_document",
            "replace_all",
            "save_document",
            "run_command",
            "select_lines",
            "new_whiteboard",
            "add_shapes",
            "update_shape",
            "delete_shapes",
            "add_tree",
            "set_tree",
        ];
        for name in changing {
            assert_eq!(find(name).map(|t| t.level), Some(Level::Edit), "{name}");
        }
    }
}
