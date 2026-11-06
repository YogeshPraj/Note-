//! Who may do what: the access levels, the token, and which files an agent may reach.

use std::path::{Component, Path, PathBuf};

use serde_json::json;

use crate::error::Error;
use crate::session::config_dir;

/// What the user lets agents do. Ordered: a higher level includes the lower ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// List, read, search, open and switch tabs. No document or file changes.
    Read,
    /// Also change documents and save them, and run the commands that work on text.
    Edit,
    /// Also run the commands that change the window and its tabs, and close unchanged tabs.
    Full,
}

impl Level {
    /// The levels in the order Settings shows them.
    pub const ALL: [Level; 3] = [Level::Read, Level::Edit, Level::Full];

    /// The key kept in the settings. Anything unknown means the lowest level.
    pub fn from_key(key: &str) -> Level {
        match key {
            "edit" => Level::Edit,
            "full" => Level::Full,
            _ => Level::Read,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Level::Read => "read",
            Level::Edit => "edit",
            Level::Full => "full",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Level::Read => "Read only",
            Level::Edit => "Edit",
            Level::Full => "Full",
        }
    }
}

/// Equal, in a time that does not depend on where the first difference is.
pub fn same_secret(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

const HEX: &[u8; 16] = b"0123456789abcdef";

fn token_path() -> PathBuf {
    config_dir().join("mcp-token")
}

fn write_token(path: &Path, token: &str) -> Result<(), Error> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Agent(format!("Cannot keep the token: {e}")))?;
    }
    crate::files::atomic_write(path, token.as_bytes()).map_err(|e| Error::Agent(format!("Cannot keep the token: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| Error::Agent(format!("Cannot protect the token: {e}")))?;
    }
    Ok(())
}

/// `N` random bytes, as text.
///
/// # Errors
/// No secure random numbers.
pub fn random_hex<const N: usize>() -> Result<String, Error> {
    let mut raw = [0u8; N];
    getrandom::fill(&mut raw).map_err(|e| Error::Random(e.to_string()))?;
    let mut text = String::with_capacity(N * 2);
    for byte in raw {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 15)]));
    }
    Ok(text)
}

/// Make a new token and keep it, replacing any old one.
///
/// # Errors
/// No secure random numbers, or the file cannot be written.
pub fn new_token() -> Result<String, Error> {
    let token = random_hex::<32>()?;
    write_token(&token_path(), &token)?;
    Ok(token)
}

/// The token agents must show, made on first use. It has its own file: `settings.json` may be shared or backed up.
///
/// # Errors
/// As [`new_token`].
pub fn token() -> Result<String, Error> {
    match std::fs::read_to_string(token_path()) {
        Ok(t) if t.trim().len() >= 32 => Ok(t.trim().to_string()),
        _ => new_token(),
    }
}

/// What the user pastes into an agent's configuration.
pub fn setup_text(port: u16, token: &str) -> String {
    let setup = json!({ "mcpServers": { "notepp": {
        "type": "http",
        "url": format!("http://127.0.0.1:{port}/mcp"),
        "headers": { "Authorization": format!("Bearer {token}") },
    } } });
    serde_json::to_string_pretty(&setup).unwrap_or_default()
}

/// The files an agent may reach: those open in tabs, and those in the workspace folder. Never the Note++ profile.
#[derive(Debug)]
pub struct Reach<'a> {
    pub workspace: Option<&'a Path>,
    pub open: Vec<&'a Path>,
    pub profile: &'a Path,
}

fn refused(path: &Path) -> Error {
    Error::Agent(format!("{} is not open in Note++ and not in the workspace folder. Ask the user to open it or its folder.", path.display()))
}

fn network_path(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.starts_with(r"\\") || text.starts_with("//")
}

/// The real path: the file's, or for a file yet to be written, its folder's plus its name.
fn resolved(path: &Path) -> Option<PathBuf> {
    match std::fs::canonicalize(path) {
        Ok(p) => Some(p),
        Err(_) => Some(std::fs::canonicalize(path.parent()?).ok()?.join(path.file_name()?)),
    }
}

impl Reach<'_> {
    /// Check `path`, and give the path to use.
    ///
    /// # Errors
    /// The path is relative, climbs with `..`, or is outside what the agent may reach. A network path outside the
    /// workspace is refused before the disk is touched, as just looking at one sends the user's credentials to its host.
    pub fn check(&self, path: &Path) -> Result<PathBuf, Error> {
        if !path.is_absolute() {
            return Err(Error::Agent(format!("{} is not a full path.", path.display())));
        }
        if path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(refused(path));
        }
        if self.open.contains(&path) {
            return Ok(path.to_path_buf());
        }
        if network_path(path) && !self.workspace.is_some_and(|w| path.starts_with(w)) {
            return Err(refused(path));
        }
        let real = resolved(path).ok_or_else(|| refused(path))?;
        let is_in = |folder: &Path| std::fs::canonicalize(folder).is_ok_and(|f| real.starts_with(f));
        if self.open.iter().any(|p| std::fs::canonicalize(p).is_ok_and(|p| p == real)) {
            return Ok(path.to_path_buf());
        }
        if is_in(self.profile) || !self.workspace.is_some_and(is_in) {
            return Err(refused(path));
        }
        // The path as given: on Windows the real one starts with \?\, which the user would see in the tab and Recent Files.
        Ok(path.to_path_buf())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_keep_their_keys_and_an_unknown_key_is_the_lowest() {
        for level in Level::ALL {
            assert_eq!(Level::from_key(level.key()), level);
        }
        assert_eq!(Level::from_key("everything"), Level::Read);
        assert!(Level::Full > Level::Edit && Level::Edit > Level::Read);
    }

    #[test]
    fn a_secret_must_match_exactly() {
        assert!(same_secret("abc", "abc"));
        assert!(!same_secret("abc", "abd"));
        assert!(!same_secret("abc", "abcd"));
        assert!(!same_secret("", "a"));
    }

    #[test]
    fn the_setup_carries_the_address_and_the_token() {
        let text = setup_text(1234, "tok");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        let server = &v["mcpServers"]["notepp"];
        assert_eq!(server["url"], "http://127.0.0.1:1234/mcp");
        assert_eq!(server["headers"]["Authorization"], "Bearer tok");
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notepp-reach-{name}-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("work/sub")).unwrap();
        std::fs::create_dir_all(dir.join("profile")).unwrap();
        std::fs::create_dir_all(dir.join("other")).unwrap();
        std::fs::write(dir.join("work/sub/a.txt"), "a").unwrap();
        std::fs::write(dir.join("other/b.txt"), "b").unwrap();
        std::fs::write(dir.join("profile/mcp-token"), "t").unwrap();
        dir
    }

    #[test]
    fn inside_the_workspace_or_an_open_tab_is_reachable_and_nothing_else() {
        let dir = scratch("reach");
        let (work, profile, tab) = (dir.join("work"), dir.join("profile"), dir.join("other/b.txt"));
        let reach = Reach { workspace: Some(&work), open: vec![&tab], profile: &profile };
        assert_eq!(reach.check(&work.join("sub/a.txt")), Ok(work.join("sub/a.txt")), "the path as given");
        assert!(reach.check(&work.join("sub/new.txt")).is_ok(), "a file yet to be written");
        assert!(reach.check(&tab).is_ok());
        assert!(reach.check(&dir.join("other/c.txt")).is_err());
        assert!(reach.check(&profile.join("mcp-token")).is_err());
        assert!(reach.check(&work.join("../other/b.txt")).is_err(), "climbing out");
        assert!(reach.check(Path::new("a.txt")).is_err(), "relative");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn with_no_workspace_only_open_tabs_are_reachable() {
        let dir = scratch("open");
        let (profile, tab) = (dir.join("profile"), dir.join("other/b.txt"));
        let reach = Reach { workspace: None, open: vec![&tab], profile: &profile };
        assert!(reach.check(&tab).is_ok());
        assert!(reach.check(&dir.join("work/sub/a.txt")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_profile_stays_out_of_reach_even_inside_the_workspace() {
        let dir = scratch("profile");
        let profile = dir.join("profile");
        let reach = Reach { workspace: Some(&dir), open: Vec::new(), profile: &profile };
        assert!(reach.check(&profile.join("mcp-token")).is_err());
        assert!(reach.check(&dir.join("work/sub/a.txt")).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[cfg(windows)]
    #[test]
    fn a_network_path_is_refused_without_looking_at_it() {
        let dir = scratch("unc");
        let (work, profile) = (dir.join("work"), dir.join("profile"));
        let reach = Reach { workspace: Some(&work), open: Vec::new(), profile: &profile };
        assert!(reach.check(Path::new(r"\\server\share\a.txt")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
