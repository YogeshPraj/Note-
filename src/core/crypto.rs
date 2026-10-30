//! Encrypted pad: password-protected files, in exactly the formats the Electron app uses, so the
//! same files and the same `profile.json` work in both.
//!
//! A random 256-bit data key (DEK) encrypts every file (gzip, then AES-256-GCM). The DEK itself is
//! stored only wrapped: once by a key derived from the password (PBKDF2-HMAC-SHA-256, 600 000
//! rounds) and once by a key derived from a printable recovery key (HKDF-SHA-256). Changing the
//! password therefore never touches the files.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::PathBuf;
use zeroize::Zeroizing;

pub const ITERATIONS: u32 = 600_000;
const SALT_BYTES: usize = 16;
const IV_BYTES: usize = 12;
const DEK_BYTES: usize = 32;
const RECOVERY_BYTES: usize = 32;
/// Crockford base32: no I, L, O or U.
const C32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const RECOVERY_INFO: &[u8] = b"notepp-recovery";

/// The data-encryption key. Wiped from memory when dropped.
#[derive(Clone)]
pub struct Dek(Zeroizing<[u8; DEK_BYTES]>);

impl Dek {
    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(&self.0[..]).expect("key length is fixed")
    }

    /// First 8 hex characters of SHA-256(DEK): which profile a file belongs to.
    pub fn fingerprint(&self) -> String {
        let h = Sha256::digest(&self.0[..]);
        h[..4].iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Wrapped {
    pub method: String,
    pub kdf: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iter: Option<u32>,
    pub salt: String,
    pub wrapped_dek: String,
    pub wrap_iv: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    #[serde(rename = "_notepp_profile")]
    pub flag: bool,
    pub version: u32,
    pub fingerprint: String,
    pub created_at: String,
    pub wrapped_keys: Vec<Wrapped>,
}

impl Profile {
    /// Reads a profile file; None when it is missing or not a profile.
    pub fn parse(text: &str) -> Option<Profile> {
        let p: Profile = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
        (p.flag && !p.wrapped_keys.is_empty()).then_some(p)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// "5f4d-cc3b", as shown to the user.
    pub fn fingerprint_display(&self) -> String {
        display_fingerprint(&self.fingerprint)
    }
}

pub fn display_fingerprint(fp: &str) -> String {
    if fp.len() == 8 { format!("{}-{}", &fp[..4], &fp[4..]) } else { fp.to_string() }
}

/// The JSON that is a protected file on disk.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    #[serde(rename = "_notepp_encrypted")]
    pub flag: bool,
    pub version: u32,
    pub alg: String,
    pub compression: String,
    pub profile: String,
    pub iv: String,
    pub ct: String,
    #[serde(default)]
    pub original_ext: String,
    #[serde(default)]
    pub created_at: String,
}

/// The envelope held in `bytes`, if the file is one. Cheap on ordinary files: it only parses
/// text that starts with `{` and names the marker.
pub fn parse_envelope(bytes: &[u8]) -> Option<Envelope> {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let first = body.iter().find(|b| !b.is_ascii_whitespace())?;
    if *first != b'{' {
        return None;
    }
    let head = &body[..body.len().min(4096)];
    if !head.windows(18).any(|w| w == b"\"_notepp_encrypted") && !body.windows(18).any(|w| w == b"\"_notepp_encrypted") {
        return None;
    }
    let e: Envelope = serde_json::from_slice(body).ok()?;
    e.flag.then_some(e)
}

// ---- bytes helpers --------------------------------------------------------------------------

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(|e| format!("no secure random numbers: {e}"))?;
    Ok(b)
}

fn b64d(s: &str) -> Result<Vec<u8>, String> {
    B64.decode(s.trim()).map_err(|_| "corrupt data (bad base64)".to_string())
}

/// ISO-8601 UTC timestamp, like JavaScript's `toISOString()` without the milliseconds.
pub fn iso_now() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, rem % 3600 / 60, rem % 60)
}

// ---- Crockford base32 (the recovery key) ----------------------------------------------------

pub fn base32_encode(bytes: &[u8]) -> String {
    let (mut bits, mut value, mut out) = (0u32, 0u32, String::new());
    for b in bytes {
        value = (value << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(C32[((value >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
        value &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(C32[((value << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Forgiving decode: case, spaces and dashes are ignored; I/L read as 1, O as 0, U as V.
pub fn base32_decode(text: &str) -> Result<Vec<u8>, String> {
    let clean: String = text
        .to_uppercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c {
            'I' | 'L' => '1',
            'O' => '0',
            'U' => 'V',
            c => c,
        })
        .collect();
    if clean.is_empty() {
        return Err("The recovery key is empty".into());
    }
    let (mut bits, mut value, mut out) = (0u32, 0u32, Vec::new());
    for c in clean.chars() {
        let idx = C32.iter().position(|x| *x as char == c).ok_or_else(|| format!("Not a valid recovery key character: {c}"))?;
        value = (value << 5) | idx as u32;
        bits += 5;
        if bits >= 8 {
            out.push(((value >> (bits - 8)) & 0xFF) as u8);
            bits -= 8;
            value &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

/// ABCD-EFGH-... groups of four, for reading aloud and typing.
pub fn format_recovery_key(raw: &[u8]) -> String {
    let s = base32_encode(raw);
    s.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect::<Vec<_>>().join("-")
}

// ---- key wrapping ---------------------------------------------------------------------------

fn kek_from_password(password: &str, salt: &[u8], iter: u32) -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iter, &mut k[..]);
    k
}

fn kek_from_recovery(raw: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    let mut k = Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<Sha256>::new(Some(salt), raw).expand(RECOVERY_INFO, &mut k[..]).map_err(|_| "key derivation failed".to_string())?;
    Ok(k)
}

fn seal(key: &[u8; 32], plain: &[u8]) -> Result<([u8; IV_BYTES], Vec<u8>), String> {
    let iv = random::<IV_BYTES>()?;
    let c = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
    let ct = c.encrypt(Nonce::from_slice(&iv), plain).map_err(|_| "encryption failed".to_string())?;
    Ok((iv, ct))
}

fn open(key: &[u8; 32], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, ()> {
    if iv.len() != IV_BYTES {
        return Err(());
    }
    let c = Aes256Gcm::new_from_slice(key).map_err(|_| ())?;
    c.decrypt(Nonce::from_slice(iv), ct).map_err(|_| ())
}

fn wrap(dek: &Dek, kek: &[u8; 32]) -> Result<(String, String), String> {
    let (iv, ct) = seal(kek, &dek.0[..])?;
    Ok((B64.encode(&ct), B64.encode(iv)))
}

fn unwrap_dek(entry: &Wrapped, kek: &[u8; 32]) -> Result<Dek, ()> {
    let raw = open(kek, &b64d(&entry.wrap_iv).map_err(|_| ())?, &b64d(&entry.wrapped_dek).map_err(|_| ())?)?;
    let arr: [u8; DEK_BYTES] = raw.try_into().map_err(|_| ())?;
    Ok(Dek(Zeroizing::new(arr)))
}

fn password_entry(dek: &Dek, password: &str) -> Result<Wrapped, String> {
    let salt = random::<SALT_BYTES>()?;
    let kek = kek_from_password(password, &salt, ITERATIONS);
    let (wrapped_dek, wrap_iv) = wrap(dek, &kek)?;
    Ok(Wrapped { method: "password".into(), kdf: "PBKDF2-SHA256".into(), iter: Some(ITERATIONS), salt: B64.encode(salt), wrapped_dek, wrap_iv })
}

fn recovery_entry(dek: &Dek, raw: &[u8]) -> Result<Wrapped, String> {
    let salt = random::<SALT_BYTES>()?;
    let kek = kek_from_recovery(raw, &salt)?;
    let (wrapped_dek, wrap_iv) = wrap(dek, &kek)?;
    Ok(Wrapped { method: "recovery".into(), kdf: "HKDF-SHA256".into(), iter: None, salt: B64.encode(salt), wrapped_dek, wrap_iv })
}

fn replace_entry(p: &mut Profile, e: Wrapped) {
    match p.wrapped_keys.iter().position(|k| k.method == e.method) {
        Some(i) => p.wrapped_keys[i] = e,
        None => p.wrapped_keys.push(e),
    }
}

// ---- the profile ----------------------------------------------------------------------------

/// A new profile protected by `password`. Returns the profile, the unlocked key and the recovery
/// key text (shown to the user once, never stored).
pub fn create_profile(password: &str) -> Result<(Profile, Dek, String), String> {
    let dek = Dek(Zeroizing::new(random::<DEK_BYTES>()?));
    let recovery = Zeroizing::new(random::<RECOVERY_BYTES>()?);
    let profile = Profile {
        flag: true,
        version: 1,
        fingerprint: dek.fingerprint(),
        created_at: iso_now(),
        wrapped_keys: vec![password_entry(&dek, password)?, recovery_entry(&dek, &recovery[..])?],
    };
    Ok((profile, dek, format_recovery_key(&recovery[..])))
}

pub fn unlock_with_password(p: &Profile, password: &str) -> Result<Dek, String> {
    let e = p.wrapped_keys.iter().find(|k| k.method == "password").ok_or("This profile has no password")?;
    let salt = b64d(&e.salt)?;
    let kek = kek_from_password(password, &salt, e.iter.unwrap_or(ITERATIONS));
    unwrap_dek(e, &kek).map_err(|_| "Incorrect password".to_string())
}

pub fn unlock_with_recovery(p: &Profile, key_text: &str) -> Result<Dek, String> {
    let e = p.wrapped_keys.iter().find(|k| k.method == "recovery").ok_or("This profile has no recovery key")?;
    let raw = Zeroizing::new(base32_decode(key_text)?);
    if raw.len() < RECOVERY_BYTES {
        return Err("That recovery key is too short".into());
    }
    let kek = kek_from_recovery(&raw[..RECOVERY_BYTES], &b64d(&e.salt)?)?;
    unwrap_dek(e, &kek).map_err(|_| "That recovery key does not match this profile".to_string())
}

/// Replace the password. Files are not touched: the DEK is unchanged.
pub fn set_password(p: &mut Profile, dek: &Dek, new_password: &str) -> Result<(), String> {
    replace_entry(p, password_entry(dek, new_password)?);
    Ok(())
}

/// Replace the recovery key; returns the new one.
pub fn regenerate_recovery(p: &mut Profile, dek: &Dek) -> Result<String, String> {
    let raw = Zeroizing::new(random::<RECOVERY_BYTES>()?);
    replace_entry(p, recovery_entry(dek, &raw[..])?);
    Ok(format_recovery_key(&raw[..]))
}

/// The downloadable recovery.json.
pub fn recovery_file(fingerprint: &str, recovery_key: &str) -> String {
    let compact: String = recovery_key.chars().filter(|c| *c != '-').collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "_notepp_recovery": true,
        "version": 1,
        "fingerprint": fingerprint,
        "createdAt": iso_now(),
        "recoveryKey": compact,
    }))
    .unwrap_or_default()
}

/// The key out of a recovery.json (or the bare key text).
pub fn recovery_key_from_file(text: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    v.get("recoveryKey")?.as_str().map(str::to_string)
}

// ---- files ----------------------------------------------------------------------------------

pub fn encrypt_text(text: &str, dek: &Dek, original_ext: &str) -> Result<String, String> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    let packed = gz.finish().map_err(|e| e.to_string())?;
    let iv = random::<IV_BYTES>()?;
    let ct = dek.cipher().encrypt(Nonce::from_slice(&iv), packed.as_slice()).map_err(|_| "encryption failed".to_string())?;
    let env = Envelope {
        flag: true,
        version: 1,
        alg: "AES-256-GCM".into(),
        compression: "gzip".into(),
        profile: dek.fingerprint(),
        iv: B64.encode(iv),
        ct: B64.encode(ct),
        original_ext: original_ext.to_string(),
        created_at: iso_now(),
    };
    serde_json::to_string_pretty(&env).map_err(|e| e.to_string())
}

pub fn decrypt_envelope(env: &Envelope, dek: &Dek) -> Result<String, String> {
    let inner = dek
        .cipher()
        .decrypt(Nonce::from_slice(&b64d(&env.iv)?), b64d(&env.ct)?.as_slice())
        .map_err(|_| "This file could not be decrypted: wrong key, or it was changed after it was saved".to_string())?;
    let plain = if env.compression == "gzip" {
        let mut out = Vec::new();
        GzDecoder::new(inner.as_slice()).read_to_end(&mut out).map_err(|e| format!("corrupt data: {e}"))?;
        out
    } else {
        inner
    };
    String::from_utf8(plain).map_err(|_| "The decrypted text is not valid UTF-8".to_string())
}

// ---- where the profile lives ----------------------------------------------------------------

/// This app's profile file.
pub fn profile_path() -> PathBuf {
    super::session::config_dir().join("encryption").join("profile.json")
}

/// The Electron app's profile file, which can be adopted so existing encrypted files still open.
pub fn electron_profile_path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("notepp").join("encryption").join("profile.json"))
}

pub fn load_profile() -> Option<Profile> {
    Profile::parse(&std::fs::read_to_string(profile_path()).ok()?)
}

pub fn save_profile(p: &Profile) -> Result<(), String> {
    if let Some(dir) = profile_path().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    super::files::atomic_write(&profile_path(), p.to_json().as_bytes()).map_err(|e| e.to_string())
}

// ---- the session ----------------------------------------------------------------------------

/// A rough password rating for the setup dialog: (0 = unusable .. 4 = strong, description).
pub fn strength(password: &str) -> (u8, &'static str) {
    let n = password.chars().count();
    if n < 8 {
        return (0, "Too short - use at least 8 characters");
    }
    let classes = [
        password.chars().any(|c| c.is_lowercase()),
        password.chars().any(|c| c.is_uppercase()),
        password.chars().any(|c| c.is_ascii_digit()),
        password.chars().any(|c| !c.is_alphanumeric()),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    let score = classes + (n >= 12) as usize + (n >= 16) as usize;
    match score {
        0..=2 => (1, "Weak - add length, capitals, digits or symbols"),
        3 => (2, "Fair"),
        4 => (3, "Good"),
        _ => (4, "Strong"),
    }
}

/// What the running app knows: the profile on disk and, while unlocked, the key.
#[derive(Default, Clone)]
pub struct Vault {
    pub profile: Option<Profile>,
    pub dek: Option<Dek>,
}

impl Vault {
    pub fn load() -> Vault {
        Vault { profile: load_profile(), dek: None }
    }

    pub fn unlocked(&self) -> bool {
        self.dek.is_some()
    }

    pub fn lock(&mut self) {
        self.dek = None;
    }

    pub fn fingerprint(&self) -> Option<&str> {
        self.profile.as_ref().map(|p| p.fingerprint.as_str())
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(unlock_with_password(&p, "wrong").err().as_deref(), Some("Incorrect password"));
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
}
