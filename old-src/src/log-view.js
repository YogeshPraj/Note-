'use strict';
// =============================================================================
// log-view.js — follow-mode log viewer
// -----------------------------------------------------------------------------
// A live view of a growing log: new lines stream in, severity is colourised,
// and a filter box narrows to what you care about. Toggled on the same tab as
// the editor, like the table view.
//
// Memory is bounded on purpose. A log that grows all afternoon would otherwise
// grow this array all afternoon; LOG_MAX_LINES caps the ring and the header
// says when older lines have been dropped, rather than quietly pretending the
// buffer is complete.
//
// Rendering is windowed: only the last LOG_RENDER_LINES matching lines become
// DOM. Filtering 200k buffered lines is a cheap array scan; putting 200k <div>s
// on the page is not.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

const LOG_MAX_LINES = 200000;   // ring buffer cap
const LOG_RENDER_LINES = 2000;  // DOM window
const LOG_TRIM_CHUNK = 20000;

const logState = {
  tabId: null,
  filePath: null,
  lines: [],
  dropped: 0,
  filter: '',
  regexMode: false,
  follow: true,
  levels: { error: true, warn: true, info: true, debug: true, other: true },
  unsub: null,
  pendingRender: false,
};

function lEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// Classify by the conventions that actually show up in logs. Deliberately
// conservative — a line mentioning "error handling" is not an error.
function logLevel(line) {
  if (/\b(ERROR|ERR|FATAL|CRITICAL|SEVERE|PANIC)\b/.test(line)) return 'error';
  if (/\b(WARN|WARNING)\b/.test(line)) return 'warn';
  if (/\b(INFO|NOTICE)\b/.test(line)) return 'info';
  if (/\b(DEBUG|TRACE|VERBOSE)\b/.test(line)) return 'debug';
  return 'other';
}

function logCanShow(tab) {
  return !!(tab && tab.type === 'editor' && tab.filePath);
}

function logIsOpen() { return logState.tabId != null; }

// ── Filtering ───────────────────────────────────────────────────────────────

function logMatches(line) {
  if (!logState.levels[logLevel(line)]) return false;
  const f = logState.filter;
  if (!f) return true;
  if (logState.regexMode) {
    try { return new RegExp(f, 'i').test(line); }
    catch { return true; }          // an in-progress regex shouldn't blank the view
  }
  return line.toLowerCase().includes(f.toLowerCase());
}

function logVisibleLines() {
  const out = [];
  // Walk backwards so a huge buffer costs only what the window needs.
  for (let i = logState.lines.length - 1; i >= 0 && out.length < LOG_RENDER_LINES; i--) {
    if (logMatches(logState.lines[i])) out.push({ n: i + 1 + logState.dropped, text: logState.lines[i] });
  }
  return out.reverse();
}

// ── Rendering ───────────────────────────────────────────────────────────────

function logRenderBody() {
  const body = document.getElementById('log-lines');
  if (!body) return;
  const vis = logVisibleLines();
  body.innerHTML = vis.map(l =>
    `<div class="lg-line lg-${logLevel(l.text)}"><span class="lg-n">${l.n}</span><span class="lg-t">${lEsc(l.text)}</span></div>`
  ).join('');
  if (logState.follow) body.scrollTop = body.scrollHeight;
  const count = document.getElementById('lg-count');
  if (count) {
    const shown = vis.length;
    const total = logState.lines.length;
    count.textContent = `${shown.toLocaleString()} shown · ${total.toLocaleString()} buffered` +
      (logState.dropped ? ` · ${logState.dropped.toLocaleString()} dropped` : '');
  }
}

// Batch renders to a frame: a chatty log can deliver several chunks per poll
// and re-rendering per chunk would thrash.
function logScheduleRender() {
  if (logState.pendingRender) return;
  logState.pendingRender = true;
  requestAnimationFrame(() => {
    logState.pendingRender = false;
    logRenderBody();
  });
}

function logRenderShell() {
  const host = document.getElementById('log-container');
  if (!host) return;
  const lv = logState.levels;
  host.innerHTML = `
    <div id="log-toolbar">
      <button class="lg-btn ${logState.follow ? 'active' : ''}" id="lg-follow" title="Scroll as new lines arrive">
        ${logState.follow ? '⏸ Following' : '▶ Follow'}
      </button>
      <input id="lg-filter" type="text" placeholder="Filter lines…" value="${lEsc(logState.filter)}">
      <button class="lg-btn ${logState.regexMode ? 'active' : ''}" id="lg-regex" title="Treat the filter as a regular expression">.*</button>
      <span class="lg-levels">
        ${['error', 'warn', 'info', 'debug', 'other'].map(k =>
          `<button class="lg-lvl lg-${k} ${lv[k] ? 'on' : ''}" data-lvl="${k}">${k}</button>`).join('')}
      </span>
      <div style="flex:1"></div>
      <span class="lg-count" id="lg-count"></span>
      <button class="lg-btn" id="lg-clear" title="Clear the buffer (the file is untouched)">Clear</button>
      <button class="lg-btn" id="lg-close">Text view</button>
    </div>
    <div id="log-lines"></div>`;

  const filt = document.getElementById('lg-filter');
  let deb;
  filt.addEventListener('input', () => {
    clearTimeout(deb);
    deb = setTimeout(() => { logState.filter = filt.value; logRenderBody(); }, 150);
  });
  document.getElementById('lg-regex').addEventListener('click', (e) => {
    logState.regexMode = !logState.regexMode;
    e.currentTarget.classList.toggle('active', logState.regexMode);
    logRenderBody();
  });
  document.getElementById('lg-follow').addEventListener('click', () => {
    logState.follow = !logState.follow;
    logRenderShell();
    logRenderBody();
  });
  document.getElementById('lg-clear').addEventListener('click', () => {
    logState.lines = []; logState.dropped = 0; logRenderBody();
  });
  document.getElementById('lg-close').addEventListener('click', () => closeLogView());
  host.querySelectorAll('.lg-lvl').forEach(b => b.addEventListener('click', () => {
    const k = b.dataset.lvl;
    logState.levels[k] = !logState.levels[k];
    b.classList.toggle('on', logState.levels[k]);
    logRenderBody();
  }));

  // Scrolling up by hand means you want to read, not chase the tail.
  const body = document.getElementById('log-lines');
  body.addEventListener('scroll', () => {
    const atBottom = body.scrollHeight - body.scrollTop - body.clientHeight < 30;
    if (!atBottom && logState.follow) {
      logState.follow = false;
      const btn = document.getElementById('lg-follow');
      if (btn) { btn.textContent = '▶ Follow'; btn.classList.remove('active'); }
    }
  }, { passive: true });
}

// ── Incoming data ───────────────────────────────────────────────────────────

function logAppend(payload) {
  if (!logIsOpen() || !payload || payload.id !== logState.tabId) return;
  if (payload.gone) { showToast('Log file disappeared — stopped following'); closeLogView(); return; }
  if (payload.rotated) { showToast('Log rotated — following the new file'); return; }
  if (!payload.lines || !payload.lines.length) return;

  for (const l of payload.lines) logState.lines.push(l);
  if (logState.lines.length > LOG_MAX_LINES) {
    const cut = logState.lines.length - LOG_MAX_LINES + LOG_TRIM_CHUNK;
    logState.lines.splice(0, cut);
    logState.dropped += cut;
  }
  logScheduleRender();
}

// ── Open / close ────────────────────────────────────────────────────────────

async function openLogView() {
  const tab = getActiveTab();
  if (!logCanShow(tab)) { showToast('Follow mode needs a saved file'); return; }

  logState.tabId = tab.id;
  logState.filePath = tab.filePath;
  logState.lines = [];
  logState.dropped = 0;
  logState.follow = true;

  document.getElementById('monaco-editor').style.display = 'none';
  document.getElementById('log-container').classList.remove('hidden');
  logRenderShell();
  logRenderBody();

  if (!logState.unsub) {
    logState.unsub = window.electronAPI.log.onData(logAppend);
  }
  const r = await window.electronAPI.log.start(tab.id, tab.filePath);
  if (!r || r.ok === false) {
    showToast('Could not follow: ' + ((r && r.error) || 'unknown'));
    closeLogView();
    return;
  }
  updateLogButton();
}

function closeLogView() {
  const id = logState.tabId;
  logState.tabId = null;
  if (id != null) { try { window.electronAPI.log.stop(id); } catch {} }
  const c = document.getElementById('log-container');
  if (c) { c.classList.add('hidden'); c.innerHTML = ''; }
  const t = getActiveTab();
  const m = document.getElementById('monaco-editor');
  if (m && t && t.type === 'editor') m.style.display = '';
  try { if (editor) editor.layout(); } catch {}
  updateLogButton();
}

function toggleLogView() {
  if (logIsOpen()) closeLogView(); else openLogView();
}

function logOnTabChanged(tab) {
  if (logIsOpen() && (!tab || tab.id !== logState.tabId)) closeLogView();
  updateLogButton();
}

function updateLogButton() {
  const btn = document.getElementById('btn-log');
  if (!btn) return;
  const tab = getActiveTab();
  const isLog = tab && tab.filePath && /\.(log|out|err)$/i.test(tab.name || '');
  btn.classList.toggle('hidden', !isLog && !logIsOpen());
  btn.classList.toggle('active', logIsOpen());
}
