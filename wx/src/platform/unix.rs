//! macOS and Linux: `open` / `xdg-open`, the system terminal, UTC date and time.

use std::path::Path;

use super::Platform;

pub struct Unix;

fn run(program: &str, args: &[&std::ffi::OsStr], dir: Option<&Path>) {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let _ = cmd.spawn();
}

impl Platform for Unix {
    fn reveal_in_file_manager(&self, path: &Path) {
        if cfg!(target_os = "macos") {
            run("open", &["-R".as_ref(), path.as_os_str()], None);
        } else {
            run("xdg-open", &[path.parent().unwrap_or(path).as_os_str()], None);
        }
    }

    fn open_terminal(&self, dir: Option<&Path>) {
        if cfg!(target_os = "macos") {
            run("open", &["-a".as_ref(), "Terminal".as_ref(), dir.unwrap_or(Path::new(".")).as_os_str()], None);
        } else {
            run("x-terminal-emulator", &[], dir);
        }
    }

    fn open_default(&self, target: &str) {
        run(if cfg!(target_os = "macos") { "open" } else { "xdg-open" }, &[target.as_ref()], None);
    }

    fn open_with(&self, program: &str, path: &Path) {
        run(program.trim_end_matches(".exe"), &[path.as_os_str()], None);
    }
}
