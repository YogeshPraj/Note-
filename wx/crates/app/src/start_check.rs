//! What the build servers check: that a release opens its window and that its web pages work there.
//!
//! When `NOTEPP_START_CHECK` names a file, a line goes into it at each milestone of a start: `window` once the window is shown,
//! `whiteboard` once the whiteboard page says it is ready. `scripts/start-check.sh` and `scripts/package.ps1` start the package
//! with a whiteboard file and wait for both lines. Without the variable nothing happens.

use std::io::Write as _;

/// The variable that asks for the lines, and the file they go to.
const VAR: &str = "NOTEPP_START_CHECK";

/// Note a milestone. A file that cannot be written fails the check, which is the point; the app itself carries on.
pub fn note(milestone: &str) {
    let Some(path) = std::env::var_os(VAR) else { return };
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{}", line(milestone));
    }
}

/// One line: the milestone and the version that reached it, so a stale file from another build cannot pass.
fn line(milestone: &str) -> String {
    format!("{milestone} {}", notepp_core::update::VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_names_the_milestone_and_the_version() {
        assert_eq!(line("window"), format!("window {}", notepp_core::update::VERSION));
    }
}
