//! The recovery key (Crockford base32) and wrapping the data key with the password or the recovery key.

use super::*;

// ---- Crockford base32 (the recovery key) ----------------------------------------------------

pub fn base32_encode(bytes: &[u8]) -> String {
    let (mut bits, mut value, mut out) = (0u32, 0u32, String::new());
    for b in bytes {
        value = (value << 8) | u32::from(*b);
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
pub fn base32_decode(text: &str) -> Result<Vec<u8>, Error> {
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
        return Err(Error::EmptyRecoveryKey);
    }
    let (mut bits, mut value, mut out) = (0u32, 0u32, Vec::new());
    for c in clean.chars() {
        let idx = C32.iter().position(|x| *x as char == c).ok_or(Error::BadKeyChar(c))?;
        value = (value << 5) | crate::num::to_u32(idx);
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

pub(super) fn kek_from_password(password: &str, salt: &[u8], iter: u32) -> Zeroizing<[u8; 32]> {
    let mut k = Zeroizing::new([0u8; 32]);
    pbkdf2::pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, iter, &mut k[..]);
    k
}

pub(super) fn kek_from_recovery(raw: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut k = Zeroizing::new([0u8; 32]);
    hkdf::Hkdf::<Sha256>::new(Some(salt), raw).expand(RECOVERY_INFO, &mut k[..]).map_err(|_| Error::KeyDerivation)?;
    Ok(k)
}

pub(super) fn seal(key: &[u8; 32], plain: &[u8]) -> Result<([u8; IV_BYTES], Vec<u8>), Error> {
    let iv = random::<IV_BYTES>()?;
    let c = Aes256Gcm::new_from_slice(key).map_err(|e| Error::Cipher(e.to_string()))?;
    let ct = c.encrypt(Nonce::from_slice(&iv), plain).map_err(|_| Error::Encryption)?;
    Ok((iv, ct))
}

pub(super) fn open(key: &[u8; 32], iv: &[u8], ct: &[u8]) -> Result<Vec<u8>, ()> {
    if iv.len() != IV_BYTES {
        return Err(());
    }
    let c = Aes256Gcm::new_from_slice(key).map_err(|_| ())?;
    c.decrypt(Nonce::from_slice(iv), ct).map_err(|_| ())
}

pub(super) fn wrap(dek: &Dek, kek: &[u8; 32]) -> Result<(String, String), Error> {
    let (iv, ct) = seal(kek, &dek.0[..])?;
    Ok((B64.encode(&ct), B64.encode(iv)))
}

pub(super) fn unwrap_dek(entry: &Wrapped, kek: &[u8; 32]) -> Result<Dek, ()> {
    let raw = open(kek, &b64d(&entry.wrap_iv).map_err(|_| ())?, &b64d(&entry.wrapped_dek).map_err(|_| ())?)?;
    let arr: [u8; DEK_BYTES] = raw.try_into().map_err(|_| ())?;
    Ok(Dek(Zeroizing::new(arr)))
}

pub(super) fn password_entry(dek: &Dek, password: &str) -> Result<Wrapped, Error> {
    let salt = random::<SALT_BYTES>()?;
    let kek = kek_from_password(password, &salt, ITERATIONS);
    let (wrapped_dek, wrap_iv) = wrap(dek, &kek)?;
    Ok(Wrapped { method: "password".into(), kdf: "PBKDF2-SHA256".into(), iter: Some(ITERATIONS), salt: B64.encode(salt), wrapped_dek, wrap_iv })
}

pub(super) fn recovery_entry(dek: &Dek, raw: &[u8]) -> Result<Wrapped, Error> {
    let salt = random::<SALT_BYTES>()?;
    let kek = kek_from_recovery(raw, &salt)?;
    let (wrapped_dek, wrap_iv) = wrap(dek, &kek)?;
    Ok(Wrapped { method: "recovery".into(), kdf: "HKDF-SHA256".into(), iter: None, salt: B64.encode(salt), wrapped_dek, wrap_iv })
}

pub(super) fn replace_entry(p: &mut Profile, e: Wrapped) {
    match p.wrapped_keys.iter().position(|k| k.method == e.method) {
        Some(i) => p.wrapped_keys[i] = e,
        None => p.wrapped_keys.push(e),
    }
}
