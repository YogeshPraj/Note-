//! Session persistence: which tabs were open, plus unsaved text, so a restart
//! (or a crash-free exit with unsaved work) brings everything back.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionTab {
    pub title: String,
    pub path: Option<String>,
    /// Present only when the buffer differs from disk or has never been saved.
    pub text: Option<String>,
    pub caret: usize,
    pub top: usize,
    pub enc: String,
    pub eol: String,
    /// Pinned tabs stay at the left and are skipped by the Close All commands.
    #[serde(default)]
    pub pinned: bool,
    /// Index into `workspace::TAB_COLORS`.
    #[serde(default)]
    pub color: Option<u8>,
    /// The tab is an encrypted file; `text` (if any) then holds the encrypted envelope, never plain text.
    #[serde(default)]
    pub encrypted: bool,
    /// "text" (default) or "whiteboard".
    #[serde(default = "text_kind")]
    pub kind: String,
    /// Language-menu entry of a tab that has no file (a saved file's language comes from its name).
    #[serde(default)]
    pub lang: Option<String>,
}

fn text_kind() -> String {
    "text".into()
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Session {
    pub tabs: Vec<SessionTab>,
    pub active: usize,
    pub wrap: bool,
    #[serde(default)]
    pub dark: bool,
    #[serde(default = "yes")]
    pub minimap: bool,
    #[serde(default)]
    pub recent: Vec<String>,
    /// Folder open in the workspace tree.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Which web search engine "Search on Internet" uses.
    #[serde(default)]
    pub search_engine: usize,
    /// Show Symbol options of the View menu.
    #[serde(default)]
    pub symbols: super::view::Symbols,
    /// Picture for Insert > Date Time (customized); empty means the default.
    #[serde(default)]
    pub date_format: String,
}

fn yes() -> bool {
    true
}

static STANDALONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A "new instance" (View > Move/Open in New Instance) neither restores nor
/// overwrites the saved session, so it cannot clobber the main window's.
pub fn set_standalone() {
    STANDALONE.store(true, std::sync::atomic::Ordering::Relaxed);
}

pub fn standalone() -> bool {
    STANDALONE.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn config_dir() -> PathBuf {
    app_data().join("notepp")
}

fn app_data() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn session_path() -> PathBuf {
    config_dir().join("autosave").join("session.json")
}

/// Tab colours as the app has always stored them (hex strings), by `workspace::TAB_COLORS` index.
const TAB_HEX: [&str; 8] = ["#e05555", "#e08030", "#c0a020", "#3dab60", "#2090a0", "#4488dd", "#8060c0", "#c060a0"];

fn enc_label(key: &str) -> String {
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

fn enc_key(label: &str) -> String {
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

fn eol_label(key: &str) -> &'static str {
    match key {
        "lf" => "Unix (LF)",
        "cr" => "Macintosh (CR)",
        _ => "Windows (CR LF)",
    }
}

fn eol_key(label: &str) -> &'static str {
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
const LANG_IDS: [(&str, &str); 25] = [
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
fn language_of(name: &str) -> &'static str {
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
struct Extras {
    #[serde(default)]
    wrap: bool,
    #[serde(default)]
    dark: bool,
    #[serde(default = "yes")]
    minimap: bool,
    #[serde(default)]
    recent: Vec<String>,
    #[serde(default)]
    workspace: Option<String>,
    #[serde(default)]
    search_engine: usize,
    #[serde(default)]
    symbols: super::view::Symbols,
    #[serde(default)]
    date_format: String,
}

/// Before this app has saved anything, take the look from the settings file.
fn extras_from_settings() -> Extras {
    let root: serde_json::Value = std::fs::read_to_string(super::settings::settings_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let flag = |k: &str, d: bool| root["ui"][k].as_bool().unwrap_or(d);
    Extras {
        wrap: flag("wordWrap", false),
        dark: flag("darkMode", false),
        minimap: flag("minimap", true),
        recent: root["recentFiles"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
        workspace: None,
        search_engine: 0,
        symbols: Default::default(),
        date_format: String::new(),
    }
}

fn tab_to_json(i: usize, t: &SessionTab, active: bool) -> serde_json::Value {
    let whiteboard = t.kind == "whiteboard";
    let by_menu = t.lang.as_deref().and_then(|l| LANG_IDS.iter().find(|(_, label)| *label == l)).map(|(id, _)| *id);
    let language = if whiteboard { "whiteboard" } else { by_menu.unwrap_or_else(|| language_of(t.path.as_deref().unwrap_or(&t.title))) };
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
        "color": t.color.and_then(|c| TAB_HEX.get(c as usize)),
        "caret": t.caret,
        "top": t.top,
    });
    if whiteboard {
        o["type"] = "whiteboard".into();
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

fn tab_from_json(v: &serde_json::Value) -> Option<SessionTab> {
    let (kind, language) = (v["type"].as_str().unwrap_or(""), v["language"].as_str().unwrap_or(""));
    // Diagrams and other tab types of the older app have no counterpart here.
    if matches!(kind, "drawio" | "game" | "diff" | "tasks" | "vault") || language == "drawio" {
        return None;
    }
    let whiteboard = kind == "whiteboard" || language == "whiteboard";
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
    let lang = if path.is_some() { None } else { v["nativeLanguage"].as_str().map(String::from).or_else(|| LANG_IDS.iter().find(|(id, _)| *id == language).map(|(_, l)| l.to_string())) };
    let title = v["name"].as_str().map(String::from).or_else(|| path.as_deref().map(|p| p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()))?;
    Some(SessionTab {
        title,
        path,
        text,
        caret: v["caret"].as_u64().unwrap_or(0) as usize,
        top: v["top"].as_u64().unwrap_or(0) as usize,
        enc: enc_key(v["encoding"].as_str().unwrap_or("UTF-8")),
        eol: eol_key(v["eol"].as_str().unwrap_or("")).into(),
        pinned: v["pinned"].as_bool().unwrap_or(false),
        color: v["color"].as_str().and_then(|c| TAB_HEX.iter().position(|h| h.eq_ignore_ascii_case(c))).map(|i| i as u8),
        encrypted,
        kind: if whiteboard { "whiteboard".into() } else { "text".into() },
        lang,
    })
}

/// The first time the app starts after earlier builds kept their files in `notepp-native`,
/// bring the preferences and right-click menu over to the shared folder.
pub fn migrate_legacy_native() {
    let legacy = app_data().join("notepp-native");
    let dir = config_dir();
    let marker = dir.join(".native-imported");
    if !legacy.is_dir() || marker.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    let menu = legacy.join("contextmenu.txt");
    if menu.is_file() && !dir.join("contextmenu.txt").exists() {
        let _ = std::fs::copy(&menu, dir.join("contextmenu.txt"));
    }
    let has_own = std::fs::read_to_string(super::settings::settings_path()).ok().and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()).is_some_and(|v| v.get("font").is_some());
    if !has_own {
        if let Some(old) = std::fs::read_to_string(legacy.join("settings.json")).ok().and_then(|t| serde_json::from_str::<super::settings::Settings>(&t).ok()) {
            let _ = old.save();
        }
    }
    let _ = std::fs::write(&marker, "");
}

impl Session {
    pub fn load(path: &std::path::Path) -> Option<Session> {
        let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let mut tabs = Vec::new();
        let mut active = 0;
        for v in root.get("tabs")?.as_array()? {
            if let Some(t) = tab_from_json(v) {
                if v["active"].as_bool() == Some(true) {
                    active = tabs.len();
                }
                tabs.push(t);
            }
        }
        let ex = root.get("native").and_then(|n| serde_json::from_value::<Extras>(n.clone()).ok()).unwrap_or_else(extras_from_settings);
        Some(Session { tabs, active, wrap: ex.wrap, dark: ex.dark, minimap: ex.minimap, recent: ex.recent, workspace: ex.workspace, search_engine: ex.search_engine, symbols: ex.symbols, date_format: ex.date_format })
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tabs: Vec<_> = self.tabs.iter().enumerate().map(|(i, t)| tab_to_json(i, t, i == self.active)).collect();
        let extras = Extras {
            wrap: self.wrap,
            dark: self.dark,
            minimap: self.minimap,
            recent: self.recent.clone(),
            workspace: self.workspace.clone(),
            search_engine: self.search_engine,
            symbols: self.symbols,
            date_format: self.date_format.clone(),
        };
        let root = serde_json::json!({ "tabs": tabs, "native": extras });
        // Write-then-rename so a crash mid-write can't leave a truncated session.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&root).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trips() {
        let s = Session {
            tabs: vec![SessionTab {
                title: "new 1".into(),
                path: None,
                text: Some("héllo\nworld".into()),
                caret: 3,
                top: 1,
                enc: "utf8".into(),
                eol: "lf".into(),
                encrypted: false,
                pinned: false,
                color: None,
                kind: "text".into(),
                lang: Some("JSON".into()),
            }],
            active: 0,
            wrap: true,
            dark: false,
            minimap: true,
            recent: vec!["C:\\a\\b.txt".into()],
            workspace: Some("C:\\proj".into()),
            search_engine: 2,
            symbols: Default::default(),
            date_format: "yyyy".into(),
        };
        let p = std::env::temp_dir().join("notepp_session_test.json");
        s.save(&p).unwrap();
        assert_eq!(Session::load(&p), Some(s));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn reads_a_session_written_by_the_previous_version_of_the_app() {
        let json = r##"{"tabs":[
            {"id":1,"name":"a.json","filePath":"C:\\x\\a.json","content":"{}","language":"json","encoding":"UTF-8","eol":"Windows (CR LF)","active":false,"dirty":false,"pinned":true,"color":"#4488dd"},
            {"id":2,"name":"new 1","filePath":null,"content":"hello","language":"plaintext","encoding":"UTF-16 LE","eol":"Unix (LF)","active":true,"dirty":true,"pinned":false,"color":null},
            {"id":3,"name":"board","filePath":null,"content":"{}","language":"whiteboard","type":"whiteboard","encoding":"UTF-8","eol":"Windows (CR LF)","active":false,"dirty":true},
            {"id":4,"name":"d.drawio","filePath":null,"content":"<x/>","language":"drawio","type":"drawio","active":false}
        ]}"##;
        let p = std::env::temp_dir().join("notepp_session_old.json");
        std::fs::write(&p, json).unwrap();
        let s = Session::load(&p).unwrap();
        let _ = std::fs::remove_file(&p);
        assert_eq!(s.tabs.len(), 3, "the diagram tab has no counterpart");
        assert_eq!(s.active, 1);
        assert_eq!(s.tabs[0].text.as_deref(), Some("{}"), "a file that is gone keeps the stored text");
        let here = serde_json::json!({"tabs":[{"name":"Cargo.toml","filePath":concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),"content":"stale","dirty":false}]});
        assert_eq!(tab_from_json(&here["tabs"][0]).unwrap().text, None, "a clean file is read from disk");
        assert!(s.tabs[0].pinned && s.tabs[0].color == Some(5));
        assert_eq!((s.tabs[1].text.as_deref(), s.tabs[1].enc.as_str(), s.tabs[1].eol.as_str()), (Some("hello"), "utf16le", "lf"));
        assert_eq!(s.tabs[2].kind, "whiteboard");
    }

    #[test]
    fn what_is_written_keeps_the_older_shape() {
        let t = SessionTab { title: "n.py".into(), path: Some("C:\\n.py".into()), text: Some("x".into()), caret: 1, top: 0, enc: "utf8bom".into(), eol: "cr".into(), pinned: false, color: Some(0), encrypted: false, kind: "text".into(), lang: None };
        let v = tab_to_json(0, &t, true);
        assert_eq!((v["id"].as_u64(), v["name"].as_str(), v["filePath"].as_str(), v["content"].as_str()), (Some(1), Some("n.py"), Some("C:\\n.py"), Some("x")));
        assert_eq!((v["language"].as_str(), v["encoding"].as_str(), v["eol"].as_str(), v["color"].as_str(), v["active"].as_bool(), v["dirty"].as_bool()), (Some("python"), Some("UTF-8 BOM"), Some("Macintosh (CR)"), Some("#e05555"), Some(true), Some(true)));
        let back = tab_from_json(&v).unwrap();
        assert_eq!(back, t);
        let enc = SessionTab { encrypted: true, text: Some("{\"env\":1}".into()), ..t };
        let v = tab_to_json(0, &enc, false);
        assert!(v["content"].is_null() && v["encrypted"] == true, "protected text never goes into `content`");
        assert_eq!(tab_from_json(&v).unwrap(), enc);
    }

    #[test]
    fn corrupt_session_is_ignored() {
        let p = std::env::temp_dir().join("notepp_session_bad.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Session::load(&p), None);
        let _ = std::fs::remove_file(&p);
    }
}
