'use strict';
// =============================================================================
// dock-pane.js — dock a tab beside the one you're editing
// -----------------------------------------------------------------------------
// Right-click a tab → Dock Left / Right / Top / Bottom. That tab leaves the tab
// strip and opens in a second Monaco editor alongside the main one, so you can
// read one file while working in another.
//
// Scope: ONE docked pane, whose side you choose. Not an arbitrary split grid —
// that's a different (much larger) feature, and the single reference pane is
// what "look at A while editing B" actually needs.
//
// Two integrations that are easy to miss, both handled below:
//   * Dirty tracking hangs off the MAIN editor's onDidChangeModelContent, which
//     never fires for a model shown only in the dock. We listen on the model.
//   * saveFile() saves getActiveTab(). Typing in the dock and hitting Ctrl+S
//     would otherwise save the wrong file. dockFocusedTab() redirects it.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

const DOCK_MIN_PCT = 15;
const DOCK_MAX_PCT = 85;

const dockState = {
  tabId: null,             // id of the docked tab, or null
  position: 'right',       // left | right | top | bottom
  sizePct: 40,             // dock's share of the wrapper, along the active axis
  editor: null,            // lazily-created second Monaco instance
  hasFocus: false,         // is the caret in the dock rather than the main editor?
  modelSub: null,          // model content subscription (dirty tracking)
  focusSubs: [],
};

function dockIsVertical() {
  return dockState.position === 'top' || dockState.position === 'bottom';
}

function dockedTab() {
  return dockState.tabId == null ? null : tabs.find(t => t.id === dockState.tabId) || null;
}

function isTabDocked(tabId) {
  return dockState.tabId != null && dockState.tabId === tabId;
}

// saveFile()/saveFileAs() call this instead of getActiveTab() so Ctrl+S follows
// the caret rather than always hitting the main pane.
function dockFocusedTab() {
  if (dockState.hasFocus) {
    const t = dockedTab();
    if (t) return t;
  }
  return typeof getActiveTab === 'function' ? getActiveTab() : null;
}

// ── The second editor ───────────────────────────────────────────────────────

function dockEnsureEditor() {
  if (dockState.editor) return dockState.editor;
  const host = document.getElementById('dock-monaco');
  if (!host || typeof monaco === 'undefined') return null;

  // Mirror the main editor's look, minus the chrome that doesn't earn its
  // space in a narrow reference pane.
  dockState.editor = monaco.editor.create(host, {
    value: '',
    language: 'plaintext',
    // Theme ids are built as 'notepp-<theme>' when applyTheme() registers them.
    theme: (typeof currentTheme === 'string' ? 'notepp-' + currentTheme : 'vs'),
    automaticLayout: false,          // we call layout() ourselves on resize
    minimap: { enabled: false },
    fontSize: (typeof currentFontSize === 'number' ? currentFontSize : 14),
    lineNumbers: 'on',
    scrollBeyondLastLine: false,
    renderLineHighlight: 'none',
    wordWrap: 'on',
  });

  dockState.focusSubs.push(
    dockState.editor.onDidFocusEditorText(() => { dockState.hasFocus = true; }),
    dockState.editor.onDidBlurEditorText(() => { dockState.hasFocus = false; })
  );
  return dockState.editor;
}

// ── Layout ──────────────────────────────────────────────────────────────────

function dockApplyLayout() {
  const wrap = document.getElementById('editor-dock-wrap');
  const pane = document.getElementById('dock-pane');
  const handle = document.getElementById('dock-resize-handle');
  if (!wrap || !pane || !handle) return;

  ['dock-left', 'dock-right', 'dock-top', 'dock-bottom'].forEach(c => wrap.classList.remove(c));

  if (dockState.tabId == null) {
    pane.classList.add('hidden');
    handle.classList.add('hidden');
    wrap.classList.add('dock-right');
  } else {
    wrap.classList.add('dock-' + dockState.position);
    pane.classList.remove('hidden');
    handle.classList.remove('hidden');
    handle.classList.toggle('vertical', dockIsVertical());
    // flex-basis follows the main axis, so one value serves all four sides.
    pane.style.flex = `0 0 ${dockState.sizePct}%`;
  }
  dockRelayoutEditors();
}

// Monaco doesn't reflow on its own here (automaticLayout is off on the dock and
// the main editor's container changes size without a window resize).
function dockRelayoutEditors() {
  requestAnimationFrame(() => {
    try { if (dockState.editor) dockState.editor.layout(); } catch {}
    try { if (typeof editor !== 'undefined' && editor) editor.layout(); } catch {}
  });
}

// ── Header ──────────────────────────────────────────────────────────────────

function dockRenderHeader() {
  const tab = dockedTab();
  if (!tab) return;
  const icon = document.getElementById('dock-icon');
  const title = document.getElementById('dock-title');
  const dirty = document.getElementById('dock-dirty');
  if (icon)  icon.textContent = (typeof getFileEmoji === 'function' ? getFileEmoji(tab.name) : '📄');
  if (title) { title.textContent = tab.name; title.title = tab.filePath || tab.name; }
  if (dirty) dirty.classList.toggle('hidden', !tab.dirty);
  document.querySelectorAll('#dock-header [data-dock-pos]').forEach(b =>
    b.classList.toggle('active', b.dataset.dockPos === dockState.position));
}

// ── Dock / undock ───────────────────────────────────────────────────────────

function dockTab(tabId, position) {
  const tab = tabs.find(t => t.id === tabId);
  if (!tab) return;

  // Only text tabs for now. The iframe-backed types (whiteboard, draw.io,
  // games) and the panel-backed ones (tasks, vault) render into their own
  // fixed containers inside #editor-container; showing them in a second pane
  // means relocating live iframes, which is a bigger change than it looks and
  // would reload their content.
  if (tab.type && tab.type !== 'editor') {
    if (typeof showToast === 'function') showToast(`Only text tabs can be docked — "${tab.name}" is a ${tab.type} tab`);
    return;
  }
  if (!tab.model) {
    if (typeof showToast === 'function') showToast('That tab has no content loaded yet');
    return;
  }
  // Docking the only tab would leave the main pane with nothing to show.
  const others = tabs.filter(t => t.id !== tabId && !isTabDocked(t.id));
  if (!others.length) {
    if (typeof showToast === 'function') showToast('Open another tab first — docking needs something to work alongside');
    return;
  }

  if (dockState.tabId != null && dockState.tabId !== tabId) undockTab({ silent: true });

  dockState.tabId = tabId;
  if (position) dockState.position = position;

  const ed = dockEnsureEditor();
  if (!ed) { dockState.tabId = null; return; }
  ed.setModel(tab.model);
  if (tab.dockViewState) { try { ed.restoreViewState(tab.dockViewState); } catch {} }

  // Dirty tracking: the main editor's listener won't fire for a model it isn't
  // showing, so mark the tab from the model itself.
  try { dockState.modelSub && dockState.modelSub.dispose(); } catch {}
  dockState.modelSub = tab.model.onDidChangeContent(() => {
    const t = dockedTab();
    if (t && !t.dirty) { t.dirty = true; renderTabs(); dockRenderHeader(); }
    if (typeof scheduleAutoSave === 'function') scheduleAutoSave();
  });

  // If the docked tab was the active one, move the main pane somewhere else —
  // the same buffer in both panes would be confusing and pointless.
  if (activeTabId === tabId) activateTab(others[others.length - 1].id);

  dockApplyLayout();
  dockRenderHeader();
  renderTabs();
  dockPersist();
}

function undockTab(opts) {
  const tab = dockedTab();
  if (!tab) return;
  try { if (dockState.editor) tab.dockViewState = dockState.editor.saveViewState(); } catch {}
  try { dockState.modelSub && dockState.modelSub.dispose(); } catch {}
  dockState.modelSub = null;
  // Detach the model before hiding, so the dock editor isn't left holding a
  // model that may be disposed when the tab closes.
  try { if (dockState.editor) dockState.editor.setModel(null); } catch {}
  dockState.tabId = null;
  dockState.hasFocus = false;
  dockApplyLayout();
  renderTabs();
  if (!(opts && opts.silent)) dockPersist();
}

function dockSetPosition(pos) {
  if (!['left', 'right', 'top', 'bottom'].includes(pos)) return;
  dockState.position = pos;
  dockApplyLayout();
  dockRenderHeader();
  dockPersist();
}

// Swap which buffer is in front and which is the reference.
function dockSwap() {
  const docked = dockedTab();
  const active = typeof getActiveTab === 'function' ? getActiveTab() : null;
  if (!docked || !active || active.id === docked.id) return;
  if (active.type && active.type !== 'editor') {
    if (typeof showToast === 'function') showToast('Can only swap with a text tab');
    return;
  }
  const pos = dockState.position;
  undockTab({ silent: true });
  activateTab(docked.id);
  dockTab(active.id, pos);
}

// Closing a docked tab must not leave the pane pointing at a dead model.
function dockNotifyTabClosed(tabId) {
  if (isTabDocked(tabId)) undockTab();
}

// ── Resize ──────────────────────────────────────────────────────────────────

function dockInitResize() {
  const handle = document.getElementById('dock-resize-handle');
  const wrap = document.getElementById('editor-dock-wrap');
  if (!handle || !wrap) return;

  handle.addEventListener('mousedown', (e) => {
    e.preventDefault();
    const vertical = dockIsVertical();
    // left/top put the dock before the editor visually via *-reverse, so the
    // drag direction inverts for those two.
    const inverted = dockState.position === 'left' || dockState.position === 'top';
    document.body.style.cursor = vertical ? 'row-resize' : 'col-resize';
    document.body.classList.add('dock-resizing');

    const onMove = (ev) => {
      const r = wrap.getBoundingClientRect();
      const total = vertical ? r.height : r.width;
      if (total <= 0) return;
      const from = vertical ? (ev.clientY - r.top) : (ev.clientX - r.left);
      let pct = ((total - from) / total) * 100;
      if (inverted) pct = 100 - pct;
      dockState.sizePct = Math.max(DOCK_MIN_PCT, Math.min(DOCK_MAX_PCT, pct));
      const pane = document.getElementById('dock-pane');
      if (pane) pane.style.flex = `0 0 ${dockState.sizePct}%`;
      dockRelayoutEditors();
    };
    const onUp = () => {
      document.removeEventListener('mousemove', onMove);
      document.removeEventListener('mouseup', onUp);
      document.body.style.cursor = '';
      document.body.classList.remove('dock-resizing');
      dockPersist();
    };
    document.addEventListener('mousemove', onMove);
    document.addEventListener('mouseup', onUp);
  });
}

// ── Persistence ─────────────────────────────────────────────────────────────

function dockPersist() {
  try {
    saveSetting('ui.dock', {
      position: dockState.position,
      sizePct: Math.round(dockState.sizePct),
    });
  } catch {}
}

// ── Theme / font follow-through ─────────────────────────────────────────────
// monaco.editor.setTheme() is global — it restyles every editor instance — so
// the dock follows a theme switch on its own. This hook exists only to force a
// relayout afterwards, since the gutter metrics can shift with the theme.
function dockApplyTheme() {
  if (dockState.editor) dockRelayoutEditors();
}
// Font size, by contrast, is per-editor and must be mirrored explicitly.
function dockApplyFontSize(px) {
  try { if (dockState.editor && px) dockState.editor.updateOptions({ fontSize: px }); } catch {}
}

// ── Init ────────────────────────────────────────────────────────────────────

function initDockPane() {
  dockInitResize();

  document.querySelectorAll('#dock-header [data-dock-pos]').forEach(b =>
    b.addEventListener('click', () => dockSetPosition(b.dataset.dockPos)));
  const close = document.getElementById('dock-close');
  if (close) close.addEventListener('click', () => undockTab());
  const swap = document.getElementById('dock-swap');
  if (swap) swap.addEventListener('click', () => dockSwap());

  window.addEventListener('resize', dockRelayoutEditors);

  try {
    Promise.resolve(window.electronAPI.readSettings()).then(s => {
      const d = s && s.ui && s.ui.dock;
      if (!d) return;
      if (['left', 'right', 'top', 'bottom'].includes(d.position)) dockState.position = d.position;
      const n = parseInt(d.sizePct, 10);
      if (!isNaN(n)) dockState.sizePct = Math.max(DOCK_MIN_PCT, Math.min(DOCK_MAX_PCT, n));
    }).catch(() => {});
  } catch {}
}
