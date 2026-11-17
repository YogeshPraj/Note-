//! Starting with the system: a per-user entry that launches Note++ at sign-in with `--tray`, so it waits in the system
//! tray. The entry is rewritten at every start so that it always names the running program.

use std::sync::atomic::{AtomicBool, Ordering};

static TRAY: AtomicBool = AtomicBool::new(false);

/// The launch flag that keeps the window hidden.
pub const FLAG: &str = "--tray";

/// Remember that this run was started to wait in the tray.
pub fn set_tray_start() {
    TRAY.store(true, Ordering::SeqCst);
}

/// Whether this run should go straight to the tray (once).
pub fn take_tray_start() -> bool {
    TRAY.swap(false, Ordering::SeqCst)
}

/// Make the system's start-up entry match `on`.
pub fn sync(on: bool) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
        const NAME: &str = "Note++";
        const NO_WINDOW: u32 = 0x0800_0000;
        let mut cmd = std::process::Command::new("reg");
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let value = format!("\"{}\" {FLAG}", exe.display());
            cmd.args(["add", KEY, "/v", NAME, "/t", "REG_SZ", "/d", &value, "/f"]);
        } else {
            cmd.args(["delete", KEY, "/v", NAME, "/f"]);
        }
        let _ = cmd.creation_flags(NO_WINDOW).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status();
    }
    #[cfg(not(windows))]
    let _ = on;
}
