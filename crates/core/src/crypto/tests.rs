use super::*;

#[test]
fn base32_round_trips_and_forgives_typos() {
    let raw: Vec<u8> = (0..32).collect();
    let text = format_recovery_key(&raw);
    assert_eq!(text.len(), 52 + 12);
    assert_eq!(base32_decode(&text).unwrap()[..32], raw[..]);
    // lower case, spaces, and the look-alike letters
    let sloppy = text.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
    assert_eq!(base32_decode(&sloppy).unwrap()[..32], raw[..]);
    assert!(base32_decode("").is_err());
    assert!(base32_decode("ab$d").is_err());
}

#[test]
fn password_strength_ratings() {
    assert_eq!(strength("short").0, 0);
    assert_eq!(strength("password").0, 1);
    assert!(strength("Tr0ub4dor&3").0 >= 2);
    assert_eq!(strength("correct-Horse-battery-9-staple").0, 4);
}

#[test]
fn iso_timestamps_look_right() {
    let t = iso_now();
    assert_eq!(t.len(), 20);
    assert!(t.starts_with("20") && t.ends_with('Z') && &t[10..11] == "T");
}

#[test]
fn whole_flow_in_one_go() {
    // One test: key derivation takes ~1 s per call in a debug build.
    let (mut p, dek, recovery) = create_profile("pässword 1").unwrap();
    assert_eq!(p.fingerprint, dek.fingerprint());
    assert_eq!(p.wrapped_keys.len(), 2);

    // password and recovery both reach the same key
    assert_eq!(unlock_with_password(&p, "pässword 1").unwrap().fingerprint(), p.fingerprint);
    assert_eq!(unlock_with_password(&p, "wrong").err(), Some(Error::WrongPassword));
    assert_eq!(unlock_with_recovery(&p, &recovery.to_lowercase()).unwrap().fingerprint(), p.fingerprint);
    assert!(unlock_with_recovery(&p, "0000-0000").is_err());

    // a file round-trips, and is not readable text
    let text = "line one\r\nsecond — ünï ✓\n".repeat(50);
    let enc = encrypt_text(&text, &dek, "md").unwrap();
    assert!(!enc.contains("second"));
    let env = parse_envelope(enc.as_bytes()).unwrap();
    assert_eq!(env.profile, p.fingerprint);
    assert_eq!(env.original_ext, "md");
    assert_eq!(decrypt_envelope(&env, &dek).unwrap(), text);
    assert!(enc.len() < text.len(), "gzip makes repetitive text smaller");

    // tampering is caught
    let mut bad = env.clone();
    let mut ct = B64.decode(&bad.ct).unwrap();
    ct[3] ^= 1;
    bad.ct = B64.encode(ct);
    assert!(decrypt_envelope(&bad, &dek).is_err());

    // a new password keeps old files readable; the old one stops working
    set_password(&mut p, &dek, "new pass 2").unwrap();
    assert!(unlock_with_password(&p, "pässword 1").is_err());
    let dek2 = unlock_with_password(&p, "new pass 2").unwrap();
    assert_eq!(decrypt_envelope(&env, &dek2).unwrap(), text);

    // a fresh recovery key replaces the old one
    let new_recovery = regenerate_recovery(&mut p, &dek2).unwrap();
    assert_ne!(new_recovery, recovery);
    assert!(unlock_with_recovery(&p, &recovery).is_err());
    assert!(unlock_with_recovery(&p, &new_recovery).is_ok());

    // the profile survives a trip through JSON
    assert_eq!(Profile::parse(&p.to_json()).unwrap(), p);
}

#[test]
fn ordinary_files_are_not_envelopes() {
    assert!(parse_envelope(b"hello").is_none());
    assert!(parse_envelope(b"{\"a\": 1}").is_none());
    assert!(parse_envelope(b"").is_none());
    assert!(parse_envelope(b"{\"_notepp_encrypted\": true").is_none()); // truncated
}

#[test]
fn files_from_the_electron_app_open() {
    let fx: serde_json::Value = serde_json::from_str(include_str!("../../tests/fixtures/electron_interop.json")).unwrap();
    let profile = Profile::parse(&fx["profile"].to_string()).unwrap();
    let env_text = fx["envelope"].to_string();
    let env = parse_envelope(env_text.as_bytes()).unwrap();
    assert_eq!(env.profile, profile.fingerprint);

    let dek = unlock_with_password(&profile, fx["password"].as_str().unwrap()).unwrap();
    assert_eq!(dek.fingerprint(), profile.fingerprint);
    assert_eq!(decrypt_envelope(&env, &dek).unwrap(), fx["text"].as_str().unwrap());

    // the printed recovery key works too
    let dek = unlock_with_recovery(&profile, fx["recoveryKey"].as_str().unwrap()).unwrap();
    assert_eq!(decrypt_envelope(&env, &dek).unwrap(), fx["text"].as_str().unwrap());
}
