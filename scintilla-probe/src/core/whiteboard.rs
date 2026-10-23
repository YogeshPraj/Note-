//! Whiteboard (Excalidraw) files: detection, naming and the empty scene.
//!
//! The format is the one the Electron app writes: a JSON envelope
//! `{"__wb__":true,"version":2,"source":"excalidraw","elements":[...],...}`.
//! Raw Excalidraw exports (`"type":"excalidraw"`) and the legacy v1 envelope
//! are also recognised; the Excalidraw page itself migrates them on load.

use std::path::Path;

/// A blank v2 scene, written for every new whiteboard.
pub fn empty_scene() -> String {
    r#"{"__wb__":true,"version":2,"source":"excalidraw","elements":[],"appState":{},"files":{}}"#.to_string()
}

/// True if `content` is a scene with no elements (nothing worth saving).
pub fn is_empty_scene(content: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| v.get("elements").and_then(|e| e.as_array()).map(|e| e.is_empty()))
        .unwrap_or(false)
}

/// Should this file open as a whiteboard rather than as text?
pub fn is_whiteboard_file(text: &str, path: &Path) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("excalidraw" | "whiteboard") => true,
        Some("json") => serde_json::from_str::<serde_json::Value>(text).is_ok_and(|v| {
            v.get("__wb__").and_then(|b| b.as_bool()) == Some(true) || v.get("type").and_then(|t| t.as_str()) == Some("excalidraw")
        }),
        _ => false,
    }
}

/// The visual content of a scene, ignoring anything that is not an edit:
/// viewport (pan/zoom), selection, deleted elements and bookkeeping fields
/// (`version`, `updated`, ...). Two scenes with equal signatures look identical.
fn drawing_signature(content: &str) -> Option<Vec<serde_json::Value>> {
    const KEYS: [&str; 15] = [
        "id", "type", "x", "y", "width", "height", "angle", "text", "strokeColor", "backgroundColor", "fillStyle", "strokeWidth",
        "opacity", "points", "fileId",
    ];
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let els = v.get("elements")?.as_array()?;
    Some(
        els.iter()
            .filter(|e| e.get("isDeleted").and_then(|d| d.as_bool()) != Some(true))
            .map(|e| {
                let mut m = serde_json::Map::new();
                for k in KEYS {
                    if let Some(val) = e.get(k) {
                        m.insert(k.to_string(), round_numbers(val));
                    }
                }
                serde_json::Value::Object(m)
            })
            .collect(),
    )
}

/// Round floats so tiny serialisation differences do not count as edits.
fn round_numbers(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Number(n) => n.as_f64().map(|f| serde_json::json!((f * 100.0).round() / 100.0)).unwrap_or_else(|| v.clone()),
        serde_json::Value::Array(a) => serde_json::Value::Array(a.iter().map(round_numbers).collect()),
        other => other.clone(),
    }
}

/// Do two scenes contain the same drawing? Unparseable input compares by text.
pub fn same_drawing(a: &str, b: &str) -> bool {
    match (drawing_signature(a), drawing_signature(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Lowest unused `whiteboard-N` title (same gap-filling rule as `new N`).
pub fn next_title<'a>(existing: impl Iterator<Item = &'a str> + Clone) -> String {
    let n = (1..).find(|n| !existing.clone().any(|t| t == format!("whiteboard-{n}") || t.starts_with(&format!("whiteboard-{n}.")))).unwrap();
    format!("whiteboard-{n}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_scene_is_valid_and_empty() {
        let s = empty_scene();
        assert!(serde_json::from_str::<serde_json::Value>(&s).is_ok());
        assert!(is_empty_scene(&s));
        assert!(!is_empty_scene(r#"{"elements":[{"id":"a"}]}"#));
        assert!(!is_empty_scene("not json"));
    }

    #[test]
    fn detects_whiteboard_files() {
        let p = |s: &str| std::path::PathBuf::from(s);
        assert!(is_whiteboard_file("{}", &p("a.excalidraw")));
        assert!(is_whiteboard_file("", &p("a.WHITEBOARD")));
        assert!(is_whiteboard_file(r#"{"__wb__":true}"#, &p("board.json")));
        assert!(is_whiteboard_file(r#"{"type":"excalidraw","elements":[]}"#, &p("x.json")));
        assert!(!is_whiteboard_file(r#"{"name":"package"}"#, &p("package.json")));
        assert!(!is_whiteboard_file("[1,2]", &p("data.json")));
        assert!(!is_whiteboard_file(r#"{"__wb__":true}"#, &p("notes.txt")));
    }

    #[test]
    fn same_drawing_ignores_viewport_and_bookkeeping() {
        let a = r#"{"elements":[{"id":"r","type":"rectangle","x":10,"y":20,"width":100,"height":50,"version":1,"updated":1}],"appState":{"scrollX":0}}"#;
        let scrolled = r#"{"elements":[{"id":"r","type":"rectangle","x":10,"y":20,"width":100,"height":50,"version":9,"updated":99}],"appState":{"scrollX":-300,"zoom":2}}"#;
        let moved = r#"{"elements":[{"id":"r","type":"rectangle","x":40,"y":20,"width":100,"height":50}]}"#;
        assert!(same_drawing(a, scrolled));
        assert!(!same_drawing(a, moved));
    }

    #[test]
    fn same_drawing_sees_adds_deletes_and_style() {
        let one = r##"{"elements":[{"id":"r","type":"rectangle","x":0,"y":0,"strokeColor":"#000"}]}"##;
        let two = r##"{"elements":[{"id":"r","type":"rectangle","x":0,"y":0,"strokeColor":"#000"},{"id":"e","type":"ellipse","x":5,"y":5}]}"##;
        let deleted = r##"{"elements":[{"id":"r","type":"rectangle","x":0,"y":0,"strokeColor":"#000"},{"id":"e","type":"ellipse","x":5,"y":5,"isDeleted":true}]}"##;
        let recoloured = r##"{"elements":[{"id":"r","type":"rectangle","x":0,"y":0,"strokeColor":"#f00"}]}"##;
        assert!(!same_drawing(one, two));
        assert!(same_drawing(one, deleted), "deleted elements are not visible");
        assert!(!same_drawing(one, recoloured));
        assert!(same_drawing(&empty_scene(), r#"{"elements":[],"appState":{"x":1}}"#));
    }

    #[test]
    fn titles_fill_gaps() {
        assert_eq!(next_title(["a.txt"].into_iter()), "whiteboard-1");
        assert_eq!(next_title(["whiteboard-1", "whiteboard-3"].into_iter()), "whiteboard-2");
        assert_eq!(next_title(["whiteboard-1.excalidraw"].into_iter()), "whiteboard-2");
    }
}
