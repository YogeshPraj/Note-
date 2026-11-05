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

/// The picture editor page behind picture tabs.
#[derive(Default)]
pub struct ImageState {
    /// Tab whose picture the page currently shows.
    pub shown: Option<u64>,
    /// A picture pasted from the clipboard that the page has not been given yet: width, height, base64 RGBA pixels.
    pub pending: Option<(u32, u32, String)>,
}

/// The preview pane (Markdown, HTML, Mermaid, JSON, logs, HTTP).
#[derive(Default)]
pub struct PreviewState {
    pub on: bool,
    /// Whether the pane is actually beside the editor (it is not while a whiteboard is shown).
    pub split: bool,
    /// The pane fills the window and the editor is hidden.
    pub maximized: bool,
    /// The editor's top line when the page was last told where to scroll (None: tell it again).
    pub sent: Option<usize>,
    /// The top line the page itself asked for: the editor scrolling there is not told back to the page.
    pub applied: Option<usize>,
    /// (document, revision, size) last rendered, to skip identical refreshes.
    pub key: Option<(isize, usize, u64)>,
    /// Where the user left the divider, in thousandths of the width. Taking the pane away forgets the position, and a
    /// share still fits when the window has changed size by the time it comes back.
    pub share: Option<i32>,
}

impl PreviewState {
    /// Note where the divider is before the pane goes.
    pub fn keep_share(&mut self, sash: i32, width: i32) {
        if width > 0 && sash > 0 {
            self.share = Some(sash * 1000 / width);
        }
    }

    /// Where the divider goes in a `width` wide splitter: where it was left, or the middle at first.
    pub fn sash(&self, width: i32) -> i32 {
        self.share.map_or(width / 2, |share| width * share / 1000)
    }
}

/// Log Follow and the HTTP scratchpad, which both feed the preview pane.
#[derive(Default)]
pub struct StreamState {
    pub log: Option<(u64, Follower)>,
    pub log_shown: Option<u64>,
    pub http_last: Option<(isize, String)>,
    pub http_shown: Option<isize>,
    pub http_busy: bool,
    /// Where the worker thread leaves the response (document, JSON) for the stream timer.
    pub http_result: std::sync::Arc<std::sync::Mutex<Option<(isize, String)>>>,
    /// Addresses the user agreed to send vault values to, this session.
    pub vault_origins: std::collections::HashSet<String>,
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
    /// A Find in Files running on a worker thread.
    pub job: Option<FindInFilesJob>,
}

/// What a Find in Files worker returns: the hits, and whether the whole folder was searched.
pub type FindInFilesOutcome = Result<(Vec<notepp_core::search::Hit>, bool), notepp_core::error::Error>;

/// A Find in Files on a worker thread: its progress (and stop request), and where it leaves its outcome.
pub struct FindInFilesJob {
    pub dir: std::path::PathBuf,
    pub progress: std::sync::Arc<notepp_core::search::SearchProgress>,
    pub outcome: std::sync::Arc<std::sync::Mutex<Option<FindInFilesOutcome>>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_divider_comes_back_where_it_was_left() {
        let mut p = PreviewState::default();
        assert_eq!(p.sash(1000), 500, "the middle until the user moves it");
        p.keep_share(300, 1000);
        assert_eq!(p.sash(1000), 300);
        assert_eq!(p.sash(2000), 600, "the same share of a window that has grown meanwhile");
        p.keep_share(0, 1000);
        p.keep_share(400, 0);
        assert_eq!(p.sash(1000), 300, "a pane that was not laid out says nothing");
    }
}
