//! Passwords: a front end for the official Bitwarden command-line client (`bw`).
//!
//! Note++ stores no credentials and implements no password cryptography here. It downloads the
//! official `bw`, checks the download against a SHA-256 pinned in this source before anything is
//! extracted or run, and then drives it. Your vault, your Bitwarden account, Bitwarden's audited
//! encryption.
//!
//! Only one write is supported: adding an item. Editing and deleting stay in Bitwarden itself.

use crate::error::Error;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub const BW_VERSION: &str = "2026.7.0";
const RELEASE_BASE: &str = "https://github.com/bitwarden/clients/releases/download/cli-v2026.7.0/";

struct Asset {
    key: &'static str,
    file: &'static str,
    sha256: &'static str,
    bin: &'static str,
}

/// The release files, with the checksums of the official artefacts. A swapped upstream file is refused.
const ASSETS: &[Asset] = &[
    Asset { key: "windows-x64", file: "bw-windows-2026.7.0.zip", sha256: "b0c22438607b789c6452dbd37ffd6be0e8a61e7a5c4e9ac57804d7ae5ed01b5b", bin: "bw.exe" },
    Asset { key: "macos-x64", file: "bw-macos-2026.7.0.zip", sha256: "b37836d539798f5adeb8a907619ee8a55b6322549bb68669aa4b3a03d5bc0452", bin: "bw" },
    Asset { key: "macos-arm64", file: "bw-macos-arm64-2026.7.0.zip", sha256: "61d5de8a279a9faf3637216f4fb02b506a1e4bb2817d1c64be0bd474466dd85a", bin: "bw" },
    Asset { key: "linux-x64", file: "bw-linux-2026.7.0.zip", sha256: "7a35145e205952f7434d2370da359543145ae0c45ba1af0fe9bdd99d40a00180", bin: "bw" },
    Asset { key: "linux-arm64", file: "bw-linux-arm64-2026.7.0.zip", sha256: "e33ed05ca0fada9bd51b8bce76a230369bf0eefd5796a0a8e60699c977327fb5", bin: "bw" },
];

fn platform_key() -> String {
    let os = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    format!("{os}-{arch}")
}

fn asset() -> Option<&'static Asset> {
    let k = platform_key();
    ASSETS.iter().find(|a| a.key == k)
}

pub fn bundle_dir() -> PathBuf {
    super::session::config_dir().join("bitwarden")
}

fn bundled_bin() -> Option<PathBuf> {
    let p = bundle_dir().join(asset()?.bin);
    p.is_file().then_some(p)
}

/// The `bw` to run: the downloaded one, else one found on PATH.
pub fn resolve_bin() -> Option<PathBuf> {
    if let Some(p) = bundled_bin() {
        return Some(p);
    }
    let name = if cfg!(windows) { "bw.exe" } else { "bw" };
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file()))
}

pub fn is_installed() -> bool {
    resolve_bin().is_some()
}

// ---- running bw -----------------------------------------------------------------------------

struct Session {
    token: Option<String>,
    touched: Option<Instant>,
    auto_lock: Option<Duration>,
}

static SESSION: Mutex<Session> = Mutex::new(Session { token: None, touched: None, auto_lock: Some(Duration::from_mins(15)) });

fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn is_unlocked() -> bool {
    session().token.is_some()
}

pub fn touch() {
    let mut s = session();
    if s.token.is_some() {
        s.touched = Some(Instant::now());
    }
}

pub fn lock() {
    let mut s = session();
    s.token = None;
}

/// Lock after `minutes` without use (0 = never).
pub fn set_auto_lock_minutes(minutes: u32) {
    session().auto_lock = (minutes > 0).then(|| Duration::from_secs(u64::from(minutes) * 60));
}

/// Called on a timer: locks when idle for too long. True if it just locked.
pub fn tick_auto_lock() -> bool {
    let mut s = session();
    match (s.token.is_some(), s.auto_lock) {
        (true, Some(limit)) if s.touched.is_some_and(|t| t.elapsed() >= limit) => {
            s.token = None;
            true
        }
        _ => false,
    }
}

struct Out {
    ok: bool,
    stdout: String,
    stderr: String,
}

/// Run `bw` with `args`. `env` are extra variables for this one call (e.g. the master password),
/// `stdin` is written to its standard input. The call is abandoned after `timeout`.
fn run(args: &[&str], env: &[(&str, &str)], stdin: Option<&str>, timeout: Duration) -> Out {
    let Some(bin) = resolve_bin() else {
        return Out { ok: false, stdout: String::new(), stderr: "Bitwarden CLI is not installed".into() };
    };
    let mut cmd = Command::new(bin);
    cmd.args(args).env("BW_NOINTERACTION", "true").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(t) = session().token.as_deref() {
        cmd.env("BW_SESSION", t);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => return Out { ok: false, stdout: String::new(), stderr: e.to_string() },
    };
    if let Some(mut si) = child.stdin.take() {
        // The master password or new item goes straight to the CLI; it is not kept here.
        if let Some(text) = stdin {
            let _ = si.write_all(text.as_bytes());
            let _ = si.write_all(b"\n");
        }
    }
    let drain = |mut r: Box<dyn Read + std::marker::Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = r.read_to_string(&mut s);
            s
        })
    };
    let out_t = child.stdout.take().map(|r| drain(Box::new(r)));
    let err_t = child.stderr.take().map(|r| drain(Box::new(r)));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if Instant::now() < deadline && !cancelled() => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = out_t.and_then(|t| t.join().ok()).unwrap_or_default();
    let mut stderr = err_t.and_then(|t| t.join().ok()).unwrap_or_default();
    if let Some(s) = status {
        Out { ok: s.success(), stdout, stderr }
    } else {
        stderr = if cancelled() { "Cancelled".into() } else { format!("bw did not answer in {} seconds", timeout.as_secs()) };
        Out { ok: false, stdout, stderr }
    }
}

const QUICK: Duration = Duration::from_secs(30);

/// Set by [`cancel`]: the running `bw` call or download stops as soon as it can.
static CANCELLED: AtomicBool = AtomicBool::new(false);

/// Stop the Bitwarden job that is running (its dialog is being closed): a `bw` call is ended, a download stops.
pub fn cancel() {
    CANCELLED.store(true, Ordering::Relaxed);
}

/// A new job begins: a cancel meant for an earlier one no longer applies. (Not reset per call, so a job of several
/// calls stops at the first call after a cancel.)
pub fn begin_job() {
    CANCELLED.store(false, Ordering::Relaxed);
}

fn cancelled() -> bool {
    CANCELLED.load(Ordering::Relaxed)
}

mod install;
pub use install::*;
// ---- account state --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Unauthenticated,
    Locked,
    Unlocked,
    Unknown(String),
}

#[derive(Debug)]
pub struct Auth {
    pub state: AuthState,
    pub email: Option<String>,
}

pub fn parse_status(json: &str) -> Auth {
    let Ok(v) = serde_json::from_str::<Value>(json) else {
        return Auth { state: AuthState::Unknown("could not read bw status".into()), email: None };
    };
    let state = match v.get("status").and_then(Value::as_str) {
        Some("unauthenticated") => AuthState::Unauthenticated,
        Some("locked") => AuthState::Locked,
        Some("unlocked") => AuthState::Unlocked,
        other => AuthState::Unknown(other.unwrap_or("").to_string()),
    };
    Auth { state, email: v.get("userEmail").and_then(Value::as_str).map(str::to_string) }
}

pub fn auth_status() -> Auth {
    let r = run(&["status"], &[], None, QUICK);
    if r.stdout.trim().is_empty() {
        return Auth { state: AuthState::Unknown(r.stderr.trim().to_string()), email: None };
    }
    parse_status(&r.stdout)
}

fn set_token(t: String) {
    let mut s = session();
    s.token = Some(t);
    s.touched = Some(Instant::now());
}

/// Use a session the user already has (`BW_SESSION` in the environment). No prompts.
pub fn adopt_env_session() -> bool {
    let Ok(tok) = std::env::var("BW_SESSION") else { return false };
    if tok.trim().is_empty() {
        return false;
    }
    use_session_token(&tok).is_ok()
}

/// Accept a token the user got themselves (e.g. `bw unlock --raw` in a terminal), so the master
/// password never goes through this app.
pub fn use_session_token(token: &str) -> Result<(), Error> {
    let token = token.trim();
    if token.is_empty() {
        return Err(Error::NoSessionToken);
    }
    let previous = session().token.take();
    set_token(token.to_string());
    if auth_status().state == AuthState::Unlocked {
        Ok(())
    } else {
        session().token = previous;
        Err(Error::SessionRejected)
    }
}

/// Unlock with the master password. It goes to `bw` in an environment variable of that one process.
pub fn unlock(master_password: &str) -> Result<(), Error> {
    let r = run(&["unlock", "--raw", "--passwordenv", "BW_UNLOCK_PW"], &[("BW_UNLOCK_PW", master_password)], None, Duration::from_secs(60));
    if !r.ok || r.stdout.trim().is_empty() {
        let msg = r.stderr.trim();
        return Err(if msg.to_lowercase().contains("not logged in") || msg.to_lowercase().contains("unauthenticated") {
            Error::NotSignedIn
        } else if msg.to_lowercase().contains("invalid") {
            Error::WrongMasterPassword
        } else if msg.is_empty() {
            Error::UnlockFailed
        } else {
            Error::Cli(msg.to_string())
        });
    }
    set_token(r.stdout.trim().to_string());
    Ok(())
}

pub fn login(email: &str, master_password: &str) -> Result<(), Error> {
    let r = run(&["login", email, "--raw", "--passwordenv", "BW_LOGIN_PW"], &[("BW_LOGIN_PW", master_password)], None, Duration::from_secs(90));
    if !r.ok || r.stdout.trim().is_empty() {
        let msg = r.stderr.trim();
        let low = msg.to_lowercase();
        return Err(if low.contains("two-step") || low.contains("two factor") || low.contains("2fa") {
            Error::TwoStepLogin
        } else if msg.is_empty() {
            Error::SignInFailed
        } else {
            Error::Cli(msg.to_string())
        });
    }
    set_token(r.stdout.trim().to_string());
    Ok(())
}

pub fn sync() -> Result<(), Error> {
    touch();
    let r = run(&["sync"], &[], None, Duration::from_secs(60));
    if r.ok { Ok(()) } else { Err(Error::Cli(nonempty(&r.stderr, "sync failed"))) }
}

fn nonempty(s: &str, fallback: &str) -> String {
    let t = s.trim();
    if t.is_empty() { fallback.to_string() } else { t.to_string() }
}

mod items;
pub use items::*;
#[cfg(test)]
mod tests;
