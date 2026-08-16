'use strict';
// =============================================================================
// bitwarden-service.js — on-demand Bitwarden CLI, wired to Note++'s UX
// -----------------------------------------------------------------------------
// Note++ does NOT implement a password manager. It downloads the official
// Bitwarden CLI on first use — the same pinned-version / fetch-into-userData
// pattern as drawio-service.js — and drives it. Bitwarden holds the vault and
// does all the cryptography; we hold, at most, a session token in this
// process's memory, and never on disk.
//
// ── What we deliberately do NOT do ──────────────────────────────────────────
//   • store the master password (it goes to `bw` stdin and is dropped)
//   • persist the session token (memory only; gone on quit or lock)
//   • cache decrypted secrets (every reveal/copy is a fresh `bw get`)
//   • bundle the binary (GPL-3.0 — the user's machine fetches it from the
//     official GitHub release, so there's no redistribution on our part)
//
// ── Why the SHA-256 pins below matter ───────────────────────────────────────
// We download an executable and pipe a master password into it. The existing
// drawio download performs no integrity check, which is survivable for a
// sandboxed diagram editor and emphatically is not here: a hijacked release
// URL would mean code execution plus the password. Hashes are pinned in
// source, so swapping the upstream artefact cannot silently change what we
// are willing to run.
// =============================================================================

const { app } = require('electron');
const fs = require('fs');
const os = require('os');
const path = require('path');
const https = require('https');
const crypto = require('crypto');
const { execFile, spawn } = require('child_process');

// Pinned release. Bump both the version and the matching digests together —
// see the header on why these are not fetched at runtime.
const BW_VERSION = '2026.7.0';
const BW_ASSETS = {
  'win32-x64':   { file: `bw-windows-${BW_VERSION}.zip`,     sha256: 'b0c22438607b789c6452dbd37ffd6be0e8a61e7a5c4e9ac57804d7ae5ed01b5b', bin: 'bw.exe' },
  'darwin-x64':  { file: `bw-macos-${BW_VERSION}.zip`,       sha256: 'b37836d539798f5adeb8a907619ee8a55b6322549bb68669aa4b3a03d5bc0452', bin: 'bw' },
  'darwin-arm64':{ file: `bw-macos-arm64-${BW_VERSION}.zip`, sha256: '61d5de8a279a9faf3637216f4fb02b506a1e4bb2817d1c64be0bd474466dd85a', bin: 'bw' },
  'linux-x64':   { file: `bw-linux-${BW_VERSION}.zip`,       sha256: '7a35145e205952f7434d2370da359543145ae0c45ba1af0fe9bdd99d40a00180', bin: 'bw' },
  'linux-arm64': { file: `bw-linux-arm64-${BW_VERSION}.zip`, sha256: 'e33ed05ca0fada9bd51b8bce76a230369bf0eefd5796a0a8e60699c977327fb5', bin: 'bw' },
};
const RELEASE_BASE = `https://github.com/bitwarden/clients/releases/download/cli-v${BW_VERSION}/`;

const EXEC_TIMEOUT_MS = 20000;
const MAX_BUFFER = 16 * 1024 * 1024;

// ── Session state (memory only) ─────────────────────────────────────────────
const session = {
  token: null,          // BW_SESSION, held only while unlocked
  unlockedAt: null,
  autoLockMs: 5 * 60 * 1000,
  timer: null,
  onLock: null,         // set by main.js so the UI can react
};

function platformKey() {
  const arch = process.arch === 'arm64' ? 'arm64' : 'x64';
  return `${process.platform}-${arch}`;
}
function assetForPlatform() { return BW_ASSETS[platformKey()] || null; }

function bundleDir() { return path.join(app.getPath('userData'), 'bitwarden-cli'); }
function versionFile() { return path.join(bundleDir(), 'version.json'); }
function binPath() {
  const a = assetForPlatform();
  return a ? path.join(bundleDir(), a.bin) : null;
}

function readInstalledVersion() {
  try { return JSON.parse(fs.readFileSync(versionFile(), 'utf-8')).version || null; }
  catch { return null; }
}

function isInstalled() {
  const p = binPath();
  return !!(p && fs.existsSync(p));
}

// Prefer our managed copy; fall back to a `bw` already on PATH so users who
// installed it themselves aren't forced into a second download.
let _pathBinChecked = false, _pathBin = null;
function resolveBin() {
  if (isInstalled()) return binPath();
  if (!_pathBinChecked) {
    _pathBinChecked = true;
    try {
      const which = process.platform === 'win32' ? 'where' : 'which';
      const r = require('child_process').spawnSync(which, ['bw'], { encoding: 'utf8', windowsHide: true });
      const first = String(r.stdout || '').split(/\r?\n/).find(Boolean);
      if (first && fs.existsSync(first.trim())) _pathBin = first.trim();
    } catch {}
  }
  return _pathBin;
}

function getStatus() {
  const asset = assetForPlatform();
  return {
    supported: !!asset,
    platform: platformKey(),
    installed: isInstalled(),
    onPath: !isInstalled() && !!resolveBin(),
    binPath: resolveBin(),
    installedVersion: readInstalledVersion(),
    pinnedVersion: BW_VERSION,
    needsUpdate: isInstalled() && readInstalledVersion() !== BW_VERSION,
    downloadBytes: asset ? null : 0,
    unlocked: !!session.token,
    autoLockMinutes: session.autoLockMs ? Math.round(session.autoLockMs / 60000) : 0,
  };
}

// ── Download + verify + extract ─────────────────────────────────────────────

function httpsGetFollowingRedirects(url, onResponse, onError, depth) {
  if ((depth || 0) > 5) return onError(new Error('too many redirects'));
  const req = https.get(url, { headers: { 'User-Agent': 'NotePP' } }, res => {
    if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
      res.resume();
      return httpsGetFollowingRedirects(res.headers.location, onResponse, onError, (depth || 0) + 1);
    }
    if (res.statusCode !== 200) {
      res.resume();
      return onError(new Error(`HTTP ${res.statusCode}`));
    }
    onResponse(res);
  });
  req.on('error', onError);
  req.setTimeout(60000, () => req.destroy(new Error('download timed out')));
}

// onProgress({ phase, percent, receivedBytes, totalBytes })
async function download(onProgress) {
  const asset = assetForPlatform();
  if (!asset) throw new Error(`No Bitwarden CLI build for ${platformKey()}`);
  const report = p => { try { onProgress && onProgress(p); } catch {} };

  const tmpZip = path.join(os.tmpdir(), `bw-${BW_VERSION}-${Date.now()}.zip`);
  const tmpDir = path.join(os.tmpdir(), `bw-${BW_VERSION}-${Date.now()}-x`);

  try {
    // 1. download, hashing as it streams so we never hold 40 MB twice
    report({ phase: 'download', percent: 0 });
    const hash = crypto.createHash('sha256');
    await new Promise((resolve, reject) => {
      httpsGetFollowingRedirects(RELEASE_BASE + asset.file, res => {
        const total = parseInt(res.headers['content-length'], 10) || 0;
        let got = 0;
        const out = fs.createWriteStream(tmpZip);
        res.on('data', chunk => {
          got += chunk.length;
          hash.update(chunk);
          if (total) report({ phase: 'download', percent: Math.round(got / total * 100), receivedBytes: got, totalBytes: total });
        });
        res.pipe(out);
        out.on('finish', () => out.close(resolve));
        out.on('error', reject);
        res.on('error', reject);
      }, reject);
    });

    // 2. verify BEFORE anything is extracted or executed
    report({ phase: 'verify', percent: 100 });
    const digest = hash.digest('hex');
    if (digest !== asset.sha256) {
      throw new Error(
        `Checksum mismatch — refusing to install.\nexpected ${asset.sha256}\ngot      ${digest}`);
    }

    // 3. extract
    report({ phase: 'extract', percent: 100 });
    const extractZip = require('extract-zip');
    fs.mkdirSync(tmpDir, { recursive: true });
    await extractZip(tmpZip, { dir: tmpDir });

    // The archive is a single binary at its root.
    const found = fs.readdirSync(tmpDir).find(f => f === asset.bin || f === 'bw' || f === 'bw.exe');
    if (!found) throw new Error('Archive did not contain the expected bw binary');

    fs.mkdirSync(bundleDir(), { recursive: true });
    const dest = path.join(bundleDir(), asset.bin);
    fs.copyFileSync(path.join(tmpDir, found), dest);
    if (process.platform !== 'win32') fs.chmodSync(dest, 0o755);
    fs.writeFileSync(versionFile(), JSON.stringify({
      version: BW_VERSION, sha256: digest, installedAt: Date.now(),
    }, null, 2));

    report({ phase: 'done', percent: 100 });
    return { success: true, version: BW_VERSION, binPath: dest };
  } finally {
    // Always clean up, even when verification threw — the drawio path leaks
    // its temp files on failure and it's a known wart there.
    try { fs.rmSync(tmpZip, { force: true }); } catch {}
    try { fs.rmSync(tmpDir, { recursive: true, force: true }); } catch {}
  }
}

function uninstall() {
  lock();
  try { fs.rmSync(bundleDir(), { recursive: true, force: true }); } catch {}
  return { success: true };
}

// ── Running bw ──────────────────────────────────────────────────────────────

function bwEnv(extra) {
  const env = { ...process.env, ...(extra || {}) };
  if (session.token) env.BW_SESSION = session.token;
  // Keep the CLI non-interactive so a prompt can never hang us invisibly.
  env.BW_NOINTERACTION = 'true';
  return env;
}

function bwRun(args, opts) {
  opts = opts || {};
  return new Promise(resolve => {
    const bin = resolveBin();
    if (!bin) return resolve({ ok: false, error: 'Bitwarden CLI is not installed' });
    const child = execFile(bin, args, {
      timeout: opts.timeout || EXEC_TIMEOUT_MS,
      maxBuffer: MAX_BUFFER,
      windowsHide: true,
      env: bwEnv(opts.env),
    }, (err, stdout, stderr) => {
      resolve({
        ok: !err,
        stdout: String(stdout || ''),
        stderr: String(stderr || (err && err.message) || ''),
      });
    });
    if (opts.stdin != null) {
      // Master password path: written straight to the CLI's stdin and never
      // retained here. `stdin` is not logged, stored, or echoed anywhere.
      try { child.stdin.write(String(opts.stdin) + '\n'); child.stdin.end(); } catch {}
    }
  });
}

function safeJson(t) { try { return JSON.parse(t); } catch { return null; } }

// ── Auth state ──────────────────────────────────────────────────────────────

// bw's own view of things, independent of whether WE hold a session.
async function authStatus() {
  const r = await bwRun(['status']);
  const j = safeJson(r.stdout);
  if (!j) return { state: 'unknown', error: r.stderr.trim() || 'could not read bw status' };
  return {
    state: j.status,                       // unauthenticated | locked | unlocked
    email: j.userEmail || null,
    server: j.serverUrl || null,
  };
}

function armAutoLock() {
  if (session.timer) clearTimeout(session.timer);
  session.timer = null;
  if (!session.autoLockMs || !session.token) return;
  session.timer = setTimeout(() => {
    lock();
    try { session.onLock && session.onLock('auto'); } catch {}
  }, session.autoLockMs);
}

function touch() { if (session.token) armAutoLock(); }

function setAutoLockMinutes(mins) {
  const m = parseInt(mins, 10);
  session.autoLockMs = (isNaN(m) || m <= 0) ? 0 : m * 60 * 1000;
  armAutoLock();
}

// Adopt a session the user already established outside Note++ (BW_SESSION in
// the environment). Zero prompts — the best case.
async function adoptExistingSession() {
  const envTok = process.env.BW_SESSION;
  if (!envTok) return { ok: false };
  session.token = envTok;
  const st = await authStatus();
  if (st.state === 'unlocked') { session.unlockedAt = Date.now(); armAutoLock(); return { ok: true, email: st.email }; }
  session.token = null;
  return { ok: false };
}

// Unlock with the master password. It goes to bw's stdin and is not kept.
async function unlock(masterPassword) {
  const r = await bwRun(['unlock', '--raw', '--passwordenv', 'BW_UNLOCK_PW'], {
    env: { BW_UNLOCK_PW: String(masterPassword || '') },
    timeout: 60000,
  });
  if (!r.ok || !r.stdout.trim()) {
    const msg = r.stderr.trim();
    if (/not logged in|unauthenticated/i.test(msg)) throw new Error('Not signed in — use "Sign in" first');
    throw new Error(/invalid/i.test(msg) ? 'Incorrect master password' : (msg || 'Unlock failed'));
  }
  session.token = r.stdout.trim();
  session.unlockedAt = Date.now();
  armAutoLock();
  return { ok: true };
}

// Accept a token the user obtained themselves (e.g. by running `bw unlock`
// in the integrated terminal), so the password never touches our UI.
async function useSessionToken(token) {
  if (!token || !String(token).trim()) throw new Error('No session token supplied');
  const prev = session.token;
  session.token = String(token).trim();
  const st = await authStatus();
  if (st.state !== 'unlocked') {
    session.token = prev;
    throw new Error('That session token was not accepted by bw');
  }
  session.unlockedAt = Date.now();
  armAutoLock();
  return { ok: true, email: st.email };
}

async function login(email, masterPassword) {
  const r = await bwRun(['login', String(email || ''), '--raw', '--passwordenv', 'BW_LOGIN_PW'], {
    env: { BW_LOGIN_PW: String(masterPassword || '') },
    timeout: 90000,
  });
  if (!r.ok || !r.stdout.trim()) {
    const msg = r.stderr.trim();
    if (/two-step|two factor|2fa/i.test(msg)) {
      throw new Error('This account needs two-step login — run `bw login` in the terminal once, then use "I have a session token".');
    }
    throw new Error(msg || 'Sign in failed');
  }
  session.token = r.stdout.trim();
  session.unlockedAt = Date.now();
  armAutoLock();
  return { ok: true };
}

function lock() {
  session.token = null;
  session.unlockedAt = null;
  if (session.timer) { clearTimeout(session.timer); session.timer = null; }
  return { ok: true };
}

// ── Vault queries (read-only) ───────────────────────────────────────────────

async function sync() {
  touch();
  const r = await bwRun(['sync'], { timeout: 60000 });
  return { ok: r.ok, error: r.ok ? null : (r.stderr.trim() || 'sync failed') };
}

async function list(query) {
  if (!session.token) return { ok: false, error: 'locked', items: [] };
  touch();
  const args = ['list', 'items'];
  if (query) args.push('--search', String(query));
  const r = await bwRun(args);
  const j = safeJson(r.stdout);
  if (!Array.isArray(j)) return { ok: false, error: r.stderr.trim() || 'could not list items', items: [] };
  return {
    ok: true,
    items: j.map(it => ({
      id: it.id,
      title: it.name || '(untitled)',
      username: it.login?.username || '',
      url: it.login?.uris?.[0]?.uri || '',
      hasPassword: !!it.login?.password,
      hasTotp: !!it.login?.totp,
      folderId: it.folderId || null,
      favorite: !!it.favorite,
    })),
  };
}

// One field, fetched fresh, never cached.
async function getField(id, field) {
  if (!session.token) return { ok: false, error: 'locked' };
  touch();
  const allowed = { password: 'password', username: 'username', totp: 'totp', uri: 'uri', notes: 'notes' };
  const obj = allowed[field];
  if (!obj) return { ok: false, error: 'field not allowed' };
  const r = await bwRun(['get', obj, String(id)]);
  if (!r.ok) return { ok: false, error: r.stderr.trim() || `bw get ${obj} failed` };
  const value = r.stdout.replace(/\r?\n$/, '');
  if (!value) return { ok: false, error: `This item has no ${field}` };
  return { ok: true, value };
}

module.exports = {
  BW_VERSION, RELEASE_BASE,
  getStatus, download, uninstall, resolveBin, assetForPlatform, platformKey,
  authStatus, adoptExistingSession, unlock, useSessionToken, login, lock,
  setAutoLockMinutes, touch, sync, list, getField,
  session,
};
