//! The workspace's vocabulary: tabs, operations, the `Ui` the shell implements, and small helpers.

use super::*;

#[derive(Debug)]
pub enum Confirm {
    Save,
    Discard,
    Cancel,
}

/// Everything the workspace needs from the host: dialogs and error display.
pub trait Ui {
    fn pick_open(&self) -> Vec<PathBuf>;
    fn pick_save(&self, suggested: &str) -> Option<PathBuf>;
    fn confirm_save(&self, name: &str) -> Confirm;
    fn ask_yes_no(&self, msg: &str) -> bool;
    /// Move a file to the Recycle Bin.
    fn recycle(&self, path: &Path) -> Result<(), crate::error::Error>;
    fn error(&self, msg: &str);
    /// Ask for the profile's password (or recovery key). On success `vault.dek` is set.
    fn unlock(&self, vault: &mut crypto::Vault, what: &str) -> bool;
    /// First-time encryption setup: choose a password and show the recovery key. On success the
    /// profile exists, is saved, and is unlocked.
    fn setup_encryption(&self, vault: &mut crypto::Vault) -> bool;
}

/// Which tabs a "Close Multiple Documents" command targets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CloseOp {
    All,
    AllButActive,
    ToTheLeft,
    ToTheRight,
    Unchanged,
}

/// Indices (in tab order) selected by `op`, given the active tab and which tabs are modified.
pub fn close_targets(op: CloseOp, active: usize, dirty: &[bool]) -> Vec<usize> {
    (0..dirty.len())
        .filter(|&i| match op {
            CloseOp::All => true,
            CloseOp::AllButActive => i != active,
            CloseOp::ToTheLeft => i < active,
            CloseOp::ToTheRight => i > active,
            CloseOp::Unchanged => !dirty[i],
        })
        .collect()
}

/// What each tab is sorted on.
#[derive(Clone, Debug, PartialEq)]
pub struct SortKeys {
    pub name: String,
    /// Full path ("" for unsaved tabs); also the tie-breaker.
    pub path: String,
    pub kind: String,
    pub len: usize,
    /// Seconds since the epoch of the file's last change (0 when there is no file).
    pub modified: u64,
}

/// What Window > Sort By can sort the tabs by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SortField {
    Name,
    Path,
    Type,
    Length,
    Modified,
}

/// New tab order (old indices) when sorting by `field`; ties fall back to the full path.
pub fn sort_order(keys: &[SortKeys], field: SortField, descending: bool) -> Vec<usize> {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| {
        let (ka, kb) = (&keys[a], &keys[b]);
        let lower = |s: &str| s.to_lowercase();
        let ord = match field {
            SortField::Name => lower(&ka.name).cmp(&lower(&kb.name)),
            SortField::Path => lower(&ka.path).cmp(&lower(&kb.path)),
            SortField::Type => lower(&ka.kind).cmp(&lower(&kb.kind)),
            SortField::Length => ka.len.cmp(&kb.len),
            SortField::Modified => ka.modified.cmp(&kb.modified),
        }
        .then_with(|| lower(&ka.path).cmp(&lower(&kb.path)));
        if descending { ord.reverse() } else { ord }
    });
    order
}

/// Push `path` to the front of a most-recent-first list without duplicates, capped at `max`.
pub fn push_recent(list: &mut Vec<PathBuf>, path: &Path, max: usize) {
    list.retain(|r| !same_path(r, path));
    list.insert(0, path.to_path_buf());
    list.truncate(max);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabKind {
    Text,
    /// An Excalidraw board: no Scintilla document, `content` holds the scene JSON.
    Whiteboard,
}

#[derive(Debug)]
pub struct Tab {
    /// Stable identity (indices shift when tabs close).
    pub id: u64,
    pub kind: TabKind,
    pub title: String,
    pub path: Option<PathBuf>,
    pub doc: isize,
    /// Scene JSON for whiteboards; unused for text tabs.
    pub content: String,
    pub dirty: bool,
    /// Set when something other than the text changed (e.g. encoding) so the tab shows as modified.
    pub forced_dirty: bool,
    pub enc: Enc,
    pub eol: Eol,
    pub lang: &'static str,
    /// The Language-menu entry in effect (several share one lexer, e.g. C and Java).
    pub lang_label: &'static str,
    /// Saved as an encrypted file (see `crypto`); the editor always shows the plain text.
    pub encrypted: bool,
    /// Pinned tabs sit at the left, show no close button and survive the Close All commands.
    pub pinned: bool,
    /// The colour mark: one of `TAB_COLORS` or a custom colour.
    pub color: Option<(u8, u8, u8)>,
    pub caret: usize,
    /// The document line at the top of the view (not a display line: wrapped lines count once).
    pub top: usize,
    /// The file's modification time and size when it was last read or written, to notice
    /// changes made by other programs.
    pub disk: Option<Stamp>,
}

pub fn stamp_of(path: &Path) -> Option<Stamp> {
    let m = std::fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

/// What the tab strip needs to draw one tab.
#[derive(Debug)]
pub struct TabInfo {
    pub title: String,
    pub dirty: bool,
    /// Small pill shown before the title (e.g. "wb" for whiteboards).
    pub badge: Option<&'static str>,
    pub pinned: bool,
    /// The colour mark of the tab.
    pub color: Option<(u8, u8, u8)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LineOp {
    Sort(edit::SortKind, bool),
    RemoveDuplicates,
    RemoveConsecutiveDuplicates,
    RemoveEmpty,
    RemoveBlank,
    Reverse,
    Randomize,
    Join,
    Split,
    TrimTrailing,
    TrimLeading,
    TrimBoth,
    EolToSpace,
    RemoveUnnecessaryBlanks,
    TabsToSpaces,
    SpacesToTabsAll,
    SpacesToTabsLeading,
    ToggleComment,
    Comment,
    Uncomment,
    BlockComment,
    BlockUncomment,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CaseOp {
    Proper,
    ProperBlend,
    Sentence,
    SentenceBlend,
    Invert,
    Random,
}

/// Which text conversion Tools > Text Tools applies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TextTool {
    Base64Encode,
    Base64Decode,
    UrlEncode,
    UrlDecode,
    JsonPretty,
    JsonMinify,
    XmlPretty,
    XmlMinify,
}

/// Which auto-completion source to use.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Completion {
    Words,
    Functions,
    Path,
    Hint,
}

/// Which path text "Copy to Clipboard" puts on the clipboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathText {
    FullPath,
    FileName,
    Directory,
    AllNames,
    AllPaths,
}

#[derive(Debug)]
pub struct Status {
    pub position: String,
    pub lines: String,
    pub length: String,
    pub eol: String,
    pub enc: String,
    pub lang: String,
}

pub(super) fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub(super) fn lang_for(path: &Path) -> &'static str {
    match path.file_name().and_then(|n| n.to_str()) {
        Some("Makefile" | "makefile" | "GNUmakefile") => return "makefile",
        Some("CMakeLists.txt") => return "cmake",
        Some(".bashrc" | ".bash_profile" | ".bash_login" | ".bash_logout" | ".profile" | ".zshrc" | ".zprofile" | ".zshenv" | "bashrc" | "profile") => {
            return "bash";
        }
        Some("Rakefile" | "Gemfile" | "Vagrantfile" | "Guardfile" | "Podfile") => return "ruby",
        _ => {}
    }
    path.extension().and_then(|e| e.to_str()).map_or("null", lexer::lexer_for_extension)
}

/// The first `lines` non-trailing lines of `text`, each cut to `width` characters (tabs become four spaces),
/// with "..." when there is more.
pub(super) fn preview_lines(text: &str, lines: usize, width: usize) -> String {
    let all: Vec<&str> = text.trim_end().lines().collect();
    let mut out: Vec<String> = all
        .iter()
        .take(lines)
        .map(|l| {
            let l = l.replace('\t', "    ");
            if l.chars().count() > width { format!("{}...", l.chars().take(width).collect::<String>()) } else { l }
        })
        .collect();
    if all.len() > lines {
        out.push("...".into());
    }
    out.join("\n")
}

pub(super) fn file_title(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// 1234567 -> "1,234,567" (as the status bar shows numbers).
pub(super) fn commafy(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub(super) fn unmappable_warning(enc: Enc) -> String {
    format!("Some characters cannot be represented in {} and would be saved as ?.\n\nContinue anyway?", enc.label())
}

pub(super) fn scene_element_count(content: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|v| {
            v.get("elements")
                .and_then(|e| e.as_array())
                .map(|e| e.iter().filter(|x| x.get("isDeleted").and_then(serde_json::Value::as_bool) != Some(true)).count())
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_preview_keeps_the_first_lines_cuts_long_ones_and_says_when_there_is_more() {
        assert_eq!(preview_lines("a\n\tb\n\n", 5, 10), "a\n    b");
        assert_eq!(preview_lines("abcdefghij", 5, 4), "abcd...");
        assert_eq!(preview_lines("1\n2\n3\n4", 2, 9), "1\n2\n...");
        assert_eq!(preview_lines("", 3, 9), "");
    }
}
