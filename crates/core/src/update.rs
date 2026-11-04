//! Auto-update for the native app: ask GitHub whether a newer release exists, download it in the
//! background, check it, unpack it next to nothing else, and swap it in on the user's click.
//!
//! Releases of this app are tagged `vX.Y.Z`. Older releases of the previous app have no zip
//! asset and are ignored. A release carries three assets: `NotePP-windows-x64.zip`,
//! `NotePP-windows-x64.zip.sha256` (the hex digest) and `NotePP-windows-x64.zip.sha256.sig`.
//! The last is an Ed25519 signature over the version and the digest file, made with the key that
//! only the release build holds (`examples/update_key.rs`). The program is not code-signed, so this
//! is what stands between a taken-over release page and every installed copy: the download is only
//! unpacked when the signature is good and the zip's SHA-256 matches the signed digest. The version
//! is in the signed text so that an old, genuine build cannot be passed off as a new one.

use crate::error::Error;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ring::signature::{ED25519, UnparsedPublicKey};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/YogeshPraj/Note-/releases?per_page=30";
const TAG_PREFIX: &str = "v";
const NEW_ISSUE: &str = "https://github.com/YogeshPraj/Note-/issues/new";

/// The page for a new issue, with the version and system already in the description.
pub fn issue_url(os: &str, build: &str) -> String {
    let body =
        format!("**Version:** {VERSION} ({build} build)\n**System:** {os}\n\n**What happened**\n\n\n**What I expected**\n\n\n**Steps to reproduce**\n1. \n");
    format!("{NEW_ISSUE}?body={}", crate::edit::url_encode(&body).replace('+', "%20"))
}
pub const ASSET: &str = "NotePP-windows-x64.zip";
/// Every update file comes from this repository's releases.
const DOWNLOADS: &str = "https://github.com/YogeshPraj/Note-/releases/download/";
/// The public half of the release key. Changing it stops installed copies from accepting what the new key signs.
const UPDATE_KEY: [u8; 32] = [
    0xbe, 0xa0, 0xcf, 0x4d, 0xfa, 0x7c, 0x6e, 0x96, 0x1c, 0x0d, 0x7e, 0x11, 0xac, 0x5d, 0x3e, 0xcc, 0x08, 0xeb, 0x89, 0xe8, 0xca, 0xe0, 0x61, 0x0e, 0xc8, 0x17,
    0xf6, 0xcf, 0xde, 0x19, 0x43, 0xf8,
];
/// The program inside the archive; an archive without it is refused.
const EXE: &str = "notepp.exe";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub zip_url: String,
    pub sha_url: String,
    pub sig_url: String,
}

/// "1.2.3" or "v1.2.3" (extra parts and suffixes such as "-beta" are ignored) -> (1, 2, 3).
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let s = s.trim().trim_start_matches('v');
    let core = s.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    matches!((parse_version(candidate), parse_version(current)), (Some(a), Some(b)) if a > b)
}

/// The newest published native release that is newer than `current` and has all its assets, from this repository.
pub fn pick_release(json: &str, current: &str) -> Result<Option<Release>, Error> {
    let v: Value = serde_json::from_str(json).map_err(|_| Error::UpdateAnswerUnreadable)?;
    let list = v.as_array().ok_or_else(|| Error::Github(v.get("message").and_then(Value::as_str).unwrap_or("unexpected answer from GitHub").to_string()))?;
    let mut best: Option<((u32, u32, u32), Release)> = None;
    for r in list {
        if r["draft"].as_bool() == Some(true) || r["prerelease"].as_bool() == Some(true) {
            continue;
        }
        let Some(tag) = r["tag_name"].as_str().and_then(|t| t.strip_prefix(TAG_PREFIX)) else { continue };
        let Some(ver) = parse_version(tag) else { continue };
        if !is_newer(tag, current) {
            continue;
        }
        let asset = |name: &str| {
            let url = r["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name))?["browser_download_url"].as_str()?;
            url.starts_with(DOWNLOADS).then(|| url.to_string())
        };
        let (Some(zip_url), Some(sha_url), Some(sig_url)) = (asset(ASSET), asset(&format!("{ASSET}.sha256")), asset(&format!("{ASSET}.sha256.sig"))) else {
            continue;
        };
        if best.as_ref().is_none_or(|(b, _)| ver > *b) {
            best = Some((ver, Release { version: tag.to_string(), zip_url, sha_url, sig_url }));
        }
    }
    Ok(best.map(|(_, r)| r))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into()
}

/// Where to ask. `NOTEPP_UPDATE_URL` points at another releases list (for testing).
fn api_url() -> String {
    std::env::var("NOTEPP_UPDATE_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| RELEASES_API.to_string())
}

/// Ask GitHub for a newer release. Ok(None) = you are up to date.
pub fn check(current: &str) -> Result<Option<Release>, Error> {
    let mut res = agent()
        .get(&api_url())
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "NotePP-native-updater")
        .call()
        .map_err(|e| Error::GithubUnreachable(e.to_string()))?;
    let body = res.body_mut().read_to_string().map_err(|e| Error::GithubRead(e.to_string()))?;
    pick_release(&body, current)
}

pub fn updates_dir() -> PathBuf {
    super::session::config_dir().join("updates")
}

/// The digest from a `.sha256` file ("<hex>" or "<hex>  filename").
pub fn parse_digest(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?.to_ascii_lowercase();
    (first.len() == 64 && first.chars().all(|c| c.is_ascii_hexdigit())).then_some(first)
}

/// What a release signs: its version and the exact text of its digest file.
pub fn signed_message(version: &str, digest_file: &str) -> Vec<u8> {
    format!("Note++ update {version}\n{digest_file}").into_bytes()
}

/// Is `signature` (base64) a signature of `message` by `key`?
fn signed_by(key: &[u8], message: &[u8], signature: &str) -> bool {
    STANDARD.decode(signature.trim()).is_ok_and(|sig| UnparsedPublicKey::new(&ED25519, key).verify(message, &sig).is_ok())
}

/// Check that the release key signed this version and digest file.
///
/// # Errors
/// [`Error::UpdateNotSigned`] when it did not.
pub fn check_signature(version: &str, digest_file: &str, signature: &str) -> Result<(), Error> {
    if signed_by(&UPDATE_KEY, &signed_message(version, digest_file), signature) { Ok(()) } else { Err(Error::UpdateNotSigned) }
}

/// Unpack `zip_path` into `dest`. Entries cannot escape `dest`. Returns the folder holding the
/// program (the archive may have one top-level folder or none).
pub fn extract(zip_path: &Path, dest: &Path) -> Result<PathBuf, Error> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| Error::Io(e.to_string()))?).map_err(|e| Error::BadArchive(e.to_string()))?;
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| Error::Io(e.to_string()))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::BadArchive(e.to_string()))?;
        // enclosed_name rejects absolute paths and "..", so nothing lands outside `dest`.
        let Some(rel) = entry.enclosed_name() else { return Err(Error::UnsafeArchivePath) };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| Error::Io(e.to_string()))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::Io(e.to_string()))?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| Error::Io(e.to_string()))?;
        std::io::copy(&mut entry, &mut f).map_err(|e| Error::Io(e.to_string()))?;
    }
    if dest.join(EXE).is_file() {
        return Ok(dest.to_path_buf());
    }
    // One wrapping folder, e.g. "NotePP/".
    let inner = std::fs::read_dir(dest).map_err(|e| Error::Io(e.to_string()))?.flatten().map(|e| e.path()).find(|p| p.join(EXE).is_file());
    inner.ok_or_else(|| Error::MissingExe(EXE.to_string()))
}

/// Download `rel`, check its signature and digest, unpack it, and remember it as ready to install.
/// `progress` gets 0..=100. Returns the folder with the new version.
pub fn download(rel: &Release, progress: &mut dyn FnMut(u32)) -> Result<PathBuf, Error> {
    let agent = agent();
    let get = |url: &str| agent.get(url).header("User-Agent", "NotePP-native-updater").call().map_err(|e| Error::Download(e.to_string()));
    let mut sha_res = get(&rel.sha_url)?;
    let sha_text = sha_res.body_mut().read_to_string().map_err(|e| Error::Io(e.to_string()))?;
    let mut sig_res = get(&rel.sig_url)?;
    let sig_text = sig_res.body_mut().read_to_string().map_err(|e| Error::Io(e.to_string()))?;
    check_signature(&rel.version, &sha_text, &sig_text)?;
    let expected = parse_digest(&sha_text).ok_or(Error::NoChecksum)?;

    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io(e.to_string()))?;
    let zip_path = dir.join(format!("{}.zip.part", rel.version));
    let mut res = get(&rel.zip_url)?;
    let total: u64 = res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = res.body_mut().as_reader();
    let mut file = std::fs::File::create(&zip_path).map_err(|e| Error::Io(e.to_string()))?;
    let (mut hash, mut buf, mut done, mut last) = (Sha256::new(), vec![0u8; 64 * 1024], 0u64, u32::MAX);
    loop {
        let n = reader.read(&mut buf).map_err(|e| Error::Download(e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| Error::Io(e.to_string()))?;
        done += n as u64;
        let pct = (done * 95).checked_div(total).map_or(0, crate::num::to_u32);
        if pct != last {
            last = pct;
            progress(pct);
        }
    }
    drop(file);
    let digest = crate::hash::hex(&hash.finalize());
    if digest != expected {
        let _ = std::fs::remove_file(&zip_path);
        return Err(Error::UpdateChecksumMismatch);
    }
    let staged = dir.join(&rel.version);
    let result = extract(&zip_path, &staged);
    let _ = std::fs::remove_file(&zip_path);
    let root = result?;
    let _ = std::fs::write(dir.join("ready.json"), serde_json::json!({ "version": rel.version, "dir": root }).to_string());
    progress(100);
    Ok(root)
}

/// An update downloaded earlier (maybe by a previous run) that is newer than `current`.
pub fn staged(current: &str) -> Option<(String, PathBuf)> {
    staged_in(&updates_dir(), current)
}

fn staged_in(dir: &Path, current: &str) -> Option<(String, PathBuf)> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("ready.json")).ok()?).ok()?;
    let (ver, root) = (v["version"].as_str()?.to_string(), PathBuf::from(v["dir"].as_str()?));
    (is_newer(&ver, current) && root.join(EXE).is_file()).then_some((ver, root))
}

/// Remembers which version the start-up update was tried for, so a failed one is never tried again on every start.
const ATTEMPT: &str = "apply-attempt.txt";

/// The staged update to install while starting up, once. Returns it only if it has not been tried yet, and records
/// the try first: when installing fails the next start goes on as usual instead of restarting in a loop.
pub fn claim_for_start(current: &str) -> Option<(String, PathBuf)> {
    claim_in(&updates_dir(), current)
}

fn claim_in(dir: &Path, current: &str) -> Option<(String, PathBuf)> {
    let (ver, root) = staged_in(dir, current)?;
    if std::fs::read_to_string(dir.join(ATTEMPT)).is_ok_and(|t| t.trim() == ver) {
        return None;
    }
    std::fs::write(dir.join(ATTEMPT), &ver).ok()?;
    Some((ver, root))
}

/// The version a start-up update has just installed (this run is that version), once.
pub fn take_applied(current: &str) -> Option<String> {
    applied_in(&updates_dir(), current)
}

fn applied_in(dir: &Path, current: &str) -> Option<String> {
    let path = dir.join(ATTEMPT);
    let ver = std::fs::read_to_string(&path).ok()?.trim().to_string();
    if parse_version(&ver)? > parse_version(current)? {
        return None; // tried, but this is still the old version: keep the record so it is not tried again
    }
    let _ = std::fs::remove_file(&path);
    Some(ver)
}

/// The commands that wait for this program to exit, copy the new files over it and start it again. The tools are
/// named in full: a script is run with a current folder, and a bare name would be looked for there first.
pub fn apply_script(staged: &Path, app_dir: &Path, log: &Path, files: &[PathBuf]) -> String {
    // `%` is special even inside quotes in a batch file.
    let q = |p: &Path| p.display().to_string().replace('%', "%%");
    // The files the program was asked to open come back with it.
    let files = files.iter().fold(String::new(), |all, f| all + " \"" + &q(f) + "\"");
    format!(
        "@echo off\r\n\
         setlocal\r\n\
         set n=0\r\n\
         :wait\r\n\
         (type nul >> \"{dst}\\notepp.exe\") 2>nul\r\n\
         if not errorlevel 1 goto copy\r\n\
         set /a n+=1\r\n\
         if %n% GEQ 120 exit /b 1\r\n\
         \"%SystemRoot%\\System32\\ping.exe\" -n 2 127.0.0.1 >nul\r\n\
         goto wait\r\n\
         :copy\r\n\
         \"%SystemRoot%\\System32\\xcopy.exe\" \"{src}\\*\" \"{dst}\\\" /E /Y /Q /I /R >nul 2>\"{log}\"\r\n\
         if errorlevel 1 (echo update copy failed >> \"{log}\") else (rmdir /S /Q \"{src}\" & del \"{ready}\")\r\n\
         start \"\" \"{dst}\\{EXE}\"{files}\r\n\
         (goto) 2>nul & del \"%~f0\"\r\n",
        src = q(staged),
        dst = q(app_dir),
        log = q(log),
        ready = q(&updates_dir().join("ready.json")),
    )
}

/// Write the script and start it detached. The caller then closes the app. `files` are opened by the new version.
pub fn launch_apply(staged: &Path, files: &[PathBuf]) -> Result<(), Error> {
    let exe = std::env::current_exe().map_err(|e| Error::Io(e.to_string()))?;
    let app_dir = exe.parent().ok_or(Error::NoAppFolder)?.to_path_buf();
    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io(e.to_string()))?;
    let script = dir.join("apply-update.cmd");
    std::fs::write(&script, apply_script(staged, &app_dir, &dir.join("update.log"), files)).map_err(|e| Error::Io(e.to_string()))?;
    let mut cmd = std::process::Command::new(crate::files::system_program("System32\\cmd.exe"));
    cmd.current_dir(&dir).arg("/c").arg(&script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0008); // CREATE_NO_WINDOW | DETACHED_PROCESS
    }
    cmd.spawn().map(|_| ()).map_err(|e| Error::LaunchUpdater(e.to_string()))
}

#[cfg(test)]
mod tests;
