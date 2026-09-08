'use strict';
// =============================================================================
// http-service.js — run .http requests from the main process
// -----------------------------------------------------------------------------
// The renderer parses the .http buffer and hands over a request TEMPLATE, with
// {{vault:Item}} placeholders left unresolved. This process substitutes them
// from the unlocked Bitwarden vault and performs the request.
//
// That split is the point: a secret used in a request never enters the
// renderer, which is the process hosting HTML previews, the whiteboard/draw.io
// iframes and Ollama output. It also means the Passwords-tab-must-be-focused
// guard doesn't need relaxing to make this feature work — the renderer is
// never handed the value in the first place.
//
// HONEST LIMITATION: a server that echoes back what you sent (a /debug or
// /anything endpoint) will put the secret in the RESPONSE, which the renderer
// does see and display. Substitution protects the request path, not a remote
// service's decision to reflect it.
// =============================================================================

const http = require('http');
const https = require('https');
const { URL } = require('url');
const zlib = require('zlib');

const MAX_BODY_BYTES = 8 * 1024 * 1024;   // beyond this we truncate and say so
const MAX_REDIRECTS = 5;
const DEFAULT_TIMEOUT_MS = 30000;

// ── Placeholder substitution ────────────────────────────────────────────────

// {{vault:Item Name}} and {{vault:Item Name.field}} — field defaults to
// password; username / totp / uri / notes are also allowed.
const VAULT_RE = /\{\{\s*vault:([^}]+?)\s*\}\}/gi;

let _bw = null;
function bw() {
  if (!_bw) _bw = require('./bitwarden-service');
  return _bw;
}

// Resolve every {{vault:…}} in a string. Returns { text, used[], missing[] }.
async function resolveVaultRefs(text, cache) {
  if (typeof text !== 'string' || text.indexOf('{{') === -1) {
    return { text, used: [], missing: [] };
  }
  const refs = [];
  let m;
  VAULT_RE.lastIndex = 0;
  while ((m = VAULT_RE.exec(text)) !== null) refs.push(m[1].trim());
  if (!refs.length) return { text, used: [], missing: [] };

  const used = [], missing = [];
  let out = text;
  for (const ref of refs) {
    if (cache.has(ref)) {
      const v = cache.get(ref);
      if (v == null) { if (!missing.includes(ref)) missing.push(ref); continue; }
      out = out.split(`{{vault:${ref}}}`).join(v);
      if (!used.includes(ref)) used.push(ref);
      continue;
    }
    const dot = ref.lastIndexOf('.');
    let title = ref, field = 'password';
    if (dot > 0 && ['password', 'username', 'totp', 'uri', 'notes'].includes(ref.slice(dot + 1))) {
      title = ref.slice(0, dot);
      field = ref.slice(dot + 1);
    }
    let value = null;
    try {
      const list = await bw().list(title);
      if (list.ok && list.items.length) {
        const exact = list.items.find(i => i.title.toLowerCase() === title.toLowerCase()) || list.items[0];
        const got = await bw().getField(exact.id, field);
        if (got.ok) value = got.value;
      }
    } catch { /* treated as missing below */ }
    cache.set(ref, value);
    if (value == null) { missing.push(ref); continue; }
    out = out.split(`{{vault:${ref}}}`).join(value);
    used.push(ref);
  }
  // Case-insensitive form of the same placeholders, for {{VAULT:...}}.
  out = out.replace(VAULT_RE, (whole, name) => {
    const v = cache.get(String(name).trim());
    return v == null ? whole : v;
  });
  return { text: out, used, missing };
}

// ── Request ─────────────────────────────────────────────────────────────────

function decodeBody(buf, encoding) {
  try {
    if (encoding === 'gzip') return zlib.gunzipSync(buf);
    if (encoding === 'deflate') return zlib.inflateSync(buf);
    if (encoding === 'br') return zlib.brotliDecompressSync(buf);
  } catch { /* fall through to the raw bytes */ }
  return buf;
}

function performOnce(reqOpts, bodyBuf, timeoutMs) {
  return new Promise((resolve, reject) => {
    const lib = reqOpts.protocol === 'https:' ? https : http;
    const started = Date.now();
    const req = lib.request(reqOpts, (res) => {
      const chunks = [];
      let total = 0, truncated = false;
      res.on('data', (c) => {
        total += c.length;
        if (total <= MAX_BODY_BYTES) chunks.push(c);
        else if (!truncated) { truncated = true; res.destroy(); }
      });
      res.on('end', () => finish());
      res.on('close', () => finish());
      let done = false;
      function finish() {
        if (done) return;
        done = true;
        const raw = decodeBody(Buffer.concat(chunks), String(res.headers['content-encoding'] || '').toLowerCase());
        resolve({
          status: res.statusCode,
          statusText: res.statusMessage || '',
          headers: res.headers,
          bodyBuf: raw,
          bytes: total,
          truncated,
          timeMs: Date.now() - started,
        });
      }
    });
    req.on('error', reject);
    req.setTimeout(timeoutMs || DEFAULT_TIMEOUT_MS, () => {
      req.destroy(new Error(`Request timed out after ${Math.round((timeoutMs || DEFAULT_TIMEOUT_MS) / 1000)}s`));
    });
    if (bodyBuf && bodyBuf.length) req.write(bodyBuf);
    req.end();
  });
}

// template: { method, url, headers: {k:v}, body: string|null, timeoutMs }
async function sendRequest(template) {
  const cache = new Map();
  const usedRefs = [], missingRefs = [];
  const take = (r) => {
    r.used.forEach(x => { if (!usedRefs.includes(x)) usedRefs.push(x); });
    r.missing.forEach(x => { if (!missingRefs.includes(x)) missingRefs.push(x); });
    return r.text;
  };

  let url = take(await resolveVaultRefs(String(template.url || ''), cache));
  const headers = {};
  for (const [k, v] of Object.entries(template.headers || {})) {
    headers[k] = take(await resolveVaultRefs(String(v), cache));
  }
  let body = template.body == null ? null : take(await resolveVaultRefs(String(template.body), cache));

  if (missingRefs.length) {
    throw new Error(
      `Could not resolve ${missingRefs.map(r => '{{vault:' + r + '}}').join(', ')} — ` +
      `unlock the Passwords tab, or check the item name.`);
  }

  let parsed;
  try { parsed = new URL(url); }
  catch { throw new Error(`Invalid URL: ${url}`); }
  if (!/^https?:$/.test(parsed.protocol)) throw new Error(`Unsupported protocol: ${parsed.protocol}`);

  const method = String(template.method || 'GET').toUpperCase();
  const bodyBuf = body != null && body !== '' ? Buffer.from(body, 'utf8') : null;

  const hdrs = { ...headers };
  const hasHeader = (n) => Object.keys(hdrs).some(k => k.toLowerCase() === n);
  if (!hasHeader('user-agent')) hdrs['User-Agent'] = 'NotePP/http';
  if (!hasHeader('accept')) hdrs['Accept'] = '*/*';
  if (!hasHeader('accept-encoding')) hdrs['Accept-Encoding'] = 'gzip, deflate, br';
  if (bodyBuf && !hasHeader('content-length')) hdrs['Content-Length'] = String(bodyBuf.length);

  let redirects = 0;
  let current = parsed;
  let currentMethod = method;
  let currentBody = bodyBuf;

  while (true) {
    const res = await performOnce({
      protocol: current.protocol,
      hostname: current.hostname,
      port: current.port || (current.protocol === 'https:' ? 443 : 80),
      path: current.pathname + current.search,
      method: currentMethod,
      headers: hdrs,
    }, currentBody, template.timeoutMs);

    const loc = res.headers.location;
    if (loc && res.status >= 300 && res.status < 400 && redirects < MAX_REDIRECTS) {
      redirects++;
      current = new URL(loc, current);
      // 303 (and 301/302 in practice) drop to GET without a body.
      if (res.status === 303 || ((res.status === 301 || res.status === 302) && currentMethod !== 'HEAD')) {
        currentMethod = 'GET';
        currentBody = null;
        delete hdrs['Content-Length'];
      }
      continue;
    }

    const ct = String(res.headers['content-type'] || '');
    const isText = /^(text\/|application\/(json|xml|javascript|x-www-form-urlencoded|graphql)|.*\+json|.*\+xml)/i.test(ct)
                   || !ct;
    return {
      ok: true,
      status: res.status,
      statusText: res.statusText,
      headers: res.headers,
      body: isText ? res.bodyBuf.toString('utf8') : null,
      binary: !isText,
      contentType: ct,
      bytes: res.bytes,
      truncated: res.truncated,
      timeMs: res.timeMs,
      redirects,
      finalUrl: current.toString(),
      // Names only — never the values. Useful for "which secrets did this use?"
      vaultRefsUsed: usedRefs,
    };
  }
}

module.exports = { sendRequest, resolveVaultRefs, MAX_BODY_BYTES };
