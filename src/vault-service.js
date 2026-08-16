'use strict';
// =============================================================================
// vault-service.js — Dev Secrets vault (MAIN PROCESS ONLY)
// -----------------------------------------------------------------------------
// Stores API keys, .env values, connection strings, SSH passphrases — the
// secrets you need *inside a file you're editing*, which is exactly the case a
// browser-centric password manager handles badly.
//
// ── Why this lives in main, and only in main ────────────────────────────────
// Note++'s regular renderer is unusually exposed for a process holding
// secrets: it renders HTML previews of arbitrary files in an iframe, hosts the
// whiteboard and draw.io iframes, and displays model output from Ollama. Any
// XSS or prompt-injection in those would be able to read a decrypted vault
// sitting in the same process.
//
// So plaintext NEVER crosses into the main renderer. The vault is decrypted
// here; the UI runs in its own dedicated window (vault.html) that never loads
// user content; and the only secret that ever reaches the editor is one the
// user explicitly asked to insert.
//
// ── Relationship to the notes encryption ────────────────────────────────────
// Deliberately independent. Same primitives and parameters as src/crypto.js,
// but its own password, salt and DEK, so unlocking your notes does not unlock
// your credentials. Reuses the *design*, not the *keys*.
// =============================================================================

const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

// Matched to src/crypto.js so the two halves of the app agree on strength.
const PBKDF2_ITER   = 600000;   // OWASP 2023 minimum
const KEY_BYTES     = 32;       // AES-256
const IV_BYTES      = 12;       // GCM standard nonce
const SALT_BYTES    = 16;
const TAG_BYTES     = 16;
const VAULT_VERSION = 1;

const DEFAULT_AUTOLOCK_MS = 5 * 60 * 1000;

// ── Low-level crypto helpers ────────────────────────────────────────────────

const b64  = buf => Buffer.from(buf).toString('base64');
const ub64 = str => Buffer.from(String(str), 'base64');

function deriveKek(password, salt, iterations) {
  return crypto.pbkdf2Sync(
    Buffer.from(String(password), 'utf8'),
    salt,
    iterations || PBKDF2_ITER,
    KEY_BYTES,
    'sha256'
  );
}

// AES-256-GCM. Auth tag is appended to the ciphertext so one field round-trips.
function aesEncrypt(key, plaintextBuf) {
  const iv = crypto.randomBytes(IV_BYTES);
  const c = crypto.createCipheriv('aes-256-gcm', key, iv);
  const ct = Buffer.concat([c.update(plaintextBuf), c.final(), c.getAuthTag()]);
  return { iv: b64(iv), ct: b64(ct) };
}

function aesDecrypt(key, ivB64, ctB64) {
  const iv = ub64(ivB64);
  const raw = ub64(ctB64);
  const tag = raw.subarray(raw.length - TAG_BYTES);
  const body = raw.subarray(0, raw.length - TAG_BYTES);
  const d = crypto.createDecipheriv('aes-256-gcm', key, iv);
  d.setAuthTag(tag);
  return Buffer.concat([d.update(body), d.final()]);   // throws if tampered/wrong key
}

// ── TOTP (RFC 6238) ─────────────────────────────────────────────────────────
// Implemented rather than pulled in as a dependency: it's ~25 lines, and a
// vault is the last place to add supply-chain surface for something this small.

function base32Decode(input) {
  const alpha = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  const clean = String(input).toUpperCase().replace(/[=\s-]/g, '');
  let bits = 0, value = 0;
  const out = [];
  for (const ch of clean) {
    const idx = alpha.indexOf(ch);
    if (idx === -1) continue;          // skip anything not base32
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) { out.push((value >>> (bits - 8)) & 0xff); bits -= 8; }
  }
  return Buffer.from(out);
}

// Accepts a raw base32 secret or a full otpauth:// URI.
function totpNow(secretOrUri, atMs) {
  if (!secretOrUri) return null;
  let secret = String(secretOrUri).trim();
  let digits = 6, period = 30, algo = 'sha1';
  if (/^otpauth:\/\//i.test(secret)) {
    try {
      const u = new URL(secret);
      secret = u.searchParams.get('secret') || '';
      digits = parseInt(u.searchParams.get('digits'), 10) || 6;
      period = parseInt(u.searchParams.get('period'), 10) || 30;
      algo   = (u.searchParams.get('algorithm') || 'SHA1').toLowerCase();
    } catch { return null; }
  }
  const key = base32Decode(secret);
  if (!key.length) return null;
  const counter = Math.floor((atMs || Date.now()) / 1000 / period);
  const buf = Buffer.alloc(8);
  buf.writeUInt32BE(Math.floor(counter / 0x100000000), 0);
  buf.writeUInt32BE(counter >>> 0, 4);
  let hmac;
  try { hmac = crypto.createHmac(algo, key).update(buf).digest(); }
  catch { return null; }
  const offset = hmac[hmac.length - 1] & 0x0f;
  const code = ((hmac[offset] & 0x7f) << 24 | hmac[offset + 1] << 16 |
                hmac[offset + 2] << 8 | hmac[offset + 3]) % (10 ** digits);
  const secondsLeft = period - Math.floor((atMs || Date.now()) / 1000) % period;
  return { code: String(code).padStart(digits, '0'), secondsLeft, period };
}

// ── Entry shape ─────────────────────────────────────────────────────────────

function newEntryId() { return 'sec-' + crypto.randomBytes(8).toString('hex'); }

function normalizeEntry(e) {
  return {
    id:       e.id || newEntryId(),
    title:    String(e.title || '').trim(),
    username: String(e.username || ''),
    password: typeof e.password === 'string' ? e.password : '',
    url:      String(e.url || ''),
    notes:    String(e.notes || ''),
    totp:     String(e.totp || ''),
    tags:     Array.isArray(e.tags) ? e.tags.filter(t => typeof t === 'string') : [],
    created:  e.created || Date.now(),
    updated:  Date.now(),
  };
}

// What the UI is allowed to see in a list. Note the absence of `password`,
// `totp` and `notes` — those are fetched one at a time, by explicit action.
function toSafeSummary(e) {
  return {
    id: e.id,
    title: e.title,
    username: e.username,
    url: e.url,
    tags: e.tags,
    hasPassword: !!e.password,
    hasTotp: !!e.totp,
    hasNotes: !!e.notes,
    updated: e.updated,
    created: e.created,
  };
}

// ── The vault ───────────────────────────────────────────────────────────────

class Vault {
  constructor(filePath) {
    this.filePath = filePath;
    this.locked = true;
    this._dek = null;        // Buffer while unlocked, null when locked
    this._entries = null;    // decrypted array while unlocked
    this._autoLockMs = DEFAULT_AUTOLOCK_MS;
    this._lockTimer = null;
    this.onAutoLock = null;  // set by main.js to notify the UI
  }

  exists() { return fs.existsSync(this.filePath); }

  _readEnvelope() {
    return JSON.parse(fs.readFileSync(this.filePath, 'utf-8'));
  }

  _writeEnvelope(env) {
    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    // Write via a temp file + rename so a crash mid-write can't truncate the
    // vault — losing every credential to a half-written JSON would be fatal.
    const tmp = this.filePath + '.tmp';
    fs.writeFileSync(tmp, JSON.stringify(env, null, 2), { mode: 0o600 });
    fs.renameSync(tmp, this.filePath);
  }

  // Create a brand-new vault. Returns a recovery key the caller must show once.
  create(password) {
    if (this.exists()) throw new Error('vault already exists');
    if (!password || String(password).length < 8) {
      throw new Error('Master password must be at least 8 characters');
    }
    const dek = crypto.randomBytes(KEY_BYTES);
    const pwSalt = crypto.randomBytes(SALT_BYTES);
    const pwKek = deriveKek(password, pwSalt, PBKDF2_ITER);

    // Recovery key: 256 bits of entropy, shown once, never stored.
    const recoveryRaw = crypto.randomBytes(KEY_BYTES);
    const recSalt = crypto.randomBytes(SALT_BYTES);
    const recKek = crypto.hkdfSync('sha256', recoveryRaw, recSalt, Buffer.from('notepp-vault-recovery'), KEY_BYTES);

    const env = {
      version: VAULT_VERSION,
      kdf: { algo: 'PBKDF2-SHA256', iterations: PBKDF2_ITER, salt: b64(pwSalt) },
      wrappedPassword: aesEncrypt(pwKek, dek),
      recovery: { salt: b64(recSalt), ...aesEncrypt(Buffer.from(recKek), dek) },
      data: aesEncrypt(dek, Buffer.from('[]', 'utf8')),
      updated: Date.now(),
    };
    this._writeEnvelope(env);

    this._dek = dek;
    this._entries = [];
    this.locked = false;
    this._armAutoLock();
    return { recoveryKey: formatRecoveryKey(recoveryRaw) };
  }

  unlock(password) {
    const env = this._readEnvelope();
    const kek = deriveKek(password, ub64(env.kdf.salt), env.kdf.iterations);
    let dek;
    try {
      dek = aesDecrypt(kek, env.wrappedPassword.iv, env.wrappedPassword.ct);
    } catch {
      // GCM auth failure — wrong password (or tampered file). Same message
      // either way; distinguishing them leaks information.
      throw new Error('Incorrect master password');
    }
    this._dek = dek;
    this._entries = JSON.parse(aesDecrypt(dek, env.data.iv, env.data.ct).toString('utf8'));
    this.locked = false;
    this._armAutoLock();
    return true;
  }

  unlockWithRecovery(recoveryKey) {
    const env = this._readEnvelope();
    if (!env.recovery) throw new Error('This vault has no recovery key');
    const raw = parseRecoveryKey(recoveryKey);
    if (!raw) throw new Error('Recovery key is not in the expected format');
    const kek = crypto.hkdfSync('sha256', raw, ub64(env.recovery.salt),
      Buffer.from('notepp-vault-recovery'), KEY_BYTES);
    let dek;
    try { dek = aesDecrypt(Buffer.from(kek), env.recovery.iv, env.recovery.ct); }
    catch { throw new Error('Recovery key did not unlock this vault'); }
    this._dek = dek;
    this._entries = JSON.parse(aesDecrypt(dek, env.data.iv, env.data.ct).toString('utf8'));
    this.locked = false;
    this._armAutoLock();
    return true;
  }

  lock() {
    // Best-effort scrub. V8 gives no guarantee that the JS strings inside
    // _entries are gone from the heap, but zeroing the DEK buffer at least
    // means the on-disk data can't be re-derived from a later memory dump.
    if (this._dek) this._dek.fill(0);
    this._dek = null;
    this._entries = null;
    this.locked = true;
    if (this._lockTimer) { clearTimeout(this._lockTimer); this._lockTimer = null; }
  }

  _assertUnlocked() {
    if (this.locked || !this._dek) throw new Error('Vault is locked');
  }

  // Any interaction pushes the auto-lock out.
  touch() { if (!this.locked) this._armAutoLock(); }

  setAutoLockMinutes(mins) {
    const m = parseInt(mins, 10);
    this._autoLockMs = (isNaN(m) || m <= 0) ? 0 : m * 60 * 1000;
    this._armAutoLock();
  }

  _armAutoLock() {
    if (this._lockTimer) clearTimeout(this._lockTimer);
    this._lockTimer = null;
    if (!this._autoLockMs || this.locked) return;
    this._lockTimer = setTimeout(() => {
      this.lock();
      try { this.onAutoLock && this.onAutoLock(); } catch {}
    }, this._autoLockMs);
  }

  _persist() {
    this._assertUnlocked();
    const env = this._readEnvelope();
    env.data = aesEncrypt(this._dek, Buffer.from(JSON.stringify(this._entries), 'utf8'));
    env.updated = Date.now();
    this._writeEnvelope(env);
  }

  // ── Entry operations ──────────────────────────────────────────────────────

  list() {
    this._assertUnlocked();
    this.touch();
    return this._entries
      .map(toSafeSummary)
      .sort((a, b) => a.title.localeCompare(b.title));
  }

  upsert(entry) {
    this._assertUnlocked();
    this.touch();
    const e = normalizeEntry(entry || {});
    if (!e.title) throw new Error('Entry needs a title');
    const idx = this._entries.findIndex(x => x.id === e.id);
    if (idx >= 0) {
      // Empty password on an edit means "leave it alone", so the UI can save
      // a form it only ever showed a masked placeholder in.
      if (!entry.password && this._entries[idx].password) e.password = this._entries[idx].password;
      if (!entry.totp && this._entries[idx].totp) e.totp = this._entries[idx].totp;
      e.created = this._entries[idx].created;
      this._entries[idx] = e;
    } else {
      this._entries.push(e);
    }
    this._persist();
    return toSafeSummary(e);
  }

  remove(id) {
    this._assertUnlocked();
    this.touch();
    const before = this._entries.length;
    this._entries = this._entries.filter(e => e.id !== id);
    if (this._entries.length !== before) this._persist();
    return true;
  }

  // Fetch ONE secret field, by explicit user action. This is the only path
  // that returns plaintext, and callers are expected to use it narrowly.
  getSecret(id, field) {
    this._assertUnlocked();
    this.touch();
    const e = this._entries.find(x => x.id === id);
    if (!e) throw new Error('not found');
    const allowed = ['password', 'username', 'notes', 'url', 'totp'];
    if (!allowed.includes(field)) throw new Error('field not allowed');
    if (field === 'totp') {
      const t = totpNow(e.totp);
      return t ? t.code : null;
    }
    return e[field] || '';
  }

  getTotp(id) {
    this._assertUnlocked();
    this.touch();
    const e = this._entries.find(x => x.id === id);
    if (!e || !e.totp) return null;
    return totpNow(e.totp);
  }

  changePassword(oldPassword, newPassword) {
    if (!newPassword || String(newPassword).length < 8) {
      throw new Error('New password must be at least 8 characters');
    }
    // Re-derive from the file rather than trusting in-memory state.
    const env = this._readEnvelope();
    const oldKek = deriveKek(oldPassword, ub64(env.kdf.salt), env.kdf.iterations);
    let dek;
    try { dek = aesDecrypt(oldKek, env.wrappedPassword.iv, env.wrappedPassword.ct); }
    catch { throw new Error('Current password is incorrect'); }

    const salt = crypto.randomBytes(SALT_BYTES);
    const newKek = deriveKek(newPassword, salt, PBKDF2_ITER);
    env.kdf = { algo: 'PBKDF2-SHA256', iterations: PBKDF2_ITER, salt: b64(salt) };
    env.wrappedPassword = aesEncrypt(newKek, dek);
    env.updated = Date.now();
    this._writeEnvelope(env);
    return true;
  }

  stats() {
    return {
      exists: this.exists(),
      locked: this.locked,
      count: this.locked ? null : this._entries.length,
      autoLockMinutes: this._autoLockMs ? Math.round(this._autoLockMs / 60000) : 0,
    };
  }
}

// ── Recovery key formatting ─────────────────────────────────────────────────
// Crockford base32 — no I/L/O/U, so it can be read aloud or written down
// without the usual 0/O and 1/l confusion.

const CROCKFORD = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';

function formatRecoveryKey(buf) {
  let bits = 0, value = 0, out = '';
  for (const byte of buf) {
    value = (value << 8) | byte;
    bits += 8;
    while (bits >= 5) { out += CROCKFORD[(value >>> (bits - 5)) & 31]; bits -= 5; }
  }
  if (bits > 0) out += CROCKFORD[(value << (5 - bits)) & 31];
  return out.match(/.{1,5}/g).join('-');   // NPPV1-XXXXX-XXXXX-…
}

function parseRecoveryKey(str) {
  const clean = String(str).toUpperCase().replace(/[^0-9A-Z]/g, '')
    .replace(/O/g, '0').replace(/[IL]/g, '1').replace(/U/g, 'V');
  let bits = 0, value = 0;
  const out = [];
  for (const ch of clean) {
    const idx = CROCKFORD.indexOf(ch);
    if (idx === -1) return null;
    value = (value << 5) | idx;
    bits += 5;
    if (bits >= 8) { out.push((value >>> (bits - 8)) & 0xff); bits -= 8; }
  }
  const buf = Buffer.from(out);
  return buf.length >= KEY_BYTES ? buf.subarray(0, KEY_BYTES) : null;
}

// ── Password generator ──────────────────────────────────────────────────────
// Rejection sampling rather than `% alphabet.length`, which biases toward the
// first characters of the alphabet.
function generatePassword(opts) {
  opts = opts || {};
  const length = Math.min(128, Math.max(8, parseInt(opts.length, 10) || 20));
  let alphabet = '';
  // Confusable glyphs are dropped by default: l/I/L/1 and O/0 look alike in
  // most terminal fonts, and these passwords get read off a screen and typed
  // into a config file by hand. Costs ~0.2 bits/char, worth it.
  if (opts.lower !== false)  alphabet += 'abcdefghijkmnopqrstuvwxyz';
  if (opts.upper !== false)  alphabet += 'ABCDEFGHJKMNPQRSTUVWXYZ';
  if (opts.digits !== false) alphabet += '23456789';
  if (opts.symbols)          alphabet += '!@#$%^&*()-_=+[]{};:,.?';
  if (opts.ambiguous)        alphabet += 'ilLI|oO01';
  if (!alphabet) alphabet = 'abcdefghijkmnopqrstuvwxyz23456789';

  const max = 256 - (256 % alphabet.length);
  let out = '';
  while (out.length < length) {
    const bytes = crypto.randomBytes(length);
    for (const b of bytes) {
      if (b >= max) continue;             // reject to keep the distribution flat
      out += alphabet[b % alphabet.length];
      if (out.length === length) break;
    }
  }
  return out;
}

module.exports = {
  Vault, totpNow, generatePassword,
  formatRecoveryKey, parseRecoveryKey,
  PBKDF2_ITER,
};
