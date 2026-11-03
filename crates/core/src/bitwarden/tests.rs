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
    assert_eq!(
        items[0],
        Item { id: "1".into(), title: "GitHub".into(), username: "me".into(), url: "https://github.com".into(), has_password: true, has_totp: false }
    );
    assert_eq!(items[1].title, "(untitled)");
    assert!(items[1].has_totp && !items[1].has_password);
    assert!(!format!("{items:?}").contains("hunter2"));
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
