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
    /// The colour mark of the tab.
    #[serde(default)]
    pub color: Option<(u8, u8, u8)>,
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

/// Which side of the main editor a docked tab is on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockSide {
    Left,
    Right,
    Top,
    Bottom,
}

impl DockSide {
    pub const ALL: [DockSide; 4] = [DockSide::Left, DockSide::Right, DockSide::Top, DockSide::Bottom];
}

/// The docked tab as saved: its place in `Session::tabs`, its side, and its share of the space in thousandths.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionDock {
    pub tab: usize,
    pub side: DockSide,
    pub share: u32,
}

/// Where a floating window is on the screen.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloatRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    #[serde(default)]
    pub maximized: bool,
}

/// A floating window as saved: its tab's place in `Session::tabs` and where it was.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionFloat {
    pub tab: usize,
    pub rect: FloatRect,
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
    /// The user closed the workspace tree: it stays closed on the next start.
    #[serde(default)]
    pub tree_hidden: bool,
    /// Which web search engine "Search on Internet" uses.
    #[serde(default)]
    pub search_engine: usize,
    /// Show Symbol options of the View menu.
    #[serde(default)]
    pub symbols: super::view::Symbols,
    /// Picture for Insert > Date Time (customized); empty means the default.
    #[serde(default)]
    pub date_format: String,
    /// Saved tabs this app cannot show (the older app's diagrams, games, ...), exactly as read: they are written back
    /// unchanged, so whatever unsaved content they hold is never destroyed.
    #[serde(default)]
    pub kept: Vec<serde_json::Value>,
    /// The tab docked beside the main editor.
    #[serde(default)]
    pub dock: Option<SessionDock>,
    /// The tabs in floating windows.
    #[serde(default)]
    pub floats: Vec<SessionFloat>,
    /// Where the main window was, and whether it was maximised (then the place it goes back to from there).
    #[serde(default)]
    pub window: Option<FloatRect>,
}

fn yes() -> bool {
    true
}

static STANDALONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The ticket of the last snapshot written, held while writing: session writes come from the autosave's worker thread
/// and from closing the window, and must go one at a time, never an older snapshot over a newer one.
static WRITTEN: std::sync::Mutex<u64> = std::sync::Mutex::new(0);
static NEXT_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A number for a snapshot about to be taken; later snapshots get larger ones.
pub fn ticket() -> u64 {
    NEXT_TICKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

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

mod format;
use format::*;

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
    let has_own = std::fs::read_to_string(super::settings::settings_path())
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .is_some_and(|v| v.get("font").is_some());
    if !has_own
        && let Some(old) = std::fs::read_to_string(legacy.join("settings.json")).ok().and_then(|t| serde_json::from_str::<super::settings::Settings>(&t).ok())
    {
        let _ = old.save();
    }
    let _ = std::fs::write(&marker, "");
}

impl Session {
    pub fn load(path: &std::path::Path) -> Option<Session> {
        let root: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
        let mut tabs = Vec::new();
        let mut kept = Vec::new();
        let mut active = 0;
        for v in root.get("tabs")?.as_array()? {
            if let Some(t) = tab_from_json(v) {
                if v["active"].as_bool() == Some(true) {
                    active = tabs.len();
                }
                tabs.push(t);
            } else {
                kept.push(v.clone());
            }
        }
        let ex = root.get("native").and_then(|n| serde_json::from_value::<Extras>(n.clone()).ok()).unwrap_or_else(extras_from_settings);
        Some(Session {
            tabs,
            active,
            wrap: ex.wrap,
            dark: ex.dark,
            minimap: ex.minimap,
            recent: ex.recent,
            workspace: ex.workspace,
            tree_hidden: ex.tree_hidden,
            search_engine: ex.search_engine,
            symbols: ex.symbols,
            date_format: ex.date_format,
            kept,
            dock: ex.dock,
            floats: ex.floats,
            window: ex.window,
        })
    }

    /// Write the snapshot that got `ticket` ([`ticket`]), unless a newer one has been written already. Safe to call from
    /// several threads: writes go one at a time.
    ///
    /// # Errors
    /// The I/O error of the write.
    pub fn save_in_order(&self, path: &std::path::Path, ticket: u64) -> std::io::Result<()> {
        let mut written = WRITTEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if ticket <= *written {
            return Ok(());
        }
        self.save(path)?;
        *written = ticket;
        Ok(())
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut tabs: Vec<_> = self.tabs.iter().enumerate().map(|(i, t)| tab_to_json(i, t, i == self.active)).collect();
        for v in &self.kept {
            let mut v = v.clone();
            v["id"] = (tabs.len() + 1).into();
            v["active"] = false.into();
            tabs.push(v);
        }
        let extras = Extras {
            wrap: self.wrap,
            dark: self.dark,
            minimap: self.minimap,
            recent: self.recent.clone(),
            workspace: self.workspace.clone(),
            tree_hidden: self.tree_hidden,
            search_engine: self.search_engine,
            symbols: self.symbols,
            date_format: self.date_format.clone(),
            dock: self.dock,
            floats: self.floats.clone(),
            window: self.window,
        };
        let root = serde_json::json!({ "tabs": tabs, "native": extras });
        // Write-then-rename so a crash mid-write can't leave a truncated session.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&root).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

#[cfg(test)]
mod tests;
