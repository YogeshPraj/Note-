//! Installing the Bitwarden CLI: the pinned download, checked and unpacked into the app's folder.

use super::*;

// ---- installing -----------------------------------------------------------------------------

/// What the Passwords dialog shows about the installation.
#[derive(Debug)]
pub struct InstallInfo {
    pub installed: bool,
    pub supported: bool,
    pub bundled: bool,
}

pub fn install_info() -> InstallInfo {
    InstallInfo { installed: is_installed(), supported: asset().is_some(), bundled: bundled_bin().is_some() }
}

/// Download, verify and unpack the official CLI. `progress` gets (percent, what is happening).
pub fn download(progress: &mut dyn FnMut(u32, &str)) -> Result<(), Error> {
    let a = asset().ok_or(Error::NoBitwardenClient)?;
    let url = format!("{RELEASE_BASE}{}", a.file);
    let tmp = std::env::temp_dir().join(format!("bw-{BW_VERSION}-{}.zip", std::process::id()));
    let result = download_to(&url, &tmp, a, progress);
    let _ = std::fs::remove_file(&tmp);
    result
}

pub(super) fn download_to(url: &str, tmp: &std::path::Path, a: &Asset, progress: &mut dyn FnMut(u32, &str)) -> Result<(), Error> {
    progress(0, "Downloading the Bitwarden CLI");
    let agent: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into();
    let mut res = agent.get(url).call().map_err(|e| Error::Download(e.to_string()))?;
    let total: u64 = res.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(0);
    let mut reader = res.body_mut().as_reader();
    let mut file = std::fs::File::create(tmp).map_err(|e| Error::Io(e.to_string()))?;
    let mut hash = Sha256::new();
    let (mut buf, mut done, mut last) = (vec![0u8; 64 * 1024], 0u64, 0u32);
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let n = reader.read(&mut buf).map_err(|e| Error::Download(e.to_string()))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| Error::Io(e.to_string()))?;
        done += n as u64;
        if let Some(pct) = (done * 90).checked_div(total).map(crate::num::to_u32)
            && pct != last
        {
            last = pct;
            progress(pct, "Downloading the Bitwarden CLI");
        }
    }
    drop(file);
    // Verify BEFORE anything is extracted or executed.
    let digest = crate::hash::hex(&hash.finalize());
    if digest != a.sha256 {
        return Err(Error::ChecksumMismatch { expected: a.sha256.to_string(), got: digest });
    }
    progress(92, "Unpacking");
    install_zip(tmp, a, &digest)?;
    progress(100, "Done");
    Ok(())
}

/// Unpack `zip_path` and put the `bw` binary in the bundle folder.
pub(super) fn install_zip(zip_path: &std::path::Path, a: &Asset, digest: &str) -> Result<(), Error> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path).map_err(|e| Error::Io(e.to_string()))?).map_err(|e| Error::BadArchive(e.to_string()))?;
    let dir = bundle_dir();
    std::fs::create_dir_all(&dir).map_err(|e| Error::Io(e.to_string()))?;
    let mut found = false;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::BadArchive(e.to_string()))?;
        // Only the binary is wanted, and never under a path the archive chooses.
        let name = entry.name().map_err(|e| Error::BadArchive(e.to_string()))?.rsplit(['/', '\\']).next().unwrap_or("").to_string();
        if name != a.bin || entry.is_dir() {
            continue;
        }
        let dest = dir.join(a.bin);
        let part = dir.join(format!("{}.part", a.bin));
        let mut out = std::fs::File::create(&part).map_err(|e| Error::Io(e.to_string()))?;
        std::io::copy(&mut entry, &mut out).map_err(|e| Error::Io(e.to_string()))?;
        drop(out);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755));
        }
        let _ = std::fs::remove_file(&dest);
        std::fs::rename(&part, &dest).map_err(|e| Error::Io(e.to_string()))?;
        found = true;
        break;
    }
    if !found {
        return Err(Error::MissingInArchive(a.bin.to_string()));
    }
    let meta = serde_json::json!({ "version": BW_VERSION, "sha256": digest });
    let _ = std::fs::write(dir.join("meta.json"), meta.to_string());
    Ok(())
}

pub fn uninstall() {
    lock();
    let _ = std::fs::remove_dir_all(bundle_dir());
}
