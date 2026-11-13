//! The session file's format: the older app's tab JSON, read and written so both versions share one file.

use super::*;

/// Tab colours as earlier builds stored them (hex strings), by `workspace::TAB_COLORS` index: read back as those colours.
pub(super) const TAB_HEX: [&str; 8] = ["#e05555", "#e08030", "#c0a020", "#3dab60", "#2090a0", "#4488dd", "#8060c0", "#c060a0"];

pub(super) fn tab_colour_from_hex(hex: &str) -> Option<(u8, u8, u8)> {
    match TAB_HEX.iter().position(|h| h.eq_ignore_ascii_case(hex)) {
        Some(i) => Some(crate::workspace::TAB_COLORS[i].1),
        None => crate::settings::parse_hex(hex),
    }
}

pub(super) fn enc_label(key: &str) -> String {
    match key {
        "utf8" => "UTF-8",
        "utf8bom" => "UTF-8 BOM",
        "utf16le" => "UTF-16 LE",
        "utf16be" => "UTF-16 BE",
        "ansi" => "ANSI",
        other => other,
    }
    .into()
}

pub(super) fn enc_key(label: &str) -> String {
    match label {
        "UTF-8" => "utf8",
        "UTF-8 BOM" => "utf8bom",
        "UTF-16 LE" => "utf16le",
        "UTF-16 BE" => "utf16be",
        "ANSI" => "ansi",
        other => other,
    }
    .into()
}

pub(super) fn eol_label(key: &str) -> &'static str {
    match key {
        "lf" => "Unix (LF)",
        "cr" => "Macintosh (CR)",
        _ => "Windows (CR LF)",
    }
}

pub(super) fn eol_key(label: &str) -> &'static str {
    if label.contains("CR LF") || label.contains("CRLF") {
        "crlf"
    } else if label.contains("LF") {
        "lf"
    } else if label.contains("CR") {
        "cr"
    } else {
        "crlf"
    }
}

/// (the stored language id, the Language-menu entry)
pub(super) const LANG_IDS: [(&str, &str); 25] = [
    ("powershell", "PowerShell"),
    ("json", "JSON"),
    ("yaml", "YAML"),
    ("javascript", "JavaScript"),
    ("typescript", "TypeScript"),
    ("python", "Python"),
    ("markdown", "Markdown"),
    ("html", "HTML"),
    ("css", "CSS"),
    ("xml", "XML"),
    ("sql", "SQL"),
    ("csharp", "C#"),
    ("cpp", "C++"),
    ("c", "C"),
    ("java", "Java"),
    ("go", "Go"),
    ("rust", "Rust"),
    ("bat", "Batch"),
    ("shell", "Shell"),
    ("php", "PHP"),
    ("ruby", "Ruby"),
    ("lua", "Lua"),
    ("kotlin", "Kotlin"),
    ("ini", "ini file"),
    ("toml", "TOML"),
];

/// The language id stored with a tab, guessed from its file name.
pub(super) fn language_of(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "json" => "json",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" => "typescript",
        "py" => "python",
        "md" | "markdown" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "xml" => "xml",
        "yml" | "yaml" => "yaml",
        "rs" => "rust",
        "go" => "go",
        "java" => "java",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "cs" => "csharp",
        "ps1" => "powershell",
        "sh" => "shell",
        "sql" => "sql",
        "bat" | "cmd" => "bat",
        "php" => "php",
        "rb" => "ruby",
        "lua" => "lua",
        "kt" => "kotlin",
        "ini" => "ini",
        "mmd" | "mermaid" => "mermaid",
        _ => "plaintext",
    }
}

/// What the session file holds beyond the tab list. Other versions of the app ignore it.
#[derive(Serialize, Deserialize, Clone)]
pub(super) struct Extras {
    #[serde(default)]
    pub(super) wrap: bool,
    #[serde(default)]
    pub(super) dark: bool,
    #[serde(default = "yes")]
    pub(super) minimap: bool,
    #[serde(default)]
    pub(super) recent: Vec<String>,
    #[serde(default)]
    pub(super) favourites: Vec<String>,
    #[serde(default)]
    pub(super) workspace: Option<String>,
    #[serde(default)]
    pub(super) tree_hidden: bool,
    #[serde(default)]
    pub(super) search_engine: usize,
    #[serde(default)]
    pub(super) symbols: crate::view::Symbols,
    #[serde(default)]
    pub(super) date_format: String,
    #[serde(default)]
    pub(super) dock: Option<SessionDock>,
    #[serde(default)]
    pub(super) floats: Vec<super::SessionFloat>,
    #[serde(default)]
    pub(super) window: Option<super::FloatRect>,
}

/// Before this app has saved anything, take the look from the settings file.
pub(super) fn extras_from_settings() -> Extras {
    let root: serde_json::Value =
        std::fs::read_to_string(crate::settings::settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let flag = |k: &str, d: bool| root["ui"][k].as_bool().unwrap_or(d);
    Extras {
        wrap: flag("wordWrap", false),
        dark: flag("darkMode", false),
        minimap: flag("minimap", true),
        recent: root["recentFiles"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        favourites: Vec::new(),
        workspace: None,
        tree_hidden: false,
        search_engine: 0,
        symbols: crate::view::Symbols::default(),
        date_format: String::new(),
        dock: None,
        floats: Vec::new(),
        window: None,
    }
}

pub(super) fn tab_to_json(i: usize, t: &SessionTab, active: bool) -> serde_json::Value {
    let whiteboard = t.kind == "whiteboard";
    let image = t.kind == "image";
    let by_menu = t.lang.as_deref().and_then(|l| LANG_IDS.iter().find(|(_, label)| *label == l)).map(|(id, _)| *id);
    let language = if whiteboard {
        "whiteboard"
    } else if image {
        "image"
    } else {
        by_menu.unwrap_or_else(|| language_of(t.path.as_deref().unwrap_or(&t.title)))
    };
    let mut o = serde_json::json!({
        "id": i + 1,
        "name": t.title,
        "filePath": t.path,
        "content": if t.encrypted { None } else { t.text.clone() },
        "language": language,
        "encoding": enc_label(&t.enc),
        "eol": eol_label(&t.eol),
        "active": active,
        "dirty": t.text.is_some(),
        "pinned": t.pinned,
        "favourite": t.favourite,
        "color": t.color.map(crate::settings::to_hex),
        "caret": t.caret,
        "top": t.top,
    });
    if whiteboard {
        o["type"] = "whiteboard".into();
    }
    if image {
        o["type"] = "image".into();
    }
    if let Some(l) = &t.lang {
        o["nativeLanguage"] = l.clone().into();
    }
    if t.encrypted {
        // Only the encrypted copy of unsaved text is ever written.
        o["encrypted"] = true.into();
        o["envelope"] = t.text.clone().into();
    }
    o
}

pub(super) fn tab_from_json(v: &serde_json::Value) -> Option<SessionTab> {
    let (kind, language) = (v["type"].as_str().unwrap_or(""), v["language"].as_str().unwrap_or(""));
    // Diagrams and other tab types of the older app have no counterpart here.
    if matches!(kind, "drawio" | "game" | "diff" | "tasks" | "vault") || language == "drawio" {
        return None;
    }
    let whiteboard = kind == "whiteboard" || language == "whiteboard";
    let image = kind == "image";
    let path = v["filePath"].as_str().filter(|p| !p.is_empty()).map(String::from);
    let encrypted = v["encrypted"].as_bool().unwrap_or(false);
    let dirty = v["dirty"].as_bool().unwrap_or(false);
    let text = if encrypted {
        v["envelope"].as_str().map(String::from)
    } else if path.is_none() || dirty || path.as_deref().is_some_and(|p| !std::path::Path::new(p).exists()) {
        Some(v["content"].as_str().unwrap_or("").to_string())
    } else {
        None
    };
    let lang = if path.is_some() {
        None
    } else {
        v["nativeLanguage"].as_str().map(String::from).or_else(|| LANG_IDS.iter().find(|(id, _)| *id == language).map(|(_, l)| l.to_string()))
    };
    let title = v["name"].as_str().map(String::from).or_else(|| path.as_deref().map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()))?;
    Some(SessionTab {
        title,
        path,
        text,
        caret: crate::num::to_usize(v["caret"].as_u64().unwrap_or(0)),
        top: crate::num::to_usize(v["top"].as_u64().unwrap_or(0)),
        enc: enc_key(v["encoding"].as_str().unwrap_or("UTF-8")),
        eol: eol_key(v["eol"].as_str().unwrap_or("")).into(),
        pinned: v["pinned"].as_bool().unwrap_or(false),
        favourite: v["favourite"].as_bool().unwrap_or(false),
        color: v["color"].as_str().and_then(tab_colour_from_hex),
        encrypted,
        kind: if image {
            "image".into()
        } else if whiteboard {
            "whiteboard".into()
        } else {
            "text".into()
        },
        lang,
    })
}
