use super::*;

#[test]
fn the_issue_page_opens_the_repository_with_version_and_system_filled_in() {
    let url = issue_url("Windows 11", "release");
    assert!(url.starts_with("https://github.com/YogeshPraj/Note-/issues/new?body="));
    assert!(url.contains(&format!("Version%3A%2A%2A%20{VERSION}")));
    assert!(url.contains("Windows%2011"));
    assert!(!url.contains(' ') && !url.contains('\n'));
}

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
        "assets": assets.iter().map(|a| serde_json::json!({ "name": a, "browser_download_url": format!("{DOWNLOADS}{tag}/{a}") })).collect::<Vec<_>>()
    })
}

#[test]
fn picks_the_newest_complete_native_release() {
    let all = [ASSET, "NotePP-windows-x64.zip.sha256", "NotePP-windows-x64.zip.sha256.sig"];
    let list = serde_json::json!([
        release("v9.9.9", false, false, &["Note++ Setup.exe"]), // the previous app: no zip, ignored
        release("v0.3.0", true, false, &all),                   // draft
        release("v0.2.5", false, true, &all),                   // prerelease
        release("v0.2.0", false, false, &all),
        release("v0.2.1", false, false, &[ASSET]),  // no checksum: unusable
        release("v0.2.2", false, false, &all[..2]), // no signature: unusable
        release("v0.1.0", false, false, &all),      // not newer
        release("v0.1.5", false, false, &all),
    ])
    .to_string();
    let r = pick_release(&list, "0.1.0").unwrap().unwrap();
    assert_eq!(r.version, "0.2.0");
    assert!(r.zip_url.ends_with("v0.2.0/NotePP-windows-x64.zip") && r.sha_url.ends_with(".zip.sha256") && r.sig_url.ends_with(".zip.sha256.sig"));
    assert!(pick_release(&list, "0.2.0").unwrap().is_none(), "drafts, prereleases and incomplete releases are skipped");
    assert!(pick_release(&list, "0.9.0").unwrap().is_none());
    assert!(pick_release("[]", "0.1.0").unwrap().is_none());
    assert!(pick_release("{\"message\":\"API rate limit exceeded\"}", "0.1.0").unwrap_err().to_string().contains("rate limit"));
    assert!(pick_release("not json", "0.1.0").is_err());
}

#[test]
fn update_files_from_anywhere_else_are_ignored() {
    let elsewhere = |a: &str| serde_json::json!({ "name": a, "browser_download_url": format!("https://example.test/v9.0.0/{a}") });
    let list = serde_json::json!([{ "tag_name": "v9.0.0", "assets": [elsewhere(ASSET), elsewhere("NotePP-windows-x64.zip.sha256"), elsewhere("NotePP-windows-x64.zip.sha256.sig")] }]);
    assert!(pick_release(&list.to_string(), "0.1.0").unwrap().is_none());
}

#[test]
fn only_the_key_s_signature_of_this_version_and_digest_is_accepted() {
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};
    let pair = |seed: &SystemRandom| Ed25519KeyPair::from_pkcs8(Ed25519KeyPair::generate_pkcs8(seed).unwrap().as_ref()).unwrap();
    let random = SystemRandom::new();
    let (key, other) = (pair(&random), pair(&random));
    let public = key.public_key().as_ref();
    let digest = format!("{}  NotePP-windows-x64.zip\r\n", "a".repeat(64));
    let signature = STANDARD.encode(key.sign(&signed_message("2.0.8", &digest)));

    assert!(signed_by(public, &signed_message("2.0.8", &digest), &format!("{signature}\n")));
    assert!(!signed_by(public, &signed_message("9.0.0", &digest), &signature), "an old build cannot pose as a new one");
    assert!(!signed_by(public, &signed_message("2.0.8", &digest.replace('a', "b")), &signature), "another digest");
    assert!(!signed_by(other.public_key().as_ref(), &signed_message("2.0.8", &digest), &signature), "another key");
    assert!(!signed_by(public, &signed_message("2.0.8", &digest), ""));
    assert!(!signed_by(public, &signed_message("2.0.8", &digest), "not base64!"));
    let own_key = STANDARD.encode(other.sign(&signed_message("2.0.8", &digest)));
    assert!(matches!(check_signature("2.0.8", &digest, &own_key), Err(Error::UpdateNotSigned)), "the built-in key decides");
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
    make_zip(&flat, &[("notepp.exe", b"MZ"), ("assets/a.txt", b"x")]);
    let root = extract(&flat, &base.join("out_flat")).unwrap();
    assert!(root.join("notepp.exe").is_file() && root.join("assets").join("a.txt").is_file());

    let wrapped = base.join("wrapped.zip");
    make_zip(&wrapped, &[("NotePP/notepp.exe", b"MZ"), ("NotePP/lib.dll", b"d")]);
    let root = extract(&wrapped, &base.join("out_wrapped")).unwrap();
    assert!(root.ends_with("NotePP") && root.join("lib.dll").is_file());

    let no_exe = base.join("noexe.zip");
    make_zip(&no_exe, &[("readme.txt", b"hi")]);
    assert!(extract(&no_exe, &base.join("out_noexe")).unwrap_err().to_string().contains("notepp.exe"));

    let evil = base.join("evil.zip");
    make_zip(&evil, &[("../escape.txt", b"no"), ("notepp.exe", b"MZ")]);
    assert!(extract(&evil, &base.join("out_evil")).is_err());
    assert!(!base.join("escape.txt").exists());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn the_apply_script_waits_copies_restarts_and_cleans_up() {
    let s = apply_script(Path::new("C:\\u\\0.2.0"), Path::new("C:\\Apps\\Note PP"), Path::new("C:\\u\\update.log"), &[]);
    assert!(s.contains("notepp.exe") && s.contains("GEQ 120"), "gives up after two minutes");
    assert!(s.contains("\"%SystemRoot%\\System32\\xcopy.exe\" \"C:\\u\\0.2.0\\*\" \"C:\\Apps\\Note PP\\\" /E /Y"), "named in full");
    assert!(s.contains("\"%SystemRoot%\\System32\\ping.exe\" -n 2"), "named in full");
    assert!(s.contains("start \"\" \"C:\\Apps\\Note PP\\notepp.exe\""));
    assert!(s.contains("del \"%~f0\""));
    assert!(s.contains("del /Q \"C:\\Apps\\Note PP\\Scintilla.dll\""), "a file of older versions goes");
    assert!(apply_script(Path::new("C:\\100%"), Path::new("D:\\a"), Path::new("l"), &[]).contains("100%%"));
}

#[test]
fn the_files_the_program_was_started_with_come_back_with_the_new_version() {
    let files = [PathBuf::from("C:\\docs\\a b.txt"), PathBuf::from("D:\\100%.md")];
    let s = apply_script(Path::new("C:\\u\\1"), Path::new("C:\\Apps\\N"), Path::new("l"), &files);
    assert!(s.contains("start \"\" \"C:\\Apps\\N\\notepp.exe\" \"C:\\docs\\a b.txt\" \"D:\\100%%.md\"\r\n"));
}

fn staged_dir(name: &str, version: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("npp_upd_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let root = dir.join(version);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("notepp.exe"), b"MZ").unwrap();
    std::fs::write(dir.join("ready.json"), serde_json::json!({ "version": version, "dir": root }).to_string()).unwrap();
    dir
}

#[test]
fn a_downloaded_update_is_installed_at_the_next_start_once() {
    let dir = staged_dir("claim", "2.0.4");
    assert_eq!(claim_in(&dir, "2.0.3").map(|c| c.0), Some("2.0.4".to_string()));
    // It was tried and the program is still the old version (the install failed): no second try, no restart loop.
    assert_eq!(claim_in(&dir, "2.0.3"), None);
    assert_eq!(applied_in(&dir, "2.0.3"), None);
    assert!(dir.join(ATTEMPT).is_file(), "the record stays so that it is not tried again");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn only_a_newer_version_is_installed_and_success_is_reported_once() {
    let dir = staged_dir("newer", "2.0.4");
    assert_eq!(claim_in(&dir, "2.0.4"), None, "the same version is not an update");
    assert_eq!(claim_in(&dir, "3.0.0"), None, "nor is an older one");
    let _ = std::fs::remove_file(dir.join(ATTEMPT));
    assert!(claim_in(&dir, "2.0.3").is_some());
    // The new version started: it is told once that it was updated.
    assert_eq!(applied_in(&dir, "2.0.4"), Some("2.0.4".to_string()));
    assert_eq!(applied_in(&dir, "2.0.4"), None);
    let _ = std::fs::remove_dir_all(&dir);
}
