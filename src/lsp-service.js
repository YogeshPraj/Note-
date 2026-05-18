// =============================================================================
// Note++ — LSP service (main process)
// =============================================================================
// One subprocess per language. JSON-RPC over stdio with standard LSP framing
// (`Content-Length: <n>\r\n\r\n<body>`). Each server is launched on demand,
// initialized with a basic capability set, and torn down a few seconds after
// the last document closes.
//
// Renderer talks to us through 3 IPC channels (wired in main.js):
//   - `lsp-ensure-started`  → look up registry, spawn if needed, init
//   - `lsp-send`            → forward an arbitrary LSP request (returns reply)
//   - `lsp-notify`          → forward a fire-and-forget notification
//
// We push server events back to the renderer:
//   - `lsp-status`        { langId, state, error? }
//   - `lsp-notification`  { langId, method, params }     // e.g. publishDiagnostics
//   - `lsp-response`      { langId, id, result, error }  // mirror, mostly debug
// =============================================================================

const { spawn } = require('child_process');
const path      = require('path');

// ── Language registry ────────────────────────────────────────────────────
// Adding a new language is one entry here + Monaco's language id matches.
const LSP_LANGUAGES = {
  python: {
    extensions: ['.py', '.pyi'],
    monacoIds:  ['python'],
    // We try each candidate in order — first one that spawns wins.
    candidates: [
      { cmd: 'pyright-langserver', args: ['--stdio'] },
      { cmd: 'npx', args: ['-y', 'pyright-langserver', '--stdio'] },
    ],
    install: {
      label: 'pyright',
      cmd:   'npm install -g pyright',
      note:  'Python IntelliSense needs pyright. Run: npm install -g pyright',
    },
  },
  // Future:
  //   go:   { extensions: ['.go'], monacoIds: ['go'],   candidates: [{ cmd: 'gopls', args: [] }], ... }
  //   rust: { extensions: ['.rs'], monacoIds: ['rust'], candidates: [{ cmd: 'rust-analyzer', args: [] }], ... }
};

// In-memory state — one entry per language we've ever started this session.
const servers = new Map();   // langId → { proc, state, buffer, pending, idleTimer, restarts, lastError }
const SHUTDOWN_IDLE_MS = 30000;
const MAX_RESTARTS = 3;
const RESTART_WINDOW_MS = 30000;

function isLanguageSupported(langId) {
  return !!LSP_LANGUAGES[langId];
}

function getLanguageConfig(langId) {
  return LSP_LANGUAGES[langId] || null;
}

// Best-effort lookup from a Monaco language id to our registry key.
function lookupByMonacoId(monacoId) {
  for (const [k, v] of Object.entries(LSP_LANGUAGES)) {
    if (v.monacoIds.includes(monacoId)) return k;
  }
  return null;
}

// ── stdio JSON-RPC framing ───────────────────────────────────────────────
function encodeMessage(obj) {
  const body = Buffer.from(JSON.stringify(obj), 'utf8');
  const header = Buffer.from(`Content-Length: ${body.length}\r\n\r\n`, 'ascii');
  return Buffer.concat([header, body]);
}

// Pulls complete LSP messages out of a rolling byte buffer.
// Returns { messages: [...], remainder: Buffer }
function parseFrames(buf) {
  const messages = [];
  let cursor = 0;
  while (cursor < buf.length) {
    // Find the header / body separator
    const headerEnd = buf.indexOf('\r\n\r\n', cursor, 'ascii');
    if (headerEnd < 0) break;     // partial header, wait for more bytes
    const header = buf.slice(cursor, headerEnd).toString('ascii');
    const m = header.match(/Content-Length:\s*(\d+)/i);
    if (!m) {
      // Malformed; resync by skipping past the separator
      cursor = headerEnd + 4;
      continue;
    }
    const len = parseInt(m[1], 10);
    const bodyStart = headerEnd + 4;
    if (bodyStart + len > buf.length) break;  // partial body
    const body = buf.slice(bodyStart, bodyStart + len).toString('utf8');
    try {
      messages.push(JSON.parse(body));
    } catch (err) {
      // skip bad frame
    }
    cursor = bodyStart + len;
  }
  return { messages, remainder: buf.slice(cursor) };
}

// ── Server lifecycle ─────────────────────────────────────────────────────

// Spawn (or return existing) server. Resolves to { ready: bool, error? }.
async function ensureStarted(langId, workspaceRoot, sender) {
  const cfg = getLanguageConfig(langId);
  if (!cfg) return { ready: false, error: 'Unsupported language: ' + langId };

  let s = servers.get(langId);
  if (s && (s.state === 'ready' || s.state === 'starting')) {
    if (s.state === 'ready') return { ready: true };
    // starting — return the in-flight init promise
    return s.initPromise;
  }
  // Fresh start (or restart after crash)
  s = createServerEntry(langId);
  servers.set(langId, s);

  s.initPromise = startServerProcess(langId, cfg, workspaceRoot, sender);
  return s.initPromise;
}

function createServerEntry(langId) {
  return {
    langId,
    proc: null,
    state: 'idle',         // idle | starting | ready | crashed | missing
    buffer: Buffer.alloc(0),
    pending: new Map(),    // id → { resolve, reject, method, sentAt }
    nextId: 1,
    initPromise: null,
    idleTimer: null,
    restarts: [],          // timestamps of recent restarts
    lastError: null,
    openDocs: new Map(),   // uri → version (so we know whether to send didOpen)
  };
}

async function startServerProcess(langId, cfg, workspaceRoot, sender) {
  const s = servers.get(langId);
  s.state = 'starting';
  emitStatus(sender, langId, 'starting');

  // Try each candidate command until one spawns
  let proc = null;
  let lastErr = null;
  for (const c of cfg.candidates) {
    try {
      proc = spawn(c.cmd, c.args, {
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
        cwd: workspaceRoot || process.cwd(),
        env: { ...process.env },
        shell: process.platform === 'win32',  // resolve .cmd shims on Windows
      });
      // Wait one microtask to catch immediate "ENOENT"
      await new Promise(r => setTimeout(r, 30));
      if (proc.killed || proc.exitCode != null) {
        proc = null;
        continue;
      }
      break;
    } catch (err) { lastErr = err; proc = null; }
  }

  if (!proc) {
    s.state = 'missing';
    s.lastError = lastErr ? lastErr.message : 'language server binary not found';
    emitStatus(sender, langId, 'missing', cfg.install?.note || s.lastError);
    return { ready: false, error: s.lastError, install: cfg.install };
  }

  s.proc = proc;

  // Wire stdio
  proc.stdout.on('data', (chunk) => onServerData(langId, chunk, sender));
  proc.stderr.on('data', (chunk) => {
    // Many LSP servers chatter on stderr — keep visible in main console for debugging
    process.stderr.write(`[lsp:${langId}] ${chunk}`);
  });
  proc.on('error', (err) => {
    s.lastError = err.message;
    emitStatus(sender, langId, 'crashed', err.message);
  });
  proc.on('exit', (code, signal) => onServerExit(langId, code, signal, sender));

  // LSP initialize handshake
  try {
    const initResult = await sendRequest(langId, 'initialize', {
      processId: process.pid,
      rootUri: workspaceRoot ? pathToFileUri(workspaceRoot) : null,
      workspaceFolders: workspaceRoot
        ? [{ uri: pathToFileUri(workspaceRoot), name: path.basename(workspaceRoot) }]
        : null,
      capabilities: clientCapabilities(),
      clientInfo: { name: 'Note++', version: '1.3' },
      initializationOptions: {},
    });
    sendNotification(langId, 'initialized', {});
    s.state = 'ready';
    s.initResult = initResult;
    emitStatus(sender, langId, 'ready');
    return { ready: true };
  } catch (err) {
    s.state = 'crashed';
    s.lastError = err.message;
    emitStatus(sender, langId, 'crashed', err.message);
    try { proc.kill(); } catch {}
    return { ready: false, error: err.message };
  }
}

// Minimal client capability set — we only consume diagnostics, hover, completion in v1.
// Servers degrade gracefully when features aren't declared.
function clientCapabilities() {
  return {
    textDocument: {
      synchronization: { didSave: true, willSave: false },
      publishDiagnostics: { relatedInformation: true },
      hover: { contentFormat: ['markdown', 'plaintext'] },
      completion: {
        completionItem: {
          snippetSupport: true,
          documentationFormat: ['markdown', 'plaintext'],
          insertReplaceSupport: false,
        },
        contextSupport: true,
      },
    },
    workspace: {
      workspaceFolders: true,
      configuration: false,
    },
  };
}

function pathToFileUri(p) {
  let s = p.replace(/\\/g, '/');
  if (!s.startsWith('/')) s = '/' + s;
  return 'file://' + encodeURI(s).replace(/#/g, '%23').replace(/\?/g, '%3F');
}

// ── Incoming data from a server ──────────────────────────────────────────
function onServerData(langId, chunk, sender) {
  const s = servers.get(langId);
  if (!s) return;
  s.buffer = Buffer.concat([s.buffer, chunk]);
  const { messages, remainder } = parseFrames(s.buffer);
  s.buffer = remainder;

  for (const msg of messages) {
    if (msg.id != null && (msg.result !== undefined || msg.error !== undefined)) {
      // Response to one of our requests
      const pending = s.pending.get(msg.id);
      if (pending) {
        s.pending.delete(msg.id);
        if (msg.error) pending.reject(new Error(msg.error.message || 'LSP error'));
        else           pending.resolve(msg.result);
      }
    } else if (msg.method) {
      // Server-pushed notification (e.g. textDocument/publishDiagnostics)
      // or a server-initiated request (we don't reply to most v1)
      if (msg.id != null) {
        // Server request — respond with empty / not-supported
        const reply = encodeMessage({ jsonrpc: '2.0', id: msg.id, result: null });
        try { s.proc.stdin.write(reply); } catch {}
      }
      try { sender.send('lsp-notification', { langId, method: msg.method, params: msg.params }); } catch {}
    }
  }
}

function onServerExit(langId, code, signal, sender) {
  const s = servers.get(langId);
  if (!s) return;
  s.proc = null;
  s.buffer = Buffer.alloc(0);
  for (const p of s.pending.values()) p.reject(new Error('LSP server exited'));
  s.pending.clear();
  if (s.state !== 'idle') {
    s.lastError = `exited (code=${code} signal=${signal})`;
    emitStatus(sender, langId, 'crashed', s.lastError);
    // Auto-restart up to MAX_RESTARTS within RESTART_WINDOW_MS
    const now = Date.now();
    s.restarts = s.restarts.filter(t => now - t < RESTART_WINDOW_MS);
    if (s.restarts.length < MAX_RESTARTS) {
      s.restarts.push(now);
      const cfg = getLanguageConfig(langId);
      if (cfg) setTimeout(() => startServerProcess(langId, cfg, null, sender), 500);
    }
  }
}

// ── Sending requests / notifications ────────────────────────────────────
function sendRequest(langId, method, params) {
  const s = servers.get(langId);
  if (!s || !s.proc) return Promise.reject(new Error('Server not running: ' + langId));
  const id = s.nextId++;
  const msg = { jsonrpc: '2.0', id, method, params };
  return new Promise((resolve, reject) => {
    s.pending.set(id, { resolve, reject, method, sentAt: Date.now() });
    try { s.proc.stdin.write(encodeMessage(msg)); }
    catch (err) { s.pending.delete(id); reject(err); }
    // Safety timeout — most requests should return in <1 s, give them 8 s
    setTimeout(() => {
      if (s.pending.has(id)) {
        s.pending.delete(id);
        reject(new Error('LSP request timed out: ' + method));
      }
    }, 8000);
  });
}

function sendNotification(langId, method, params) {
  const s = servers.get(langId);
  if (!s || !s.proc) return;
  try { s.proc.stdin.write(encodeMessage({ jsonrpc: '2.0', method, params })); }
  catch {}
}

// Graceful shutdown — for app exit or explicit stop
async function stopServer(langId) {
  const s = servers.get(langId);
  if (!s || !s.proc) return;
  try { await sendRequest(langId, 'shutdown', null); } catch {}
  try { sendNotification(langId, 'exit', null); } catch {}
  setTimeout(() => { try { s.proc?.kill(); } catch {} }, 500);
  s.state = 'idle';
}

async function stopAllServers() {
  await Promise.allSettled([...servers.keys()].map(stopServer));
}

// ── Status emitter (uses provided sender so we can target a specific window) ─
function emitStatus(sender, langId, state, error) {
  try { sender?.send('lsp-status', { langId, state, error: error || null }); } catch {}
}

module.exports = {
  LSP_LANGUAGES,
  isLanguageSupported,
  getLanguageConfig,
  lookupByMonacoId,
  ensureStarted,
  sendRequest,
  sendNotification,
  stopServer,
  stopAllServers,
};
