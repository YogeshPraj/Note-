'use strict';
// =============================================================================
// toolbar-context.js — show only the buttons that can actually do something
// -----------------------------------------------------------------------------
// The toolbar had grown to 34 buttons, all shown all the time. Cut/Copy/Undo/
// Find/Format on a whiteboard tab do nothing; Run File on a .txt does nothing;
// Encrypt on the Passwords tab does nothing. This hides what can't act on the
// current tab, and hides separators whose whole group went with them.
//
// The guiding rule: a visible button must be one that WORKS. Showing a button
// that then says "no runner configured for .txt" is worse than not showing it,
// because it teaches people the toolbar is decorative.
//
// Ownership note: grid / log / icons keep their own updaters (updateGridButton,
// updateLogButton and the whiteboard icons logic). They carry extra state —
// "stay visible while the view is open" — that doesn't belong here, so this
// module deliberately does not touch them.
//
// Loaded as a classic script sharing renderer.js's global scope.
// =============================================================================

// Buttons owned elsewhere. Listed so the intent is explicit rather than an
// accident of omission.
const TB_EXTERNALLY_OWNED = new Set(['grid', 'log', 'icons']);

// App-level actions — meaningful no matter what tab is in front.
const TB_ALWAYS = new Set([
  'new', 'open', 'save-all', 'close',
  'quick-open', 'terminal',
  'whiteboard-new', 'tasks', 'vault', 'source-control', 'ai', 'games',
  'dark-mode',
]);

// Need a Monaco text buffer to act on.
const TB_TEXT_ONLY = new Set([
  'print', 'cut', 'copy', 'paste', 'undo', 'redo',
  'find', 'replace', 'zoom-in', 'zoom-out', 'word-wrap',
  'format-doc', 'toggle-comment', 'spell-toggle', 'encrypt-toggle',
]);

// Tab types that own a file we can write back.
const TB_SAVEABLE_TYPES = new Set(['editor', 'whiteboard', 'drawio']);

function tbIsTextTab(tab) {
  return !!(tab && tab.type === 'editor' && tab.model);
}

// Mirrors runCurrentFile(): it saves a dirty buffer first, so an unsaved tab
// with a runnable extension still qualifies.
function tbCanRun(tab) {
  if (!tbIsTextTab(tab)) return false;
  const name = tab.filePath || tab.name || '';
  if (!name.includes('.')) return false;
  const ext = name.split('.').pop().toLowerCase();
  return typeof FILE_RUNNERS === 'object' && Object.prototype.hasOwnProperty.call(FILE_RUNNERS, ext);
}

function tbCanPreview(tab) {
  if (!tbIsTextTab(tab)) return false;
  return typeof isPreviewable === 'function' && isPreviewable(tab.language);
}

// Decide one button. Returns true when it should be visible.
function tbShouldShow(action, tab) {
  if (TB_ALWAYS.has(action)) return true;
  if (action === 'save') return !!(tab && TB_SAVEABLE_TYPES.has(tab.type));
  if (action === 'run-file') return tbCanRun(tab);
  if (action === 'preview') return tbCanPreview(tab);
  if (TB_TEXT_ONLY.has(action)) return tbIsTextTab(tab);
  // Anything not classified stays visible — a new button should appear by
  // default rather than silently vanish because nobody updated this file.
  return true;
}

// A separator earns its place only if there's a visible button on both sides
// of it; otherwise you get leading, trailing or doubled dividers.
function tbUpdateSeparators(toolbar) {
  const kids = Array.from(toolbar.children);
  const isSep = (el) => el.classList.contains('tb-separator');
  const visBtn = (el) => el.tagName === 'BUTTON' && !el.classList.contains('hidden');

  // Precompute "is there a visible button after index i", so a trailing
  // separator with nothing left to divide gets dropped.
  const anyAfter = new Array(kids.length).fill(false);
  let seen = false;
  for (let i = kids.length - 1; i >= 0; i--) { anyAfter[i] = seen; if (visBtn(kids[i])) seen = true; }

  let anyBefore = false;          // kills a leading separator
  let sinceLastShown = false;     // kills doubled separators
  for (let i = 0; i < kids.length; i++) {
    const el = kids[i];
    if (visBtn(el)) { anyBefore = true; sinceLastShown = true; continue; }
    if (!isSep(el)) continue;
    const keep = anyBefore && anyAfter[i] && sinceLastShown;
    el.classList.toggle('hidden', !keep);
    if (keep) sinceLastShown = false;
  }
}

function updateToolbarForTab(tab) {
  const toolbar = document.getElementById('toolbar');
  if (!toolbar) return;
  if (tab === undefined) tab = typeof getActiveTab === 'function' ? getActiveTab() : null;

  toolbar.querySelectorAll('button[data-action]').forEach(btn => {
    const action = btn.dataset.action;
    if (TB_EXTERNALLY_OWNED.has(action)) return;
    btn.classList.toggle('hidden', !tbShouldShow(action, tab));
  });

  tbUpdateSeparators(toolbar);
}
