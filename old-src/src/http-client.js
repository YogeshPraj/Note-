'use strict';
// =============================================================================
// http-client.js — .http / .rest scratchpad
// -----------------------------------------------------------------------------
// Write a request as plain text, press Ctrl+Enter, read the response below the
// editor. A text file you can run — which is what Note++ is for.
//
//     @base = https://api.example.com
//
//     ### create a thing
//     POST {{base}}/things
//     Authorization: Bearer {{vault:Stripe API}}
//     Content-Type: application/json
//
//     { "amount": 2000 }
//
// Requests are separated by ### and Ctrl+Enter runs the one under the cursor.
// {{name}} resolves from @name assignments in the file; {{vault:Item}} is left
// alone here on purpose and resolved in the MAIN process, so a credential never
// reaches this one.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

const HTTP_METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'HEAD', 'OPTIONS', 'TRACE'];
const HTTP_PANEL_PCT = 45;

const httpState = { running: false, lastResponse: null, tabId: null };

function hEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function isHttpTab(tab) {
  if (!tab || tab.type !== 'editor') return false;
  return /\.(http|rest)$/i.test(tab.name || '');
}

// ── Parsing ─────────────────────────────────────────────────────────────────

// File-level variables: `@name = value`, usable as {{name}} anywhere after.
function httpParseVars(lines) {
  const vars = {};
  for (const line of lines) {
    const m = /^\s*@([A-Za-z_][\w-]*)\s*=\s*(.*)$/.exec(line);
    if (m) vars[m[1]] = m[2].trim();
  }
  return vars;
}

// Expand {{name}} from file vars. {{vault:…}} is deliberately NOT touched —
// main resolves those so the secret never lands in this process.
function httpExpand(text, vars) {
  if (typeof text !== 'string') return text;
  return text.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (whole, name) => {
    if (/^vault:/i.test(name)) return whole;
    return Object.prototype.hasOwnProperty.call(vars, name) ? vars[name] : whole;
  });
}

// Split the buffer into request blocks on ### separators, tracking line spans
// so we can tell which one the cursor is in.
function httpSplitBlocks(text) {
  const lines = text.split(/\r?\n/);
  const blocks = [];
  let start = 0, name = null;
  const push = (endExclusive) => {
    if (endExclusive > start) blocks.push({ startLine: start, endLine: endExclusive - 1, name });
  };
  for (let i = 0; i < lines.length; i++) {
    const m = /^\s*###\s*(.*)$/.exec(lines[i]);
    if (m) { push(i); start = i + 1; name = m[1].trim() || null; }
  }
  push(lines.length);
  return { lines, blocks };
}

// Turn one block into { method, url, headers, body }. Returns null when the
// block holds no request line (e.g. it's only @vars or comments).
function httpParseBlock(lines, block, vars) {
  let i = block.startLine;
  const end = block.endLine;
  const skip = (l) => !l.trim() || /^\s*(#|\/\/)/.test(l) || /^\s*@[A-Za-z_]/.test(l);
  while (i <= end && skip(lines[i])) i++;
  if (i > end) return null;

  // "POST https://x" or a bare "https://x" (implicitly GET)
  let m = new RegExp(`^\\s*(${HTTP_METHODS.join('|')})\\s+(\\S+)`, 'i').exec(lines[i]);
  let method, url;
  if (m) { method = m[1].toUpperCase(); url = m[2]; }
  else {
    const bare = /^\s*(\S+)\s*$/.exec(lines[i]);
    if (!bare || !/^(https?:\/\/|\{\{)/i.test(bare[1])) return null;
    method = 'GET'; url = bare[1];
  }
  i++;

  const headers = {};
  for (; i <= end; i++) {
    const line = lines[i];
    if (!line.trim()) { i++; break; }               // blank line ends headers
    if (/^\s*(#|\/\/)/.test(line)) continue;
    const hm = /^\s*([A-Za-z0-9-]+)\s*:\s*(.*)$/.exec(line);
    if (hm) headers[hm[1]] = hm[2].trim();
  }

  const bodyLines = [];
  for (; i <= end; i++) {
    if (/^\s*(#|\/\/)/.test(lines[i]) && !bodyLines.length) continue;
    bodyLines.push(lines[i]);
  }
  const body = bodyLines.join('\n').trim();

  return {
    method,
    url: httpExpand(url, vars),
    headers: Object.fromEntries(Object.entries(headers).map(([k, v]) => [k, httpExpand(v, vars)])),
    body: body ? httpExpand(body, vars) : null,
    name: block.name,
  };
}

function httpRequestAtCursor() {
  const tab = getActiveTab();
  if (!isHttpTab(tab)) return null;
  const text = tab.model.getValue();
  const { lines, blocks } = httpSplitBlocks(text);
  const vars = httpParseVars(lines);
  const cursor = (editor.getPosition()?.lineNumber || 1) - 1;
  const block = blocks.find(b => cursor >= b.startLine && cursor <= b.endLine)
             || blocks.find(b => httpParseBlock(lines, b, vars));
  if (!block) return null;
  return httpParseBlock(lines, block, vars);
}

// ── Send ────────────────────────────────────────────────────────────────────

async function httpSend() {
  const tab = getActiveTab();
  if (!isHttpTab(tab)) { showToast('Open a .http or .rest file to send requests'); return; }
  if (httpState.running) { showToast('A request is already running'); return; }

  const req = httpRequestAtCursor();
  if (!req) { showToast('No request found at the cursor'); return; }

  httpState.running = true;
  httpState.tabId = tab.id;
  httpShowPanel();
  httpRenderPending(req);

  try {
    const res = await window.electronAPI.http.send({
      method: req.method, url: req.url, headers: req.headers, body: req.body,
    });
    httpState.lastResponse = res;
    if (!res || res.success === false) httpRenderError(req, (res && res.error) || 'Request failed');
    else httpRenderResponse(req, res);
  } catch (err) {
    httpRenderError(req, err.message || String(err));
  } finally {
    httpState.running = false;
  }
}

// ── Response panel ──────────────────────────────────────────────────────────

function httpShowPanel() {
  const p = document.getElementById('http-panel');
  if (!p) return;
  p.classList.remove('hidden');
  p.style.flex = `0 0 ${HTTP_PANEL_PCT}%`;
  try { if (editor) editor.layout(); } catch {}
}

function httpHidePanel() {
  const p = document.getElementById('http-panel');
  if (p) { p.classList.add('hidden'); p.innerHTML = ''; }
  try { if (editor) editor.layout(); } catch {}
}

function httpHeaderShell(inner) {
  return `<div id="http-head">
      <span id="http-status"></span>
      <div style="flex:1"></div>
      <button class="h-btn" id="http-copy">Copy body</button>
      <button class="h-btn" id="http-close">&times;</button>
    </div><div id="http-body">${inner}</div>`;
}

function httpRenderPending(req) {
  const p = document.getElementById('http-panel');
  p.innerHTML = httpHeaderShell(`<div class="h-pending">Sending…</div>`);
  document.getElementById('http-status').innerHTML =
    `<span class="h-method">${hEsc(req.method)}</span> <span class="h-url">${hEsc(req.url)}</span>`;
  document.getElementById('http-close').addEventListener('click', httpHidePanel);
}

function httpRenderError(req, msg) {
  const p = document.getElementById('http-panel');
  p.innerHTML = httpHeaderShell(`<div class="h-error">${hEsc(msg)}</div>`);
  document.getElementById('http-status').innerHTML =
    `<span class="h-method">${hEsc(req.method)}</span> <span class="h-badge err">failed</span>`;
  document.getElementById('http-close').addEventListener('click', httpHidePanel);
}

function httpStatusClass(s) {
  if (s >= 200 && s < 300) return 'ok';
  if (s >= 300 && s < 400) return 'redir';
  return 'err';
}

function httpRenderResponse(req, res) {
  const p = document.getElementById('http-panel');
  let bodyHtml;
  if (res.binary) {
    bodyHtml = `<div class="h-pending">${hEsc(res.contentType || 'binary')} · ${res.bytes.toLocaleString()} bytes (not shown)</div>`;
  } else {
    let text = res.body || '';
    // Pretty-print JSON so a one-line API response is actually readable.
    if (/json/i.test(res.contentType || '')) {
      try { text = JSON.stringify(JSON.parse(text), null, 2); } catch {}
    }
    bodyHtml = `<pre class="h-code">${hEsc(text)}</pre>`;
  }

  const headerRows = Object.entries(res.headers || {})
    .map(([k, v]) => `<div class="h-hrow"><span class="h-hk">${hEsc(k)}</span><span class="h-hv">${hEsc(Array.isArray(v) ? v.join(', ') : v)}</span></div>`)
    .join('');

  p.innerHTML = httpHeaderShell(`
    <div class="h-tabs">
      <button class="h-tab active" data-h="body">Body</button>
      <button class="h-tab" data-h="headers">Headers (${Object.keys(res.headers || {}).length})</button>
    </div>
    <div class="h-pane" data-p="body">${bodyHtml}</div>
    <div class="h-pane hidden" data-p="headers">${headerRows}</div>`);

  const bits = [`${res.bytes.toLocaleString()} B`, `${res.timeMs} ms`];
  if (res.redirects) bits.push(`${res.redirects} redirect${res.redirects === 1 ? '' : 's'}`);
  if (res.truncated) bits.push('truncated');
  // Names only — the values were resolved in main and never came here.
  if (res.vaultRefsUsed && res.vaultRefsUsed.length) bits.push(`vault: ${res.vaultRefsUsed.join(', ')}`);

  document.getElementById('http-status').innerHTML =
    `<span class="h-badge ${httpStatusClass(res.status)}">${res.status} ${hEsc(res.statusText)}</span>` +
    `<span class="h-method">${hEsc(req.method)}</span>` +
    `<span class="h-meta">${hEsc(bits.join(' · '))}</span>`;

  document.getElementById('http-close').addEventListener('click', httpHidePanel);
  document.getElementById('http-copy').addEventListener('click', () => {
    try { navigator.clipboard.writeText(res.body || ''); showToast('Response body copied'); } catch {}
  });
  p.querySelectorAll('.h-tab').forEach(t => t.addEventListener('click', () => {
    p.querySelectorAll('.h-tab').forEach(x => x.classList.toggle('active', x === t));
    p.querySelectorAll('.h-pane').forEach(x => x.classList.toggle('hidden', x.dataset.p !== t.dataset.h));
  }));
}

// ── Init ────────────────────────────────────────────────────────────────────

function initHttpClient() {
  // Ctrl+Enter / Cmd+Enter sends the request under the cursor. Capture phase so
  // Monaco doesn't swallow it first.
  document.addEventListener('keydown', (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      const tab = getActiveTab();
      if (isHttpTab(tab)) { e.preventDefault(); e.stopPropagation(); httpSend(); }
    }
  }, true);
}

// Hide the response when moving to a tab it doesn't belong to.
function httpOnTabChanged(tab) {
  if (httpState.tabId != null && (!tab || tab.id !== httpState.tabId)) httpHidePanel();
}
