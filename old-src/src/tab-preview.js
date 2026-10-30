'use strict';
// =============================================================================
// tab-preview.js — hover peek for tabs
// -----------------------------------------------------------------------------
// Hovering a tab shows a translucent card with the first screenful of that
// tab's content, so you can find the buffer you want without clicking through
// every tab and disturbing the one you're in.
//
// Design notes:
//   * Read-only and side-effect free — it never activates the tab, never
//     mutates the model, and never changes what the editor is showing.
//   * Content is pulled lazily on hover and not retained; only a small
//     colorized-HTML cache keyed by model version is kept, so repeat hovers
//     over an unchanged file don't re-tokenize.
//   * Deliberately blind to secrets — see tpBuildBody() for the tab types
//     that are summarised instead of previewed.
//
// Loaded as a classic script after renderer.js, same as the other panels.
// =============================================================================

const TAB_PREVIEW_DELAY_MS  = 420;   // long enough that skimming the bar stays quiet
const TAB_PREVIEW_LINES     = 26;
const TAB_PREVIEW_MAX_COL   = 200;   // hard-wrap guard for minified files
const TAB_PREVIEW_CACHE_MAX = 24;

let tpShowTimer = null;
let tpEl        = null;
let tpSeq       = 0;                 // guards against out-of-order async colorize
const tpCache   = new Map();         // `tabId:version` -> html

function tpEsc(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g,
    c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

function tpEnabled() {
  // Opt-out lives with the other view toggles; default on.
  return window.__tabPreviewEnabled !== false;
}

// ── Element ─────────────────────────────────────────────────────────────────

function tpEnsureEl() {
  if (tpEl) return tpEl;
  tpEl = document.createElement('div');
  tpEl.id = 'tab-preview';
  tpEl.className = 'tab-preview hidden';
  // The card must never swallow pointer events: hovering it shouldn't count
  // as leaving the tab, and it must not block a click on the bar beneath.
  tpEl.style.pointerEvents = 'none';
  document.body.appendChild(tpEl);
  return tpEl;
}

// ── Content ─────────────────────────────────────────────────────────────────

// First meaningful lines. Leading blanks are skipped so a file that opens
// with a gap doesn't preview as an empty box.
function tpExtractLines(tab) {
  if (tab.model && typeof tab.model.getLineCount === 'function') {
    const total = tab.model.getLineCount();
    let start = 1;
    while (start < total && start < 40 && !tab.model.getLineContent(start).trim()) start++;
    const end = Math.min(total, start + TAB_PREVIEW_LINES - 1);
    const lines = [];
    for (let i = start; i <= end; i++) lines.push(tab.model.getLineContent(i));
    return { lines, total, start };
  }
  if (typeof tab.content === 'string' && tab.content) {
    const all = tab.content.split(/\r?\n/);
    let start = 0;
    while (start < all.length - 1 && start < 40 && !all[start].trim()) start++;
    return { lines: all.slice(start, start + TAB_PREVIEW_LINES), total: all.length, start: start + 1 };
  }
  return null;
}

function tpCountJson(content, key) {
  try {
    const j = JSON.parse(content);
    return Array.isArray(j && j[key]) ? j[key].length : null;
  } catch { return null; }
}

function tpSummary(icon, title, detail) {
  return '<div class="tp-summary"><div class="tp-sum-icon">' + icon + '</div>' +
         '<div class="tp-sum-title">' + tpEsc(title) + '</div>' +
         '<div class="tp-sum-detail">' + tpEsc(detail || '') + '</div></div>';
}

// Returns { html } for something renderable immediately, or { code, lang, … }
// for content that should be syntax-highlighted.
function tpBuildBody(tab) {
  switch (tab.type) {
    // The Passwords tab lists credentials. Previewing it on hover would put
    // secrets on screen without anyone asking for them — precisely the
    // accident this feature could otherwise cause. Summarised, never shown.
    case 'vault':
      return { html: tpSummary('🔑', 'Passwords', 'Bitwarden vault — contents are never previewed') };
    case 'tasks':
      return { html: tpSummary('🗓', 'Tasks & Schedule', 'Task list, calendar and reminders') };
    case 'game':
      return { html: tpSummary('🎮', tab.name, 'Dev Arcade') };
    case 'whiteboard': {
      const n = tpCountJson(tab.content, 'elements');
      return { html: tpSummary('🎨', 'Whiteboard', n == null ? 'Excalidraw canvas' : n + ' element' + (n === 1 ? '' : 's')) };
    }
    case 'drawio':
      return { html: tpSummary('📐', 'Diagram', 'draw.io') };
    case 'diff':
      return { html: tpSummary('⇄', 'Compare', tab.name) };
    case 'folder-diff':
      return { html: tpSummary('⇄', 'Folder compare', tab.name) };
  }

  // A still-locked encrypted tab holds ciphertext — previewing that is noise.
  // Once unlocked it's just the file the user already has open, so the gate is
  // on the locked flag only.
  if (tab.encrypted && tab.encLocked) {
    return { html: tpSummary('🔒', 'Encrypted', 'Unlock the tab to view its contents') };
  }

  const ex = tpExtractLines(tab);
  if (!ex || !ex.lines.length || !ex.lines.some(l => l.trim())) {
    return { html: '<div class="tp-empty">Empty file</div>' };
  }
  const code = ex.lines
    .map(l => (l.length > TAB_PREVIEW_MAX_COL ? l.slice(0, TAB_PREVIEW_MAX_COL) + ' …' : l))
    .join('\n');
  return { code, lang: tab.language || 'plaintext', total: ex.total, start: ex.start, shown: ex.lines.length };
}

function tpHeader(tab) {
  const icon = tab.type === 'game' ? '🎮'
             : (typeof getFileEmoji === 'function' ? getFileEmoji(tab.name) : '📄');
  const bits = [];
  if (tab.language && tab.type === 'editor') bits.push(tab.language);
  if (tab.model && typeof tab.model.getLineCount === 'function') {
    const n = tab.model.getLineCount();
    bits.push(n + ' line' + (n === 1 ? '' : 's'));
  }
  if (tab.encoding && tab.type === 'editor') bits.push(tab.encoding);
  return '<div class="tp-head">' +
           '<span class="tp-icon">' + icon + '</span>' +
           '<span class="tp-name">' + tpEsc(tab.name) + '</span>' +
           (tab.dirty ? '<span class="tp-dot" title="Unsaved changes"></span>' : '') +
         '</div>' +
         (tab.filePath ? '<div class="tp-path">' + tpEsc(tab.filePath) + '</div>' : '') +
         (bits.length ? '<div class="tp-meta">' + tpEsc(bits.join(' · ')) + '</div>' : '');
}

// ── Position ────────────────────────────────────────────────────────────────

function tpPosition(tabEl) {
  const r  = tabEl.getBoundingClientRect();
  const pr = tpEl.getBoundingClientRect();
  const margin = 8;
  let left = r.left;
  let top  = r.bottom + 6;
  if (left + pr.width > window.innerWidth - margin) left = window.innerWidth - pr.width - margin;
  if (left < margin) left = margin;
  // Flip above the tab when there's no room below (short window, low tab bar).
  if (top + pr.height > window.innerHeight - margin) {
    const above = r.top - pr.height - 6;
    top = above > margin ? above : Math.max(margin, window.innerHeight - pr.height - margin);
  }
  tpEl.style.left = Math.round(left) + 'px';
  tpEl.style.top  = Math.round(top) + 'px';
}

// ── Show / hide ─────────────────────────────────────────────────────────────

async function tpShow(tab, tabEl) {
  const el  = tpEnsureEl();
  const seq = ++tpSeq;

  const head = tpHeader(tab);
  const body = tpBuildBody(tab);

  const paint = (inner, footer) => {
    if (seq !== tpSeq) return;               // a newer hover won the race
    el.innerHTML = head + '<div class="tp-body">' + inner + '</div>' + (footer || '');
    el.classList.remove('hidden');
    tpPosition(tabEl);
  };

  if (body.html) { paint(body.html); return; }

  const hidden = body.total - (body.start - 1 + body.shown);
  const more = hidden > 0 ? '<div class="tp-more">… ' + hidden + ' more lines</div>' : '';

  const key = (tab.model && tab.model.getVersionId)
    ? tab.id + ':' + tab.model.getVersionId() : null;
  const cached = key ? tpCache.get(key) : null;
  if (cached) { paint(cached, more); return; }

  // Plain text first so the card appears instantly, then upgrade to the
  // syntax-highlighted version once Monaco has tokenized it.
  paint('<pre class="tp-code">' + tpEsc(body.code) + '</pre>', more);

  try {
    if (typeof monaco !== 'undefined' && monaco.editor && monaco.editor.colorize) {
      const html = await monaco.editor.colorize(body.code, body.lang, { tabSize: 2 });
      const wrapped = '<pre class="tp-code">' + html + '</pre>';
      if (key) {
        tpCache.set(key, wrapped);
        if (tpCache.size > TAB_PREVIEW_CACHE_MAX) tpCache.delete(tpCache.keys().next().value);
      }
      paint(wrapped, more);
    }
  } catch { /* the plain-text card is already up; highlighting is a bonus */ }
}

function tpHide() {
  clearTimeout(tpShowTimer);
  tpShowTimer = null;
  tpSeq++;                                   // cancels any in-flight colorize
  if (tpEl) tpEl.classList.add('hidden');
}

// ── Wiring ──────────────────────────────────────────────────────────────────

// Called from renderTabs() for each tab element.
function attachTabPreview(el, tab) {
  // The tab's native `title` tooltip would pop up over the card and repeat
  // information the card already shows in full, so suppress it while the
  // preview is armed — and put it straight back when the feature is off, so
  // disabling the preview doesn't also cost you the path tooltip.
  const nameEl = el.querySelector('.tab-name');
  const stashTitle = () => {
    if (nameEl && nameEl.title) { nameEl.dataset.tpTitle = nameEl.title; nameEl.title = ''; }
  };
  const restoreTitle = () => {
    if (nameEl && nameEl.dataset.tpTitle) { nameEl.title = nameEl.dataset.tpTitle; delete nameEl.dataset.tpTitle; }
  };

  el.addEventListener('mouseenter', () => {
    if (!tpEnabled()) return;
    // Never preview the active tab: its content is already on screen, so the
    // card would cover the editor with a copy of what you're looking at. Note
    // the title tooltip is deliberately left alone in that case — with no
    // card to replace it, the full-path tooltip is still worth having.
    if (tab.id === activeTabId) return;
    stashTitle();
    clearTimeout(tpShowTimer);
    tpShowTimer = setTimeout(() => {
      // Re-read the tab — it may have been closed, renamed, re-rendered or
      // activated during the delay. A detached element has nothing sensible
      // to anchor the card to, and an activated one no longer needs a card.
      const live = tabs.find(t => t.id === tab.id);
      if (live && el.isConnected && live.id !== activeTabId) tpShow(live, el);
    }, TAB_PREVIEW_DELAY_MS);
  });
  el.addEventListener('mouseleave', () => { restoreTitle(); tpHide(); });
  // Any real interaction means the user has decided — get out of the way.
  el.addEventListener('mousedown', tpHide);
  el.addEventListener('contextmenu', tpHide);
}

function initTabPreview() {
  // Scrolling the bar slides tabs out from under the card; window changes
  // invalidate its position outright.
  try {
    const bar = document.getElementById('tab-bar');
    if (bar) bar.addEventListener('scroll', tpHide, { passive: true });
  } catch {}
  window.addEventListener('blur', tpHide);
  window.addEventListener('resize', tpHide);
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape') tpHide(); }, true);

  // Restore the opt-out. Deferred to idle: this costs an IPC round trip and a
  // settings-file read, and the value isn't needed until someone hovers a tab
  // — which cannot happen before the window is interactive. Keeping it on the
  // boot path would be pure startup cost for nothing.
  const loadPref = () => {
    try {
      Promise.resolve(window.electronAPI.readSettings()).then(s => {
        if (s && s.ui && s.ui.tabPreview === false) window.__tabPreviewEnabled = false;
      }).catch(() => {});
    } catch {}
  };
  if (typeof requestIdleCallback === 'function') requestIdleCallback(loadPref, { timeout: 3000 });
  else setTimeout(loadPref, 400);
}

function toggleTabPreview() {
  window.__tabPreviewEnabled = !tpEnabled();
  if (!window.__tabPreviewEnabled) tpHide();
  try { saveSetting('ui.tabPreview', window.__tabPreviewEnabled); } catch {}
  if (typeof showToast === 'function') {
    showToast(window.__tabPreviewEnabled ? 'Tab hover preview on' : 'Tab hover preview off');
  }
}
