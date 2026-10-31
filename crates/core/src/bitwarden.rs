//! Passwords: a front end for the official Bitwarden command-line client (`bw`).
//!
//! Note++ stores no credentials and implements no password cryptography here. It downloads the
//! official `bw`, checks the download against a SHA-256 pinned in this source before anything is
//! extracted or run, and then drives it. Your vault, your Bitwarden account, Bitwarden's audited
//! encryption.
//!
//! Only one write is supported: adding an item. Editing and deleting stay in Bitwarden itself.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
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

static SESSION: Mutex<Session> = Mutex::new(Session { token: None, touched: None, auto_lock: Some(Duration::from_secs(15 * 60)) });

fn session() -> std::sync::MutexGuard<'static, Session> {
    SESSION.lock().unwrap_or_else(|e| e.into_inner())
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
    session().auto_lock = (minutes > 0).then(|| Duration::from_secs(minutes as u64 * 60));
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
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let stdout = out_t.and_then(|t| t.join().ok()).unwrap_or_default();
    let mut stderr = err_t.and_then(|t| t.join().ok()).unwrap_or_default();
    match status {
        Some(s) => Out { ok: s.success(), stdout, stderr },
        None => {
            stderr = format!("bw did not answer in {} seconds", timeout.as_secs());
            Out { ok: false, stdout, stderr }
        }
    }
}

const QUICK: Duration = Duration::from_secs(30);

// ---- installing -----------------------------------------------------------------------------

/// What the Passwords dialog shows about the installation.
pub struct InstallInfo {
    pub installed: bool,
    pub supported: bool,
    pub bundled: bool,
}

pub fn install_info() -> InstallInfo {
    InstallInfo { installed: is_installed(), supported: asset().is_some(), bundled: bundled_bin().is_some() }
}

/// Download, verify and unpack the official CLI. `progress` gets (percent, what is happening).
pub fn download(progress: &mut dyn FnMut(u32, &str)) -> Result<(), String> {
    let a = asset().ok_or("Bitwarden does not publish a command-line client for this system")?;
    let url = format!("{RELEASE_BASE}{}", a.file);
    let tmp = std::env::temp_dir().join(format!("bw-{BW_VERSION}-{}.zip", std::process::id()));
    let result = download_to(&url, &tmp, a, progress);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn download_to(url: &str, tmp: &std::path::Path, a: &Asset, progress: &mut dyn FnMut(u32, &str)) -> Result<(), String> {
    progress(0, "Downloading the Bitwarden CLI");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into();
    let mut res = agent.get(url).call().map_err(|e| format!("Download failed: {e}"))?;
    let total: u64 = res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = res.body_mut().as_reader();
    let mut file = std::fs::File::create(tmp).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let (mut buf, mut done, mut last) = (vec![0u8; 64 * 1024], 0u64, 0u32);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if total > 0 {
            let pct = (done * 90 / total) as u32;
            if pct != last {
                last = pct;
                progress(pct, "Downloading the Bitwarden CLI");
            }
        }
    }
    drop(file);
    // Verify BEFORE anything is extracted or executed.
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != a.sha256 {
        return Err(format!("Checksum mismatch - refusing to install.\nexpected {}\ngot      {digest}", a.sha256));
    }
    progress(92, "Unpacking");
    install_zip(tmp, a, &digest)?;
    progress(100, "Done");
    Ok(())
}

/// Unpack `zip_path` and put the `bw` binary in the bundle folder.
fn install_zip(zip_path: &std::path::Path, a: &Asset, digest: &str) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| e.to_string())?).map_err(|e| format!("The download is not a valid archive: {e}"))?;
    let dir = bundle_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut found = false;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // Only the binary is wanted, and never under a path the archive chooses.
        let name = entry.name().rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if name != a.bin || entry.is_dir() {
            continue;
        }
        let dest = dir.join(a.bin);
        let part = dir.join(format!("{}.part", a.bin));
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        found = true;
        break;
    }
    if !found {
        return Err(format!("{} was not in the download", a.bin));
    }
    let meta = serde_json::json!({ "version": BW_VERSION, "sha256": digest });
    let _ = std::fs::write(dir.join("meta.json"), meta.to_string());
    Ok(())
}

pub fn uninstall() {
    lock();
    let _ = std::fs::remove_dir_all(bundle_dir());
}

// ---- account state --------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    Unauthenticated,
    Locked,
    Unlocked,
    Unknown(String),
}

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

/// Use a session the user already has (BW_SESSION in the environment). No prompts.
pub fn adopt_env_session() -> bool {
    let Ok(tok) = std::env::var("BW_SESSION") else { return false };
    if tok.trim().is_empty() {
        return false;
    }
    use_session_token(&tok).is_ok()
}

/// Accept a token the user got themselves (e.g. `bw unlock --raw` in a terminal), so the master
/// password never goes through this app.
pub fn use_session_token(token: &str) -> Result<(), String> {
    let token = token.trim();
    if token.is_empty() {
        return Err("No session token supplied".into());
    }
    let previous = session().token.take();
    set_token(token.to_string());
    if auth_status().state == AuthState::Unlocked {
        Ok(())
    } else {
        session().token = previous;
        Err("That session token was not accepted by bw".into())
    }
}

/// Unlock with the master password. It goes to `bw` in an environment variable of that one process.
pub fn unlock(master_password: &str) -> Result<(), String> {
    let r = run(&["unlock", "--raw", "--passwordenv", "BW_UNLOCK_PW"], &[("BW_UNLOCK_PW", master_password)], None, Duration::from_secs(60));
    if !r.ok || r.stdout.trim().is_empty() {
        let msg = r.stderr.trim();
        return Err(if msg.to_lowercase().contains("not logged in") || msg.to_lowercase().contains("unauthenticated") {
            "Not signed in - use \"Sign in\" first".into()
        } else if msg.to_lowercase().contains("invalid") {
            "Incorrect master password".into()
        } else if msg.is_empty() {
            "Unlock failed".into()
        } else {
            msg.to_string()
        });
    }
    set_token(r.stdout.trim().to_string());
    Ok(())
}

pub fn login(email: &str, master_password: &str) -> Result<(), String> {
    let r = run(&["login", email, "--raw", "--passwordenv", "BW_LOGIN_PW"], &[("BW_LOGIN_PW", master_password)], None, Duration::from_secs(90));
    if !r.ok || r.stdout.trim().is_empty() {
        let msg = r.stderr.trim();
        let low = msg.to_lowercase();
        return Err(if low.contains("two-step") || low.contains("two factor") || low.contains("2fa") {
            "This account needs two-step login. Run `bw login` in a terminal once, then use \"I have a session token\".".into()
        } else if msg.is_empty() {
            "Sign in failed".into()
        } else {
            msg.to_string()
        });
    }
    set_token(r.stdout.trim().to_string());
    Ok(())
}

pub fn sync() -> Result<(), String> {
    touch();
    let r = run(&["sync"], &[], None, Duration::from_secs(60));
    if r.ok { Ok(()) } else { Err(nonempty(r.stderr, "sync failed")) }
}

fn nonempty(s: String, fallback: &str) -> String {
    let t = s.trim();
    if t.is_empty() { fallback.to_string() } else { t.to_string() }
}

// ---- reading --------------------------------------------------------------------------------

/// What the list shows about an item. Never the secret itself.
#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url: String,
    pub has_password: bool,
    pub has_totp: bool,
}

pub fn parse_items(json: &str) -> Result<Vec<Item>, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "could not list items".to_string())?;
    let arr = v.as_array().ok_or("could not list items")?;
    Ok(arr
        .iter()
        .filter_map(|it| {
            let id = it.get("id")?.as_str()?.to_string();
            let login = it.get("login");
            let text = |v: Option<&Value>, k: &str| v.and_then(|l| l.get(k)).and_then(Value::as_str).unwrap_or("").to_string();
            Some(Item {
                id,
                title: it.get("name").and_then(Value::as_str).filter(|s| !s.is_empty()).unwrap_or("(untitled)").to_string(),
                username: text(login, "username"),
                url: login.and_then(|l| l.get("uris")).and_then(Value::as_array).and_then(|u| u.first()).and_then(|u| u.get("uri")).and_then(Value::as_str).unwrap_or("").to_string(),
                has_password: login.and_then(|l| l.get("password")).is_some_and(|p| !p.is_null()),
                has_totp: login.and_then(|l| l.get("totp")).is_some_and(|p| !p.is_null()),
            })
        })
        .collect())
}

pub fn list(query: &str) -> Result<Vec<Item>, String> {
    if !is_unlocked() {
        return Err("locked".into());
    }
    touch();
    let mut args = vec!["list", "items"];
    if !query.is_empty() {
        args.extend(["--search", query]);
    }
    let r = run(&args, &[], None, QUICK);
    parse_items(&r.stdout).map_err(|_| nonempty(r.stderr, "could not list items"))
}

pub const FIELDS: [&str; 5] = ["password", "username", "totp", "uri", "notes"];

/// One field of one item, fetched fresh and never cached.
pub fn get_field(id: &str, field: &str) -> Result<String, String> {
    if !is_unlocked() {
        return Err("locked".into());
    }
    if !FIELDS.contains(&field) {
        return Err("field not allowed".into());
    }
    touch();
    let r = run(&["get", field, id], &[], None, QUICK);
    if !r.ok {
        return Err(nonempty(r.stderr, &format!("bw get {field} failed")));
    }
    let v = r.stdout.trim_end_matches(['\r', '\n']).to_string();
    if v.is_empty() { Err(format!("This item has no {field}")) } else { Ok(v) }
}

/// "Item name" or "Item name.field" (field: password, username, totp, uri, notes) -> its value.
/// An item is found by exact (case-insensitive) title, else the first search hit.
pub fn resolve_reference(reference: &str) -> Result<String, String> {
    let (title, field) = split_reference(reference);
    let items = list(&title)?;
    let item = items.iter().find(|i| i.title.eq_ignore_ascii_case(&title)).or_else(|| items.first()).ok_or_else(|| format!("No vault item matches \"{title}\""))?;
    get_field(&item.id, field)
}

pub fn split_reference(reference: &str) -> (String, &'static str) {
    let r = reference.trim();
    if let Some(dot) = r.rfind('.') {
        let tail = &r[dot + 1..];
        if dot > 0 {
            if let Some(f) = FIELDS.iter().find(|f| **f == tail) {
                return (r[..dot].to_string(), f);
            }
        }
    }
    (r.to_string(), "password")
}

// ---- writing --------------------------------------------------------------------------------

#[derive(Default, Clone)]
pub struct NewItem {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
}

pub fn create_payload(item: &NewItem) -> Result<String, String> {
    let title = item.title.trim();
    if title.is_empty() {
        return Err("Title is required".into());
    }
    let opt = |s: &str| if s.is_empty() { Value::Null } else { Value::String(s.to_string()) };
    let uris = if item.url.trim().is_empty() { Value::Null } else { serde_json::json!([{ "match": null, "uri": item.url.trim() }]) };
    let payload = serde_json::json!({
        "organizationId": null, "collectionIds": null, "folderId": null,
        "type": 1, "name": title, "notes": opt(&item.notes), "favorite": false, "fields": [],
        "login": { "uris": uris, "username": opt(&item.username), "password": opt(&item.password), "totp": null },
        "reprompt": 0
    });
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(payload.to_string()))
}

/// Add a login to the vault. The item goes to `bw` on stdin, not on the command line, where any
/// other program could read it.
pub fn create_item(item: &NewItem) -> Result<(), String> {
    if !is_unlocked() {
        return Err("locked".into());
    }
    touch();
    let b64 = create_payload(item)?;
    let r = run(&["create", "item"], &[], Some(&b64), Duration::from_secs(45));
    if r.ok { Ok(()) } else { Err(nonempty(r.stderr, "bw create item failed")) }
}

pub struct GenOpts {
    pub length: u32,
    pub upper: bool,
    pub lower: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Default for GenOpts {
    fn default() -> Self {
        GenOpts { length: 20, upper: true, lower: true, digits: true, symbols: false }
    }
}

pub fn generate_args(o: &GenOpts) -> Vec<String> {
    let mut a = vec!["generate".to_string(), "--length".into(), o.length.clamp(5, 128).to_string()];
    for (on, flag) in [(o.upper, "--uppercase"), (o.lower, "--lowercase"), (o.digits, "--number"), (o.symbols, "--special")] {
        if on {
            a.push(flag.into());
        }
    }
    a
}

/// A password from Bitwarden's own generator (no generator of our own to get wrong).
pub fn generate(o: &GenOpts) -> Result<String, String> {
    let args = generate_args(o);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = run(&refs, &[], None, QUICK);
    if !r.ok {
        return Err(nonempty(r.stderr, "bw generate failed"));
    }
    let v = r.stdout.trim_end_matches(['\r', '\n']).to_string();
    if v.is_empty() { Err("bw generate returned nothing".into()) } else { Ok(v) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_checksums_are_well_formed() {
        for a in ASSETS {
            assert_eq!(a.sha256.len(), 64, "{}", a.file);
            assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(a.file.contains(BW_VERSION));
        }
        assert!(RELEASE_BASE.contains(BW_VERSION));
    }

    #[test]
    fn status_json() {
        let a = parse_status(r#"{"serverUrl":null,"lastSync":null,"userEmail":"a@b.c","userId":"x","status":"locked"}"#);
        assert_eq!(a.state, AuthState::Locked);
        assert_eq!(a.email.as_deref(), Some("a@b.c"));
        assert_eq!(parse_status(r#"{"status":"unlocked"}"#).state, AuthState::Unlocked);
        assert_eq!(parse_status(r#"{"status":"unauthenticated"}"#).state, AuthState::Unauthenticated);
        assert!(matches!(parse_status("garbage").state, AuthState::Unknown(_)));
    }

    #[test]
    fn items_never_carry_secrets() {
        let json = r#"[{"id":"1","name":"GitHub","login":{"username":"me","password":"hunter2","totp":null,"uris":[{"uri":"https://github.com"}]}},
                       {"id":"2","name":"","login":{"username":null,"password":null,"totp":"otpauth://x"}},
                       {"object":"item"}]"#;
        let items = parse_items(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], Item { id: "1".into(), title: "GitHub".into(), username: "me".into(), url: "https://github.com".into(), has_password: true, has_totp: false });
        assert_eq!(items[1].title, "(untitled)");
        assert!(items[1].has_totp && !items[1].has_password);
        assert!(!format!("{:?}", items).contains("hunter2"));
        assert!(parse_items("nope").is_err());
    }

    #[test]
    fn vault_references() {
        assert_eq!(split_reference("GitHub"), ("GitHub".into(), "password"));
        assert_eq!(split_reference("GitHub.username"), ("GitHub".into(), "username"));
        assert_eq!(split_reference("my.site.totp"), ("my.site".into(), "totp"));
        assert_eq!(split_reference("v1.5"), ("v1.5".into(), "password"));
        assert_eq!(split_reference(".password"), (".password".into(), "password"));
    }

    #[test]
    fn create_payload_is_base64_json_with_the_login() {
        use base64::Engine;
        let item = NewItem { title: " Site ".into(), username: "u".into(), password: "p".into(), url: "https://x.y".into(), notes: String::new() };
        let raw = base64::engine::general_purpose::STANDARD.decode(create_payload(&item).unwrap()).unwrap();
        let v: Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["name"], "Site");
        assert_eq!(v["type"], 1);
        assert_eq!(v["login"]["password"], "p");
        assert_eq!(v["login"]["uris"][0]["uri"], "https://x.y");
        assert!(v["notes"].is_null());
        assert!(create_payload(&NewItem::default()).is_err());
    }

    #[test]
    fn generator_flags() {
        let a = generate_args(&GenOpts { length: 500, symbols: true, ..Default::default() });
        assert_eq!(a[..3], ["generate", "--length", "128"]);
        assert!(a.contains(&"--special".to_string()) && a.contains(&"--number".to_string()));
        assert!(!generate_args(&GenOpts { digits: false, ..Default::default() }).contains(&"--number".to_string()));
    }

    #[test]
    fn locking_and_auto_lock() {
        set_token("t".into());
        assert!(is_unlocked());
        set_auto_lock_minutes(0);
        assert!(!tick_auto_lock());
        lock();
        assert!(!is_unlocked());
        set_auto_lock_minutes(15);
    }
}
