//! The tools that a page carries out, as a whiteboard's and a picture's latest state is in their pages.

use serde_json::{Value, json};

use super::access::Level;
use super::tools::Tool;
use crate::workspace::TabKind;

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

fn picture() -> Value {
    json!({ "type": "integer", "description": "The picture tab's id from list_tabs. The active tab when left out." })
}

fn annotation() -> Value {
    json!({
        "type": "object",
        "properties": {
            "type": { "type": "string", "enum": ["rectangle", "ellipse", "line", "arrow", "text", "bubble", "step", "highlight", "hide"] },
            "x": { "type": "number", "description": "Left edge, the start of a line, or the centre of a step." },
            "y": { "type": "number" },
            "width": { "type": "number" },
            "height": { "type": "number" },
            "x2": { "type": "number", "description": "Lines and arrows: where they end." },
            "y2": { "type": "number" },
            "text": { "type": "string", "description": "What a text or a bubble says." },
            "color": { "type": "string", "description": "Colour, like #e53935." },
            "fill": { "type": "string", "description": "Colour inside a box, an ellipse, a bubble or a step." },
            "thickness": { "type": "number", "description": "Line width; for hide, the size of its blocks." },
            "font_size": { "type": "number" },
            "tail_x": { "type": "number", "description": "Bubbles: where the tail points." },
            "tail_y": { "type": "number" },
        },
        "required": ["type"],
    })
}

/// The tools for pages.
pub const TOOLS: &[Tool] = &[
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
    Tool {
        name: "read_picture",
        title: "Read a picture",
        description: "Brings a picture to the front and gives its size and its annotations, which stay editable until the picture is saved and reopened.",
        level: Level::Read,
        properties: || json!({ "tab": picture() }),
        required: &[],
    },
    Tool {
        name: "picture_image",
        title: "See a picture",
        description: "Brings a picture to the front and gives it as a PNG with its annotations, at most 1600 pixels on its longest side. Use it to see what is on it and to check your work.",
        level: Level::Read,
        properties: || json!({ "tab": picture() }),
        required: &[],
    },
    Tool {
        name: "new_picture",
        title: "New picture",
        description: "Opens a new empty picture of the size given, and makes it active.",
        level: Level::Edit,
        properties: || json!({ "width": { "type": "integer" }, "height": { "type": "integer" }, "background": { "type": "string", "description": "Colour. White when left out." } }),
        required: &["width", "height"],
    },
    Tool {
        name: "add_annotations",
        title: "Annotate a picture",
        description: "Adds annotations to a picture as one undo step: boxes, ellipses, lines, arrows, text, speech bubbles, numbered steps, highlights, and hide, which pixelates what is under it. Pixelation can leave a shape or colour showing: to remove something for good, cover it with a filled rectangle.",
        level: Level::Edit,
        properties: || json!({ "tab": picture(), "items": { "type": "array", "items": annotation() } }),
        required: &["items"],
    },
    Tool {
        name: "update_annotation",
        title: "Change an annotation",
        description: "Moves, resizes, rewords or recolours an annotation, as one undo step. Give only what changes.",
        level: Level::Edit,
        properties: || {
            let mut fields = annotation()["properties"].clone();
            fields["id"] = json!({ "type": "string" });
            fields["tab"] = picture();
            fields
        },
        required: &["id"],
    },
    Tool {
        name: "delete_annotations",
        title: "Remove annotations",
        description: "Removes annotations by id, as one undo step.",
        level: Level::Edit,
        properties: || json!({ "tab": picture(), "ids": { "type": "array", "items": { "type": "string" } } }),
        required: &["ids"],
    },
    Tool {
        name: "edit_picture",
        title: "Change the whole picture",
        description: "Rotates, flips, crops or resizes a picture, as one undo step. Annotations become part of the picture.",
        level: Level::Edit,
        properties: || {
            json!({
                "tab": picture(),
                "action": { "type": "string", "enum": ["rotate_left", "rotate_right", "flip_horizontal", "flip_vertical", "crop", "resize"] },
                "x": { "type": "integer", "description": "Crop: left edge." },
                "y": { "type": "integer", "description": "Crop: top edge." },
                "width": { "type": "integer", "description": "Crop and resize." },
                "height": { "type": "integer", "description": "Crop and resize." },
            })
        },
        required: &["action"],
    },
];

/// What kind of tab `tool` works on, when a page carries it out. A new whiteboard needs no page: its empty scene is made here.
pub fn kind_of(tool: &str) -> Option<TabKind> {
    let at = TOOLS.iter().position(|t| t.name == tool && t.name != "new_whiteboard")?;
    Some(if TOOLS[at].name.contains("picture") || TOOLS[at].name.contains("annotation") { TabKind::Image } else { TabKind::Whiteboard })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_page_tool_knows_its_kind_of_tab() {
        assert_eq!(kind_of("add_shapes"), Some(TabKind::Whiteboard));
        assert_eq!(kind_of("whiteboard_image"), Some(TabKind::Whiteboard));
        assert_eq!(kind_of("add_annotations"), Some(TabKind::Image));
        assert_eq!(kind_of("picture_image"), Some(TabKind::Image));
        assert_eq!(kind_of("edit_text"), None);
        assert_eq!(kind_of("new_whiteboard"), None, "there is no whiteboard yet for a page to hold");
    }
}
