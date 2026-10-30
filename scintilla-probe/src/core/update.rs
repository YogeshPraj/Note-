//! Auto-update for the native app: ask GitHub whether a newer release exists, download it in the
//! background, check it, unpack it next to nothing else, and swap it in on the user's click.
//!
//! Releases of this app are tagged `native-vX.Y.Z` (the repository also carries the Electron
//! app's `vX.Y.Z` releases, which are ignored). A release carries two assets:
//! `NotePP-native-windows-x64.zip` and `NotePP-native-windows-x64.zip.sha256` (the hex digest).
//! The download is only unpacked when its SHA-256 matches. That catches damaged or swapped
//! files; it is not a code signature, because the app is not signed.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_API: &str = "https://api.github.com/repos/YogeshPraj/Note-/releases?per_page=30";
const TAG_PREFIX: &str = "native-v";
pub const ASSET: &str = "NotePP-native-windows-x64.zip";
/// The program inside the archive; an archive without it is refused.
const EXE: &str = "sci_probe.exe";

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub zip_url: String,
    pub sha_url: String,
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

/// The newest published native release that is newer than `current` and has both assets.
pub fn pick_release(json: &str, current: &str) -> Result<Option<Release>, String> {
    let v: Value = serde_json::from_str(json).map_err(|_| "GitHub sent an answer that could not be read".to_string())?;
    let list = v.as_array().ok_or_else(|| v.get("message").and_then(Value::as_str).unwrap_or("unexpected answer from GitHub").to_string())?;
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
        let asset = |name: &str| r["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(name))?["browser_download_url"].as_str().map(str::to_string);
        let (Some(zip_url), Some(sha_url)) = (asset(ASSET), asset(&format!("{ASSET}.sha256"))) else { continue };
        if best.as_ref().is_none_or(|(b, _)| ver > *b) {
            best = Some((ver, Release { version: tag.to_string(), zip_url, sha_url }));
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
pub fn check(current: &str) -> Result<Option<Release>, String> {
    let mut res = agent()
        .get(&api_url())
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "NotePP-native-updater")
        .call()
        .map_err(|e| format!("Could not reach GitHub: {e}"))?;
    let body = res.body_mut().read_to_string().map_err(|e| format!("Could not read GitHub's answer: {e}"))?;
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

/// Unpack `zip_path` into `dest`. Entries cannot escape `dest`. Returns the folder holding the
/// program (the archive may have one top-level folder or none).
pub fn extract(zip_path: &Path, dest: &Path) -> Result<PathBuf, String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| e.to_string())?).map_err(|e| format!("The download is not a valid archive: {e}"))?;
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        // enclosed_name rejects absolute paths and "..", so nothing lands outside `dest`.
        let Some(rel) = entry.enclosed_name() else { return Err("The archive contains an unsafe path".into()) };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = std::fs::File::create(&out).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
    }
    if dest.join(EXE).is_file() {
        return Ok(dest.to_path_buf());
    }
    // One wrapping folder, e.g. "NotePP/".
    let inner = std::fs::read_dir(dest).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join(EXE).is_file());
    inner.ok_or_else(|| format!("The download does not contain {EXE}"))
}

/// Download `rel`, check its digest, unpack it, and remember it as ready to install.
/// `progress` gets 0..=100. Returns the folder with the new version.
pub fn download(rel: &Release, progress: &mut dyn FnMut(u32)) -> Result<PathBuf, String> {
    let agent = agent();
    let get = |url: &str| agent.get(url).header("User-Agent", "NotePP-native-updater").call().map_err(|e| format!("Download failed: {e}"));
    let mut sha_res = get(&rel.sha_url)?;
    let sha_text = sha_res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let expected = parse_digest(&sha_text).ok_or("The release has no valid checksum file")?;

    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip_path = dir.join(format!("{}.zip.part", rel.version));
    let mut res = get(&rel.zip_url)?;
    let total: u64 = res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = res.body_mut().as_reader();
    let mut file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let (mut hash, mut buf, mut done, mut last) = (Sha256::new(), vec![0u8; 64 * 1024], 0u64, u32::MAX);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Download failed: {e}"))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        let pct = if total > 0 { (done * 95 / total) as u32 } else { 0 };
        if pct != last {
            last = pct;
            progress(pct);
        }
    }
    drop(file);
    let digest: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if digest != expected {
        let _ = std::fs::remove_file(&zip_path);
        return Err("The download does not match its checksum - it was not installed.".into());
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
    let v: Value = serde_json::from_str(&std::fs::read_to_string(updates_dir().join("ready.json")).ok()?).ok()?;
    let (ver, dir) = (v["version"].as_str()?.to_string(), PathBuf::from(v["dir"].as_str()?));
    (is_newer(&ver, current) && dir.join(EXE).is_file()).then_some((ver, dir))
}

/// The commands that wait for this program to exit, copy the new files over it and start it again.
pub fn apply_script(staged: &Path, app_dir: &Path, log: &Path) -> String {
    // `%` is special even inside quotes in a batch file.
    let q = |p: &Path| p.display().to_string().replace('%', "%%");
    format!(
        "@echo off\r\n\
         setlocal\r\n\
         set n=0\r\n\
         :wait\r\n\
         (type nul >> \"{dst}\\sci_probe.exe\") 2>nul\r\n\
         if not errorlevel 1 goto copy\r\n\
         set /a n+=1\r\n\
         if %n% GEQ 120 exit /b 1\r\n\
         ping -n 2 127.0.0.1 >nul\r\n\
         goto wait\r\n\
         :copy\r\n\
         xcopy \"{src}\\*\" \"{dst}\\\" /E /Y /Q /I /R >nul 2>\"{log}\"\r\n\
         if errorlevel 1 (echo update copy failed >> \"{log}\") else (rmdir /S /Q \"{src}\" & del \"{ready}\")\r\n\
         start \"\" \"{dst}\\{EXE}\"\r\n\
         (goto) 2>nul & del \"%~f0\"\r\n",
        src = q(staged),
        dst = q(app_dir),
        log = q(log),
        ready = q(&updates_dir().join("ready.json")),
    )
}

/// Write the script and start it detached. The caller then closes the app.
pub fn launch_apply(staged: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let app_dir = exe.parent().ok_or("cannot find the app folder")?.to_path_buf();
    let dir = updates_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("apply-update.cmd");
    std::fs::write(&script, apply_script(staged, &app_dir, &dir.join("update.log"))).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("cmd");
    cmd.arg("/c").arg(&script).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000 | 0x0000_0008); // CREATE_NO_WINDOW | DETACHED_PROCESS
    }
    cmd.spawn().map(|_| ()).map_err(|e| format!("Cannot start the updater: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_version("0.10"), Some((0, 10, 0)));
        assert_eq!(parse_version("2.0.0-beta.1"), Some((2, 0, 0)));
        assert_eq!(parse_version("x"), None);
        assert!(is_newer("0.10.0", "0.9.9") && is_newer("1.0.0", "0.99.99"));
        assert!(!is_newer("0.1.0", "0.1.0") && !is_newer("0.0.9", "0.1.0") && !is_newer("junk", "0.1.0"));
    }

    fn release(tag: &str, draft: bool, pre: bool, assets: &[&str]) -> Value {
        serde_json::json!({
            "tag_name": tag, "draft": draft, "prerelease": pre,
            "assets": assets.iter().map(|a| serde_json::json!({ "name": a, "browser_download_url": format!("https://example.test/{tag}/{a}") })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn picks_the_newest_complete_native_release() {
        let both = [ASSET, "NotePP-native-windows-x64.zip.sha256"];
        let list = serde_json::json!([
            release("v9.9.9", false, false, &["Note++ Setup.exe"]), // the Electron app: ignored
            release("native-v0.3.0", true, false, &both),           // draft
            release("native-v0.2.5", false, true, &both),           // prerelease
            release("native-v0.2.0", false, false, &both),
            release("native-v0.2.1", false, false, &[ASSET]),       // no checksum: unusable
            release("native-v0.1.0", false, false, &both),          // not newer
            release("native-v0.1.5", false, false, &both),
        ])
        .to_string();
        let r = pick_release(&list, "0.1.0").unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(r.zip_url.ends_with("native-v0.2.0/NotePP-native-windows-x64.zip") && r.sha_url.ends_with(".zip.sha256"));
        assert!(pick_release(&list, "0.2.0").unwrap().is_none(), "drafts, prereleases and incomplete releases are skipped");
        assert!(pick_release(&list, "0.9.0").unwrap().is_none());
        assert!(pick_release("[]", "0.1.0").unwrap().is_none());
        assert!(pick_release("{\"message\":\"API rate limit exceeded\"}", "0.1.0").unwrap_err().contains("rate limit"));
        assert!(pick_release("not json", "0.1.0").is_err());
    }

    #[test]
    fn digests_are_read_from_sha256_files() {
        let d = "a".repeat(64);
        assert_eq!(parse_digest(&format!("{d}  NotePP.zip\n")), Some(d.clone()));
        assert_eq!(parse_digest(&d.to_uppercase()), Some(d));
        assert_eq!(parse_digest("short"), None);
        assert_eq!(parse_digest(""), None);
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut w = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap();
    }

    #[test]
    fn extracts_flat_and_wrapped_archives_and_refuses_bad_ones() {
        let base = std::env::temp_dir().join(format!("notepp_upd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();

        let flat = base.join("flat.zip");
        make_zip(&flat, &[("sci_probe.exe", b"MZ"), ("assets/a.txt", b"x")]);
        let root = extract(&flat, &base.join("out_flat")).unwrap();
        assert!(root.join("sci_probe.exe").is_file() && root.join("assets").join("a.txt").is_file());

        let wrapped = base.join("wrapped.zip");
        make_zip(&wrapped, &[("NotePP/sci_probe.exe", b"MZ"), ("NotePP/lib.dll", b"d")]);
        let root = extract(&wrapped, &base.join("out_wrapped")).unwrap();
        assert!(root.ends_with("NotePP") && root.join("lib.dll").is_file());

        let no_exe = base.join("noexe.zip");
        make_zip(&no_exe, &[("readme.txt", b"hi")]);
        assert!(extract(&no_exe, &base.join("out_noexe")).unwrap_err().contains("sci_probe.exe"));

        let evil = base.join("evil.zip");
        make_zip(&evil, &[("../escape.txt", b"no"), ("sci_probe.exe", b"MZ")]);
        assert!(extract(&evil, &base.join("out_evil")).is_err());
        assert!(!base.join("escape.txt").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_apply_script_waits_copies_restarts_and_cleans_up() {
        let s = apply_script(Path::new("C:\\u\\0.2.0"), Path::new("C:\\Apps\\Note PP"), Path::new("C:\\u\\update.log"));
        assert!(s.contains("sci_probe.exe") && s.contains("GEQ 120"), "gives up after two minutes");
        assert!(s.contains("xcopy \"C:\\u\\0.2.0\\*\" \"C:\\Apps\\Note PP\\\" /E /Y"));
        assert!(s.contains("start \"\" \"C:\\Apps\\Note PP\\sci_probe.exe\""));
        assert!(s.contains("del \"%~f0\""));
        assert!(apply_script(Path::new("C:\\100%"), Path::new("D:\\a"), Path::new("l")).contains("100%%"));
    }
}
