// =============================================================================
// Note++ — Crypto core (browser-only, no dependencies)
// =============================================================================
// Exposed as `window.NotePPCrypto`. Loaded BEFORE renderer.js in index.html.
// See features/ENCRYPTION.md for the design rationale and threat model.
//
// Primitives:
//   - AES-256-GCM            (authenticated encryption, Web Crypto API)
//   - PBKDF2-HMAC-SHA-256    (password → KEK, 600 000 iterations)
//   - HKDF-SHA-256           (recovery key → KEK; recovery key is already 256-bit)
//   - SHA-256                (DEK fingerprint, first 8 hex chars)
//   - CompressionStream/DecompressionStream  (gzip, native in Chromium)
//   - crypto.getRandomValues (CSPRNG for DEK, salts, IVs, recovery key)
//
// Recovery key encoding: Crockford base32 (no I/L/O/U) for human readability,
// dash-grouped every 4 chars (e.g. ABCD-EFGH-…).
// =============================================================================

(function () {
  'use strict';

  // ── Constants ──────────────────────────────────────────────────────────────
  const PBKDF2_ITER = 600000;          // OWASP 2023 minimum
  const KEY_BITS = 256;
  const SALT_BYTES = 16;
  const IV_BYTES = 12;                  // AES-GCM standard nonce size
  const FINGERPRINT_BYTES = 4;          // 8 hex chars
  const DEK_BYTES = 32;                 // 256-bit DEK
  const RECOVERY_BYTES = 32;            // 256-bit recovery key
  // Crockford base32 alphabet (no I, L, O, U — avoids ambiguity)
  const C32 = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  const ENVELOPE_FLAG = '_notepp_encrypted';
  const PROFILE_FLAG  = '_notepp_profile';

  // ── Bytes utilities ────────────────────────────────────────────────────────
  function randomBytes(n) {
    return crypto.getRandomValues(new Uint8Array(n));
  }
  function strToBytes(s) { return new TextEncoder().encode(s); }
  function bytesToStr(b) { return new TextDecoder().decode(b); }
  function bytesToHex(b) {
    return Array.from(b).map(x => x.toString(16).padStart(2, '0')).join('');
  }
  function b64encode(bytes) {
    const arr = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
    // Avoid String.fromCharCode(...arr) — call-stack limit on large arrays.
    let s = '';
    const CHUNK = 0x8000;
    for (let i = 0; i < arr.length; i += CHUNK) {
      s += String.fromCharCode.apply(null, arr.subarray(i, i + CHUNK));
    }
    return btoa(s);
  }
  function b64decode(str) {
    const bin = atob(str);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  }

  // ── Crockford base32 (recovery key encoding) ───────────────────────────────
  function base32Encode(bytes) {
    let bits = 0, value = 0, out = '';
    for (let i = 0; i < bytes.length; i++) {
      value = (value << 8) | bytes[i];
      bits += 8;
      while (bits >= 5) {
        out += C32[(value >>> (bits - 5)) & 31];
        bits -= 5;
      }
    }
    if (bits > 0) out += C32[(value << (5 - bits)) & 31];
    return out;
  }
  function base32Decode(str) {
    // Normalise: uppercase, strip whitespace/dashes, map ambiguous chars to canonicals.
    let clean = String(str || '').toUpperCase().replace(/[\s\-]/g, '');
    // Map common typos: I/L → 1, O → 0, U → V (Crockford's recommended fuzzy decode)
    clean = clean.replace(/[IL]/g, '1').replace(/O/g, '0').replace(/U/g, 'V');
    if (!clean.length) throw new Error('Empty recovery key');
    let bits = 0, value = 0;
    const out = [];
    for (let i = 0; i < clean.length; i++) {
      const idx = C32.indexOf(clean[i]);
      if (idx < 0) throw new Error('Invalid recovery key character: ' + clean[i]);
      value = (value << 5) | idx;
      bits += 5;
      if (bits >= 8) {
        out.push((value >>> (bits - 8)) & 0xff);
        bits -= 8;
      }
    }
    return new Uint8Array(out);
  }
  function formatRecoveryKey(bytes) {
    const raw = base32Encode(bytes);
    // Group every 4 chars: ABCD-EFGH-...
    return raw.match(/.{1,4}/g).join('-');
  }

  // ── Compression (native CompressionStream — no deps, available in Chromium) ─
  async function streamToBytes(readable) {
    const chunks = [];
    const reader = readable.getReader();
    let total = 0;
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      total += value.length;
    }
    const out = new Uint8Array(total);
    let off = 0;
    for (const c of chunks) { out.set(c, off); off += c.length; }
    return out;
  }
  async function gzipCompress(bytes) {
    const cs = new CompressionStream('gzip');
    const writer = cs.writable.getWriter();
    writer.write(bytes);
    writer.close();
    return await streamToBytes(cs.readable);
  }
  async function gzipDecompress(bytes) {
    const ds = new DecompressionStream('gzip');
    const writer = ds.writable.getWriter();
    writer.write(bytes);
    writer.close();
    return await streamToBytes(ds.readable);
  }

  // ── Key derivation ─────────────────────────────────────────────────────────
  async function pbkdf2DeriveKey(password, salt, iter) {
    const km = await crypto.subtle.importKey(
      'raw', strToBytes(password), 'PBKDF2', false, ['deriveKey']
    );
    return crypto.subtle.deriveKey(
      { name: 'PBKDF2', salt, iterations: iter, hash: 'SHA-256' },
      km,
      { name: 'AES-GCM', length: KEY_BITS },
      false,
      ['encrypt', 'decrypt']
    );
  }
  async function hkdfDeriveKey(rawKeyBytes, salt, info) {
    const km = await crypto.subtle.importKey(
      'raw', rawKeyBytes, 'HKDF', false, ['deriveKey']
    );
    return crypto.subtle.deriveKey(
      { name: 'HKDF', hash: 'SHA-256', salt, info: strToBytes(info) },
      km,
      { name: 'AES-GCM', length: KEY_BITS },
      false,
      ['encrypt', 'decrypt']
    );
  }
  // Import a raw 32-byte DEK as a usable AES-GCM CryptoKey.
  async function importDekKey(rawDek) {
    return crypto.subtle.importKey(
      'raw', rawDek, { name: 'AES-GCM', length: KEY_BITS },
      false, ['encrypt', 'decrypt']
    );
  }

  // ── AES-GCM ────────────────────────────────────────────────────────────────
  async function aesGcmEncrypt(key, plaintextBytes, iv) {
    iv = iv || randomBytes(IV_BYTES);
    const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, key, plaintextBytes);
    return { iv, ct: new Uint8Array(ct) };
  }
  async function aesGcmDecrypt(key, iv, ctBytes) {
    const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv }, key, ctBytes);
    return new Uint8Array(pt);
  }

  // ── Key wrapping (DEK ↔ KEK) ───────────────────────────────────────────────
  async function wrapDek(rawDek, kek) {
    const { iv, ct } = await aesGcmEncrypt(kek, rawDek);
    return { wrappedDek: b64encode(ct), wrapIv: b64encode(iv) };
  }
  async function unwrapDek(wrappedB64, ivB64, kek) {
    const wrapped = b64decode(wrappedB64);
    const iv = b64decode(ivB64);
    return await aesGcmDecrypt(kek, iv, wrapped);
  }

  // ── Profile fingerprint ────────────────────────────────────────────────────
  async function dekFingerprint(rawDek) {
    const hash = await crypto.subtle.digest('SHA-256', rawDek);
    return bytesToHex(new Uint8Array(hash).subarray(0, FINGERPRINT_BYTES));
  }

  // ── Profile (envelope of wrapped keys) ─────────────────────────────────────
  // Returns { profile, rawDek, recoveryKey } where rawDek stays in memory only.
  async function createProfile(password) {
    const rawDek = randomBytes(DEK_BYTES);
    const recoveryBytes = randomBytes(RECOVERY_BYTES);
    const fingerprint = await dekFingerprint(rawDek);

    // Wrap with password
    const pwSalt = randomBytes(SALT_BYTES);
    const pwKek = await pbkdf2DeriveKey(password, pwSalt, PBKDF2_ITER);
    const pwWrap = await wrapDek(rawDek, pwKek);

    // Wrap with recovery key
    const recSalt = randomBytes(SALT_BYTES);
    const recKek = await hkdfDeriveKey(recoveryBytes, recSalt, 'notepp-recovery');
    const recWrap = await wrapDek(rawDek, recKek);

    const profile = {
      [PROFILE_FLAG]: true,
      version: 1,
      fingerprint,
      createdAt: new Date().toISOString(),
      wrappedKeys: [
        {
          method: 'password',
          kdf: 'PBKDF2-SHA256',
          iter: PBKDF2_ITER,
          salt: b64encode(pwSalt),
          wrappedDek: pwWrap.wrappedDek,
          wrapIv: pwWrap.wrapIv,
        },
        {
          method: 'recovery',
          kdf: 'HKDF-SHA256',
          salt: b64encode(recSalt),
          wrappedDek: recWrap.wrappedDek,
          wrapIv: recWrap.wrapIv,
        },
      ],
    };
    return { profile, rawDek, recoveryKey: formatRecoveryKey(recoveryBytes) };
  }

  // Unlock: returns raw DEK (Uint8Array). Throws on wrong password / corruption.
  async function unlockWithPassword(profile, password) {
    const entry = profile.wrappedKeys.find(k => k.method === 'password');
    if (!entry) throw new Error('Profile has no password method');
    const salt = b64decode(entry.salt);
    const kek = await pbkdf2DeriveKey(password, salt, entry.iter || PBKDF2_ITER);
    return await unwrapDek(entry.wrappedDek, entry.wrapIv, kek);
  }
  async function unlockWithRecoveryKey(profile, recoveryKey) {
    const entry = profile.wrappedKeys.find(k => k.method === 'recovery');
    if (!entry) throw new Error('Profile has no recovery method');
    const salt = b64decode(entry.salt);
    const rawKey = base32Decode(recoveryKey);
    if (rawKey.length < RECOVERY_BYTES) throw new Error('Recovery key too short');
    const kek = await hkdfDeriveKey(rawKey.subarray(0, RECOVERY_BYTES), salt, 'notepp-recovery');
    return await unwrapDek(entry.wrappedDek, entry.wrapIv, kek);
  }

  // Re-wrap DEK with a new password (or add password method if missing). Mutates profile.
  async function setPasswordOnProfile(profile, rawDek, newPassword) {
    const salt = randomBytes(SALT_BYTES);
    const kek = await pbkdf2DeriveKey(newPassword, salt, PBKDF2_ITER);
    const wrap = await wrapDek(rawDek, kek);
    const entry = {
      method: 'password',
      kdf: 'PBKDF2-SHA256',
      iter: PBKDF2_ITER,
      salt: b64encode(salt),
      wrappedDek: wrap.wrappedDek,
      wrapIv: wrap.wrapIv,
    };
    const idx = profile.wrappedKeys.findIndex(k => k.method === 'password');
    if (idx >= 0) profile.wrappedKeys[idx] = entry;
    else profile.wrappedKeys.push(entry);
    return profile;
  }

  // Generate a new recovery key, replacing any existing recovery entry. Mutates profile.
  async function regenerateRecoveryKey(profile, rawDek) {
    const recoveryBytes = randomBytes(RECOVERY_BYTES);
    const salt = randomBytes(SALT_BYTES);
    const kek = await hkdfDeriveKey(recoveryBytes, salt, 'notepp-recovery');
    const wrap = await wrapDek(rawDek, kek);
    const entry = {
      method: 'recovery',
      kdf: 'HKDF-SHA256',
      salt: b64encode(salt),
      wrappedDek: wrap.wrappedDek,
      wrapIv: wrap.wrapIv,
    };
    const idx = profile.wrappedKeys.findIndex(k => k.method === 'recovery');
    if (idx >= 0) profile.wrappedKeys[idx] = entry;
    else profile.wrappedKeys.push(entry);
    return { profile, recoveryKey: formatRecoveryKey(recoveryBytes) };
  }

  // ── File-level encryption (the on-disk JSON envelope) ──────────────────────
  async function encryptFile(plaintext, rawDek, profileFingerprint, originalExt) {
    const dekKey = await importDekKey(rawDek);
    const ptBytes = strToBytes(plaintext);
    const compressed = await gzipCompress(ptBytes);
    const { iv, ct } = await aesGcmEncrypt(dekKey, compressed);
    return {
      [ENVELOPE_FLAG]: true,
      version: 1,
      alg: 'AES-256-GCM',
      compression: 'gzip',
      profile: profileFingerprint,
      iv: b64encode(iv),
      ct: b64encode(ct),
      originalExt: originalExt || '',
      createdAt: new Date().toISOString(),
    };
  }
  async function decryptFile(envelope, rawDek) {
    if (!envelope || envelope[ENVELOPE_FLAG] !== true) {
      throw new Error('Not a Note++ encrypted file');
    }
    const dekKey = await importDekKey(rawDek);
    const iv = b64decode(envelope.iv);
    const ct = b64decode(envelope.ct);
    const inner = await aesGcmDecrypt(dekKey, iv, ct);
    const ptBytes = envelope.compression === 'gzip' ? await gzipDecompress(inner) : inner;
    return bytesToStr(ptBytes);
  }

  // Quick check: does this file content look like a Note++ encrypted envelope?
  // Returns the parsed envelope or null. Safe on huge files (only parses first 4 KB if it
  // doesn't look like JSON, to avoid choking on multi-MB plain text).
  function detectEncrypted(content) {
    if (!content || typeof content !== 'string') return null;
    // Quick reject: must start with optional whitespace then `{`
    let i = 0;
    while (i < content.length && /\s/.test(content[i])) i++;
    if (content[i] !== '{') return null;
    // Quick string match for the flag — avoids parsing large unrelated JSON files
    if (content.indexOf('"' + ENVELOPE_FLAG + '"') < 0) return null;
    try {
      const obj = JSON.parse(content);
      if (obj && obj[ENVELOPE_FLAG] === true) return obj;
    } catch (e) { /* not JSON */ }
    return null;
  }

  // Same idea for profile.json — used when loading the user's profile from disk.
  function parseProfile(content) {
    if (!content || typeof content !== 'string') return null;
    try {
      const obj = JSON.parse(content);
      if (obj && obj[PROFILE_FLAG] === true && Array.isArray(obj.wrappedKeys)) return obj;
    } catch (e) { /* not JSON */ }
    return null;
  }

  // ── Public surface ─────────────────────────────────────────────────────────
  window.NotePPCrypto = {
    // Constants
    PBKDF2_ITER, ENVELOPE_FLAG, PROFILE_FLAG,
    // Bytes utilities (rarely needed outside this module)
    randomBytes, b64encode, b64decode, bytesToHex,
    // Recovery key formatting
    formatRecoveryKey, base32Decode,
    // Compression
    gzipCompress, gzipDecompress,
    // KDFs
    pbkdf2DeriveKey, hkdfDeriveKey,
    // AES-GCM
    aesGcmEncrypt, aesGcmDecrypt, importDekKey,
    // DEK wrap/unwrap
    wrapDek, unwrapDek, dekFingerprint,
    // Profile API (Phase 2)
    createProfile, unlockWithPassword, unlockWithRecoveryKey,
    setPasswordOnProfile, regenerateRecoveryKey,
    // File-level API (Phase 3)
    encryptFile, decryptFile, detectEncrypted, parseProfile,
  };
})();
