'use strict';
// =============================================================================
// vault-cli.js — read-only bridge to an existing password manager (MAIN ONLY)
// -----------------------------------------------------------------------------
// Option C of the vault plan: rather than asking you to migrate your real
// credentials into Note++, we talk to the manager you already trust. Note++
// stores nothing, syncs nothing, and holds no master password — it shells out
// to a CLI that is already unlocked and asks it for one field at a time.
//
// Supported:
//   • Bitwarden / Vaultwarden  `bw`              — needs an unlocked session
//   • 1Password                `op`              — manages its own session
//   • KeePassXC                `keepassxc-cli`   — detected, but see the note
//                                                  on why it can't be seamless
//
// Everything here is read-only by construction: there is no code path that
// writes to, edits, or unlocks the upstream vault.
// =============================================================================

const { execFile } = require('child_process');
const path = require('path');

const EXEC_TIMEOUT_MS = 15000;   // a hung CLI must never wedge the app
const MAX_BUFFER = 8 * 1024 * 1024;

// Promise wrapper. Never rejects on non-zero exit — callers want the stderr
// to decide whether it's "locked" vs "not installed" vs a real failure.
function run(cmd, args, opts) {
  opts = opts || {};
  return new Promise(resolve => {
    let done = false;
    const child = execFile(cmd, args, {
      timeout: opts.timeout || EXEC_TIMEOUT_MS,
      maxBuffer: MAX_BUFFER,
      windowsHide: true,
      env: { ...process.env, ...(opts.env || {}) },
    }, (err, stdout, stderr) => {
      if (done) return;
      done = true;
      resolve({
        ok: !err,
        code: err ? (err.code === undefined ? -1 : err.code) : 0,
        stdout: String(stdout || ''),
        stderr: String(stderr || (err && err.message) || ''),
        timedOut: !!(err && err.killed),
      });
    });
    child.on('error', () => {
      if (done) return;
      done = true;
      resolve({ ok: false, code: -1, stdout: '', stderr: 'ENOENT', notFound: true });
    });
  });
}

function safeJson(text) {
  try { return JSON.parse(text); } catch { return null; }
}

// ── Bitwarden / Vaultwarden ─────────────────────────────────────────────────
// `bw` keeps its unlock state in a session token the user exports as
// BW_SESSION. We read it from the environment; we never ask for, store, or
// transmit the master password.

const bitwarden = {
  id: 'bw',
  name: 'Bitwarden',
  bin: 'bw',
  hint: 'Run `bw unlock` and export BW_SESSION, then reopen this panel.',

  async detect() {
    const r = await run(this.bin, ['--version']);
    if (r.notFound || (!r.ok && /not recognized|ENOENT|not found/i.test(r.stderr))) {
      return { installed: false };
    }
    return { installed: true, version: r.stdout.trim() || null };
  },

  async status() {
    const r = await run(this.bin, ['status']);
    const j = safeJson(r.stdout);
    if (!j) return { unlocked: false, reason: r.stderr.trim() || 'could not read status' };
    // status is one of: unauthenticated | locked | unlocked
    if (j.status === 'unlocked') return { unlocked: true };
    if (j.status === 'locked')   return { unlocked: false, reason: 'Vault is locked — run `bw unlock`' };
    return { unlocked: false, reason: 'Not signed in — run `bw login`' };
  },

  async list(query) {
    const args = ['list', 'items'];
    if (query) args.push('--search', query);
    const r = await run(this.bin, args);
    const j = safeJson(r.stdout);
    if (!Array.isArray(j)) {
      return { ok: false, error: r.stderr.trim() || 'bw returned no items', items: [] };
    }
    return {
      ok: true,
      items: j.map(it => ({
        id: it.id,
        title: it.name || '(untitled)',
        username: it.login?.username || '',
        url: it.login?.uris?.[0]?.uri || '',
        hasPassword: !!it.login?.password,
        hasTotp: !!it.login?.totp,
        folder: it.folderId || null,
      })),
    };
  },

  async getField(id, field) {
    // `bw get` takes an object type directly — password / username / totp.
    const objectMap = { password: 'password', username: 'username', totp: 'totp' };
    const obj = objectMap[field];
    if (!obj) return { ok: false, error: 'unsupported field' };
    const r = await run(this.bin, ['get', obj, id]);
    if (!r.ok) return { ok: false, error: r.stderr.trim() || 'bw get failed' };
    return { ok: true, value: r.stdout.replace(/\r?\n$/, '') };
  },
};

// ── 1Password ───────────────────────────────────────────────────────────────
// `op` 2.x manages its own session (biometric / desktop app integration), so
// there's no token for us to handle.

const onepassword = {
  id: 'op',
  name: '1Password',
  bin: 'op',
  hint: 'Sign in with `op signin`, or enable the 1Password desktop app integration.',

  async detect() {
    const r = await run(this.bin, ['--version']);
    if (r.notFound || (!r.ok && /not recognized|ENOENT|not found/i.test(r.stderr))) {
      return { installed: false };
    }
    return { installed: true, version: r.stdout.trim() || null };
  },

  async status() {
    const r = await run(this.bin, ['account', 'list', '--format', 'json']);
    const j = safeJson(r.stdout);
    if (Array.isArray(j) && j.length) return { unlocked: true };
    return { unlocked: false, reason: r.stderr.trim() || 'Not signed in — run `op signin`' };
  },

  async list(query) {
    const r = await run(this.bin, ['item', 'list', '--format', 'json']);
    const j = safeJson(r.stdout);
    if (!Array.isArray(j)) {
      return { ok: false, error: r.stderr.trim() || 'op returned no items', items: [] };
    }
    const q = (query || '').toLowerCase();
    return {
      ok: true,
      items: j
        .filter(it => !q || String(it.title || '').toLowerCase().includes(q))
        .map(it => ({
          id: it.id,
          title: it.title || '(untitled)',
          username: it.additional_information || '',
          url: it.urls?.[0]?.href || '',
          // `op item list` doesn't say which fields exist without a per-item
          // fetch, so we optimistically offer both and let getField fail
          // cleanly if the item has neither.
          hasPassword: true,
          hasTotp: false,
          folder: it.vault?.name || null,
        })),
    };
  },

  async getField(id, field) {
    const fieldMap = { password: 'password', username: 'username', totp: 'otp' };
    const f = fieldMap[field];
    if (!f) return { ok: false, error: 'unsupported field' };
    const r = await run(this.bin, ['item', 'get', id, '--fields', f, '--reveal']);
    if (!r.ok) return { ok: false, error: r.stderr.trim() || 'op get failed' };
    return { ok: true, value: r.stdout.replace(/\r?\n$/, '') };
  },
};

// ── KeePassXC ───────────────────────────────────────────────────────────────
// Detected for completeness, but deliberately NOT wired up for browsing.
// keepassxc-cli has no session daemon: every command re-prompts for the
// database password on stdin. Making this "seamless" would mean Note++ holding
// your KeePass master password in memory for the lifetime of the app — which
// is precisely the thing this design exists to avoid. Use the built-in vault
// or bw/op instead.

const keepassxc = {
  id: 'keepassxc',
  name: 'KeePassXC',
  bin: 'keepassxc-cli',
  hint: 'Detected, but browsing is not supported: keepassxc-cli re-prompts for '
      + 'your database password on every command, and caching it in Note++ '
      + 'would defeat the point. Use the built-in vault, bw, or op.',
  browsable: false,

  async detect() {
    const r = await run(this.bin, ['--version']);
    if (r.notFound || (!r.ok && /not recognized|ENOENT|not found/i.test(r.stderr))) {
      return { installed: false };
    }
    return { installed: true, version: r.stdout.trim() || null };
  },
  async status() { return { unlocked: false, reason: this.hint }; },
  async list()   { return { ok: false, error: this.hint, items: [] }; },
  async getField() { return { ok: false, error: this.hint }; },
};

const PROVIDERS = [bitwarden, onepassword, keepassxc];

function getProvider(id) {
  return PROVIDERS.find(p => p.id === id) || null;
}

// Probe every provider once. Detection spawns three short-lived processes, so
// the caller should cache this rather than calling it per keystroke.
async function detectAll() {
  const out = [];
  for (const p of PROVIDERS) {
    let det = { installed: false };
    try { det = await p.detect(); } catch {}
    let status = { unlocked: false, reason: null };
    if (det.installed && p.browsable !== false) {
      try { status = await p.status(); } catch (e) { status = { unlocked: false, reason: e.message }; }
    } else if (det.installed) {
      status = { unlocked: false, reason: p.hint };
    }
    out.push({
      id: p.id,
      name: p.name,
      bin: p.bin,
      hint: p.hint,
      browsable: p.browsable !== false,
      installed: !!det.installed,
      version: det.version || null,
      unlocked: !!status.unlocked,
      reason: status.reason || null,
    });
  }
  return out;
}

module.exports = { detectAll, getProvider, PROVIDERS };
