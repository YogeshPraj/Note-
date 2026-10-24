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
    /// "text" (default) or "whiteboard".
    #[serde(default = "text_kind")]
    pub kind: String,
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
}

fn yes() -> bool {
    true
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("notepp-native")
}

pub fn session_path() -> PathBuf {
    config_dir().join("session.json")
}

impl Session {
    pub fn load(path: &std::path::Path) -> Option<Session> {
        serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Write-then-rename so a crash mid-write can't leave a truncated session.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
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
                kind: "text".into(),
            }],
            active: 0,
            wrap: true,
            dark: false,
            minimap: true,
            recent: vec!["C:\\a\\b.txt".into()],
            workspace: Some("C:\\proj".into()),
        };
        let p = std::env::temp_dir().join("notepp_session_test.json");
        s.save(&p).unwrap();
        assert_eq!(Session::load(&p), Some(s));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn corrupt_session_is_ignored() {
        let p = std::env::temp_dir().join("notepp_session_bad.json");
        std::fs::write(&p, "{ not json").unwrap();
        assert_eq!(Session::load(&p), None);
        let _ = std::fs::remove_file(&p);
    }
}
