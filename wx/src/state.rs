//! Cohesive pieces of shell state, owned by `App` instead of loose fields.

use notepp_core::logfollow::Follower;

use crate::whiteboard::Pending;

/// The Excalidraw page behind whiteboard tabs.
#[derive(Default)]
pub struct WhiteboardState {
    /// Tab whose scene the page currently shows.
    pub shown: Option<u64>,
    /// What to do once the page hands back its latest scene.
    pub pending: Option<Pending>,
}

/// The preview pane (Markdown, HTML, Mermaid, JSON, logs, HTTP).
#[derive(Default)]
pub struct PreviewState {
    pub on: bool,
    /// (document, revision, size) last rendered, to skip identical refreshes.
    pub key: Option<(isize, usize, u64)>,
}

/// Log Follow and the HTTP scratchpad, which both feed the preview pane.
#[derive(Default)]
pub struct StreamState {
    pub log: Option<(u64, Follower)>,
    pub log_shown: Option<u64>,
    pub http_last: Option<(isize, String)>,
    pub http_shown: Option<isize>,
    pub http_busy: bool,
    /// Tabs being tailed (Monitoring) and the file length last seen.
    pub monitoring: std::collections::HashMap<u64, u64>,
}

/// The last Find-all / Find-in-files listing, for stepping with F4 / Shift+F4.
#[derive(Default)]
pub struct SearchResults {
    pub hits: Vec<notepp_core::search::Hit>,
    pub cursor: usize,
    /// Folder of the last Find in Files; empty after a current-document search.
    pub root: std::path::PathBuf,
}

/// A password put on the clipboard.
#[derive(Default)]
pub struct ClipboardState {
    /// Digest of the secret and when it was copied: cleared after 30 seconds and kept out of the history.
    pub secret: Option<(Vec<u8>, std::time::Instant)>,
}