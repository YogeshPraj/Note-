'use strict';

// Build a friendly Boot Performance report and show it in a modal.
// Stays self-contained — no DevTools needed. Adds a "Copy report"
// button so the user can paste numbers into a bug report.
function showBootPerfDialog() {
  if (!performance?.getEntriesByType) {
    window.electronAPI.messageDialog({
      type: 'warning',
      title: 'Boot Performance',
      message: 'Performance API unavailable on this platform.',
      buttons: ['OK'],
    });
    return;
  }
  const marks = performance.getEntriesByType('mark')
    .map(m => ({ name: m.name, ms: Math.round(m.startTime) }))
    .sort((a, b) => a.ms - b.ms);
  const idx = Object.fromEntries(marks.map(m => [m.name, m.ms]));
  const phases = [];
  const addPhase = (label, from, to) => {
    if (idx[from] != null && idx[to] != null) phases.push({ label, ms: idx[to] - idx[from] });
  };
  addPhase('HTML head → script parse',     'html-head-start',        'renderer-script-parsed');
  addPhase('Script parse → Monaco AMD',    'renderer-script-parsed', 'monaco-amd-loaded');
  addPhase('Monaco AMD → editor created',  'monaco-amd-loaded',      'monaco-editor-created');
  addPhase('Editor created → ready',       'monaco-editor-created',  'renderer-ready');
  addPhase('Total (head → ready)',         'html-head-start',        'renderer-ready');

  // Strip an existing dialog if user opened this twice in a row
  document.getElementById('boot-perf-dialog')?.remove();

  const dlg = document.createElement('div');
  dlg.id = 'boot-perf-dialog';
  dlg.className = 'modal-overlay';

  const markRows = marks.map(m =>
    `<tr><td style="font-family:monospace;font-size:11px">${escHtml(m.name)}</td><td style="text-align:right;font-family:monospace;font-size:11px">${m.ms} ms</td></tr>`
  ).join('');
  const phaseRows = phases.map((p, i) => {
    const isTotal = i === phases.length - 1;
    return `<tr style="${isTotal ? 'font-weight:600;border-top:1px solid var(--find-border)' : ''}"><td style="font-size:12px">${escHtml(p.label)}</td><td style="text-align:right;font-family:monospace;font-size:12px">${p.ms} ms</td></tr>`;
  }).join('');

  dlg.innerHTML = `
    <div class="modal-box" style="width:520px;max-height:80vh;overflow:auto">
      <div class="modal-header">
        <span>⚡ Boot Performance</span>
        <button class="modal-close" id="boot-perf-close">×</button>
      </div>
      <div class="modal-body" style="padding:14px 18px">
        <p style="font-size:12px;color:#888;margin:0 0 10px;line-height:1.45">
          Cold-start timing for this session. All numbers are milliseconds since
          the V8 isolate started (<code>performance.timeOrigin</code>).
          Lower is better — the totals row below is the at-a-glance number.
        </p>
        <div style="font-weight:600;font-size:12px;margin:14px 0 4px">Phase deltas</div>
        <table style="width:100%;border-collapse:collapse">
          ${phaseRows || '<tr><td colspan="2" style="color:#888;font-size:11px">No phase data yet</td></tr>'}
        </table>
        <div style="font-weight:600;font-size:12px;margin:18px 0 4px">Absolute marks</div>
        <table style="width:100%;border-collapse:collapse">
          ${markRows || '<tr><td colspan="2" style="color:#888;font-size:11px">No marks recorded</td></tr>'}
        </table>
      </div>
      <div class="modal-footer">
        <button class="modal-btn modal-btn-primary" id="boot-perf-copy">📋 Copy report</button>
        <button class="modal-btn" id="boot-perf-ok">Close</button>
      </div>
    </div>
  `;
  document.body.appendChild(dlg);

  const close = () => dlg.remove();
  dlg.querySelector('#boot-perf-close')?.addEventListener('click', close);
  dlg.querySelector('#boot-perf-ok')?.addEventListener('click', close);
  dlg.querySelector('#boot-perf-copy')?.addEventListener('click', async () => {
    const text =
      'Note++ Boot Performance\n' +
      '======================\n\n' +
      'Phase deltas:\n' +
      phases.map(p => `  ${p.label.padEnd(36, ' ')} ${String(p.ms).padStart(6, ' ')} ms`).join('\n') +
      '\n\nAbsolute marks:\n' +
      marks.map(m => `  ${m.name.padEnd(28, ' ')} ${String(m.ms).padStart(6, ' ')} ms`).join('\n');
    try {
      await navigator.clipboard.writeText(text);
      showToast('Boot performance report copied to clipboard');
    } catch {
      showToast('Copy failed');
    }
  });
}

// ===== Spell check =====
// Simple 2-state model matching user intent:
//   Toolbar button OFF — squiggles + right-click suggestions
//   Toolbar button ON  — squiggles + right-click + autocorrect on space
//
// Spell-check itself (squiggles) is always on for prose files. The
// toolbar button just controls whether typed misspellings are auto-
// replaced when the user hits a word boundary.
//
// Only active for natural-language languages (markdown/plaintext/log).
// In source code the dictionary would mark every identifier wrong,
// which is the opposite of useful.
const SPELL_PROSE_LANGS = new Set(['markdown', 'plaintext', 'log']);
// Toolbar-driven: when off, no spell-check work happens at all. Defaults
// to off because the en_US dictionary flags common technical terms
// ("autoscale", "kubectl", etc.) as misspelled, which is more noise
// than signal for most developer documents.
let spellEnabled = false;
// Preferences-driven: when both this AND spellEnabled are true,
// misspellings get auto-replaced on space/punctuation. Independent of
// spellEnabled — flipping it in Preferences only takes effect when
// spell-check is active on the toolbar.
let spellAutocorrect = false;
const _spellCache = new Map();      // word → bool (correct?)
const _spellSuggestCache = new Map(); // word → string[]
let _spellScanTimer = null;
let _spellDecorationIds = [];
let _spellLastTokens = null;        // last scanned [{word, start, end, range}]
let _spellCodeActionsRegistered = false;
let _spellAutoCorrectActive = false; // re-entrancy guard for the editor edit
const SPELL_SCAN_DEBOUNCE_MS = 500;

function _spellBtn() { return document.getElementById('btn-spell'); }
function _spellBadge() { return document.getElementById('spell-mode-badge'); }

function updateSpellButtonAppearance() {
  const btn = _spellBtn();
  const badge = _spellBadge();
  if (!btn) return;
  btn.classList.toggle('spell-active', spellEnabled);
  btn.classList.toggle('spell-auto', spellEnabled && spellAutocorrect);
  if (badge) badge.textContent = (spellEnabled && spellAutocorrect) ? 'A' : '';
  if (!spellEnabled) {
    btn.title = 'Spell check — OFF. Click to turn ON.';
  } else if (spellAutocorrect) {
    btn.title = 'Spell check — ON (autocorrect active, fixes typos on space). Click to turn OFF.';
  } else {
    btn.title = 'Spell check — ON (squiggles + right-click suggestions). Enable autocorrect in Preferences → Editing.';
  }
}

// Toolbar click: toggle spell check on/off. Autocorrect remains a
// separate preference (see Preferences → Editing).
function cycleSpellMode() {
  spellEnabled = !spellEnabled;
  saveSetting('ui.spellEnabled', spellEnabled);
  updateSpellButtonAppearance();
  if (spellEnabled) {
    scheduleSpellScan();
    _ensureSpellCodeActions();
  } else {
    _clearSpellDecorations();
  }
}

// Called by the Preferences "Enable autocorrect" checkbox handler.
function setSpellAutocorrect(value) {
  spellAutocorrect = !!value;
  saveSetting('ui.spellAutocorrect', spellAutocorrect);
  updateSpellButtonAppearance();
}

// Hydrate from saved settings. Old tri-state `ui.spellMode` is
// translated for back-compat. New users get both flags off by default.
function _hydrateSpellFromLegacySetting(ui) {
  if (typeof ui.spellEnabled === 'boolean') {
    spellEnabled = ui.spellEnabled;
  } else if (typeof ui.spellMode === 'string') {
    spellEnabled = ui.spellMode !== 'off';
  }
  if (typeof ui.spellAutocorrect === 'boolean') {
    spellAutocorrect = ui.spellAutocorrect;
  } else if (typeof ui.spellMode === 'string') {
    spellAutocorrect = ui.spellMode === 'auto';
  }
  updateSpellButtonAppearance();
  if (spellEnabled) {
    scheduleSpellScan();
    _ensureSpellCodeActions();
  }
}

function _clearSpellDecorations() {
  if (!editor) return;
  if (_spellDecorationIds.length) {
    _spellDecorationIds = editor.deltaDecorations(_spellDecorationIds, []);
  }
  _spellLastTokens = null;
}

function isSpellEligibleTab(tab) {
  if (!tab || tab.type !== 'editor') return false;
  return SPELL_PROSE_LANGS.has(tab.language);
}

function scheduleSpellScan() {
  clearTimeout(_spellScanTimer);
  _spellScanTimer = setTimeout(runSpellScan, SPELL_SCAN_DEBOUNCE_MS);
}

// Tokenize a text range into word tokens. We keep this simple: any run
// of Latin letters (incl. apostrophes for contractions) is a candidate.
// Acronyms/all-caps and tokens shorter than 3 chars are skipped to
// avoid false positives.
function _tokenizeForSpell(text, baseOffset = 0) {
  const out = [];
  // Match contractions (don't, I'll, you're) but not numbers / underscores.
  const re = /[A-Za-z]+(?:'[A-Za-z]+)?/g;
  let m;
  while ((m = re.exec(text)) !== null) {
    const word = m[0];
    if (word.length < 3) continue;
    // Skip all-caps acronyms / shouts — usually not misspellings worth flagging.
    if (word === word.toUpperCase() && word.length <= 5) continue;
    // Skip CamelCase / mixed-case identifiers
    if (/[A-Z]/.test(word.slice(1)) && /[a-z]/.test(word)) continue;
    out.push({
      word,
      lookup: word.toLowerCase(),
      start: baseOffset + m.index,
      end:   baseOffset + m.index + word.length,
    });
  }
  return out;
}

async function runSpellScan() {
  if (!editor || !spellEnabled) return;
  const tab = getActiveTab();
  if (!isSpellEligibleTab(tab)) { _clearSpellDecorations(); return; }
  const model = editor.getModel();
  if (!model) return;

  // Limit work to the visible range (+ a bit of padding). For huge
  // documents this keeps the scan capped at ~200 lines instead of the
  // whole file.
  const vis = editor.getVisibleRanges?.()[0];
  const startLine = vis ? Math.max(1, vis.startLineNumber - 30) : 1;
  const endLine   = vis ? Math.min(model.getLineCount(), vis.endLineNumber + 30) : model.getLineCount();
  const text = model.getValueInRange({
    startLineNumber: startLine, startColumn: 1,
    endLineNumber: endLine, endColumn: model.getLineMaxColumn(endLine),
  });
  const baseOffset = model.getOffsetAt({ lineNumber: startLine, column: 1 });
  const tokens = _tokenizeForSpell(text, baseOffset);
  if (!tokens.length) { _clearSpellDecorations(); return; }

  // Resolve correctness using cache first. Only query main for unknowns.
  const needed = [];
  for (const t of tokens) {
    if (!_spellCache.has(t.lookup)) needed.push(t.lookup);
  }
  if (needed.length) {
    try {
      const r = await window.electronAPI.spell.check(needed);
      if (r?.success && Array.isArray(r.results)) {
        needed.forEach((w, i) => _spellCache.set(w, !!r.results[i]));
      }
    } catch { /* swallow — leave cache as-is */ }
  }

  // Build inline decorations using OUR own CSS class. We don't use
  // Monaco's built-in `squiggly-warning` because that class is hard-
  // coded to depend on VS Code CSS variables our themes don't define,
  // so the squiggle would be invisible. `text-decoration: underline
  // wavy` (see .notepp-spell-error in style.css) renders cleanly in
  // every Monaco theme without relying on any external variable.
  const decos = [];
  for (const t of tokens) {
    if (_spellCache.get(t.lookup) === false) {
      const startPos = model.getPositionAt(t.start);
      const endPos   = model.getPositionAt(t.end);
      const range = new monaco.Range(startPos.lineNumber, startPos.column, endPos.lineNumber, endPos.column);
      t.range = range;
      decos.push({
        range,
        options: {
          inlineClassName: 'notepp-spell-error',
          hoverMessage: { value: 'Misspelled: **' + t.word + '** — right-click for suggestions' },
          stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
        },
      });
    }
  }
  _spellLastTokens = tokens.filter(t => _spellCache.get(t.lookup) === false);
  _spellDecorationIds = editor.deltaDecorations(_spellDecorationIds, decos);
  if (decos.length > 0) {
    console.log('[spell] flagged', decos.length, 'word(s):', _spellLastTokens.map(t => t.word).join(', '));
  }
}

// Right-click code action — surfaces "Replace with ..." entries in the
// editor context menu when the cursor is over a misspelled word.
function _ensureSpellCodeActions() {
  if (_spellCodeActionsRegistered) return;
  _spellCodeActionsRegistered = true;

  // Replace command — applies one edit using serialised numeric range
  // data. We pass primitives (numbers + string) rather than an object so
  // Monaco's command system can't lose the args during serialisation.
  monaco.editor.registerCommand?.('notepp.spell.applyReplace', (_acc, sl, sc, el, ec, text) => {
    if (!editor) return;
    try {
      const range = new monaco.Range(sl, sc, el, ec);
      editor.executeEdits('spell-replace', [{ range, text }]);
      editor.focus();
    } catch (e) { console.warn('[spell] applyReplace failed', e); }
  });

  // Add-to-dictionary command
  monaco.editor.registerCommand?.('notepp.spell.addWord', async (_acc, word) => {
    try {
      await window.electronAPI.spell.addWord(word);
      _spellCache.set(String(word).toLowerCase(), true);
      _spellSuggestCache.delete(String(word).toLowerCase());
      scheduleSpellScan();
      showToast('Added "' + word + '" to dictionary');
    } catch (e) { showToast('Add failed: ' + (e?.message || e)); }
  });

  monaco.languages.registerCodeActionProvider('*', {
    async provideCodeActions(model, range) {
      const tab = getActiveTab();
      if (!isSpellEligibleTab(tab)) return { actions: [], dispose() {} };
      // Find any misspelled token that the current cursor/click range
      // overlaps. Position-only `containsPosition` was too strict — if
      // the user selects the whole word, range.start is exactly the
      // word start and that's still fine, but a slight off-by-one from
      // Monaco's bulb positioning could miss. Use overlap instead.
      const t = (_spellLastTokens || []).find(tok => {
        if (!tok.range) return false;
        return monaco.Range.areIntersectingOrTouching
          ? monaco.Range.areIntersectingOrTouching(tok.range, range)
          : tok.range.containsPosition({ lineNumber: range.startLineNumber, column: range.startColumn });
      });
      if (!t) return { actions: [], dispose() {} };

      let suggestions = _spellSuggestCache.get(t.lookup);
      if (!suggestions) {
        try {
          const r = await window.electronAPI.spell.suggest(t.word, 6);
          if (r && r.success) suggestions = r.suggestions || [];
        } catch (e) { console.warn('[spell] suggest IPC failed', e); }
        suggestions = suggestions || [];
        _spellSuggestCache.set(t.lookup, suggestions);
      }
      console.log('[spell] code actions for "' + t.word + '" — suggestions:', suggestions);

      const actions = suggestions.slice(0, 6).map((s) => ({
        title: '✓ Replace with "' + s + '"',
        kind: 'quickfix',
        // Command form with primitive arguments — matches the working
        // "Add to dictionary" entry which also uses primitive args.
        command: {
          id: 'notepp.spell.applyReplace',
          title: 'Replace',
          arguments: [
            t.range.startLineNumber,
            t.range.startColumn,
            t.range.endLineNumber,
            t.range.endColumn,
            _matchCase(t.word, s),
          ],
        },
        isPreferred: false,
      }));
      actions.push({
        title: '📚 Add "' + t.word + '" to dictionary',
        kind: 'quickfix',
        command: { id: 'notepp.spell.addWord', title: 'Add to dictionary', arguments: [t.word] },
      });
      return { actions, dispose() {} };
    },
  });
}

function _matchCase(original, replacement) {
  if (!original || !replacement) return replacement;
  if (original === original.toUpperCase()) return replacement.toUpperCase();
  if (original[0] === original[0].toUpperCase()) return replacement[0].toUpperCase() + replacement.slice(1);
  return replacement;
}

// Auto-correct: when in 'auto' mode and the user just typed a delimiter,
// look at the word they just finished and replace it with the top
// suggestion *if* nspell reports it as misspelled AND offers any options.
// Conservative — we only act on single-word boundaries to keep surprises
// minimal.
async function _maybeAutoCorrect(e) {
  if (!spellEnabled || !spellAutocorrect || !editor || _spellAutoCorrectActive) return;
  if (!e || !e.changes || e.changes.length !== 1) return;
  const ch = e.changes[0];
  // Only trigger on insertion of a single delimiter (space, newline, punctuation)
  if (!ch.text || ch.text.length !== 1) return;
  if (!/[\s.,;:!?]/.test(ch.text)) return;
  const tab = getActiveTab();
  if (!isSpellEligibleTab(tab)) return;
  const model = editor.getModel();
  if (!model) return;
  const pos = ch.range.getStartPosition();
  const line = model.getLineContent(pos.lineNumber);
  const upTo = line.slice(0, pos.column - 1);
  const m = /([A-Za-z]+(?:'[A-Za-z]+)?)\s*$/.exec(upTo);
  if (!m) return;
  const word = m[1];
  if (word.length < 3) return;
  const lookup = word.toLowerCase();
  let isCorrect = _spellCache.get(lookup);
  if (isCorrect === undefined) {
    try {
      const r = await window.electronAPI.spell.check([lookup]);
      if (r?.success) { isCorrect = !!r.results[0]; _spellCache.set(lookup, isCorrect); }
    } catch { return; }
  }
  if (isCorrect !== false) return; // word is fine OR check failed → bail
  let suggestions = _spellSuggestCache.get(lookup);
  if (!suggestions) {
    try {
      const r = await window.electronAPI.spell.suggest(word, 3);
      if (r?.success) suggestions = r.suggestions || [];
    } catch {}
    suggestions = suggestions || [];
    _spellSuggestCache.set(lookup, suggestions);
  }
  if (!suggestions.length) return;
  const top = _matchCase(word, suggestions[0]);
  // Edit the previous word — placement: line `pos.lineNumber`, columns
  // from (pos.column - 1 - word.length) to (pos.column - 1).
  const startCol = pos.column - word.length;
  const endCol   = pos.column;
  _spellAutoCorrectActive = true;
  try {
    editor.executeEdits('spell-autocorrect', [{
      range: new monaco.Range(pos.lineNumber, startCol, pos.lineNumber, endCol),
      text: top,
    }]);
  } finally {
    _spellAutoCorrectActive = false;
  }
}

// ===== Boot performance instrumentation =====
// performance.mark() is essentially free at runtime; we use it to track
// the critical-path stages of cold start so any future optimisation
// can be measured rather than guessed at. Call `notepp_perf()` from
// DevTools console to dump a sorted timing table. Stages we mark:
//   • renderer-script-parsed   — top of renderer.js parsed
//   • monaco-amd-loaded        — require(['vs/editor/editor.main']) cb
//   • monaco-editor-created    — monaco.editor.create() returned
//   • renderer-ready           — main was signalled we're done wiring
performance.mark?.('renderer-script-parsed');
window.notepp_perf = function () {
  if (!performance.getEntriesByType) {
    console.log('Performance API unavailable');
    return;
  }
  const marks = performance.getEntriesByType('mark')
    .map(m => ({ name: m.name, ms: Math.round(m.startTime) }))
    .sort((a, b) => a.ms - b.ms);
  console.table(marks);
  // Also dump phase deltas for at-a-glance reading.
  const idx = Object.fromEntries(marks.map(m => [m.name, m.ms]));
  const phases = [];
  const add = (label, from, to) => {
    if (idx[from] != null && idx[to] != null) phases.push({ phase: label, ms: idx[to] - idx[from] });
  };
  add('script parse → Monaco AMD loaded', 'renderer-script-parsed', 'monaco-amd-loaded');
  add('Monaco AMD → editor created',     'monaco-amd-loaded',      'monaco-editor-created');
  add('editor created → renderer-ready', 'monaco-editor-created',  'renderer-ready');
  add('TOTAL (script parsed → ready)',   'renderer-script-parsed', 'renderer-ready');
  console.table(phases);
};

// ===== State =====
const tabs = [];
let activeTabId = null;
let editor = null;
let isDarkMode = false;
let isWordWrap = false;
let isColumnSelectMode = false;
let currentTheme = 'light';   // RESOLVED theme actually applied (a THEMES key)
let themePref = 'light';      // user PREFERENCE: 'light' | 'dark' | 'system'
let isThemedTitlebar = false;  // custom HTML title bar + menu vs native OS chrome
let tabCounter = 0;
let searchDecorations = [];
let findReplaceMode = 'find';

// Auto-save
let autoSaveTimer = null;
const AUTO_SAVE_DELAY = 1500; // ms after last keystroke

// New-document defaults (loaded from settings)
let newDocDefaults = { encoding: 'UTF-8', eol: 'Windows (CR LF)', language: 'plaintext', template: '' };

// Auto-backup
let autoBackupTimer = null;

// Terminal state
let term = null;
let fitAddon = null;
let terminalId = 1;
let terminalOpen = false;
let termLineBuffer = '';

// Preview state
let previewOpen = false;
let previewTimer = null;
const PREVIEW_DELAY = 400;

// Game state
let gameTabId = null;

// Whiteboard state
let wbReady = false;
let wbPendingContent = null;
const wbFileSaveTimers = new Map(); // tabId → timer handle

// File tree state
let fileTreeRootPath = null;
// Whether the user explicitly wants the file tree visible. We track
// this separately from the DOM `hidden` class so we can auto-hide the
// panel when the active tab's file is outside the current tree root
// (no point showing a folder tree for an unrelated workspace), then
// restore it automatically when the user switches back to a file
// that IS under the root.
let fileTreeUserShown = false;

// Zen mode
let isZenMode = false;

// Macro recording & playback
let isRecording    = false;
let currentMacroOps = [];       // ops accumulated during active recording
let lastRecordedMacro = null;   // most recently completed macro (for "Run Last")
let savedMacros = [];           // persisted named macros

// Disk auto-save — periodically writes dirty editor files to their real paths
let diskAutoSaveTimer = null;

// Large-file safe mode threshold (10 MB)
const LARGE_FILE_THRESHOLD = 10 * 1024 * 1024;

// ===== Encryption state =====
// `appEnc.profile` is the loaded profile JSON (or null if encryption not yet set up).
// `appEnc.rawDek` is the unlocked DEK as Uint8Array (or null when locked).
// Helpers: `isEncConfigured()`, `isEncUnlocked()`.
const appEnc = {
  profile: null,
  rawDek: null,
};
function isEncConfigured() { return appEnc.profile != null; }
function isEncUnlocked()   { return appEnc.profile != null && appEnc.rawDek != null; }

// ===== DOM =====
const tabBar       = document.getElementById('tab-bar');
const findPanel    = document.getElementById('find-replace-panel');
const statusLnCol  = document.getElementById('status-ln-col');
const statusLines  = document.getElementById('status-lines');
const statusLength = document.getElementById('status-length');
const statusLang   = document.getElementById('status-lang');
const statusInsert = document.getElementById('status-insert');
const statusCol    = document.getElementById('status-col');
const contextMenu  = document.getElementById('context-menu');

// ===== Theme Definitions =====
// Each entry: { label, isDark, preview:{tabbar,editor,statusbar,text},
//               monacoBase, monacoColors, monacoRules }
const THEMES = {
  light: {
    label: 'Light', isDark: false,
    preview: { tabbar: '#f0f0f0', editor: '#ffffff', statusbar: '#007acc', text: '#333' },
    monacoBase: 'vs', monacoRules: [],
    monacoColors: { 'editor.background': '#FFFFFE', 'editor.lineHighlightBackground': '#F0F7FF' },
  },
  dark: {
    label: 'Dark', isDark: true,
    preview: { tabbar: '#2d2d2d', editor: '#1e1e1e', statusbar: '#007acc', text: '#d4d4d4' },
    monacoBase: 'vs-dark',
    monacoRules: [
      { token: 'comment',  foreground: '6A9955', fontStyle: 'italic' },
      { token: 'keyword',  foreground: '569CD6', fontStyle: 'bold'   },
      { token: 'string',   foreground: 'CE9178' },
      { token: 'number',   foreground: 'B5CEA8' },
      { token: 'type',     foreground: '4EC9B0' },
      { token: 'function', foreground: 'DCDCAA' },
      { token: 'variable', foreground: '9CDCFE' },
    ],
    monacoColors: {
      'editor.background': '#1E1E1E', 'editor.lineHighlightBackground': '#2A2D2E',
      'editorLineNumber.foreground': '#858585', 'editorLineNumber.activeForeground': '#C6C6C6',
      'editor.selectionBackground': '#264F78',
      'editor.findMatchBackground': '#515C6A', 'editor.findMatchHighlightBackground': '#EA5C0055',
    },
  },
  flower: {
    label: 'Flower 🌸', isDark: false,
    preview: { tabbar: '#f9c6d4', editor: '#fff0f5', statusbar: '#c2185b', text: '#5d1a32' },
    monacoBase: 'vs',
    monacoRules: [
      { token: 'comment',  foreground: 'ad7fa8', fontStyle: 'italic' },
      { token: 'keyword',  foreground: 'c2185b', fontStyle: 'bold'   },
      { token: 'string',   foreground: '7b5ea7' },
      { token: 'number',   foreground: 'e91e63' },
      { token: 'type',     foreground: '9c27b0' },
      { token: 'function', foreground: 'd81b60' },
      { token: 'variable', foreground: '5d1a32' },
    ],
    monacoColors: {
      'editor.background': '#fff0f5', 'editor.lineHighlightBackground': '#ffe4ef',
      'editorLineNumber.foreground': '#d48099', 'editorLineNumber.activeForeground': '#c2185b',
      'editor.selectionBackground': '#f8bbd0',
      'editor.findMatchBackground': '#f48fb1', 'editor.findMatchHighlightBackground': '#f8bbd055',
    },
  },
  dracula: {
    label: 'Dracula', isDark: true,
    preview: { tabbar: '#383a59', editor: '#282a36', statusbar: '#bd93f9', text: '#f8f8f2' },
    monacoBase: 'vs-dark',
    monacoRules: [
      { token: 'comment',  foreground: '6272a4', fontStyle: 'italic' },
      { token: 'keyword',  foreground: 'ff79c6', fontStyle: 'bold'   },
      { token: 'string',   foreground: 'f1fa8c' },
      { token: 'number',   foreground: 'bd93f9' },
      { token: 'type',     foreground: '8be9fd' },
      { token: 'function', foreground: '50fa7b' },
      { token: 'variable', foreground: 'f8f8f2' },
    ],
    monacoColors: {
      'editor.background': '#282a36', 'editor.lineHighlightBackground': '#44475a',
      'editorLineNumber.foreground': '#6272a4', 'editorLineNumber.activeForeground': '#f8f8f2',
      'editor.selectionBackground': '#44475a',
      'editor.findMatchBackground': '#ffb86c55', 'editor.findMatchHighlightBackground': '#ffb86c33',
    },
  },
  tokyonight: {
    label: 'Tokyo Night', isDark: true,
    preview: { tabbar: '#24283b', editor: '#1a1b26', statusbar: '#7aa2f7', text: '#c0caf5' },
    monacoBase: 'vs-dark',
    monacoRules: [
      { token: 'comment',  foreground: '565f89', fontStyle: 'italic' },
      { token: 'keyword',  foreground: 'bb9af7', fontStyle: 'bold'   },
      { token: 'string',   foreground: '9ece6a' },
      { token: 'number',   foreground: 'ff9e64' },
      { token: 'type',     foreground: '2ac3de' },
      { token: 'function', foreground: '7aa2f7' },
      { token: 'variable', foreground: 'c0caf5' },
    ],
    monacoColors: {
      'editor.background': '#1a1b26', 'editor.lineHighlightBackground': '#1e2030',
      'editorLineNumber.foreground': '#3b4261', 'editorLineNumber.activeForeground': '#737aa2',
      'editor.selectionBackground': '#283457',
      'editor.findMatchBackground': '#3d5270', 'editor.findMatchHighlightBackground': '#3d527055',
    },
  },
  nord: {
    label: 'Nord', isDark: true,
    preview: { tabbar: '#3b4252', editor: '#2e3440', statusbar: '#5e81ac', text: '#d8dee9' },
    monacoBase: 'vs-dark',
    monacoRules: [
      { token: 'comment',  foreground: '616e88', fontStyle: 'italic' },
      { token: 'keyword',  foreground: '81a1c1', fontStyle: 'bold'   },
      { token: 'string',   foreground: 'a3be8c' },
      { token: 'number',   foreground: 'b48ead' },
      { token: 'type',     foreground: '8fbcbb' },
      { token: 'function', foreground: '88c0d0' },
      { token: 'variable', foreground: 'd8dee9' },
    ],
    monacoColors: {
      'editor.background': '#2e3440', 'editor.lineHighlightBackground': '#3b4252',
      'editorLineNumber.foreground': '#4c566a', 'editorLineNumber.activeForeground': '#d8dee9',
      'editor.selectionBackground': '#434c5e',
      'editor.findMatchBackground': '#5e81ac55', 'editor.findMatchHighlightBackground': '#5e81ac33',
    },
  },
  monokai: {
    label: 'Monokai', isDark: true,
    preview: { tabbar: '#3e3d32', editor: '#272822', statusbar: '#a6e22e', text: '#f8f8f2' },
    monacoBase: 'vs-dark',
    monacoRules: [
      { token: 'comment',  foreground: '75715e', fontStyle: 'italic' },
      { token: 'keyword',  foreground: 'f92672', fontStyle: 'bold'   },
      { token: 'string',   foreground: 'e6db74' },
      { token: 'number',   foreground: 'ae81ff' },
      { token: 'type',     foreground: '66d9e8' },
      { token: 'function', foreground: 'a6e22e' },
      { token: 'variable', foreground: 'f8f8f2' },
    ],
    monacoColors: {
      'editor.background': '#272822', 'editor.lineHighlightBackground': '#3e3d32',
      'editorLineNumber.foreground': '#90908a', 'editorLineNumber.activeForeground': '#f8f8f2',
      'editor.selectionBackground': '#49483e',
      'editor.findMatchBackground': '#ffe79233', 'editor.findMatchHighlightBackground': '#ffe79222',
    },
  },
  solarizedLight: {
    label: 'Solarized Light', isDark: false,
    preview: { tabbar: '#eee8d5', editor: '#fdf6e3', statusbar: '#268bd2', text: '#657b83' },
    monacoBase: 'vs',
    monacoRules: [
      { token: 'comment',  foreground: '93a1a1', fontStyle: 'italic' },
      { token: 'keyword',  foreground: '859900', fontStyle: 'bold'   },
      { token: 'string',   foreground: '2aa198' },
      { token: 'number',   foreground: 'd33682' },
      { token: 'type',     foreground: '268bd2' },
      { token: 'function', foreground: '6c71c4' },
      { token: 'variable', foreground: '657b83' },
    ],
    monacoColors: {
      'editor.background': '#fdf6e3', 'editor.lineHighlightBackground': '#eee8d5',
      'editorLineNumber.foreground': '#93a1a1', 'editorLineNumber.activeForeground': '#657b83',
      'editor.selectionBackground': '#eee8d5',
      'editor.findMatchBackground': '#cb4b1633', 'editor.findMatchHighlightBackground': '#cb4b1622',
    },
  },
};

// ===== Custom Menu Bar — Structure & Dispatch =====
// _menuActions is populated by the shim inside setupMenuListeners (m() helper).
// setupCustomMenuBar() reads MENU_STRUCTURE to build the HTML menu bar.
const _menuActions = {};
function _dispatchMenu(ch, ...args) { if (_menuActions[ch]) _menuActions[ch](...args); }

const MENU_STRUCTURE = [
  { label: 'File', items: [
    { label: 'New',                ch: 'menu-new',           key: 'Ctrl+N' },
    { sep: true },
    { label: 'Open…',             ch: 'menu-open',          key: 'Ctrl+O' },
    { label: 'Open Folder…',      ch: 'menu-open-folder',   key: 'Ctrl+Shift+O' },
    { label: 'Open Recent',        sub: [
      { label: '(see File menu → Open Recent)', disabled: true },
    ]},
    { sep: true },
    { label: 'Compare', sub: [
      { label: 'Compare Files…',      ch: 'menu-compare-files' },
      { label: 'Compare with Saved…', ch: 'menu-compare-with-saved' },
      { label: 'Compare with Clipboard', ch: 'menu-compare-clipboard' },
      { sep: true },
      { label: 'Compare Folders…',    ch: 'menu-compare-folders' },
    ]},
    { label: 'Reload from Disk',   ch: 'menu-reload',        key: 'Ctrl+Shift+R' },
    { sep: true },
    { label: 'Close',              ch: 'menu-close',         key: 'Ctrl+W' },
    { label: 'Close All',          ch: 'menu-close-all' },
    { label: 'Close All But Current', ch: 'menu-close-others' },
    { sep: true },
    { label: 'Save',               ch: 'menu-save',          key: 'Ctrl+S' },
    { label: 'Save As…',          ch: 'menu-save-as',       key: 'Ctrl+Alt+S' },
    { label: 'Save All',           ch: 'menu-save-all',      key: 'Ctrl+Shift+S' },
    { sep: true },
    { label: 'Print…',            ch: 'menu-print',         key: 'Ctrl+P' },
    { sep: true },
    { label: 'Exit',               fn: () => window.electronAPI.closeWindow(), key: 'Alt+F4' },
  ]},
  { label: 'Edit', items: [
    { label: 'Undo',               ch: 'menu-undo',          key: 'Ctrl+Z' },
    { label: 'Redo',               ch: 'menu-redo',          key: 'Ctrl+Y' },
    { sep: true },
    { label: 'Cut',                fn: () => document.execCommand('cut'),   key: 'Ctrl+X' },
    { label: 'Copy',               fn: () => document.execCommand('copy'),  key: 'Ctrl+C' },
    { label: 'Paste',              fn: () => document.execCommand('paste'), key: 'Ctrl+V' },
    { label: 'Select All',         fn: () => editor?.getAction('editor.action.selectAll')?.run(), key: 'Ctrl+A' },
    { sep: true },
    { label: 'Duplicate Line',     ch: 'menu-duplicate-line', key: 'Ctrl+D' },
    { label: 'Delete Line',        ch: 'menu-delete-line',   key: 'Ctrl+Shift+K' },
    { label: 'Move Line Up',       ch: 'menu-move-line-up',  key: 'Alt+↑' },
    { label: 'Move Line Down',     ch: 'menu-move-line-down',key: 'Alt+↓' },
    { sep: true },
    { label: 'Convert Case', sub: [
      { label: 'UPPERCASE',        ch: 'menu-uppercase',     key: 'Ctrl+Shift+U' },
      { label: 'lowercase',        ch: 'menu-lowercase',     key: 'Ctrl+U' },
      { label: 'Title Case',       ch: 'menu-titlecase' },
    ]},
    { label: 'Line Operations', sub: [
      { label: 'Sort Ascending',    ch: 'menu-sort-asc' },
      { label: 'Sort Descending',   ch: 'menu-sort-desc' },
      { label: 'Remove Duplicates', ch: 'menu-remove-dup-lines' },
      { label: 'Remove Empty Lines',ch: 'menu-remove-empty-lines' },
      { label: 'Join Lines',        ch: 'menu-join-lines' },
    ]},
    { sep: true },
    { label: 'Set Read-Only',      ch: 'menu-readonly' },
    { label: 'Clear Read-Only',    ch: 'menu-clear-readonly' },
  ]},
  { label: 'Code', items: [
    { label: 'Format Document',    ch: 'menu-format-doc',    key: 'Ctrl+Shift+F' },
    { label: 'Format Selection',   ch: 'menu-format-sel' },
    { sep: true },
    { label: 'Toggle Line Comment',ch: 'menu-toggle-comment',key: 'Ctrl+/' },
    { label: 'Toggle Block Comment',ch:'menu-block-comment', key: 'Shift+Alt+A' },
    { sep: true },
    { label: 'Indent',             ch: 'menu-indent-increase',key:'Tab' },
    { label: 'Outdent',            ch: 'menu-indent-decrease',key:'Shift+Tab' },
    { sep: true },
    { label: 'Fold All',           ch: 'menu-fold-all' },
    { label: 'Unfold All',         ch: 'menu-unfold-all' },
    { sep: true },
    { label: 'Go to Definition',   ch: 'menu-goto-definition',key:'F12' },
    { label: 'Go to Symbol…',     ch: 'menu-goto-symbol' },
    { label: 'Go to References',   ch: 'menu-goto-refs',     key:'Shift+F12' },
    { label: 'Rename Symbol',      ch: 'menu-rename-symbol', key:'F2' },
    { sep: true },
    { label: 'Pretty Print JSON',  ch: 'menu-json-format' },
    { label: 'Pretty Print XML',   ch: 'menu-xml-format' },
    { label: 'Minify JSON',        ch: 'menu-json-minify' },
    { sep: true },
    { label: 'Diagram (draw.io)', sub: [
      { label: 'New Diagram',          ch: 'menu-new-drawio' },
      { sep: true },
      { label: 'Flowchart',            fn: () => _dispatchMenu('menu-new-drawio-template','flowchart') },
      { label: 'Sequence Diagram',     fn: () => _dispatchMenu('menu-new-drawio-template','sequence') },
      { label: 'Class Diagram (UML)',  fn: () => _dispatchMenu('menu-new-drawio-template','classDiagram') },
      { label: 'Entity Relationship',  fn: () => _dispatchMenu('menu-new-drawio-template','erDiagram') },
      { sep: true },
      { label: 'Check for Updates',    ch: 'menu-drawio-check-updates' },
    ]},
  ]},
  { label: 'Search', items: [
    { label: 'Find…',             ch: 'menu-find',          key: 'Ctrl+F' },
    { label: 'Find Next',          ch: 'menu-find-next',     key: 'F3' },
    { label: 'Find Previous',      ch: 'menu-find-prev',     key: 'Shift+F3' },
    { label: 'Find All in Document',ch:'menu-find-all' },
    { sep: true },
    { label: 'Replace…',          ch: 'menu-replace',       key: 'Ctrl+H' },
    { sep: true },
    { label: 'Quick Open',         ch: 'menu-quick-open',    key: 'Ctrl+P' },
    { label: 'Command Palette',    ch: 'menu-cmd-palette',   key: 'Ctrl+Shift+P' },
    { sep: true },
    { label: 'Go to Line…',       ch: 'menu-goto-line',     key: 'Ctrl+G' },
    { sep: true },
    { label: 'Toggle Bookmark',    ch: 'menu-toggle-bookmark',key:'Ctrl+F2' },
    { label: 'Next Bookmark',      ch: 'menu-next-bookmark', key: 'F2' },
    { label: 'Previous Bookmark',  ch: 'menu-prev-bookmark', key: 'Shift+F2' },
    { sep: true },
    { label: 'Regex Tester…',     ch: 'menu-regex-tester' },
  ]},
  { label: 'View', items: [
    { label: 'Toggle Preview',     ch: 'menu-toggle-preview',key: 'Ctrl+Shift+V' },
    { label: 'Toggle Terminal',    ch: 'menu-toggle-terminal',key:'Ctrl+`' },
    { label: 'Toggle Sidebar',     ch: 'menu-toggle-sidebar' },
    { sep: true },
    { label: 'Zen Mode',           ch: 'menu-zen-mode', key: 'F11' },
    { sep: true },
    { label: 'Zoom In',            ch: 'menu-zoom-in',       key: 'Ctrl+=' },
    { label: 'Zoom Out',           ch: 'menu-zoom-out',      key: 'Ctrl+-' },
    { label: 'Reset Zoom',         ch: 'menu-zoom-reset',    key: 'Ctrl+0' },
    { sep: true },
    { label: 'Toggle Word Wrap',   ch: 'menu-word-wrap',     key: 'Alt+Z' },
    { label: 'Toggle Minimap',     ch: 'menu-minimap' },
    { label: 'Toggle Whitespace',  ch: 'menu-show-whitespace' },
    { label: 'Toggle Indent Guides',ch:'menu-show-indent' },
    { sep: true },
    { label: 'Toggle Toolbar',     ch: 'menu-toolbar' },
    { label: 'Toggle Status Bar',  ch: 'menu-statusbar' },
    { label: 'Toggle Tab Bar',     ch: 'menu-tabbar' },
  ]},
  { label: 'Language', items: [
    { label: 'Plain Text',   fn: () => _dispatchMenu('menu-lang','plaintext') },
    { sep: true },
    { label: 'Batch',        fn: () => _dispatchMenu('menu-lang','bat') },
    { label: 'C',            fn: () => _dispatchMenu('menu-lang','c') },
    { label: 'C++',          fn: () => _dispatchMenu('menu-lang','cpp') },
    { label: 'C#',           fn: () => _dispatchMenu('menu-lang','csharp') },
    { label: 'CSS',          fn: () => _dispatchMenu('menu-lang','css') },
    { label: 'Dockerfile',   fn: () => _dispatchMenu('menu-lang','dockerfile') },
    { label: 'Go',           fn: () => _dispatchMenu('menu-lang','go') },
    { label: 'HTML',         fn: () => _dispatchMenu('menu-lang','html') },
    { label: 'Java',         fn: () => _dispatchMenu('menu-lang','java') },
    { label: 'JavaScript',   fn: () => _dispatchMenu('menu-lang','javascript') },
    { label: 'JSON',         fn: () => _dispatchMenu('menu-lang','json') },
    { label: 'Kotlin',       fn: () => _dispatchMenu('menu-lang','kotlin') },
    { label: 'Lua',          fn: () => _dispatchMenu('menu-lang','lua') },
    { label: 'Markdown',     fn: () => _dispatchMenu('menu-lang','markdown') },
    { label: 'Mermaid',      fn: () => _dispatchMenu('menu-lang','mermaid') },
    { label: 'PHP',          fn: () => _dispatchMenu('menu-lang','php') },
    { label: 'PowerShell',   fn: () => _dispatchMenu('menu-lang','powershell') },
    { label: 'Python',       fn: () => _dispatchMenu('menu-lang','python') },
    { label: 'R',            fn: () => _dispatchMenu('menu-lang','r') },
    { label: 'Ruby',         fn: () => _dispatchMenu('menu-lang','ruby') },
    { label: 'Rust',         fn: () => _dispatchMenu('menu-lang','rust') },
    { label: 'SCSS',         fn: () => _dispatchMenu('menu-lang','scss') },
    { label: 'Shell Script', fn: () => _dispatchMenu('menu-lang','shell') },
    { label: 'SQL',          fn: () => _dispatchMenu('menu-lang','sql') },
    { label: 'Swift',        fn: () => _dispatchMenu('menu-lang','swift') },
    { label: 'TOML',         fn: () => _dispatchMenu('menu-lang','ini') },
    { label: 'TypeScript',   fn: () => _dispatchMenu('menu-lang','typescript') },
    { label: 'XML',          fn: () => _dispatchMenu('menu-lang','xml') },
    { label: 'YAML',         fn: () => _dispatchMenu('menu-lang','yaml') },
  ]},
  { label: 'Terminal', items: [
    { label: 'New Terminal',          ch: 'menu-new-terminal',   key: 'Ctrl+Shift+`' },
    { label: 'Toggle Terminal',       ch: 'menu-toggle-terminal',key: 'Ctrl+`' },
    { label: 'Kill Terminal',         ch: 'menu-kill-terminal' },
    { label: 'Clear Terminal',        ch: 'menu-clear-terminal' },
    { sep: true },
    { label: 'Run File',              ch: 'menu-run-file',        key: 'F5' },
    { label: 'Run Selection',         ch: 'menu-run-selection',   key: 'Shift+F5' },
    { sep: true },
    { label: 'Open Containing Folder',ch: 'menu-open-explorer' },
    { label: 'Copy File Path',        ch: 'menu-copy-path' },
  ]},
  { label: 'Settings', items: [
    { label: 'Preferences…',      ch: 'menu-preferences',   key: 'Ctrl+,' },
    { sep: true },
    { label: 'Toggle Dark Mode',  ch: 'menu-dark-mode',     key: 'Ctrl+Alt+D' },
    { sep: true },
    { label: 'Font Size +',       ch: 'menu-zoom-in',       key: 'Ctrl+=' },
    { label: 'Font Size -',       ch: 'menu-zoom-out',      key: 'Ctrl+-' },
  ]},
  { label: 'Tools', items: [
    { label: 'Regex Tester',      ch: 'menu-regex-tester' },
    { sep: true },
    { label: 'MD5 from Selection',    ch: 'menu-md5-selection' },
    { label: 'SHA-256 from Selection',ch: 'menu-sha256-selection' },
    { sep: true },
    { label: 'Base64 Encode',     ch: 'menu-b64-encode' },
    { label: 'Base64 Decode',     ch: 'menu-b64-decode' },
    { sep: true },
    { label: 'Pretty Print JSON', ch: 'menu-json-format' },
    { label: 'Pretty Print XML',  ch: 'menu-xml-format' },
    { label: 'Minify JSON',       ch: 'menu-json-minify' },
    { sep: true },
    { label: 'Diagram (draw.io)', sub: [
      { label: 'New Diagram',         ch: 'menu-new-drawio' },
      { sep: true },
      { label: 'Flowchart',           fn: () => _dispatchMenu('menu-new-drawio-template','flowchart') },
      { label: 'Sequence Diagram',    fn: () => _dispatchMenu('menu-new-drawio-template','sequence') },
      { label: 'Class Diagram (UML)', fn: () => _dispatchMenu('menu-new-drawio-template','classDiagram') },
      { label: 'Entity Relationship', fn: () => _dispatchMenu('menu-new-drawio-template','erDiagram') },
      { sep: true },
      { label: 'Check for Updates',   ch: 'menu-drawio-check-updates' },
    ]},
  ]},
  { label: 'Macros', items: [
    { label: 'Start / Stop Recording', ch: 'menu-macro-record', key: 'Ctrl+Shift+R' },
    { sep: true },
    { label: 'Run Last Macro',   ch: 'menu-macro-run',   key: 'F9' },
    { label: 'Run N Times…',    ch: 'menu-macro-run-n', key: 'Ctrl+F9' },
    { sep: true },
    { label: 'Manage Macros…',  ch: 'menu-macro-manage' },
  ]},
  { label: 'Window', items: [
    { label: 'Previous Document', ch: 'menu-prev-tab',      key: 'Ctrl+PgUp' },
    { label: 'Next Document',     ch: 'menu-next-tab',      key: 'Ctrl+PgDn' },
    { sep: true },
    { label: 'Tab 1',  fn: () => _dispatchMenu('menu-tab',0), key: 'Ctrl+1' },
    { label: 'Tab 2',  fn: () => _dispatchMenu('menu-tab',1), key: 'Ctrl+2' },
    { label: 'Tab 3',  fn: () => _dispatchMenu('menu-tab',2), key: 'Ctrl+3' },
    { label: 'Tab 4',  fn: () => _dispatchMenu('menu-tab',3), key: 'Ctrl+4' },
    { label: 'Tab 5',  fn: () => _dispatchMenu('menu-tab',4), key: 'Ctrl+5' },
  ]},
  { label: '?', items: [
    { label: 'About Note++',              ch: 'menu-about' },
    { sep: true },
    { label: 'Show Feature Tour',         ch: 'menu-feature-tour' },
    { label: 'Show Boot Performance',     ch: 'menu-boot-perf' },
    { sep: true },
    { label: 'Keyboard Shortcuts Reference', ch: 'menu-shortcuts-ref' },
    { sep: true },
    { label: 'Check for Updates Now',    ch: 'menu-check-updates' },
  ]},
];

// ── Custom menu bar rendering & interaction ────────────────────────────────
let _cmbOpenEl    = null;   // highlighted top-level .cmb-item
let _cmbDropdown  = null;   // current open dropdown element
let _cmbSubmenu   = null;   // current open submenu element
let _cmbSubTimer  = null;   // debounce timer for submenu open/close

function _cmbClose() {
  clearTimeout(_cmbSubTimer);
  if (_cmbSubmenu) { _cmbSubmenu.remove(); _cmbSubmenu = null; }
  if (_cmbDropdown){ _cmbDropdown.remove(); _cmbDropdown = null; }
  if (_cmbOpenEl)  { _cmbOpenEl.classList.remove('cmb-active'); _cmbOpenEl = null; }
}

function _cmbBuildPanel(items, cls) {
  const panel = document.createElement('div');
  panel.className = cls;
  items.forEach(it => {
    if (it.sep) {
      const s = document.createElement('div'); s.className = 'cmb-dd-sep'; panel.appendChild(s); return;
    }
    const row = document.createElement('div');
    row.className = 'cmb-dd-item' + (it.disabled ? ' cmb-disabled' : '') + (it.sub ? ' cmb-has-sub' : '');
    const lbl = document.createElement('span'); lbl.className = 'cmb-dd-label'; lbl.textContent = it.label; row.appendChild(lbl);
    if (it.key) { const k = document.createElement('span'); k.className = 'cmb-dd-key'; k.textContent = it.key; row.appendChild(k); }
    if (it.sub) { const a = document.createElement('span'); a.className = 'cmb-dd-arrow'; a.textContent = '▶'; row.appendChild(a); }

    if (it.sub) {
      row.addEventListener('mouseenter', () => {
        clearTimeout(_cmbSubTimer);
        _cmbSubTimer = setTimeout(() => {
          if (_cmbSubmenu) { _cmbSubmenu.remove(); _cmbSubmenu = null; }
          const sub = _cmbBuildPanel(it.sub, 'cmb-submenu');
          const r = row.getBoundingClientRect();
          sub.style.left = r.right + 'px'; sub.style.top = r.top + 'px';
          document.body.appendChild(sub);
          _cmbSubmenu = sub;
          // flip left if off-screen
          const sr = sub.getBoundingClientRect();
          if (sr.right > window.innerWidth) sub.style.left = (r.left - sr.width) + 'px';
          if (sr.bottom > window.innerHeight) sub.style.top = Math.max(4, window.innerHeight - sr.height - 4) + 'px';
        }, 80);
      });
      row.addEventListener('mouseleave', e => {
        if (!e.relatedTarget?.closest?.('.cmb-submenu')) {
          clearTimeout(_cmbSubTimer);
          _cmbSubTimer = setTimeout(() => { if (_cmbSubmenu) { _cmbSubmenu.remove(); _cmbSubmenu = null; } }, 120);
        }
      });
    } else {
      row.addEventListener('mouseenter', () => {
        clearTimeout(_cmbSubTimer);
        if (_cmbSubmenu) { _cmbSubmenu.remove(); _cmbSubmenu = null; }
        document.querySelectorAll('.cmb-dd-item.cmb-dd-hover').forEach(e => e.classList.remove('cmb-dd-hover'));
        row.classList.add('cmb-dd-hover');
      });
      row.addEventListener('click', () => {
        _cmbClose();
        if (it.fn) it.fn(); else if (it.ch) _dispatchMenu(it.ch);
      });
    }
    panel.appendChild(row);
  });
  return panel;
}

function _cmbOpenMenu(anchorEl, items) {
  _cmbClose();
  _cmbOpenEl = anchorEl;
  anchorEl.classList.add('cmb-active');
  const dd = _cmbBuildPanel(items, 'cmb-dropdown');
  const r  = anchorEl.getBoundingClientRect();
  dd.style.left = r.left + 'px';
  dd.style.top  = r.bottom + 'px';
  document.body.appendChild(dd);
  _cmbDropdown = dd;
  // keep on screen horizontally
  const dr = dd.getBoundingClientRect();
  if (dr.right > window.innerWidth) dd.style.left = Math.max(0, window.innerWidth - dr.width - 4) + 'px';
}

function setupCustomMenuBar() {
  const bar = document.getElementById('custom-menubar');
  if (!bar) return;
  MENU_STRUCTURE.forEach(menu => {
    const el = document.createElement('div');
    el.className = 'cmb-item';
    el.textContent = menu.label;
    el.addEventListener('mousedown', e => {
      e.preventDefault();
      if (_cmbOpenEl === el && _cmbDropdown) { _cmbClose(); return; }
      _cmbOpenMenu(el, menu.items);
    });
    el.addEventListener('mouseenter', () => {
      if (_cmbOpenEl && _cmbOpenEl !== el) _cmbOpenMenu(el, menu.items);
    });
    bar.appendChild(el);
  });
  // close on outside click
  document.addEventListener('mousedown', e => {
    if (_cmbDropdown && !_cmbDropdown.contains(e.target) && !_cmbSubmenu?.contains(e.target) && !e.target.closest('.cmb-item')) _cmbClose();
  }, true);
  // close on Escape
  document.addEventListener('keydown', e => { if (e.key === 'Escape' && _cmbDropdown) _cmbClose(); }, true);
}

// ===== Monaco Loader =====
require.config({ paths: { vs: '../node_modules/monaco-editor/min/vs' } });

require(['vs/editor/editor.main'], () => {
  performance.mark?.('monaco-amd-loaded');
  // Register ONLY the active theme upfront — `monaco.editor.create()`
  // below needs that one resolved, the other 7 are only consulted when
  // the user opens the theme picker. We push the rest to
  // requestIdleCallback so they don't delay first paint. Each
  // defineTheme call iterates the rules array (~300 entries per theme)
  // and converts colours; doing all 8 eagerly cost ~20-30 ms of cold
  // start for no first-paint benefit.
  (function _defineActiveTheme() {
    const t = THEMES[currentTheme];
    if (!t) return;
    monaco.editor.defineTheme('notepp-' + currentTheme, {
      base: t.monacoBase,
      inherit: true,
      rules: t.monacoRules,
      colors: t.monacoColors,
    });
  })();
  const _defineRemainingThemes = () => {
    Object.entries(THEMES).forEach(([id, t]) => {
      if (id === currentTheme) return; // already registered upfront
      try {
        monaco.editor.defineTheme('notepp-' + id, {
          base: t.monacoBase,
          inherit: true,
          rules: t.monacoRules,
          colors: t.monacoColors,
        });
      } catch (e) { console.warn('[theme] late-register failed for', id, e); }
    });
  };
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(_defineRemainingThemes, { timeout: 2000 });
  } else {
    setTimeout(_defineRemainingThemes, 350);
  }

  editor = monaco.editor.create(document.getElementById('monaco-editor'), {
    value: '',
    language: 'plaintext',
    theme: 'notepp-' + currentTheme,
    fontSize: 13,
    fontFamily: "'Cascadia Code', 'Fira Code', Consolas, 'Courier New', monospace",
    // ── Snappy-defaults: disable ligatures + caret animations.
    // Devs typically want `!=` to stay `!=` (not render as ≠), and the cursor
    // to snap to position instantly with a classic blink. Users who prefer the
    // animated/ligated look can opt back in via Preferences → Developer.
    fontLigatures: false,
    lineNumbers: 'on',
    renderWhitespace: 'selection',
    scrollBeyondLastLine: false,
    wordWrap: 'off',
    automaticLayout: true,
    // Heavy first-paint features (minimap, bracket-pair colorization,
    // active-bracket guides) start OFF so the initial layout/paint is
    // faster, then get switched on after the editor is interactive.
    // See requestIdleCallback below.
    minimap: { enabled: false },
    folding: true,
    foldingHighlight: true,
    foldingStrategy: 'indentation',
    glyphMargin: true,
    lineDecorationsWidth: 10,
    renderLineHighlight: 'line',
    cursorBlinking: 'blink',                 // was 'smooth' (faded in/out)
    cursorSmoothCaretAnimation: 'off',       // was 'on' (slid between positions)
    cursorStyle: 'line',
    tabSize: 4,
    insertSpaces: true,
    detectIndentation: true,
    autoIndent: 'full',
    formatOnPaste: true,
    formatOnType: false,
    dragAndDrop: true,
    links: true,
    matchBrackets: 'always',
    bracketPairColorization: { enabled: false },
    guides: { bracketPairs: false, indentation: true },
    overviewRulerLanes: 3,
    overviewRulerBorder: true,
    scrollbar: { verticalScrollbarSize: 10, horizontalScrollbarSize: 10, useShadows: true },
    suggestOnTriggerCharacters: true,
    quickSuggestions: { other: 'on', comments: 'off', strings: 'off' },
    acceptSuggestionOnEnter: 'on',
    parameterHints: { enabled: true },
    wordBasedSuggestions: 'currentDocument',
    hover: { enabled: true, delay: 200 },     // was 300 — snappier tooltip
    contextmenu: false,
    smoothScrolling: false,                   // was true — Page-Down jumps instantly
    mouseWheelZoom: true,
    multiCursorModifier: 'alt',
    snippetSuggestions: 'inline',
    occurrencesHighlight: 'singleFile',
    selectionHighlight: true,
    renderControlCharacters: false,
    colorDecorators: false,                   // was true — no inline colour swatches
    inlayHints: { enabled: 'on' },
    lightbulb: { enabled: 'off' },            // was 'on' — kills the random 💡 popup
    stickyScroll: { enabled: false },         // was true — no floating header while scrolling
    padding: { top: 4, bottom: 4 },
  });
  performance.mark?.('monaco-editor-created');

  // Expose for lsp-client.js (finds tab by Monaco model + reads activeGitRepo)
  window.tabs = tabs;
  window.NotePPLsp?.attachEditor(editor);

  // Post-paint upgrade: switch on heavy editor features (minimap,
  // bracket-pair colorization, bracket guides) once the editor is
  // actually visible. Saves ~50-100 ms on first paint by deferring the
  // initial token-pair walk that those features trigger.
  const enableHeavyEditorOptions = () => {
    if (!editor) return;
    const wantMinimap = (typeof window.__uiPrefMinimap === 'boolean')
      ? window.__uiPrefMinimap : true;
    editor.updateOptions({
      minimap: wantMinimap
        ? { enabled: true, scale: 1, showSlider: 'mouseover' }
        : { enabled: false },
      bracketPairColorization: { enabled: true, independentColorPoolPerBracketType: true },
      guides: { bracketPairs: 'active', indentation: true },
    });
  };
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(enableHeavyEditorOptions, { timeout: 1200 });
  } else {
    setTimeout(enableHeavyEditorOptions, 250);
  }

  registerMermaidLanguage();
  // Load persisted preferences before restoring session
  window.electronAPI.readSettings().then(s => {
    const nd = s.newDoc || {};
    newDocDefaults = {
      encoding: nd.encoding || 'UTF-8',
      eol:      nd.eol      || 'Windows (CR LF)',
      language: nd.language || 'plaintext',
      template: nd.template || '',
    };
    const bkp = s.backup || {};
    startAutoBackup(!!bkp.enabled, (bkp.intervalMin || 5) * 60 * 1000);
    const das = s.diskAutoSave || {};
    startDiskAutoSave(!!das.enabled, das.intervalSecs || 30);
    if (Array.isArray(s.macros)) savedMacros = s.macros;
    // Pre-populate terminal pref elements so they're ready when the terminal opens
    const t = s.terminal || {};
    const shellEl    = document.getElementById('pref-shell');
    const fontSizeEl = document.getElementById('pref-term-fontsize');
    if (shellEl    && t.shell)    shellEl.value    = t.shell;
    if (fontSizeEl && t.fontSize) fontSizeEl.value = t.fontSize;

    // ── Persistent UI toggles: theme, word wrap, editor zoom ────────────
    const ui = s.ui || {};
    // ui.theme (new) takes precedence; fall back to legacy ui.darkMode boolean
    applyTheme(ui.theme || (ui.darkMode ? 'dark' : 'light'), false);
    // Title bar mode: default true (themed). Only call if explicitly saved to false.
    applyTitlebarMode(ui.themedTitlebar === true, false);
    if (ui.wordWrap === true  && !isWordWrap) {
      // Inline the toggle to skip the re-save (we just LOADED this value)
      isWordWrap = true;
      editor.updateOptions({ wordWrap: 'on' });
      document.getElementById('btn-wordwrap')?.classList.add('active');
    }
    if (typeof ui.fontSize === 'number' && ui.fontSize >= 8 && ui.fontSize <= 40) {
      currentFontSize = ui.fontSize;
      editor.updateOptions({ fontSize: currentFontSize });
    }
    if (typeof ui.minimap === 'boolean') {
      window.__uiPrefMinimap = ui.minimap;
      editor.updateOptions({ minimap: ui.minimap
        ? { enabled: true, scale: 1, showSlider: 'mouseover' }
        : { enabled: false } });
    }
    _hydrateSpellFromLegacySetting(ui);
    if (typeof ui.renderWhitespace === 'boolean') {
      editor.updateOptions({ renderWhitespace: ui.renderWhitespace ? 'all' : 'selection' });
    }
    if (typeof ui.indentGuides === 'boolean') {
      editor.updateOptions({ guides: { indentation: ui.indentGuides } });
    }
    if (ui.showToolbar === false) {
      const el = document.getElementById('toolbar'); if (el) el.style.display = 'none';
    }
    if (ui.showStatusbar === false) {
      const el = document.getElementById('status-bar'); if (el) el.style.display = 'none';
    }
    if (ui.showTabbar === false) {
      const el = document.getElementById('tab-bar-container'); if (el) el.style.display = 'none';
    }
    if (ui.columnSelectMode === true) {
      isColumnSelectMode = true;
      editor.updateOptions({ columnSelection: true });
      updateColSelectStatus();
    }
  });

  // Populate the About dialog with the real app version (from package.json
  // via the main process, so we never get a stale hardcoded number again).
  if (window.electronAPI?.getAppVersion) {
    window.electronAPI.getAppVersion()
      .then(v => { const el = document.getElementById('about-version'); if (el && v) el.textContent = v; })
      .catch(() => {});
  }

  // ── Auto-update status pill (right side of the toolbar) ────────────────
  // Drives a small pill that flips through three states. The "ready" state
  // is the only clickable one; clicking it tells main to close the main
  // window and open the updater progress window (which then installs +
  // relaunches). No silent close-time install — explicit user gesture only.
  (function wireUpdatePill() {
    const pill = document.getElementById('toolbar-update-pill');
    const txt  = document.getElementById('toolbar-update-text');
    if (!pill || !txt || !window.electronAPI?.autoUpdate?.onStatus) return;

    let pendingVersion = null;

    function setState(state, label, tooltip) {
      pill.dataset.state = state;
      txt.textContent    = label;
      pill.title         = tooltip || label;
      pill.classList.remove('hidden');
    }

    function triggerInstall() {
      if (pill.dataset.state !== 'ready') return;
      // Disable repeat clicks while we're handing off to the updater window
      pill.style.pointerEvents = 'none';
      txt.textContent = 'Updating…';
      window.electronAPI.autoUpdate.installNow();
    }
    pill.addEventListener('click', triggerInstall);
    pill.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); triggerInstall(); }
    });

    window.electronAPI.autoUpdate.onStatus(({ state, version, percent }) => {
      if (version) pendingVersion = version;
      switch (state) {
        case 'available':
          setState('available', `New update available${pendingVersion ? ` (v${pendingVersion})` : ''}`,
                   'Downloading in the background…');
          break;
        case 'downloading': {
          const p = percent != null ? `${Math.round(percent)}%` : '';
          setState('downloading', `Downloading${p ? ' ' + p : '…'}`, 'Downloading update…');
          break;
        }
        case 'downloaded':
        case 'ready':
          setState('ready', `Click to update${pendingVersion ? ` to v${pendingVersion}` : ''}`,
                   'Restart and install the downloaded update');
          break;
        case 'error':
          // Auto-updater failed (signature mismatch, network drop, etc.).
          // Show a clickable pill that takes the user to the GitHub release
          // page so they can grab the installer manually.
          setState('error', 'Update failed — download manually',
                   'Auto-update failed. Click to open the GitHub release page.');
          pill.style.pointerEvents = '';
          pill.onclick = () => {
            try { window.electronAPI.shellOpen('https://github.com/YogeshPraj/Note-/releases/latest'); } catch {}
          };
          break;
        case 'none':
        case 'checking':
        default:
          // Don't show the pill when there's nothing actionable.
          pill.classList.add('hidden');
          break;
      }
    });
  })();

  // Show a usable editor immediately, then restore session asynchronously.
  // This keeps time-to-first-interaction snappy even with large sessions.
  const startupPlaceholder = createTab();
  const restoreTask = async () => {
    const restored = await restoreSession({ replacePlaceholderId: startupPlaceholder.id });
    if (!restored && tabs.length === 0) createTab();
  };
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(() => { restoreTask(); }, { timeout: 600 });
  } else {
    setTimeout(() => { restoreTask(); }, 0);
  }

  const deferredInit = () => {
    // Encryption profile detection (only reads one small JSON file).
    loadEncryptionProfile().then(() => updateEncryptionStatusIndicator());
    // Git: spawns `git --version` + walks for .git + wires the 5-min poll.
    // None of this affects the editor working — defer to idle.
    setupSourceControlPanel();
    initGitIntegration();
  };
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(deferredInit, { timeout: 1500 });
  } else {
    setTimeout(deferredInit, 200);
  }

  // Events
  editor.onDidChangeCursorPosition(updateStatusBar);
  editor.onDidChangeCursorSelection(updateStatusBar);
  // Spell-check: rescan the newly visible range when the user scrolls
  // (debounced — many scroll events fire in a single drag).
  editor.onDidScrollChange?.(() => scheduleSpellScan());
  editor.onDidChangeModelContent((e) => {
    const tab = getActiveTab();
    if (tab && !tab.dirty) { tab.dirty = true; renderTabs(); }
    updateStatusBar();
    updateTitle();
    scheduleAutoSave();
    if (!_jsonEditorUpdating) schedulePreviewUpdate();
    scheduleGitDiffUpdate(tab);   // re-paint inline git-diff gutter
    try { window.NotePPLsp?.onTabContentChange(tab); } catch {}
    // Spell-check: try auto-correct of the just-finished word (no-op
    // unless mode === 'auto'), then schedule a debounced re-scan.
    try { _maybeAutoCorrect(e); } catch {}
    scheduleSpellScan();
    // Fallback auto-detect: catches pastes via Edit menu / execCommand that
    // don't fire onDidPaste. Guard: only plaintext, only once per tab, only
    // after the content is big enough to be meaningful (> 60 chars, multi-line).
    if (tab && tab.language === 'plaintext' && !tab._autoDetectTriggered) {
      const v = editor.getValue();
      if (v.length > 60 && v.includes('\n')) _scheduleAutoDetect(tab);
    }
  });

  // ── Macro recording hooks ─────────────────────────────────────────────────
  // onDidType fires for every printed character (incl. Enter → \n, Tab → \t).
  editor.onDidType(text => {
    if (!isRecording) return;
    const last = currentMacroOps[currentMacroOps.length - 1];
    if (last?.op === 'type') last.text += text; // merge consecutive chars
    else currentMacroOps.push({ op: 'type', text });
  });
  // onKeyDown fires for ALL keys before Monaco processes them.  We only
  // capture the non-printable navigation/editing keys here; printable chars
  // and Enter/Tab come through onDidType.
  editor.onKeyDown(e => {
    if (!isRecording) return;
    _handleMacroKeydown(e);
  });

  // Primary path: onDidPaste fires immediately after Ctrl+V / right-click Paste.
  editor.onDidPaste(() => {
    const tab = getActiveTab();
    if (!tab || tab.language !== 'plaintext') return;
    tab._autoDetectTriggered = true; // prevent the debounced fallback firing again
    _applyAutoDetectedLanguage(tab, editor.getValue());
  });

  // Strip "rich" Unicode characters that sneak in when pasting from
  // Word / browsers / Slack / etc. Smart quotes, em-dashes, non-
  // breaking spaces, zero-width chars — they look like normal text
  // but break code, regex, JSON, command-line snippets. We intercept
  // the DOM-level `paste` event in the capture phase so Monaco never
  // sees the original; then we re-insert the cleaned text via
  // `executeEdits` so format-on-paste / multi-cursor / undo all
  // behave correctly.
  setupPastePreprocessor();

  // Keyboard shortcuts
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyN, () => newTab());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyO, () => openFile());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveFile());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyS, () => saveFileAs());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyS, () => saveAll());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyW, () => closeTab(activeTabId));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyF, () => openFindReplace('find'));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyF, () => formatWithAutoDetect());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyH, () => openFindReplace('replace'));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyG, () => openGotoLine());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyP, () => openQuickOpen());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyP, () => openCmdPalette());
  editor.addCommand(monaco.KeyCode.F3, () => doFind(1));
  editor.addCommand(monaco.KeyMod.Shift | monaco.KeyCode.F3, () => doFind(-1));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.PageUp, () => switchTab(-1));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.PageDown, () => switchTab(1));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyW, () => toggleWordWrap());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyD, () => toggleDarkMode());
  editor.addCommand(monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyF, () => formatWithAutoDetect());
  editor.addCommand(monaco.KeyCode.F5, () => runCurrentFile());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyV, () => togglePreview());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyMod.Shift | monaco.KeyCode.KeyG, () => openGameTab());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyG, () => toggleSourceControlPanel());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyA, () => toggleAiPanel());
  editor.addCommand(monaco.KeyCode.F12, () => editor.getAction('editor.action.revealDefinition')?.run());
  editor.addCommand(monaco.KeyMod.Shift | monaco.KeyCode.F12, () => editor.getAction('editor.action.goToReferences')?.run());
  editor.addCommand(monaco.KeyCode.F2, () => editor.getAction('editor.action.rename')?.run());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Backquote, () => toggleTerminal());
  // Macro shortcuts
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyR, toggleMacroRecording);
  editor.addCommand(monaco.KeyCode.F9,  () => runLastMacro(1));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.F9, () => {
    const macro = lastRecordedMacro || savedMacros[0] || null;
    if (macro) openRunNTimesDialog(macro);
    else showToast('No macro recorded yet — press Ctrl+Shift+R to start recording');
  });

  // ── Column selection & multi-cursor shortcuts ───────────────────────────
  // Select line (Monaco has expandLineSelection but no default keybinding)
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyL,
    () => editor.trigger('keyboard', 'expandLineSelection', null));
  // Add cursors to ends of all lines in the current selection
  editor.addCommand(monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.KeyI,
    () => editor.getAction('editor.action.insertCursorAtEndOfEachLineSelected')?.run());
  // Select all occurrences of the current text selection
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyL,
    () => editor.getAction('editor.action.selectHighlights')?.run());
  // Select all occurrences of the word at cursor and rename/edit all simultaneously
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.F2,
    () => editor.getAction('editor.action.changeAll')?.run());
  // Column select via keyboard — Monaco has Ctrl+Shift+Alt+Arrows as built-ins,
  // but we register them explicitly so they show up in the command log and work
  // even if Monaco's built-in bindings are overridden elsewhere.
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.UpArrow,
    () => editor.trigger('keyboard', 'cursorColumnSelectUp', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.DownArrow,
    () => editor.trigger('keyboard', 'cursorColumnSelectDown', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.LeftArrow,
    () => editor.trigger('keyboard', 'cursorColumnSelectLeft', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.RightArrow,
    () => editor.trigger('keyboard', 'cursorColumnSelectRight', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.Home,
    () => editor.trigger('keyboard', 'cursorColumnSelectHome', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.End,
    () => editor.trigger('keyboard', 'cursorColumnSelectEnd', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.PageUp,
    () => editor.trigger('keyboard', 'cursorColumnSelectPageUp', null));
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyMod.Alt | monaco.KeyCode.PageDown,
    () => editor.trigger('keyboard', 'cursorColumnSelectPageDown', null));

  // Ctrl+Shift+T (reopen closed tab) and Ctrl± / Ctrl0 (zoom) are wired
  // as document-level shortcuts in setupGlobalShortcuts() so they fire
  // from whiteboard/drawio/game tabs too — Monaco's addCommand only
  // fires while the editor has focus.

  setupMenuListeners();
  setupExternalChangeWatcher();
  setupContextMenu();
  setupDragDrop();
  setupIconsPanel();
  setupFindReplace();
  setupModals();
  setupThemePicker();
  setupCustomMenuBar();
  setupToolbar();
  setupTerminal();
  setupTerminalResize();
  setupFileTreeResize();
  setupQuickOpen();
  setupCmdPalette();
  setupPreview();
  setupGlobalEscape();
  setupGlobalShortcuts();
  // Load onboarding state from settings.json (survives quota clears,
  // Chromium upgrades, etc. — unlike localStorage). Then, if the tour
  // has never been seen, fire it ~1.2 s after paint. Users can replay
  // any time from ? menu → Show Feature Tour.
  loadOnboardingState().then(() => {
    if (!onboardingState.tourSeen) {
      setTimeout(() => runFeatureTour(), 1200);
    }
  });
  setupAltMouseColumnSelect();
  updateStatusBar();
  statusCol?.addEventListener('click', () => toggleColumnSelectMode());

  // Defer non-essential panes/tooling so startup stays focused on editor interactivity.
  const deferredUiInit = () => {
    setupRegexTester();
    setupAiPanel();
    setupDiffToolbars();
  };
  if (typeof requestIdleCallback === 'function') {
    requestIdleCallback(deferredUiInit, { timeout: 1800 });
  } else {
    setTimeout(deferredUiInit, 250);
  }

  // Signal main that all our IPC listeners (especially 'open-files') are now
  // wired up. Main will flush any files queued from double-click / "Open with".
  try { window.electronAPI.rendererReady(); } catch {}
  performance.mark?.('renderer-ready');
});

// ===== Mermaid Language Registration =====
function registerMermaidLanguage() {
  if (monaco.languages.getLanguages().some(l => l.id === 'mermaid')) return;

  monaco.languages.register({
    id: 'mermaid',
    extensions: ['.mmd', '.mermaid'],
    aliases: ['Mermaid', 'mermaid'],
    mimetypes: ['text/x-mermaid'],
  });

  // Monarch tokenizer — syntax highlighting
  monaco.languages.setMonarchTokensProvider('mermaid', {
    defaultToken: '',
    tokenPostfix: '.mermaid',

    diagramTypes: [
      'graph','flowchart','sequenceDiagram','classDiagram','stateDiagram',
      'stateDiagram-v2','erDiagram','gantt','pie','journey','gitGraph',
      'mindmap','timeline','xychart-beta','block-beta','architecture-beta',
      'quadrantChart','requirementDiagram',
      'C4Context','C4Container','C4Component','C4Dynamic','C4Deployment',
    ],

    keywords: [
      'subgraph','end','direction','style','classDef','class','linkStyle',
      'click','callback','call','href','participant','actor','activate',
      'deactivate','note','loop','alt','else','opt','par','and','critical',
      'break','rect','over','section','title','dateFormat','axisFormat',
      'excludes','includes','todayMarker','accTitle','accDescr',
      'commit','branch','checkout','merge','cherry-pick','reset','revert',
    ],

    directions: ['LR','RL','TB','TD','BT'],

    tokenizer: {
      root: [
        // Comments (%% to end of line)
        [/%%.*$/, 'comment'],
        // Strings
        [/"[^"]*"/, 'string'],
        // Diagram type on its own line (first meaningful token)
        [/\b(graph|flowchart|sequenceDiagram|classDiagram|stateDiagram(?:-v2)?|erDiagram|gantt|pie|journey|gitGraph|mindmap|timeline|xychart-beta|block-beta|architecture-beta|quadrantChart|requirementDiagram|C4(?:Context|Container|Component|Dynamic|Deployment))\b/,
          'keyword.control'],
        // Structural + flow keywords
        [/\b(subgraph|end|direction|style|classDef|class|linkStyle|click|callback|call|href)\b/, 'keyword'],
        // Sequence/interaction keywords
        [/\b(participant|actor|activate|deactivate|note|loop|alt|else|opt|par|and|critical|break|rect|over)\b/, 'keyword'],
        // Gantt / timeline keywords
        [/\b(section|title|dateFormat|axisFormat|excludes|includes|todayMarker|accTitle|accDescr)\b/, 'keyword'],
        // Git graph keywords
        [/\b(commit|branch|checkout|merge|cherry-pick|reset|revert)\b/, 'keyword'],
        // Direction tokens
        [/\b(LR|RL|TB|TD|BT)\b/, 'type'],
        // Class decorators (:::className)
        [/:::\w+/, 'type.identifier'],
        // Arrow / edge operators
        [/(-{1,3}[>|ox*]|={2,3}[>|ox*]|-\.-[>|ox*]|<(?:-{1,3}|={2,3})|(--[>|x]|===[>|x]))/, 'keyword.operator'],
        [/(-{2,}|={2,}|-\.-)/, 'keyword.operator'],
        // Edge labels  |text|
        [/\|[^|]+\|/, 'string'],
        // Node shape delimiters
        [/[(\[{]/, { token: 'delimiter.bracket', bracket: '@open'  }],
        [/[)\]}]/, { token: 'delimiter.bracket', bracket: '@close' }],
        // Numbers
        [/\b\d+(?:\.\d+)?%?(?:d|w|h|m)?\b/, 'number'],
        // Identifiers / node IDs
        [/[A-Za-z_$][\w$-]*/, 'identifier'],
        // Separators
        [/[;:,]/, 'delimiter'],
        [/\s+/, 'white'],
      ],
    },
  });

  // Language configuration — comments, bracket-matching, auto-close
  monaco.languages.setLanguageConfiguration('mermaid', {
    comments: { lineComment: '%%' },
    brackets: [['[',']'], ['(',')'], ['{','}']],
    autoClosingPairs: [
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '{', close: '}' },
      { open: '"', close: '"' },
    ],
    surroundingPairs: [
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '{', close: '}' },
      { open: '"', close: '"' },
    ],
  });

  // Completion provider — diagram type snippets + keywords
  monaco.languages.registerCompletionItemProvider('mermaid', {
    provideCompletionItems(model, position) {
      const word  = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber, endLineNumber: position.lineNumber,
        startColumn: word.startColumn,        endColumn: word.endColumn,
      };
      const Snippet  = monaco.languages.CompletionItemKind.Snippet;
      const Keyword  = monaco.languages.CompletionItemKind.Keyword;
      const InsertAs = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;

      const snippets = [
        { label:'flowchart', kind:Snippet,
          insertText:'flowchart ${1|LR,TB,RL,BT|}\n    A[${2:Start}] --> B{${3:Decision?}}\n    B -- Yes --> C[${4:Action A}]\n    B -- No  --> D[${5:Action B}]\n    C --> E[${6:End}]\n    D --> E',
          insertTextRules:InsertAs, detail:'Flowchart diagram', range },
        { label:'sequenceDiagram', kind:Snippet,
          insertText:'sequenceDiagram\n    participant ${1:A} as ${2:Alice}\n    participant ${3:B} as ${4:Bob}\n    ${1}->>+${3}: ${5:Hello!}\n    ${3}-->>-${1}: ${6:Hi there!}',
          insertTextRules:InsertAs, detail:'Sequence diagram', range },
        { label:'classDiagram', kind:Snippet,
          insertText:'classDiagram\n    class ${1:Animal} {\n        +${2:String} name\n        +${3:makeSound}() void\n    }\n    class ${4:Dog} {\n        +fetch() void\n    }\n    ${1} <|-- ${4}',
          insertTextRules:InsertAs, detail:'Class diagram', range },
        { label:'erDiagram', kind:Snippet,
          insertText:'erDiagram\n    ${1:CUSTOMER} ||--o{ ${2:ORDER} : places\n    ${2} ||--|{ ${3:LINE-ITEM} : contains\n    ${1} {\n        string name\n        string email\n    }',
          insertTextRules:InsertAs, detail:'Entity-Relationship diagram', range },
        { label:'stateDiagram-v2', kind:Snippet,
          insertText:'stateDiagram-v2\n    [*] --> ${1:Idle}\n    ${1} --> ${2:Running} : ${3:start}\n    ${2} --> ${1} : ${4:stop}\n    ${2} --> [*] : finish',
          insertTextRules:InsertAs, detail:'State diagram', range },
        { label:'gantt', kind:Snippet,
          insertText:'gantt\n    title ${1:Project Timeline}\n    dateFormat YYYY-MM-DD\n    section ${2:Planning}\n        ${3:Research}  :a1, ${4:2024-01-01}, ${5:7d}\n        ${6:Design}    :a2, after a1, ${7:5d}\n    section ${8:Development}\n        ${9:Coding}    :a3, after a2, ${10:14d}',
          insertTextRules:InsertAs, detail:'Gantt chart', range },
        { label:'pie', kind:Snippet,
          insertText:'pie title ${1:Distribution}\n    "${2:Category A}" : ${3:40}\n    "${4:Category B}" : ${5:30}\n    "${6:Category C}" : ${7:20}\n    "${8:Other}"      : ${9:10}',
          insertTextRules:InsertAs, detail:'Pie chart', range },
        { label:'mindmap', kind:Snippet,
          insertText:'mindmap\n  root((${1:Main Topic}))\n    ${2:Branch 1}\n      ${3:Leaf A}\n      ${4:Leaf B}\n    ${5:Branch 2}\n      ${6:Leaf C}',
          insertTextRules:InsertAs, detail:'Mindmap', range },
        { label:'gitGraph', kind:Snippet,
          insertText:'gitGraph\n   commit id: "${1:init}"\n   branch ${2:develop}\n   checkout ${2}\n   commit id: "${3:feature}"\n   checkout main\n   merge ${2}\n   commit id: "${4:release}"',
          insertTextRules:InsertAs, detail:'Git graph', range },
        { label:'timeline', kind:Snippet,
          insertText:'timeline\n    title ${1:History}\n    section ${2:Early}\n        ${3:2000} : ${4:Event A}\n    section ${5:Modern}\n        ${6:2010} : ${7:Event B}\n        ${8:2020} : ${9:Event C}',
          insertTextRules:InsertAs, detail:'Timeline', range },
        { label:'xychart-beta', kind:Snippet,
          insertText:'xychart-beta\n    title "${1:Sales Chart}"\n    x-axis [${2:jan, feb, mar, apr, may, jun}]\n    y-axis "${3:Revenue}" ${4:4000} --> ${5:11000}\n    bar [${6:5000, 6000, 7500, 8200, 9100, 10000}]\n    line [${6}]',
          insertTextRules:InsertAs, detail:'XY Chart', range },
      ];

      // Keyword completions
      const kws = ['subgraph','end','participant','actor','note','loop','alt','else','opt',
                   'section','title','dateFormat','classDef','style','linkStyle','LR','RL','TB','TD','BT',
                   'direction','click','href','activate','deactivate','commit','branch','merge','checkout'];
      kws.forEach(kw => snippets.push({ label:kw, kind:Keyword, insertText:kw, range }));

      return { suggestions: snippets };
    },
  });
}

// ===== Tab Management =====

// Returns the lowest integer N ≥ 1 not already used by an untitled tab
function nextNewTabNumber() {
  const used = new Set(
    tabs
      .filter(t => !t.filePath && /^new \d+$/.test(t.name))
      .map(t => parseInt(t.name.slice(4), 10))
  );
  let n = 1;
  while (used.has(n)) n++;
  return n;
}

function createTab(filePath = null, content = '') {
  tabCounter++;
  const id = tabCounter;
  const name = filePath ? filePath.split(/[\\/]/).pop() : `new ${nextNewTabNumber()}`;
  const language = filePath ? detectLanguage(filePath) : (newDocDefaults.language || 'plaintext');
  // Apply template only for brand-new empty tabs (no filePath, no explicit content)
  const body = filePath ? content : (content || newDocDefaults.template || '');
  const model = monaco.editor.createModel(body, language);
  const tab = {
    id, name, filePath, content: body, dirty: false, language,
    encoding: filePath ? 'UTF-8' : (newDocDefaults.encoding || 'UTF-8'),
    eol:      filePath ? 'Windows (CR LF)' : (newDocDefaults.eol || 'Windows (CR LF)'),
    model, viewState: null, type: 'editor',
    encrypted: false, protectedBy: null,    // see features/ENCRYPTION.md
  };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function createGameTab() {
  // Only allow one game tab at a time
  if (gameTabId !== null) {
    const existing = tabs.find(t => t.id === gameTabId);
    if (existing) { activateTab(gameTabId); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const tab = { id, name: '🎮 Dev Arcade', filePath: null, content: '', dirty: false, language: 'plaintext', encoding: 'UTF-8', eol: 'Windows (CR LF)', model: null, viewState: null, type: 'game' };
  gameTabId = id;
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function openGameTab() {
  createGameTab();
}

// ── Whiteboard ────────────────────────────────────────────────────────────────

// Lowest integer N ≥ 1 not already used by an existing whiteboard tab name.
// Accepts both the new "whiteboard-N" form and the legacy "whiteboard-N.json"
// form so a session restored from before v1.5.x doesn't reuse numbers.
// nextWbTabNumber + sendToWhiteboard now live in src/whiteboard-helpers.js.
// Loaded as a classic <script> before renderer.js — both remain globals.

// =============================================================================
// draw.io integration — bundle is downloaded on-demand, lives in userData
// =============================================================================
// Tabs of type 'drawio' share a single iframe (#drawio-frame) the same way
// whiteboard tabs share #whiteboard-frame. The iframe loads drawio.html which
// in turn loads the actual draw.io webapp via the drawio:// custom protocol.
//
// First-use flow: clicking "New draw.io Diagram" (or opening a .drawio file)
// calls ensureDrawioInstalled() — if the bundle isn't already in userData,
// shows a modal with a progress bar that drives the main-process download.
// =============================================================================

let drawioReady = false;
let drawioPendingContent = null;

// Lowest integer N ≥ 1 not used by an existing drawio tab name

// =============================================================================
// Compare (File diff + Folder diff) — see features/DIFF.md
// =============================================================================

// One Monaco DiffEditor instance reused across diff tabs. Tabs swap their
// (originalModel, modifiedModel) into it on activation.
let diffEditor = null;
let diffEditorSideBySide = true;

// Separate Monaco DiffEditor for the quick-compare panel (right-click → Compare).
let quickDiffEditor = null;
let quickDiffSideBySide = true;

// Last "Select for Compare" tab — picked from the tab right-click menu
let compareSelectedTabId = null;

function createFileDiffTab(leftPath, rightPath) {
  tabCounter++;
  const id = tabCounter;
  const leftName  = leftPath  ? leftPath.split(/[\\/]/).pop()  : 'left';
  const rightName = rightPath ? rightPath.split(/[\\/]/).pop() : 'right';
  const tab = {
    id,
    name: `⇆ ${leftName} ↔ ${rightName}`,
    filePath: null,                              // not a "real" file tab
    dirty: false,
    type: 'diff',
    diff: {
      leftPath, rightPath,
      leftLang: leftPath ? detectLanguage(leftPath) : 'plaintext',
      rightLang: rightPath ? detectLanguage(rightPath) : 'plaintext',
      originalModel: null,
      modifiedModel: null,
      mounted: false,
    },
  };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function createFolderDiffTab(leftDir, rightDir) {
  tabCounter++;
  const id = tabCounter;
  const ln = leftDir.split(/[\\/]/).pop();
  const rn = rightDir.split(/[\\/]/).pop();
  const tab = {
    id,
    name: `⇆ ${ln} ↔ ${rn}`,
    filePath: null,
    dirty: false,
    type: 'folder-diff',
    folderDiff: {
      leftDir, rightDir,
      entries: null,
      summary: null,
      onlyChanges: true,
      mounted: false,
    },
  };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

// Mount + load both files into the Monaco DiffEditor when a diff tab is activated.
async function mountFileDiffTab(tab) {
  const host = document.getElementById('diff-monaco');
  if (!diffEditor) {
    diffEditor = monaco.editor.createDiffEditor(host, {
      automaticLayout: true,
      readOnly: true,
      renderSideBySide: diffEditorSideBySide,
      fontSize: 13,
      fontFamily: "'Cascadia Code', 'Fira Code', Consolas, monospace",
      minimap: { enabled: false },
      scrollBeyondLastLine: false,
      theme: isDarkMode ? 'notepp-dark' : 'notepp-light',
    });
  }
  // Update title
  document.getElementById('diff-title').textContent =
    `⇆  ${tab.diff.leftPath || '(left)'}   ↔   ${tab.diff.rightPath || '(right)'}`;

  // Lazy-load + cache the models on the tab
  if (!tab.diff.mounted) {
    await reloadDiffTab(tab);
    tab.diff.mounted = true;
  } else {
    diffEditor.setModel({ original: tab.diff.originalModel, modified: tab.diff.modifiedModel });
  }
  // Trigger a layout pass (container just became visible)
  setTimeout(() => { try { diffEditor.layout(); } catch {} }, 50);
}

async function reloadDiffTab(tab) {
  if (!tab || tab.type !== 'diff') return;
  let leftContent = '', rightContent = '';
  if (tab.diff.leftPath) {
    const r = await window.electronAPI.readFile(tab.diff.leftPath);
    leftContent = r.success ? r.content : `// failed to read: ${r.error || tab.diff.leftPath}`;
  }
  if (tab.diff.rightPath) {
    const r = await window.electronAPI.readFile(tab.diff.rightPath);
    rightContent = r.success ? r.content : `// failed to read: ${r.error || tab.diff.rightPath}`;
  }
  try { tab.diff.originalModel?.dispose(); } catch {}
  try { tab.diff.modifiedModel?.dispose(); } catch {}
  tab.diff.originalModel = monaco.editor.createModel(leftContent,  tab.diff.leftLang);
  tab.diff.modifiedModel = monaco.editor.createModel(rightContent, tab.diff.rightLang);
  diffEditor.setModel({ original: tab.diff.originalModel, modified: tab.diff.modifiedModel });
}

// Wire the diff-tab toolbar buttons (called once at startup from setupModals or similar).
function setupDiffToolbars() {
  document.getElementById('btn-diff-swap')?.addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab || tab.type !== 'diff') return;
    [tab.diff.leftPath,  tab.diff.rightPath]  = [tab.diff.rightPath,  tab.diff.leftPath];
    [tab.diff.leftLang,  tab.diff.rightLang]  = [tab.diff.rightLang,  tab.diff.leftLang];
    const ln  = tab.diff.leftPath  ? tab.diff.leftPath.split(/[\\/]/).pop()  : 'left';
    const rn  = tab.diff.rightPath ? tab.diff.rightPath.split(/[\\/]/).pop() : 'right';
    tab.name = `⇆ ${ln} ↔ ${rn}`;
    reloadDiffTab(tab);
    document.getElementById('diff-title').textContent =
      `⇆  ${tab.diff.leftPath || '(left)'}   ↔   ${tab.diff.rightPath || '(right)'}`;
    renderTabs(); updateTitle();
  });
  document.getElementById('btn-diff-inline')?.addEventListener('click', () => {
    diffEditorSideBySide = !diffEditorSideBySide;
    diffEditor?.updateOptions({ renderSideBySide: diffEditorSideBySide });
    document.getElementById('btn-diff-inline').textContent =
      diffEditorSideBySide ? '≡ Inline' : '⫶ Side-by-side';
  });
  document.getElementById('btn-diff-next')?.addEventListener('click', () => diffEditor?.getModifiedEditor()?.trigger('toolbar', 'editor.action.diffReview.next', null) ?? null);
  document.getElementById('btn-diff-prev')?.addEventListener('click', () => diffEditor?.getModifiedEditor()?.trigger('toolbar', 'editor.action.diffReview.prev', null) ?? null);
  document.getElementById('btn-diff-reload')?.addEventListener('click', () => {
    const tab = getActiveTab();
    if (tab?.type === 'diff') reloadDiffTab(tab);
  });

  // Folder-diff toolbar
  document.getElementById('btn-folder-diff-swap')?.addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab || tab.type !== 'folder-diff') return;
    [tab.folderDiff.leftDir, tab.folderDiff.rightDir] = [tab.folderDiff.rightDir, tab.folderDiff.leftDir];
    const ln = tab.folderDiff.leftDir.split(/[\\/]/).pop();
    const rn = tab.folderDiff.rightDir.split(/[\\/]/).pop();
    tab.name = `⇆ ${ln} ↔ ${rn}`;
    tab.folderDiff.mounted = false;
    mountFolderDiffTab(tab);
    renderTabs(); updateTitle();
  });
  document.getElementById('btn-folder-diff-reload')?.addEventListener('click', () => {
    const tab = getActiveTab();
    if (tab?.type === 'folder-diff') { tab.folderDiff.mounted = false; mountFolderDiffTab(tab); }
  });
  document.getElementById('folder-diff-only-changes')?.addEventListener('change', (e) => {
    const tab = getActiveTab();
    if (!tab || tab.type !== 'folder-diff') return;
    tab.folderDiff.onlyChanges = e.target.checked;
    renderFolderDiffBody(tab);
  });
  setupQuickDiffToolbar();
}

// Run dir-compare via IPC, then render the resulting tree.
async function mountFolderDiffTab(tab) {
  document.getElementById('folder-diff-title').textContent =
    `⇆  ${tab.folderDiff.leftDir}   ↔   ${tab.folderDiff.rightDir}`;
  document.getElementById('folder-diff-only-changes').checked = tab.folderDiff.onlyChanges !== false;

  if (!tab.folderDiff.mounted) {
    document.getElementById('folder-diff-body').innerHTML = '<div style="padding:20px;color:#888">Comparing folders…</div>';
    document.getElementById('folder-diff-summary').textContent = '';
    const r = await window.electronAPI.compareFolders({
      left:  tab.folderDiff.leftDir,
      right: tab.folderDiff.rightDir,
    });
    if (!r.success) {
      document.getElementById('folder-diff-body').innerHTML =
        `<div style="padding:20px;color:#e03131">Compare failed: ${escapeHtml(r.error || 'unknown')}</div>`;
      return;
    }
    tab.folderDiff.entries = r.entries;
    tab.folderDiff.summary = r.summary;
    tab.folderDiff.mounted = true;
  }
  renderFolderDiffBody(tab);
}

function renderFolderDiffBody(tab) {
  const body = document.getElementById('folder-diff-body');
  const sum  = tab.folderDiff.summary;
  const summaryEl = document.getElementById('folder-diff-summary');
  if (sum) {
    summaryEl.textContent =
      `${sum.rightOnlyCount} added · ${sum.leftOnlyCount} removed · ${sum.distinctCount} differ · ${sum.equalCount} equal`;
  }
  const onlyChanges = tab.folderDiff.onlyChanges !== false;
  const entries = (tab.folderDiff.entries || [])
    .filter(e => !onlyChanges || e.status !== 'equal')
    .sort((a, b) => (a.relPath + a.name).localeCompare(b.relPath + b.name));

  if (!entries.length) {
    body.innerHTML = `<div style="padding:20px;color:#16a34a">✓ No differences${onlyChanges ? ' (filtering equal entries)' : ''}.</div>`;
    return;
  }

  const ICONS = { added: '⊕', removed: '⊖', modified: '✎', equal: '·' };
  const FILE_ICON = '📄';
  const DIR_ICON  = '📁';
  body.innerHTML = entries.map(e => {
    const icon = e.isDir ? DIR_ICON : FILE_ICON;
    const status = e.status;
    const leftHas  = status !== 'added';
    const rightHas = status !== 'removed';
    const display  = (path) => `<span class="fd-icon">${icon}</span><span class="fd-name">${escapeHtml(path)}</span>`;
    const left  = leftHas  ? display((e.relPath || '') + (e.name || '')) : '';
    const right = rightHas ? display((e.relPath || '') + (e.name || '')) : '';
    const clickable = (status === 'modified' && !e.isDir) ? 'row-clickable' : '';
    return `<div class="fd-row ${status} ${clickable} ${e.isDir ? 'is-dir' : ''}"
                 data-status="${status}"
                 data-left="${escapeHtml(e.leftPath || '')}"
                 data-right="${escapeHtml(e.rightPath || '')}">` +
             `<div class="fd-cell">${left}<span class="fd-icon">${ICONS[status]}</span></div>` +
             `<div class="fd-cell">${right}</div>` +
           `</div>`;
  }).join('');

  // Click a modified file row → open a file-diff tab
  body.querySelectorAll('.fd-row.row-clickable').forEach(row => {
    row.addEventListener('click', () => {
      const left  = row.dataset.left;
      const right = row.dataset.right;
      if (left && right) createFileDiffTab(left, right);
    });
  });
}

// ── Menu / picker entry points ───────────────────────────────────────────
async function compareFilesFlow() {
  const a = await window.electronAPI.openDialog({
    title: 'Compare — pick LEFT file',
    properties: ['openFile'],
    filters: [{ name: 'All Files', extensions: ['*'] }],
  });
  if (a.canceled || !a.filePaths?.length) return;
  const b = await window.electronAPI.openDialog({
    title: 'Compare — pick RIGHT file',
    properties: ['openFile'],
    filters: [{ name: 'All Files', extensions: ['*'] }],
  });
  if (b.canceled || !b.filePaths?.length) return;
  createFileDiffTab(a.filePaths[0], b.filePaths[0]);
}

async function compareWithSavedFlow() {
  const tab = getActiveTab();
  if (!tab || tab.type !== 'editor' || !tab.filePath) {
    showToast('Open a saved file first to use Compare with Saved');
    return;
  }
  const b = await window.electronAPI.openDialog({
    title: `Compare "${tab.name}" with…`,
    properties: ['openFile'],
    filters: [{ name: 'All Files', extensions: ['*'] }],
  });
  if (b.canceled || !b.filePaths?.length) return;
  createFileDiffTab(tab.filePath, b.filePaths[0]);
}

async function compareFoldersFlow() {
  const a = await window.electronAPI.openDialog({
    title: 'Compare folders — pick LEFT folder',
    properties: ['openDirectory'],
  });
  if (a.canceled || !a.filePaths?.length) return;
  const b = await window.electronAPI.openDialog({
    title: 'Compare folders — pick RIGHT folder',
    properties: ['openDirectory'],
  });
  if (b.canceled || !b.filePaths?.length) return;
  createFolderDiffTab(a.filePaths[0], b.filePaths[0]);
}

// Cleanup: when a diff tab is closed, dispose its models
function disposeDiffTab(tab) {
  if (tab?.type !== 'diff') return;
  try { tab.diff.originalModel?.dispose(); } catch {}
  try { tab.diff.modifiedModel?.dispose(); } catch {}
}

// ── Quick Compare ─────────────────────────────────────────────────────────────
// Launched from the tab right-click menu ("Compare"). Uses the current buffer
// content (even if unsaved) as the LEFT side. User picks a file for the RIGHT
// side via a file dialog; cancelling the dialog opens an editable right pane
// where they can paste arbitrary text.

async function quickCompareFlow(tabId) {
  const tab = tabs.find(t => t.id === tabId);
  if (!tab) return;

  // Capture left content from the live editor buffer when possible.
  const leftContent = (tab.id === activeTabId && editor && tab.type === 'editor')
    ? editor.getValue()
    : (tab.content || '');
  const leftName = tab.name;
  const leftLang = tab.language || 'plaintext';

  // Prompt user for a right-side file.
  const res = await window.electronAPI.openDialog({
    title: 'Select file to compare against (Cancel = paste custom text)',
    properties: ['openFile'],
  });

  let rightContent = '', rightPath = null, rightName = 'Custom text', rightLang = leftLang, isCustom = true;

  if (!res.canceled && res.filePaths?.length) {
    rightPath = res.filePaths[0];
    const r = await window.electronAPI.readFile(rightPath);
    rightContent = r.success ? r.content : '';
    rightName    = rightPath.split(/[\\/]/).pop();
    rightLang    = detectLanguage(rightPath);
    isCustom     = false;
  }

  tabCounter++;
  const qdTab = {
    id: tabCounter, name: `↔ ${leftName}`,
    filePath: null, content: '', dirty: false,
    language: leftLang, encoding: 'UTF-8', eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'quick-diff', encrypted: false, protectedBy: null,
    diff: {
      leftContent, leftName, leftLang,
      rightPath, rightContent, rightName, rightLang, isCustom,
      originalModel: null, modifiedModel: null, mounted: false,
    },
  };
  tabs.push(qdTab);
  activateTab(qdTab.id);
  renderTabs();
}

// Compare the current tab's (possibly unsaved) buffer against clipboard text.
// Solves "I have two unsaved files open — how do I diff them?": copy one
// (Ctrl+A, Ctrl+C), switch to the other tab, then Compare with Clipboard.
async function compareWithClipboardFlow(tabId) {
  const tab = tabs.find(t => t.id === tabId);
  if (!tab || tab.type !== 'editor') { showToast('Open a text tab to compare'); return; }
  let clip = '';
  try { clip = await navigator.clipboard.readText(); }
  catch { showToast('Could not read clipboard'); return; }
  if (!clip) { showToast('Clipboard is empty — copy some text first'); return; }

  const leftContent = (tab.id === activeTabId && editor && tab.type === 'editor')
    ? editor.getValue()
    : (tab.content || '');
  const leftName = tab.name;
  const leftLang = tab.language || 'plaintext';

  tabCounter++;
  const qdTab = {
    id: tabCounter, name: `↔ ${leftName}`,
    filePath: null, content: '', dirty: false,
    language: leftLang, encoding: 'UTF-8', eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'quick-diff', encrypted: false, protectedBy: null,
    diff: {
      leftContent, leftName, leftLang,
      rightPath: null, rightContent: clip, rightName: 'Clipboard',
      rightLabel: 'Clipboard', rightLang: leftLang, isCustom: true,
      originalModel: null, modifiedModel: null, mounted: false,
    },
  };
  tabs.push(qdTab);
  activateTab(qdTab.id);
  renderTabs();
}

function mountQuickDiffTab(tab) {
  const d = tab.diff;
  document.getElementById('quick-diff-left-label').textContent  = d.leftName;
  document.getElementById('quick-diff-right-label').textContent = d.rightLabel || (d.isCustom ? 'Custom text' : d.rightName);
  const hint = document.getElementById('quick-diff-hint');
  if (hint) hint.classList.toggle('hidden', !d.isCustom);

  const host = document.getElementById('quick-diff-monaco');
  // Recreate the diff editor fresh on every mount. Reusing a single shared
  // instance across mounts intermittently renders BLANK on both sides: once the
  // host has been hidden (tab switch / tab close) and reshown, the reused editor
  // keeps a stale zero-size layout and never repaints its models. A fresh editor
  // per mount sidesteps that entirely.
  try { quickDiffEditor?.setModel(null); } catch {}
  try { quickDiffEditor?.dispose(); } catch {}
  quickDiffEditor = monaco.editor.createDiffEditor(host, {
    automaticLayout: true,
    renderSideBySide: quickDiffSideBySide,
    fontSize: 13,
    fontFamily: "'Cascadia Code', 'Fira Code', Consolas, monospace",
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
  });

  // Dispose old models before creating new ones.
  try { d.originalModel?.dispose(); } catch {}
  try { d.modifiedModel?.dispose(); } catch {}
  d.originalModel = monaco.editor.createModel(d.leftContent,   d.leftLang);
  d.modifiedModel = monaco.editor.createModel(d.rightContent,  d.rightLang);

  quickDiffEditor.setModel({ original: d.originalModel, modified: d.modifiedModel });
  // Left is always read-only. Right is editable only in custom (paste) mode.
  quickDiffEditor.getOriginalEditor().updateOptions({ readOnly: true });
  quickDiffEditor.getModifiedEditor().updateOptions({ readOnly: !d.isCustom });

  // Force a layout once the host has real dimensions. A double rAF (plus a
  // safety timeout) guarantees we run after the container's `hidden` class was
  // removed and the browser has laid it out.
  const relayout = () => { try { quickDiffEditor.layout(); } catch {} };
  requestAnimationFrame(() => requestAnimationFrame(relayout));
  setTimeout(relayout, 60);
  d.mounted = true;
}

function disposeQuickDiffTab(tab) {
  try { tab.diff?.originalModel?.dispose(); } catch {}
  try { tab.diff?.modifiedModel?.dispose(); } catch {}
}

function setupQuickDiffToolbar() {
  document.getElementById('btn-qdiff-inline')?.addEventListener('click', () => {
    if (!quickDiffEditor) return;
    quickDiffSideBySide = !quickDiffSideBySide;
    quickDiffEditor.updateOptions({ renderSideBySide: quickDiffSideBySide });
    document.getElementById('btn-qdiff-inline').textContent =
      quickDiffSideBySide ? '≡ Inline' : '⫶ Side-by-side';
  });
  document.getElementById('btn-qdiff-prev')?.addEventListener('click', () =>
    quickDiffEditor?.getModifiedEditor()?.trigger('toolbar', 'editor.action.diffReview.prev', null));
  document.getElementById('btn-qdiff-next')?.addEventListener('click', () =>
    quickDiffEditor?.getModifiedEditor()?.trigger('toolbar', 'editor.action.diffReview.next', null));
  document.getElementById('btn-qdiff-refresh')?.addEventListener('click', async () => {
    const tab = tabs.find(t => t.id === activeTabId && t.type === 'quick-diff');
    if (!tab) return;
    if (!tab.diff.isCustom && tab.diff.rightPath) {
      const r = await window.electronAPI.readFile(tab.diff.rightPath);
      if (r.success) tab.diff.modifiedModel?.setValue(r.content);
    } else {
      // Custom mode: capture whatever the user typed and force a re-render.
      if (quickDiffEditor) quickDiffEditor.layout();
    }
  });
}

// Tiny HTML escape — shared with other helpers but defined here too to keep
// the diff module self-contained.
function escapeHtml(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g, c =>
    ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
}

// createWhiteboardTab now lives in src/whiteboard-helpers.js — global.

function activateTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  const prev = getActiveTab();
  if (prev && editor && prev.type !== 'game' && prev.type !== 'whiteboard'
                     && prev.type !== 'drawio'
                     && prev.type !== 'diff' && prev.type !== 'folder-diff'
                     && prev.type !== 'quick-diff') {
    prev.viewState = editor.saveViewState();
    prev.content = editor.getValue();
    prev.previewOpen = previewOpen; // persist per-tab preview visibility
    // Clear find / mark decorations from the OUTGOING tab's model so
    // they don't get stranded there. Without this, switching from a
    // tab where you Found "foo" would leave "foo" highlights painted
    // on that tab's model forever (or until the next Find action on
    // it). We use model.deltaDecorations directly because the editor
    // is about to swap its model — `editor.deltaDecorations` would
    // operate on the wrong target.
    if (prev.model) {
      try {
        if (searchDecorations.length) prev.model.deltaDecorations(searchDecorations, []);
        for (const ids of markDecorations) {
          if (ids && ids.length) prev.model.deltaDecorations(ids, []);
        }
      } catch (e) { console.warn('[tab-switch] decoration cleanup failed', e); }
    }
    searchDecorations = [];
    markDecorations = [];
    setFindStatus('');
  }
  activeTabId = id;

  const gameContainer  = document.getElementById('game-container');
  const wbContainer    = document.getElementById('whiteboard-container');
  const wbFrame        = document.getElementById('whiteboard-frame');
  const dwContainer    = document.getElementById('drawio-container');
  const dwFrame        = document.getElementById('drawio-frame-host');
  const monacoEl       = document.getElementById('monaco-editor');
  const gameFrame      = document.getElementById('game-frame');
  const diffContainer  = document.getElementById('diff-container');
  const fdiffContainer = document.getElementById('folder-diff-container');
  const qdiffContainer = document.getElementById('quick-diff-container');

  // Always hide all special containers first, then show the right one
  gameContainer.classList.add('hidden');
  wbContainer.classList.add('hidden');
  dwContainer?.classList.add('hidden');
  diffContainer?.classList.add('hidden');
  fdiffContainer?.classList.add('hidden');
  qdiffContainer?.classList.add('hidden');

  if (tab.type === 'diff') {
    monacoEl.style.display = 'none';
    diffContainer.classList.remove('hidden');
    if (previewOpen) closePreview();
    mountFileDiffTab(tab);
  } else if (tab.type === 'folder-diff') {
    monacoEl.style.display = 'none';
    fdiffContainer.classList.remove('hidden');
    if (previewOpen) closePreview();
    mountFolderDiffTab(tab);
  } else if (tab.type === 'game') {
    // Show game container, hide monaco editor
    monacoEl.style.display = 'none';
    gameContainer.classList.remove('hidden');
    // Load launcher only if not already loaded
    if (!gameFrame.src || !gameFrame.src.endsWith('launcher.html')) {
      const base = window.location.href.replace(/[^/]*$/, '');
      gameFrame.src = base + 'games/launcher.html';
    }
    if (previewOpen) closePreview();
  } else if (tab.type === 'whiteboard') {
    // Show whiteboard container, hide monaco editor
    monacoEl.style.display = 'none';
    wbContainer.classList.remove('hidden');
    if (previewOpen) closePreview();

    // The whiteboard iframe is a single SHARED instance reused across every
    // whiteboard tab. When switching tabs we MUST always send wb-load —
    // even for an empty / freshly-created tab — otherwise the iframe keeps
    // displaying whichever scene was loaded last (bug: switching to an
    // unsaved whiteboard-2 would show whiteboard-1's drawing).
    //
    // Empty content → push a blank v2 envelope so the iframe resets to a
    // clean canvas instead of keeping stale state.
    const BLANK_WB = JSON.stringify({
      __wb__: true, version: 2, source: 'excalidraw',
      elements: [], appState: {}, files: {}
    });
    const contentToLoad = (tab.content && tab.content.trim()) ? tab.content : BLANK_WB;

    // Lazy-load the iframe on first activation
    const base = window.location.href.replace(/[^/]*$/, '');
    const wbSrc = base + 'whiteboard.html';
    if (!wbFrame.src || !wbFrame.src.includes('whiteboard.html')) {
      wbReady = false;
      wbPendingContent = contentToLoad;
      wbFrame.src = wbSrc;
    } else if (wbReady) {
      sendToWhiteboard({ type: 'wb-load', content: contentToLoad });
      sendToWhiteboard({ type: 'wb-theme', dark: isDarkMode });
    } else {
      wbPendingContent = contentToLoad;
    }
  } else if (tab.type === 'drawio') {
    // Show drawio container, hide everything else
    monacoEl.style.display = 'none';
    dwContainer.classList.remove('hidden');
    if (previewOpen) closePreview();

    // Same shared-iframe pattern as whiteboard. Lazy-load on first activation;
    // on subsequent activations just push the new diagram XML.
    const contentToLoad = tab.content || '';
    const base = window.location.href.replace(/[^/]*$/, '');
    const dwSrc = base + 'drawio.html';
    if (!dwFrame.src || !dwFrame.src.includes('drawio.html')) {
      drawioReady = false;
      drawioPendingContent = contentToLoad;
      dwFrame.src = dwSrc;
    } else if (drawioReady) {
      sendToDrawio({ type: 'dw-load', content: contentToLoad });
      sendToDrawio({ type: 'dw-theme', dark: isDarkMode });
    } else {
      drawioPendingContent = contentToLoad;
    }
  } else if (tab.type === 'quick-diff') {
    monacoEl.style.display = 'none';
    qdiffContainer.classList.remove('hidden');
    if (previewOpen) closePreview();
    mountQuickDiffTab(tab);
    // Hand keyboard focus to the diff editor so shortcuts (Ctrl+W, etc.)
    // unambiguously target THIS tab. Clicking a compare tab already lands focus
    // inside the diff editor and behaves correctly; keyboard-driven activation
    // (e.g. Ctrl+Shift+T reopen) otherwise leaves focus on <body>, which is the
    // one observable difference that made Ctrl+W act on the previously-focused
    // tab. Deferred with a double rAF to match mountQuickDiffTab's relayout so
    // focus lands after the editor has real dimensions.
    requestAnimationFrame(() => requestAnimationFrame(() => {
      try { quickDiffEditor?.focus(); } catch {}
    }));
  } else {
    monacoEl.style.display = '';
    if (editor) {
      editor.setModel(tab.model);
      if (tab.viewState) editor.restoreViewState(tab.viewState);
      editor.focus();
    }
    // Large-file safe mode: apply restricted options for big files, restore
    // normal options when returning from one.
    if (tab.largeFileSafeMode) {
      _applyLargeFileMode();
    } else if (prev?.largeFileSafeMode) {
      _restoreNormalEditorMode();
    }
    // Per-tab preview state: restore whatever this tab had.
    // tab.previewOpen === true  → user opened it on this tab, show it
    // tab.previewOpen === false → user closed it (or never opened), hide it
    // tab.previewOpen === undefined → first visit; auto-open only for mermaid
    if (tab.previewOpen === true) {
      if (!previewOpen) openPreview();
      else if (isPreviewable(tab.language)) updatePreview();
      else showPreviewPlaceholder();
    } else if (tab.language === 'mermaid' && tab.previewOpen === undefined) {
      openPreview(); // first-visit auto-open for .mmd files
    } else {
      if (previewOpen) closePreview();
    }
    // Show/hide Mermaid toolbar based on active language
    updateMermaidToolbar(tab.language === 'mermaid');
  }

  renderTabs();
  // Keep the active tab visible when tab overflow is present (e.g. clicking +).
  const activeEl = tabBar?.querySelector('.tab.active');
  if (activeEl) activeEl.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  updateTabScrollButtons();
  // Re-assert editor focus after tab button clicks (new/open) so caret lands in the active tab.
  if (tab.type !== 'game' && tab.type !== 'whiteboard' && tab.type !== 'drawio'
                     && tab.type !== 'diff' && tab.type !== 'folder-diff'
                     && tab.type !== 'quick-diff') {
    requestAnimationFrame(() => editor?.focus());
  }
  updateStatusBar();
  updateTitle();
  updateLanguageStatus();
  updateEncryptToolbarButton();
  updateEncryptionStatusIndicator();
  updateActiveGitRepo();        // Git status follows the active tab's repo
  // LSP — start (or sync) the language server for this tab's language
  try { window.NotePPLsp?.onTabActivated(tab); } catch (e) { console.warn('[lsp]', e); }
  // Spell-check: clear any squiggles from the previous tab if the new
  // one isn't eligible (e.g. user just switched from .md to .js), then
  // re-scan the new tab's visible range.
  _clearSpellDecorations();
  if (spellEnabled && isSpellEligibleTab(tab)) scheduleSpellScan();
  // File tree: hide it when the active tab's file lives outside the
  // workspace root, restore it when the file IS under the root. Keeps
  // the tree from sticking around for unrelated tabs.
  updateFileTreeForActiveTab();
  // Icons library toolbar button: whiteboard tab only. When leaving a
  // whiteboard tab with the panel open, hide the panel (it makes no
  // sense over the editor / diff view).
  updateIconsButtonVisibility();
  // First-run contextual tips — fired here because activateTab is the
  // one path all tab-visibility transitions flow through. Each individual
  // tip is guarded by a localStorage marker (see showTipOnce).
  try {
    if (tab.type === 'whiteboard') {
      maybeFireContextualTip('whiteboard-open');
    } else if (tab.type === 'editor') {
      const lang = tab.language || '';
      if (lang === 'markdown' || lang === 'html' || lang === 'mermaid') {
        maybeFireContextualTip('previewable-file', { lang });
      }
      // Git tip — active file lives in a repo we already detected.
      if (tab.filePath && typeof gitFileToRepo !== 'undefined' && gitFileToRepo?.get?.(tab.filePath)) {
        maybeFireContextualTip('git-repo-file');
      }
      // Python + no LSP — only fire if the LSP pill is showing the
      // "missing" state (className has 'lsp-missing' etc.)
      if (lang === 'python') {
        const pill = document.getElementById('status-lsp');
        if (pill && /missing|install/i.test(pill.textContent || '')) {
          maybeFireContextualTip('python-no-lsp');
        }
      }
    }
  } catch (e) { console.warn('[tips] trigger failed', e); }
}

// Show the "🎨 Icons" toolbar button only on whiteboard tabs, and hide
// the icons panel automatically when the active tab isn't a whiteboard.
function updateIconsButtonVisibility() {
  const btn = document.getElementById('btn-icons');
  const panel = document.getElementById('icons-panel');
  const resize = document.getElementById('icons-panel-resize');
  const tab = getActiveTab();
  const isWb = !!(tab && tab.type === 'whiteboard');
  if (btn) btn.classList.toggle('hidden', !isWb);
  if (!isWb) {
    panel?.classList.add('hidden');
    resize?.classList.add('hidden');
  }
}

// Reflect active-tab encryption state on the toolbar 🔒 button.
function updateEncryptToolbarButton() {
  const btn = document.getElementById('btn-encrypt');
  if (!btn) return;
  const tab = getActiveTab();
  const isEnc = !!(tab && tab.encrypted);
  btn.classList.toggle('active', isEnc);
  btn.title = isEnc ? 'Remove encryption from this file' : 'Encrypt this file';
}

// ── Tab color palette ─────────────────────────────────────────────────────
const TAB_COLORS = [
  { name: 'Red',    value: '#e05555' },
  { name: 'Orange', value: '#e08030' },
  { name: 'Yellow', value: '#c0a020' },
  { name: 'Green',  value: '#3dab60' },
  { name: 'Teal',   value: '#2090a0' },
  { name: 'Blue',   value: '#4488dd' },
  { name: 'Purple', value: '#8060c0' },
  { name: 'Pink',   value: '#c060a0' },
];

function togglePinTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  tab.pinned = !tab.pinned;
  renderTabs();
  scheduleAutoSave();
}

function setTabColor(id, color) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  tab.color = color || null;
  renderTabs();
  scheduleAutoSave();
}

function renderTabs() {
  tabBar.innerHTML = '';
  // Pinned tabs always appear first in the bar, preserving relative order within each group.
  const ordered = [...tabs.filter(t => t.pinned), ...tabs.filter(t => !t.pinned)];
  ordered.forEach(tab => {
    const el = document.createElement('div');
    el.className = 'tab' + (tab.id === activeTabId ? ' active' : '') + (tab.dirty ? ' dirty' : '')
                       + (tab.pinned ? ' tab-pinned' : '');
    el.dataset.id = tab.id;

    // Color accent bar — 3 px stripe on the left edge of the tab
    if (tab.color) {
      el.style.setProperty('--tab-accent-color', tab.color);
      el.classList.add('tab-colored');
    }

    // Pin indicator — shown before the file icon for pinned tabs
    if (tab.pinned) {
      const pin = document.createElement('span');
      pin.className = 'tab-pin-icon';
      pin.textContent = '📌';
      pin.title = 'Pinned — right-click to unpin';
      el.appendChild(pin);
    }

    // Whiteboard / drawio tabs use only their pill badge — skip the emoji
    // icon so we don't render a tofu/blank-box for systems missing the glyph.
    if (tab.type !== 'whiteboard' && tab.type !== 'drawio') {
      const icon = document.createElement('span');
      icon.className = 'tab-icon';
      icon.textContent = tab.type === 'game' ? '🎮' : getFileEmoji(tab.name);
      el.appendChild(icon);
    }

    const name = document.createElement('span');
    name.className = 'tab-name';
    name.title = tab.filePath || tab.name;
    name.textContent = tab.name;

    const close = document.createElement('button');
    close.className = 'tab-close';
    close.textContent = '×';
    // Pinned tabs: × is hidden; middle-click is also blocked below
    if (tab.pinned) close.style.display = 'none';
    close.addEventListener('click', (e) => { e.stopPropagation(); closeTab(tab.id); });

    // Pill badge on whiteboard tabs so the type is unmistakable even when the
    // filename is a generic .json.
    if (tab.type === 'whiteboard') {
      const badge = document.createElement('span');
      badge.className = 'tab-wb-badge';
      badge.textContent = 'wb';
      el.appendChild(badge);
    }
    // Pill badge on draw.io tabs (amber to differ from wb's blue)
    if (tab.type === 'drawio') {
      const badge = document.createElement('span');
      badge.className = 'tab-dw-badge';
      badge.textContent = 'dw';
      el.appendChild(badge);
    }
    // Diff / folder-diff badge
    if (tab.type === 'diff' || tab.type === 'folder-diff') {
      const badge = document.createElement('span');
      badge.className = 'tab-diff-badge';
      badge.textContent = tab.type === 'diff' ? 'diff' : 'fdiff';
      el.appendChild(badge);
    }
    // Lock badge on encrypted text tabs
    if (tab.encrypted) {
      const lock = document.createElement('span');
      lock.className = 'tab-enc-badge';
      lock.textContent = '🔒';
      lock.title = 'Encrypted file';
      el.appendChild(lock);
    }
    el.appendChild(name);
    el.appendChild(close);
    el.addEventListener('click', () => activateTab(tab.id));
    el.addEventListener('mousedown', e => {
      if (e.button === 1) { e.preventDefault(); if (!tab.pinned) closeTab(tab.id); return; }
      if (e.button === 0 && !e.target.classList.contains('tab-close')) {
        if (!tab.pinned) initTabDrag(e, tab.id, el); // pinned tabs can't be dragged
      }
    });
    el.addEventListener('contextmenu', (e) => { e.preventDefault(); showTabContextMenu(e, tab.id); });
    tabBar.appendChild(el);
  });
  updateTabScrollButtons();
}

function updateTabScrollButtons() {
  const left = document.getElementById('tab-scroll-left');
  const right = document.getElementById('tab-scroll-right');
  if (!left || !right || !tabBar) return;
  const overflow = tabBar.scrollWidth - tabBar.clientWidth > 1;
  if (!overflow) {
    left.disabled = true;
    right.disabled = true;
    left.style.display = 'none';
    right.style.display = 'none';
    return;
  }
  left.style.display = '';
  right.style.display = '';
  left.disabled = tabBar.scrollLeft <= 0;
  right.disabled = tabBar.scrollLeft + tabBar.clientWidth >= tabBar.scrollWidth - 1;
}

function initTabDrag(e, tabId, tabEl) {
  const startX  = e.clientX;
  const offsetX = e.clientX - tabEl.getBoundingClientRect().left;
  let ghost = null, indicator = null, started = false, dropIdx = -1;
  let lastEv = null;
  let autoScrollRAF = 0;

  function autoScrollTick() {
    autoScrollRAF = 0;
    if (!started || !lastEv) return;
    const barRect = tabBar.getBoundingClientRect();
    const edge = 40;
    const maxSpeed = 18;
    let dx = 0;
    if (lastEv.clientX < barRect.left + edge) {
      const t = (barRect.left + edge - lastEv.clientX) / edge;
      dx = -Math.ceil(maxSpeed * Math.min(1, t));
    } else if (lastEv.clientX > barRect.right - edge) {
      const t = (lastEv.clientX - (barRect.right - edge)) / edge;
      dx = Math.ceil(maxSpeed * Math.min(1, t));
    }
    if (dx !== 0) {
      const before = tabBar.scrollLeft;
      tabBar.scrollLeft = Math.max(0, Math.min(tabBar.scrollWidth - tabBar.clientWidth, before + dx));
      if (tabBar.scrollLeft !== before) {
        onMove(lastEv);
      }
    }
    if (started) autoScrollRAF = requestAnimationFrame(autoScrollTick);
  }

  function onMove(ev) {
    lastEv = ev;
    if (!started) {
      if (Math.abs(ev.clientX - startX) < 5) return;
      started = true;
      document.body.classList.add('tab-dragging');

      ghost = tabEl.cloneNode(true);
      Object.assign(ghost.style, {
        position: 'fixed', pointerEvents: 'none', opacity: '0.8',
        zIndex: '9999', height: '23px', boxShadow: '0 2px 8px rgba(0,0,0,0.3)',
      });
      document.body.appendChild(ghost);

      indicator = document.createElement('div');
      Object.assign(indicator.style, {
        position: 'fixed', width: '2px', background: 'var(--accent,#0078d4)',
        pointerEvents: 'none', zIndex: '9999', borderRadius: '1px',
      });
      document.body.appendChild(indicator);
      tabEl.style.opacity = '0.4';

      if (!autoScrollRAF) autoScrollRAF = requestAnimationFrame(autoScrollTick);
    }

    const barRect  = tabBar.getBoundingClientRect();
    const tabRect  = tabEl.getBoundingClientRect();
    ghost.style.left  = (ev.clientX - offsetX) + 'px';
    ghost.style.top   = barRect.top + 'px';
    ghost.style.width = tabRect.width + 'px';

    // Find where the tab would be inserted (clamp pointer x to bar bounds so
    // edge-drags resolve to first/last slot instead of falling through).
    const probeX = Math.max(barRect.left, Math.min(barRect.right, ev.clientX));
    const allTabs = Array.from(tabBar.querySelectorAll('.tab'));
    dropIdx = allTabs.length;
    for (let i = 0; i < allTabs.length; i++) {
      const r = allTabs[i].getBoundingClientRect();
      if (probeX < r.left + r.width / 2) { dropIdx = i; break; }
    }

    // Position the drop indicator (clamped to visible bar so it doesn't leak
    // over the scroll-arrow buttons).
    let ix;
    if (dropIdx < allTabs.length) {
      ix = allTabs[dropIdx].getBoundingClientRect().left;
    } else {
      const last = allTabs[allTabs.length - 1];
      ix = last ? last.getBoundingClientRect().right : barRect.left;
    }
    ix = Math.max(barRect.left, Math.min(barRect.right, ix));
    indicator.style.left   = (ix - 1) + 'px';
    indicator.style.top    = barRect.top + 'px';
    indicator.style.height = barRect.height + 'px';
  }

  function onUp() {
    document.removeEventListener('mousemove', onMove);
    document.removeEventListener('mouseup', onUp);
    if (autoScrollRAF) { cancelAnimationFrame(autoScrollRAF); autoScrollRAF = 0; }
    ghost?.remove();
    indicator?.remove();
    document.body.classList.remove('tab-dragging');
    tabEl.style.opacity = '';

    if (started && dropIdx >= 0) {
      const fromIdx = tabs.findIndex(t => t.id === tabId);
      let toIdx = dropIdx;
      if (toIdx !== fromIdx && toIdx !== fromIdx + 1) {
        const [moved] = tabs.splice(fromIdx, 1);
        if (toIdx > fromIdx) toIdx--;
        tabs.splice(toIdx, 0, moved);
        renderTabs();
      }
    }
  }

  document.addEventListener('mousemove', onMove);
  document.addEventListener('mouseup', onUp);
  e.preventDefault();
}

// ── Reopen-closed-tab stack (Ctrl+Shift+T) ──────────────────────────────
// LIFO of snapshots taken at the moment a tab is confirmed to close
// (after the "Save?" prompt, before splice). Reopen restores editor,
// whiteboard, and drawio tabs — never compare views (they're transient)
// and never the Dev Arcade (reopen it from the toolbar). Cap keeps the
// stack from growing unbounded across a long session.
const closedTabStack = [];
const CLOSED_TAB_STACK_MAX = 20;
function pushClosedTab(tab) {
  if (!tab) return;
  // File/folder compare tabs reference on-disk paths (re-openable from the
  // menu); games are trivial to reopen from the toolbar.
  if (tab.type === 'diff' || tab.type === 'folder-diff' || tab.type === 'game') return;
  // Quick-compare tabs (Compare / Compare with Clipboard) hold their content
  // in-memory, so stash a serialisable snapshot to allow Ctrl+Shift+T reopen.
  if (tab.type === 'quick-diff') {
    const d = tab.diff || {};
    let leftContent = d.leftContent || '';
    let rightContent = d.rightContent || '';
    // Capture whatever is live in the diff editor (the right pane is editable
    // in custom/clipboard mode, so the user may have changed it).
    try { if (d.originalModel) leftContent  = d.originalModel.getValue(); } catch {}
    try { if (d.modifiedModel) rightContent = d.modifiedModel.getValue(); } catch {}
    closedTabStack.push({
      type: 'quick-diff', name: tab.name,
      diff: {
        leftContent, leftName: d.leftName, leftLang: d.leftLang || 'plaintext',
        rightPath: d.rightPath || null, rightContent,
        rightName: d.rightName, rightLabel: d.rightLabel || null,
        rightLang: d.rightLang || 'plaintext', isCustom: !!d.isCustom,
      },
    });
    while (closedTabStack.length > CLOSED_TAB_STACK_MAX) closedTabStack.shift();
    return;
  }
  let content = tab.content || '';
  if (tab.type === 'editor' && tab.model) {
    try { content = tab.model.getValue(); } catch {}
  }
  closedTabStack.push({
    type: tab.type,
    name: tab.name,
    filePath: tab.filePath || null,
    content,
    language: tab.language || null,
  });
  while (closedTabStack.length > CLOSED_TAB_STACK_MAX) closedTabStack.shift();
}
function reopenClosedTab() {
  const entry = closedTabStack.pop();
  if (!entry) { showToast('Nothing to reopen'); return; }
  // Quick-compare tabs are recreated from their stashed snapshot.
  if (entry.type === 'quick-diff' && entry.diff) {
    try {
      tabCounter++;
      const d = entry.diff;
      const qdTab = {
        id: tabCounter, name: `↔ ${d.leftName || 'compare'}`,
        filePath: null, content: '', dirty: false,
        language: d.leftLang || 'plaintext', encoding: 'UTF-8', eol: 'Windows (CR LF)',
        model: null, viewState: null,
        type: 'quick-diff', encrypted: false, protectedBy: null,
        diff: { ...d, originalModel: null, modifiedModel: null, mounted: false },
      };
      tabs.push(qdTab);
      activateTab(qdTab.id);
      renderTabs();
    } catch (err) {
      console.warn('[reopen-tab] quick-diff failed', err);
      showToast('Failed to reopen compare tab');
    }
    return;
  }
  // If the tab had a real file backing it, openFile handles everything —
  // it also gives us de-duplication if that file is already open in
  // another tab (focuses the existing one instead of duplicating).
  if (entry.filePath) { openFile([entry.filePath]); return; }
  // Unsaved tab — recreate with its stashed content.
  try {
    if (entry.type === 'whiteboard') { createWhiteboardTab(null, entry.content || ''); return; }
    if (entry.type === 'drawio')     { createDrawioTab(null, entry.content || ''); return; }
    // Default: editor.
    const tab = createTab(null, entry.content || '');
    if (tab && entry.language) { tab.language = entry.language; try { monaco.editor.setModelLanguage(tab.model, entry.language); } catch {} }
  } catch (err) {
    console.warn('[reopen-tab] failed', err);
    showToast('Failed to reopen tab');
  }
}

const _closingTabs = new Set();
async function closeTab(id) {
  // Re-entrancy guard. While a tab's Save/close prompt is awaiting, ignore
  // repeat close requests for the SAME tab. Without this, pressing Ctrl+W or
  // clicking the tab's × several times in quick succession (the native dialog
  // can appear behind the window, so it's easy to double-fire) spawns
  // duplicate "Save?" dialogs. Worse: the later invocations still hold the
  // same `tab` reference, so they run pushClosedTab AFTER the first one has
  // disposed the model — getValue() throws and an EMPTY snapshot is pushed
  // onto the reopen stack, so Ctrl+Shift+T then restores blank tabs.
  if (_closingTabs.has(id)) return;
  _closingTabs.add(id);
  try {
    return await _closeTabInner(id);
  } finally {
    _closingTabs.delete(id);
  }
}
async function _closeTabInner(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  if (tab.pinned) { showToast('Unpin the tab before closing it'); return; }
  if (tab.type === 'diff' || tab.type === 'folder-diff' || tab.type === 'quick-diff') {
    // Compare tabs: no save prompt, just dispose models and remove
    if (tab.type === 'quick-diff') pushClosedTab(tab); // stash before models are disposed
    if (tab.type === 'diff')       disposeDiffTab(tab);
    if (tab.type === 'quick-diff') disposeQuickDiffTab(tab);
    const idx = tabs.findIndex(t => t.id === id);
    tabs.splice(idx, 1);
    if (tabs.length === 0) createTab();
    else activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
    renderTabs();
    return;
  }
  if (tab.type === 'game') {
    // Game tab: no model, just remove
    const idx = tabs.findIndex(t => t.id === id);
    gameTabId = null;
    // Unload the iframe to free memory
    const gameFrame = document.getElementById('game-frame');
    if (gameFrame) gameFrame.src = '';
    document.getElementById('game-container').classList.add('hidden');
    document.getElementById('monaco-editor').style.display = '';
    tabs.splice(idx, 1);
    if (tabs.length === 0) createTab();
    else activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
    renderTabs();
    return;
  }
  if (tab.type === 'whiteboard') {
    // Flush any pending legacy debounced write (only present for sessions
    // restored from older whiteboards whose AppData backing file we still
    // honour). New whiteboards never schedule disk writes.
    clearTimeout(wbFileSaveTimers.get(tab.id));
    wbFileSaveTimers.delete(tab.id);
    // Identical UX to text "new N" tabs: if there are unsaved changes, ask.
    // "Save" with no filePath → Save As. With a real filePath → write to it.
    if (tab.dirty) {
      const r = await window.electronAPI.messageDialog({
        type: 'question', title: 'Save',
        message: `Save "${tab.name}"?`,
        buttons: ['Save', "Don't Save", 'Cancel'],
        defaultId: 0, cancelId: 2
      });
      if (r.response === 2) return;
      if (r.response === 0) {
        // Legacy auto-backed whiteboards (pre-v1.5.x sessions) live under
        // the per-user app data folder (Windows: %AppData%\notepp\Whiteboards,
        // macOS: ~/Library/Application Support/notepp/Whiteboards,
        // Linux: ~/.config/notepp/Whiteboards). For those, force Save As so
        // the user picks a real location rather than silently writing back
        // to the hidden app-data copy.
        const isLegacyAutoBacking =
          tab.filePath && /[\\/]Whiteboards[\\/]whiteboard-\d+\.json$/i.test(tab.filePath);
        const ok = await saveTabFile(tab, /* forceAs */ isLegacyAutoBacking);
        if (!ok) return;
      }
    }
    pushClosedTab(tab);
    const idx = tabs.findIndex(t => t.id === id);
    tabs.splice(idx, 1);
    // If no more whiteboard tabs exist, unload the iframe to free memory
    if (!tabs.some(t => t.type === 'whiteboard')) {
      const wbFrame = document.getElementById('whiteboard-frame');
      if (wbFrame) wbFrame.src = '';
      wbReady = false;
      wbPendingContent = null;
    }
    document.getElementById('whiteboard-container').classList.add('hidden');
    document.getElementById('monaco-editor').style.display = '';
    if (tabs.length === 0) createTab();
    else activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
    renderTabs();
    return;
  }
  if (tab.type === 'drawio') {
    if (tab.dirty) {
      const r = await window.electronAPI.messageDialog({
        type: 'question', title: 'Save',
        message: `Save "${tab.name}"?`,
        buttons: ['Save', "Don't Save", 'Cancel'],
        defaultId: 0, cancelId: 2,
      });
      if (r.response === 2) return;
      if (r.response === 0) { const ok = await saveTabFile(tab); if (!ok) return; }
    }
    pushClosedTab(tab);
    const idx = tabs.findIndex(t => t.id === id);
    tabs.splice(idx, 1);
    if (!tabs.some(t => t.type === 'drawio')) {
      const dwFrame = document.getElementById('drawio-frame-host');
      if (dwFrame) dwFrame.src = '';
      drawioReady = false;
      drawioPendingContent = null;
    }
    document.getElementById('drawio-container').classList.add('hidden');
    document.getElementById('monaco-editor').style.display = '';
    if (tabs.length === 0) createTab();
    else activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
    renderTabs();
    return;
  }
  if (tab.dirty) {
    const r = await window.electronAPI.messageDialog({
      type: 'question', title: 'Save',
      message: `Save "${tab.name}"?`,
      buttons: ['Save', "Don't Save", 'Cancel'],
      defaultId: 0, cancelId: 2
    });
    if (r.response === 2) return;
    if (r.response === 0) { const ok = await saveTabFile(tab); if (!ok) return; }
  }
  const idx = tabs.findIndex(t => t.id === id);
  // Unsubscribe the external-change watcher for this file
  if (tab.filePath) {
    try { window.electronAPI.unwatchFile(tab.filePath); } catch {}
  }
  // LSP: send textDocument/didClose
  try { window.NotePPLsp?.onTabClosed(tab); } catch {}
  // Snapshot BEFORE model.dispose — pushClosedTab reads model.getValue()
  // for editor tabs.
  pushClosedTab(tab);
  tab.model.dispose();
  tabs.splice(idx, 1);
  if (tabs.length === 0) createTab();
  else activateTab(tabs[Math.min(idx, tabs.length - 1)].id);
  renderTabs();
}

function getActiveTab() { return tabs.find(t => t.id === activeTabId); }
function switchTab(dir) {
  const idx = tabs.findIndex(t => t.id === activeTabId);
  activateTab(tabs[(idx + dir + tabs.length) % tabs.length].id);
}

function showTabContextMenu(e, tabId) {
  const tab = tabs.find(t => t.id === tabId);
  if (!tab) return;
  const compareEligible = tab.type === 'editor' && tab.filePath;
  const haveSelected = compareSelectedTabId != null &&
                       tabs.some(t => t.id === compareSelectedTabId && t.filePath);
  const selectedTab = haveSelected ? tabs.find(t => t.id === compareSelectedTabId) : null;

  const items = [
    [tab.pinned ? '📌 Unpin Tab' : '📌 Pin Tab', () => togglePinTab(tabId)],
    null,
    ['Close', () => closeTab(tabId)],
    ['Close All', () => closeAllTabs()],
    ['Close All But This', () => closeOtherTabs(tabId)],
    null,
    { type: 'color-row', current: tab.color, onChange: color => setTabColor(tabId, color) },
    null,
    ['Copy Full Path', () => { if (tab.filePath) navigator.clipboard.writeText(tab.filePath); }],
    ['Open Containing Folder', () => { if (tab.filePath) window.electronAPI.shellShowItem(tab.filePath); }],
    null,
  ];
  // Quick Compare — available for any editor tab (saved or unsaved)
  if (tab.type === 'editor') {
    items.push(['Compare…', () => quickCompareFlow(tabId)]);
    items.push(['Compare with Clipboard', () => compareWithClipboardFlow(tabId)]);
    items.push(null);
  }
  // Classic diff entry points — only for saved editor tabs
  if (compareEligible) {
    items.push(['Select for Compare', () => {
      compareSelectedTabId = tabId;
      showToast(`Marked "${tab.name}" as LEFT — right-click another tab → "Compare with Selected"`);
    }]);
    if (haveSelected && selectedTab.id !== tabId) {
      items.push([`Compare with "${selectedTab.name}"`, () => {
        createFileDiffTab(selectedTab.filePath, tab.filePath);
        compareSelectedTabId = null;
      }]);
    }
    items.push(null);
  }
  items.push(['Reveal in File Tree', () => revealInFileTree(tab.filePath)]);
  showFloatingMenu(e.clientX, e.clientY, items);
}

function dismissFloatingMenus() {
  document.querySelectorAll('.floating-ctx-menu').forEach(m => m.remove());
}

function showFloatingMenu(x, y, items) {
  // Close any other open context menus first — both the static editor one
  // and any prior floating menu — so two never overlap.
  dismissFloatingMenus();
  if (typeof contextMenu !== 'undefined' && contextMenu) contextMenu.classList.add('hidden');
  const menu = document.createElement('div');
  menu.className = 'floating-ctx-menu';
  menu.style.cssText = `position:fixed;left:${x}px;top:${y}px;z-index:2000;background:var(--ctx-bg);border:1px solid var(--ctx-border);padding:2px 0;min-width:200px;box-shadow:2px 2px 8px rgba(0,0,0,0.25);font-size:12px;max-height:80vh;overflow-y:auto;`;
  items.forEach(item => {
    if (!item) { const sep = document.createElement('div'); sep.className = 'ctx-sep'; menu.appendChild(sep); return; }
    // Special color-picker row — renders a compact swatch strip instead of a text item
    if (item.type === 'color-row') {
      const row = document.createElement('div');
      row.className = 'ctx-color-row';
      TAB_COLORS.forEach(c => {
        const sw = document.createElement('span');
        sw.className = 'ctx-color-swatch' + (item.current === c.value ? ' active' : '');
        sw.style.background = c.value;
        sw.title = c.name;
        sw.addEventListener('click', () => { item.onChange(c.value); document.body.removeChild(menu); });
        row.appendChild(sw);
      });
      const clear = document.createElement('span');
      clear.className = 'ctx-color-swatch ctx-color-clear';
      clear.title = 'Clear color';
      clear.textContent = '✕';
      clear.addEventListener('click', () => { item.onChange(null); document.body.removeChild(menu); });
      row.appendChild(clear);
      menu.appendChild(row);
      return;
    }
    const el = document.createElement('div');
    el.className = 'ctx-item';
    el.textContent = item[0];
    el.addEventListener('click', () => { item[1](); document.body.removeChild(menu); });
    menu.appendChild(el);
  });
  document.body.appendChild(menu);
  // Flip horizontally if overflowing right edge
  if (menu.getBoundingClientRect().right > window.innerWidth) {
    menu.style.left = Math.max(0, window.innerWidth - menu.offsetWidth - 4) + 'px';
  }
  // Flip vertically if overflowing bottom edge — open upward instead
  if (menu.getBoundingClientRect().bottom > window.innerHeight) {
    menu.style.top = Math.max(0, y - menu.offsetHeight) + 'px';
  }
  const hide = () => { if (menu.parentNode) document.body.removeChild(menu); document.removeEventListener('click', hide); };
  setTimeout(() => document.addEventListener('click', hide), 0);
}

// ===== File Operations =====
function newTab() { createTab(); }

// Cheap pre-check used before lazy-loading crypto.js — Note++ encrypted
// envelopes start with this magic JSON key. Files that don't match
// definitely aren't encrypted, so we skip the crypto module load
// entirely on the cold-start path for the vast majority of opens.
function _maybeEncrypted(content) {
  if (!content || typeof content !== 'string') return false;
  // Allow leading whitespace just in case the source got reformatted.
  return /^\s*\{\s*"_notepp_encrypted"\s*:\s*true/.test(content.slice(0, 64));
}

// Normalise a filesystem path for case-/separator-insensitive identity checks.
// On Windows two strings can refer to the same file while differing in case
// (`C:\…` vs `c:\…`) or in separator style — Explorer / file-association /
// recent-files all hand us slightly different spellings. POSIX comparisons
// stay case-sensitive.
function _normalizePathForCompare(p) {
  if (!p) return '';
  let s = String(p).replace(/[\\/]+/g, '/');
  // navigator.platform is "Win32" on 64-bit Windows too.
  const isWin = (navigator.platform || '').toLowerCase().startsWith('win');
  if (isWin) s = s.toLowerCase();
  return s;
}
function _findTabByPath(fp) {
  const key = _normalizePathForCompare(fp);
  if (!key) return null;
  return tabs.find(t => t.filePath && _normalizePathForCompare(t.filePath) === key) || null;
}

async function openFile(filePaths) {
  if (!filePaths) {
    // Per-category extension sets — these feed both the combined "Note++
    // Files" default filter AND the individual category filters below.
    // Order matters: the OS dialog picks the first entry as the default
    // selected filter unless the user previously chose another.
    const EXT_TEXT       = ['txt','md','markdown','log','rtf'];
    const EXT_CONFIG     = ['json','jsonc','xml','ini','conf','cfg','yaml','yml','toml','env','properties','gitignore'];
    const EXT_DATA       = ['csv','tsv'];
    const EXT_SOURCE     = ['js','jsx','ts','tsx','mjs','cjs','py','java','c','h','cpp','hpp','cs','go','rs','rb','php','swift','kt','scala','dart','lua','sql','sh','bash','ps1','bat'];
    const EXT_WEB        = ['html','htm','css','scss','sass','less','svg'];
    const EXT_WHITEBOARD = ['whiteboard','excalidraw','drawio'];
    const EXT_MERMAID    = ['mmd','mermaid'];
    const EXT_DOCS       = ['pdf','docx']; // auto-converted to markdown on open
    // De-dup while preserving insertion order — some extensions appear in
    // multiple categories (e.g. json is both Config and Web-adjacent).
    const NOTEPP_ALL = Array.from(new Set([
      ...EXT_TEXT, ...EXT_CONFIG, ...EXT_DATA, ...EXT_SOURCE,
      ...EXT_WEB, ...EXT_WHITEBOARD, ...EXT_MERMAID, ...EXT_DOCS,
    ]));

    const r = await window.electronAPI.openDialog({
      properties: ['openFile', 'multiSelections'],
      // First filter is the dialog default. "Note++ Files" is a single
      // umbrella that surfaces every extension Note++ understands —
      // matches the user expectation of "show me files this app can
      // actually open". The individual categories below let the user
      // narrow further when they want to.
      filters: [
        { name: 'Note++ Files', extensions: NOTEPP_ALL },
        { name: 'All Files',              extensions: ['*'] },
        { name: 'Text',                   extensions: EXT_TEXT },
        { name: 'Config / Data',          extensions: [...EXT_CONFIG, ...EXT_DATA] },
        { name: 'Source Code',            extensions: EXT_SOURCE },
        { name: 'Web',                    extensions: EXT_WEB },
        { name: 'Whiteboard / Diagrams',  extensions: EXT_WHITEBOARD },
        { name: 'Mermaid',                extensions: EXT_MERMAID },
        { name: 'Documents (auto-convert)', extensions: EXT_DOCS },
      ]
    });
    if (r.canceled) return;
    filePaths = r.filePaths;
  }
  for (const fp of filePaths) {
    const existing = _findTabByPath(fp);
    if (existing) { activateTab(existing.id); continue; }
    // Binary documents (.pdf, .docx) — route through the markdown converter
    // and open the result as a new untitled .md tab. The source file is
    // never modified; the original path is shown in the tab tooltip.
    const _lcExt = fp.toLowerCase();
    if (_lcExt.endsWith('.pdf') || _lcExt.endsWith('.docx')) {
      await convertAndOpenAsMarkdown(fp);
      continue;
    }
    const res = await window.electronAPI.readFile(fp);
    if (!res.success) { showToast('Error: ' + res.error); continue; }
    // Track in Recent Files (main owns the persisted list + menu)
    try { window.electronAPI.recentFileOpened(fp); } catch {}
    // Watch this file for external changes (git pull, another editor, etc.)
    try { window.electronAPI.watchFile(fp); } catch {}

    // ── Encrypted file? — detect, unlock, decrypt, then open as editor tab.
    // Quick-check first so we skip the crypto.js module load entirely on
    // the ~100 % of files that aren't encrypted. Note++ encrypted files
    // are JSON envelopes whose first ~32 chars contain this magic.
    let envelope = null;
    if (_maybeEncrypted(res.content)) {
      await ensureCrypto();
      envelope = window.NotePPCrypto.detectEncrypted(res.content);
    }
    if (envelope) {
      const ok = await openEncryptedFile(fp, envelope);
      if (!ok) continue; // user cancelled or wrong profile
      continue;
    }

    // Route .whiteboard / .excalidraw / whiteboard-format JSON to the whiteboard tab.
    // Route .drawio (or .xml that starts with <mxfile / <mxGraphModel) to drawio.
    // Detection for whiteboard JSON accepts our envelope (`__wb__: true`) AND
    // raw Excalidraw files (`type: "excalidraw"` from Excalidraw's Save-As).
    const lower = fp.toLowerCase();
    if (lower.endsWith('.whiteboard') || lower.endsWith('.excalidraw')) {
      createWhiteboardTab(fp, res.content);
    } else if (lower.endsWith('.drawio')) {
      createDrawioTab(fp, res.content);
    } else if (lower.endsWith('.xml')) {
      const head = (res.content || '').slice(0, 200);
      if (/<\s*mxfile|<\s*mxGraphModel/i.test(head)) createDrawioTab(fp, res.content);
      else _checkLargeFile(createTab(fp, res.content), res.size);
    } else if (lower.endsWith('.json')) {
      let isWb = false;
      try {
        const parsed = JSON.parse(res.content);
        isWb = parsed && (parsed.__wb__ === true || parsed.type === 'excalidraw');
      } catch (e) {}
      if (isWb) createWhiteboardTab(fp, res.content);
      else       _checkLargeFile(createTab(fp, res.content), res.size);
    } else {
      _checkLargeFile(createTab(fp, res.content), res.size);
    }
  }
}

// ── Binary-document → Markdown (PDF / DOCX) ─────────────────────────────────
// User opens a PDF or DOCX → we route through the main-process converter
// (pdf-parse / mammoth) and surface the result as a fresh untitled .md tab.
// The original file is never modified. A small modal reflects progress
// percent + stage so the user knows we're working, not frozen.
let _convertJobSeq = 0;
// Tracks in-flight jobIds — the dialog stays open until *every* in-flight
// job has finished. Each job owns its own listener disposer locally, so
// there's no shared `_convertOffProgress` that concurrent calls can stomp
// on (the previous version did, and one of the listeners would be
// orphaned every time the user double-opened a file).
const _activeConvertJobs = new Set();

function _showConvertProgress(srcPath) {
  const dlg     = document.getElementById('convert-progress-dialog');
  const nameEl  = document.getElementById('convert-progress-filename');
  const stageEl = document.getElementById('convert-progress-stage');
  const barEl   = document.getElementById('convert-progress-bar');
  const pctEl   = document.getElementById('convert-progress-pct');
  if (!dlg) return;
  if (nameEl)  nameEl.textContent  = srcPath;
  if (stageEl) stageEl.textContent = 'Starting…';
  if (barEl)   { barEl.style.width = '0%'; barEl.classList.remove('indeterminate'); }
  if (pctEl)   pctEl.textContent   = '0%';
  dlg.classList.remove('hidden');
}

function _updateConvertProgress(percent, stage) {
  const stageEl = document.getElementById('convert-progress-stage');
  const barEl   = document.getElementById('convert-progress-bar');
  const pctEl   = document.getElementById('convert-progress-pct');
  if (stageEl && stage) stageEl.textContent = stage;
  if (barEl && pctEl) {
    if (typeof percent === 'number' && percent >= 0) {
      barEl.classList.remove('indeterminate');
      const clamped = Math.max(0, Math.min(100, percent));
      barEl.style.width = clamped + '%';
      pctEl.textContent = Math.round(clamped) + '%';
    } else {
      barEl.classList.add('indeterminate');
      pctEl.textContent = '…';
    }
  }
}

function _hideConvertProgress() {
  const dlg = document.getElementById('convert-progress-dialog');
  if (dlg) dlg.classList.add('hidden');
}

async function convertAndOpenAsMarkdown(srcPath) {
  const api = window.electronAPI?.convert;
  if (!api) { showToast('Converter unavailable'); return; }

  const jobId = ++_convertJobSeq;
  _activeConvertJobs.add(jobId);
  _showConvertProgress(srcPath);

  // Subscribe to progress events for THIS job. The disposer is strictly
  // local, so two parallel conversions each own and clean up their own
  // listener. The previous version stored a single shared disposer that
  // got stomped when a second conversion started.
  const offProgress = api.onProgress((payload) => {
    if (!payload || payload.jobId !== jobId) return;
    _updateConvertProgress(payload.percent, payload.stage);
  });

  // Track in Recent Files so the user can re-open later
  try { window.electronAPI.recentFileOpened(srcPath); } catch {}

  let result;
  try {
    result = await api.start(srcPath, jobId);
  } catch (err) {
    showToast('Conversion failed: ' + (err?.message || String(err)));
    return;
  } finally {
    // Always dispose THIS job's listener, even if start() threw.
    try { offProgress(); } catch {}
    _activeConvertJobs.delete(jobId);
    // Keep the dialog open while any other job is still running.
    if (_activeConvertJobs.size === 0) _hideConvertProgress();
  }

  if (!result || !result.success) {
    showToast('Conversion failed: ' + (result?.error || 'unknown error'));
    return;
  }

  // Create a fresh tab. Tab.filePath stays null so Ctrl+S prompts Save As
  // (we never want to overwrite the original PDF/DOCX with markdown text).
  const baseName = srcPath.split(/[\\/]/).pop().replace(/\.(pdf|docx)$/i, '');
  const tab = createTab(null, result.markdown || '');
  tab.name = baseName + '.md';
  tab.language = 'markdown';
  tab.dirty = true; // unsaved derived content
  if (tab.model) {
    try { monaco.editor.setModelLanguage(tab.model, 'markdown'); } catch {}
  }
  renderTabs();
  updateTitle();
  updateLanguageStatus();
  showToast(`Converted ${baseName} → Markdown`);
}

// Open an already-detected encrypted file: validate profile, prompt unlock if
// needed, decrypt, create an editor tab. Returns true on success, false on
// user cancel / wrong profile / decryption error.
async function openEncryptedFile(fp, envelope) {
  // Profile not configured at all
  if (!isEncConfigured()) {
    await window.electronAPI.messageDialog({
      type: 'warning',
      title: 'Encrypted file',
      message: 'This file is a Note++ encrypted document, but no encryption profile is set up on this installation.',
      detail: 'Use Preferences → Encryption to set up a profile, or restore one using your recovery key.',
      buttons: ['OK'],
    });
    return false;
  }
  // Profile fingerprint mismatch
  if (envelope.profile && envelope.profile !== appEnc.profile.fingerprint) {
    await window.electronAPI.messageDialog({
      type: 'warning',
      title: 'Different encryption profile',
      message: `This file was encrypted with profile "${envelope.profile}", but the active profile is "${appEnc.profile.fingerprint}".`,
      detail: 'Restore the other profile or use its recovery key to access this file.',
      buttons: ['OK'],
    });
    return false;
  }
  // Unlock if needed
  if (!isEncUnlocked()) {
    const unlocked = await promptUnlockDialog();
    if (!unlocked) return false;
  }
  // Decrypt
  let plaintext;
  try {
    await ensureCrypto();
    plaintext = await window.NotePPCrypto.decryptFile(envelope, appEnc.rawDek);
  } catch (e) {
    showToast('Failed to decrypt file: ' + (e.message || e));
    return false;
  }
  // Create the tab using the original extension for syntax highlighting
  const tab = createTab(fp, plaintext);
  tab.encrypted = true;
  tab.protectedBy = envelope.profile || appEnc.profile.fingerprint;
  // If originalExt differs from the file's actual extension, prefer the
  // original for language detection (so a `secret-notes.md.enc` style scheme
  // would still get markdown highlighting).
  if (envelope.originalExt) {
    const langForExt = detectLanguage('x.' + envelope.originalExt);
    if (langForExt && langForExt !== 'plaintext') {
      tab.language = langForExt;
      if (tab.model) monaco.editor.setModelLanguage(tab.model, langForExt);
    }
  }
  renderTabs();
  updateTitle();
  updateEncryptionStatusIndicator();
  return true;
}

async function saveFile() { const tab = getActiveTab(); if (tab) await saveTabFile(tab); }
async function saveFileAs() { const tab = getActiveTab(); if (tab) await saveTabFile(tab, true); }
async function saveAll() { for (const tab of tabs) { if (tab.dirty) await saveTabFile(tab); } }

async function saveTabFile(tab, forceAs = false) {
  // ── draw.io: content is already in tab.content (XML synced via postMessage) ──
  if (tab.type === 'drawio') {
    if (!tab.filePath || forceAs) {
      const defaultName = /\.[a-z0-9]+$/i.test(tab.name) ? tab.name : `${tab.name}.drawio`;
      const r = await window.electronAPI.saveDialog({
        defaultPath: defaultName,
        filters: [
          { name: 'draw.io',  extensions: ['drawio'] },
          { name: 'XML',      extensions: ['xml'] },
          { name: 'All Files', extensions: ['*'] },
        ],
      });
      if (r.canceled) return false;
      tab.filePath = r.filePath;
      tab.name = r.filePath.split(/[\\/]/).pop();
    }
    // Force a fresh export from the iframe in case the user has un-saved
    // edits debouncing — drawio doesn't push state synchronously on save.
    sendToDrawio({ type: 'dw-get-data' });
    // Small wait to let the dw-state response arrive (debounced ~50ms in
    // drawio's autosave). Worst case the next save catches it.
    await new Promise(r => setTimeout(r, 120));
    const body = tab.content && tab.content.length ? tab.content : '<mxfile></mxfile>';
    const res = await window.electronAPI.writeFile(tab.filePath, body);
    if (!res.success) { showToast('Error saving: ' + res.error); return false; }
    tab.content = body;
    tab.dirty = false;
    renderTabs();
    updateTitle();
    return true;
  }

  // ── Whiteboard: content is already in tab.content (synced via postMessage) ──
  if (tab.type === 'whiteboard') {
    if (!tab.filePath || forceAs) {
      // New (unsaved) whiteboards have a bare "whiteboard-N" name with no
      // extension — append .json so the Save dialog picks the right filter.
      const defaultName = /\.[a-z0-9]+$/i.test(tab.name) ? tab.name : `${tab.name}.json`;
      const r = await window.electronAPI.saveDialog({
        defaultPath: defaultName,
        filters: [
          { name: 'Whiteboard JSON', extensions: ['json'] },
          { name: 'Excalidraw', extensions: ['excalidraw'] },
          { name: 'Whiteboard (legacy)', extensions: ['whiteboard'] },
          { name: 'All Files', extensions: ['*'] }
        ]
      });
      if (r.canceled) return false;
      tab.filePath = r.filePath;
      tab.name = r.filePath.split(/[\\/]/).pop();
    }
    // Empty unsaved whiteboards (user saved before drawing anything) need a
    // valid v2 envelope so the file opens correctly next time — otherwise
    // openFile() routes it through Monaco as a malformed JSON.
    const bodyToWrite = tab.content && tab.content.trim()
      ? tab.content
      : JSON.stringify({ __wb__: true, version: 2, source: 'excalidraw', elements: [], appState: {}, files: {} });
    const res = await window.electronAPI.writeFile(tab.filePath, bodyToWrite);
    if (!res.success) { showToast('Error saving: ' + res.error); return false; }
    tab.content = bodyToWrite;
    tab.dirty = false;
    renderTabs();
    updateTitle();
    return true;
  }
  if (!tab.filePath || forceAs) {
    // Smart defaults: if the user picked a language on the tab (e.g.
    // Markdown), pre-select the matching filter + extension so a quick
    // "type filename + Enter" save lands on `name.md` instead of an
    // extension-less file. _saveDialogOptionsForTab figures out the
    // right defaultPath/filters from tab.language + tab.name.
    const opts = _saveDialogOptionsForTab(tab);
    const r = await window.electronAPI.saveDialog(opts);
    if (r.canceled) return false;
    // Save-As: stop watching the previous path before overwriting
    // tab.filePath, otherwise the old watcher leaks until the app
    // exits. The new path gets its watcher added after the write
    // succeeds, further down.
    if (tab.filePath && tab.filePath !== r.filePath) {
      try { window.electronAPI.unwatchFile(tab.filePath); } catch {}
    }
    tab.filePath = r.filePath;
    tab.name = r.filePath.split(/[\\/]/).pop();
    tab.language = detectLanguage(tab.filePath);
    monaco.editor.setModelLanguage(tab.model, tab.language);
  }
  const content = tab.model.getValue();

  // ── Encrypted save path: gzip → AES-GCM → JSON envelope. ──────────────────
  // tab.encrypted = true on tabs that opted in via "Encrypt this file" or that
  // were opened from an already-encrypted file. Requires an unlocked profile.
  let bytesToWrite = content;
  if (tab.encrypted) {
    if (!isEncConfigured()) {
      showToast('Encryption not configured — open Preferences → Encryption');
      return false;
    }
    if (!isEncUnlocked()) {
      const unlocked = await promptUnlockDialog();
      if (!unlocked) return false;
    }
    try {
      await ensureCrypto();
      const originalExt = (tab.filePath.match(/\.([^.\\/]+)$/) || [, ''])[1].toLowerCase();
      const envelope = await window.NotePPCrypto.encryptFile(
        content, appEnc.rawDek, appEnc.profile.fingerprint, originalExt
      );
      tab.protectedBy = appEnc.profile.fingerprint;
      bytesToWrite = JSON.stringify(envelope, null, 2);
    } catch (e) {
      showToast('Encryption failed: ' + (e.message || e));
      return false;
    }
  }
  const res = await window.electronAPI.writeFile(tab.filePath, bytesToWrite);
  if (!res.success) { showToast('Error saving: ' + res.error); return false; }
  tab.dirty = false;
  tab.content = content;
  // Tell the file watcher this mtime change came from us (don't fire externally-changed)
  try { window.electronAPI.fileSavedByApp(tab.filePath); } catch {}
  // Start watching newly-saved files (first Save As of an unsaved tab)
  try { window.electronAPI.watchFile(tab.filePath); } catch {}
  renderTabs();
  updateTitle();
  updateLanguageStatus();
  // Saving may move the file in/out of git tracking — refresh repo status.
  if (activeGitRepo) refreshGitStatus(activeGitRepo);
  // Saving doesn't change HEAD, but if the file was previously untracked
  // and the user `git add`-ed it externally, the cached "no HEAD" needs to
  // refresh. Cheapest correct behaviour: invalidate + reschedule.
  tab._gitHeadContent = undefined;
  scheduleGitDiffUpdate(tab);
  return true;
}

// Toggle encryption on the active tab (or specified tab). For new/unsaved tabs,
// triggers a Save As first and writes the file as encrypted immediately.
async function toggleTabEncryption(tab) {
  tab = tab || getActiveTab();
  if (!tab) return;
  // Only text editor tabs can be encrypted (excludes game / whiteboard).
  // Note: tabs created via createTab() have type:'editor', but legacy tabs
  // restored from older sessions may have no `type` field — treat those as editors.
  if (tab.type === 'whiteboard' || tab.type === 'game') {
    showToast('Encryption is only available for text files');
    return;
  }

  if (tab.encrypted) {
    const r = await window.electronAPI.messageDialog({
      type: 'question', title: 'Remove encryption',
      message: `Remove encryption from "${tab.name}"?`,
      detail: 'The file will be saved as plaintext on the next save.',
      buttons: ['Remove encryption', 'Cancel'], defaultId: 0, cancelId: 1,
    });
    if (r.response !== 0) return;
    tab.encrypted = false;
    tab.protectedBy = null;
    if (!tab.dirty) tab.dirty = true;
    renderTabs();
    updateTitle();
    updateEncryptToolbarButton();
    updateEncryptionStatusIndicator();
    return;
  }

  // Encrypting — needs a configured + unlocked profile
  if (!isEncConfigured()) {
    const r = await window.electronAPI.messageDialog({
      type: 'info', title: 'Set up encryption',
      message: 'You need to set up an encryption profile first.',
      detail: 'Open Preferences → Encryption to create one.',
      buttons: ['Open Preferences', 'Cancel'], defaultId: 0, cancelId: 1,
    });
    if (r.response === 0) openPreferences();
    return;
  }
  if (!isEncUnlocked()) {
    const ok = await promptUnlockDialog();
    if (!ok) return;
  }

  // ── Unsaved tab (no filePath yet) → "Save and Encrypt" flow ──
  // The user wants the Save As dialog up front, then the file is written
  // straight to disk in encrypted form (no intermediate plaintext save).
  if (!tab.filePath) {
    const confirm = await window.electronAPI.messageDialog({
      type: 'question', title: 'Save and encrypt',
      message: `"${tab.name}" hasn't been saved yet.`,
      detail: 'Note++ will open the Save As dialog, then write the file in encrypted form.',
      buttons: ['Save and Encrypt…', 'Cancel'], defaultId: 0, cancelId: 1,
    });
    if (confirm.response !== 0) return;
    // Mark the tab as encrypted BEFORE calling saveTabFile so the save path
    // writes the JSON envelope directly. If the user cancels the Save As
    // dialog, revert the flag.
    tab.encrypted = true;
    tab.protectedBy = appEnc.profile.fingerprint;
    const ok = await saveTabFile(tab, /* forceAs */ true);
    if (!ok) {
      tab.encrypted = false;
      tab.protectedBy = null;
      return;
    }
    showToast('🔒 File saved as encrypted');
    renderTabs();
    updateTitle();
    updateEncryptToolbarButton();
    updateEncryptionStatusIndicator();
    return;
  }

  // ── Already-saved tab → confirm, then mark; next save writes envelope ──
  const r = await window.electronAPI.messageDialog({
    type: 'question', title: 'Encrypt this file',
    message: `Encrypt "${tab.name}" with the active Note++ profile?`,
    detail: 'The file will be encrypted on the next save. Anyone with the profile password (or the recovery key) can decrypt it.',
    buttons: ['Encrypt', 'Cancel'], defaultId: 0, cancelId: 1,
  });
  if (r.response !== 0) return;
  tab.encrypted = true;
  tab.protectedBy = appEnc.profile.fingerprint;
  if (!tab.dirty) tab.dirty = true;
  renderTabs();
  updateTitle();
  updateEncryptToolbarButton();
  updateEncryptionStatusIndicator();
}

// Wire the main-process file-change watcher. Called once at startup.
// Behaviour:
//   - File deleted on disk → notify user, leave tab alone (so user can save back)
//   - File changed AND tab is CLEAN → auto-reload silently (toast confirmation)
//   - File changed AND tab is DIRTY → ask "Reload?" / "Keep mine"
// User can disable auto-reload entirely via the new pref.
function setupExternalChangeWatcher() {
  if (!window.electronAPI.onFileChangedExternally) return;
  window.electronAPI.onFileChangedExternally(async ({ filePath, deleted }) => {
    const tab = tabs.find(t => t.filePath === filePath);
    if (!tab || tab.type !== 'editor' || !tab.model) return;

    if (deleted) {
      showToast(`⚠ "${tab.name}" was deleted on disk`);
      tab.dirty = true;
      renderTabs();
      updateTitle();
      return;
    }

    // Don't bother if tab is currently encrypted (envelope JSON changes on every save anyway)
    if (tab.encrypted) return;

    // Reload from disk
    const res = await window.electronAPI.readFile(filePath);
    if (!res.success) return;
    const onDisk = res.content;
    const inEditor = tab.model.getValue();
    if (onDisk === inEditor) return; // nothing to do (e.g., we just saved)

    if (!tab.dirty) {
      // Clean tab — auto-reload
      tab.model.setValue(onDisk);
      tab.content = onDisk;
      showToast(`↻ Reloaded "${tab.name}" — changed on disk`);
      return;
    }
    // Dirty tab — ask the user
    const r = await window.electronAPI.messageDialog({
      type: 'question',
      title: 'File changed on disk',
      message: `"${tab.name}" was changed outside Note++.`,
      detail: 'You have unsaved changes in this tab. Reloading will discard them.',
      buttons: ['Reload from disk', 'Keep my version'],
      defaultId: 1,
      cancelId: 1,
    });
    if (r.response === 0) {
      tab.model.setValue(onDisk);
      tab.content = onDisk;
      tab.dirty = false;
      renderTabs();
      updateTitle();
      showToast(`↻ Reloaded "${tab.name}"`);
    }
  });
}

async function reloadFile() {
  const tab = getActiveTab();
  if (!tab || !tab.filePath) return;
  if (tab.dirty) {
    const r = await window.electronAPI.messageDialog({ type: 'question', title: 'Reload', message: 'Reload and lose unsaved changes?', buttons: ['Reload', 'Cancel'], defaultId: 0, cancelId: 1 });
    if (r.response === 1) return;
  }
  const res = await window.electronAPI.readFile(tab.filePath);
  if (!res.success) { showToast('Error: ' + res.error); return; }
  tab.model.setValue(res.content);
  tab.dirty = false;
  renderTabs();
}

async function closeAllTabs() { for (const id of tabs.filter(t => !t.pinned).map(t => t.id)) await closeTab(id); }
async function closeOtherTabs(keepId) { for (const id of tabs.filter(t => t.id !== keepId && !t.pinned).map(t => t.id)) await closeTab(id); }

// ===== Language Detection =====

// ── Paste normalisation ────────────────────────────────────────────────────
// Maps "rich" Unicode characters that come in from Word / browsers /
// chat apps to their plain-ASCII equivalents. These look like normal
// text but break JSON, regex, code, command-line snippets — every
// developer has been bitten by smart quotes pasted into a config file.
const PASTE_NORMALISE_MAP = {
  '‘': "'", '’': "'", '‚': "'", '‛': "'", // single smart quotes
  '“': '"', '”': '"', '„': '"', '‟': '"', // double smart quotes
  '′': "'", '″': '"',                               // prime / double-prime
  '–': '-', '—': '-', '―': '-',                // en/em/horizontal dash
  '·': '.',                                              // middle dot (rare paste artefact)
  ' ': ' ',                                              // non-breaking space
  ' ': ' ', ' ': ' ', ' ': ' ', ' ': ' ', // en/em/three-per/four-per em
  ' ': ' ', ' ': ' ', ' ': ' ', ' ': ' ', // six-per/figure/punctuation/thin
  ' ': ' ', ' ': ' ', ' ': ' ',                // hair/narrow/medium math space
  ' ': '\n', ' ': '\n',                             // line/paragraph separator
  '…': '...',                                            // horizontal ellipsis
};
const PASTE_NORMALISE_RE = new RegExp(
  '[' +
    Object.keys(PASTE_NORMALISE_MAP).join('') +
    '​-‍﻿' +  // zero-width space / non-joiner / joiner / BOM
  ']',
  'g'
);

function normalizeForPaste(text) {
  if (!text) return text;
  return text.replace(PASTE_NORMALISE_RE, (ch) => PASTE_NORMALISE_MAP[ch] ?? '');
}

// DOM-level paste interceptor — runs in capture phase before Monaco's
// own paste handler. When the clipboard text has rich characters, we
// stop Monaco from inserting the original and substitute our cleaned
// version. This is the primary path for Ctrl+V and the editor's
// right-click → Paste.
function setupPastePreprocessor() {
  const el = document.getElementById('monaco-editor');
  if (!el) return;
  el.addEventListener('paste', (e) => {
    const text = e.clipboardData?.getData('text/plain');
    if (!text) return;
    const cleaned = normalizeForPaste(text);
    if (cleaned === text) return; // nothing to do — let Monaco insert normally
    e.preventDefault();
    e.stopPropagation();
    _insertAsPaste(cleaned);
  }, true);
}

// Insert text using Monaco's edit pipeline so undo/redo + multi-cursor
// + selection-replace all behave correctly. We use executeEdits rather
// than 'type' because we want to skip format-on-paste — the whole
// point is "paste exactly what was on the clipboard, no transforms".
function _insertAsPaste(text) {
  if (!editor) return;
  const selections = editor.getSelections() || [editor.getSelection()].filter(Boolean);
  if (!selections.length) return;
  const edits = selections.map(sel => ({
    range: sel,
    text,
    forceMoveMarkers: true,
  }));
  editor.executeEdits('paste-normalized', edits);
  editor.pushUndoStop();
}

// Used by the toolbar / Edit menu / context-menu "Paste" actions so
// they go through the same normalisation as Ctrl+V. Reads via
// navigator.clipboard so it works even when Monaco's own paste action
// would have bypassed the DOM paste event.
async function pasteFromClipboardClean() {
  if (!editor) return;
  let text = '';
  try { text = await navigator.clipboard.readText(); }
  catch {
    // Permissions / unfocused window → fall back to Monaco's native paste.
    editor.trigger('cleanpaste', 'editor.action.clipboardPasteAction', null);
    return;
  }
  if (!text) return;
  _insertAsPaste(normalizeForPaste(text));
}

// Apply a detected language to a tab (shared by onDidPaste and the debounced fallback).
function _applyAutoDetectedLanguage(tab, content) {
  if (!tab || !content.trim()) return;
  const detected = autoDetectLanguage(content, tab.name);
  if (!detected || detected === 'plaintext') return;
  tab.language = detected;
  monaco.editor.setModelLanguage(tab.model, detected);
  updateLanguageStatus();
  updateMermaidToolbar(detected === 'mermaid');
  if (detected === 'mermaid' && !previewOpen) openPreview();
  else if (previewOpen) updatePreview();
  showToast(`Language auto-detected: ${detected}`);
}

// Debounced fallback: runs 400 ms after the last content change on a plaintext tab.
// Covers pastes via Edit menu / execCommand that bypass Monaco's onDidPaste.
let _autoDetectTimer = null;
function _scheduleAutoDetect(tab) {
  clearTimeout(_autoDetectTimer);
  _autoDetectTimer = setTimeout(() => {
    if (!tab || tab.language !== 'plaintext') return;
    tab._autoDetectTriggered = true;
    _applyAutoDetectedLanguage(tab, editor.getValue());
  }, 400);
}
// Reverse-lookup table for the Save As dialog: given a Monaco language
// id, pick the canonical extension + a human-readable filter label.
// Only entries here unlock the "smart save" path; anything else falls
// back to the All-Files filter (and the user must type their own ext).
const LANGUAGE_TO_EXT = {
  markdown:   { ext: 'md',   label: 'Markdown' },
  javascript: { ext: 'js',   label: 'JavaScript' },
  typescript: { ext: 'ts',   label: 'TypeScript' },
  python:     { ext: 'py',   label: 'Python' },
  java:       { ext: 'java', label: 'Java' },
  c:          { ext: 'c',    label: 'C source' },
  cpp:        { ext: 'cpp',  label: 'C++ source' },
  csharp:     { ext: 'cs',   label: 'C#' },
  go:         { ext: 'go',   label: 'Go' },
  rust:       { ext: 'rs',   label: 'Rust' },
  ruby:       { ext: 'rb',   label: 'Ruby' },
  php:        { ext: 'php',  label: 'PHP' },
  html:       { ext: 'html', label: 'HTML' },
  css:        { ext: 'css',  label: 'CSS' },
  scss:       { ext: 'scss', label: 'SCSS' },
  less:       { ext: 'less', label: 'LESS' },
  xml:        { ext: 'xml',  label: 'XML' },
  json:       { ext: 'json', label: 'JSON' },
  yaml:       { ext: 'yaml', label: 'YAML' },
  mermaid:    { ext: 'mmd',  label: 'Mermaid' },
  sql:        { ext: 'sql',  label: 'SQL' },
  shell:      { ext: 'sh',   label: 'Shell script' },
  powershell: { ext: 'ps1',  label: 'PowerShell' },
  bat:        { ext: 'bat',  label: 'Batch file' },
  lua:        { ext: 'lua',  label: 'Lua' },
  swift:      { ext: 'swift',label: 'Swift' },
  kotlin:     { ext: 'kt',   label: 'Kotlin' },
  ini:        { ext: 'ini',  label: 'INI / Config' },
  hcl:        { ext: 'hcl',  label: 'HCL / Terraform' },
  dockerfile: { ext: 'dockerfile', label: 'Dockerfile' },
  plaintext:  { ext: 'txt',  label: 'Text' },
};

// Build the saveDialog options for a tab. The key trick is `defaultPath`:
// Electron uses the *extension* of defaultPath to decide which filter the
// OS dialog opens with, AND to append the extension when the user types a
// bare filename. So if the tab is markdown and named "new 3", we hand it
// "new 3.md" — Windows / macOS / Linux then default-select the Markdown
// filter and silently add `.md` when the user hits Enter.
function _saveDialogOptionsForTab(tab) {
  const meta = LANGUAGE_TO_EXT[tab?.language] || null;
  const baseName = (tab?.name || 'untitled').replace(/[\\/]+/g, '_');
  // If the tab name already has an extension matching `meta.ext`, keep
  // it as-is; otherwise tack on the canonical extension so the dialog
  // primes with the right filter.
  let defaultPath = baseName;
  if (meta) {
    const hasMatchingExt = new RegExp('\\.' + meta.ext + '$', 'i').test(baseName);
    if (!hasMatchingExt) {
      // Strip any *other* extension the user didn't intend, then add ours.
      // We only strip if the existing extension is short (≤6 chars) and
      // alphanumeric — protects names like "config.dev" that the user
      // legitimately wants to keep.
      const stripped = baseName.replace(/\.[a-z0-9]{1,6}$/i, '');
      defaultPath = stripped + '.' + meta.ext;
    }
  }
  const filters = meta
    ? [
        { name: meta.label, extensions: [meta.ext] },
        { name: 'All Files', extensions: ['*'] },
      ]
    : [{ name: 'All Files', extensions: ['*'] }];
  return { defaultPath, filters };
}

function detectLanguage(fp) {
  const ext = fp.split('.').pop().toLowerCase();
  const map = {
    js:'javascript',mjs:'javascript',cjs:'javascript',jsx:'javascript',
    ts:'typescript',tsx:'typescript',
    py:'python',pyw:'python',
    java:'java', c:'c', h:'c',
    cpp:'cpp',cc:'cpp',cxx:'cpp',hpp:'cpp',
    cs:'csharp', go:'go', rs:'rust', rb:'ruby', php:'php',
    html:'html',htm:'html', css:'css', scss:'scss', less:'less',
    xml:'xml',svg:'xml',xaml:'xml',
    json:'json',jsonc:'json',
    yaml:'yaml',yml:'yaml', md:'markdown',mdx:'markdown',
    mmd:'mermaid', mermaid:'mermaid',
    whiteboard:'whiteboard',
    sql:'sql', sh:'shell',bash:'shell',zsh:'shell',
    ps1:'powershell',psm1:'powershell',
    bat:'bat',cmd:'bat', lua:'lua', r:'r',
    swift:'swift', kt:'kotlin',kts:'kotlin',
    vb:'vb', dockerfile:'dockerfile',
    ini:'ini',cfg:'ini',conf:'ini', toml:'ini',
    tf:'hcl', hcl:'hcl',
  };
  return map[ext] || 'plaintext';
}

function getFileEmoji(name) {
  const ext = name.split('.').pop().toLowerCase();
  const m = { js:'📜',ts:'📘',jsx:'⚛',tsx:'⚛',py:'🐍',java:'☕',c:'©',cpp:'➕',cs:'#',go:'🔵',rs:'🦀',rb:'💎',php:'🐘',html:'🌐',css:'🎨',json:'📋',xml:'📄',md:'📝',sql:'🗄',sh:'🖥',ps1:'💙',bat:'🦇',yaml:'⚙',yml:'⚙',dockerfile:'🐳',svg:'🖼',txt:'📃',log:'📋',mmd:'📊',mermaid:'📊',whiteboard:'🖼' };
  return m[ext] || '📄';
}

// ===== Editor Actions =====
function formatDocument() {
  editor.getAction('editor.action.formatDocument')?.run();
}

// Languages Monaco has a built-in formatter for
const FORMATTABLE_LANGS = new Set([
  'javascript', 'typescript', 'json', 'html', 'css', 'scss', 'less', 'markdown', 'yaml'
]);

async function formatWithAutoDetect() {
  const tab = getActiveTab();
  if (!tab) return;

  let lang = tab.language;
  let detected = false;

  // Auto-detect if language is plaintext or unknown
  if (lang === 'plaintext' || !lang) {
    const content = editor.getModel().getValue();
    const guessed = autoDetectLanguage(content, tab.name);
    if (guessed) {
      lang = guessed;
      tab.language = lang;
      monaco.editor.setModelLanguage(tab.model, lang);
      updateLanguageStatus();
      detected = true;
    }
  }

  // Monaco needs a tick to apply the new language model before formatting
  await new Promise(r => setTimeout(r, 80));

  if (FORMATTABLE_LANGS.has(lang)) {
    await editor.getAction('editor.action.formatDocument')?.run();
    showToast(detected ? `Auto-detected ${lang} — formatted` : `Formatted as ${lang}`);
  } else if (lang === 'xml') {
    xmlFormat();
    showToast(detected ? `Auto-detected XML — formatted` : 'Formatted as XML');
  } else {
    if (detected) {
      showToast(`Auto-detected ${lang} — no formatter available`);
    } else {
      showToast(`No formatter for ${lang}`);
    }
  }
}

function autoDetectLanguage(content, fileName) {
  // 1. Extension-based (skip for unsaved "new N" tabs)
  if (fileName && !/^new \d+$/.test(fileName)) {
    const byExt = detectLanguage(fileName);
    if (byExt !== 'plaintext') return byExt;
  }

  const head = content.slice(0, 3000);
  const firstLine = content.split('\n')[0].trim();

  // 2. Shebang
  if (firstLine.startsWith('#!')) {
    if (/python/.test(firstLine))                    return 'python';
    if (/node|nodejs/.test(firstLine))               return 'javascript';
    if (/ruby/.test(firstLine))                      return 'ruby';
    if (/php/.test(firstLine))                       return 'php';
    if (/bash|sh|zsh|fish/.test(firstLine))          return 'shell';
    if (/perl/.test(firstLine))                      return 'shell';
  }

  // 3. JSON — must parse cleanly
  const trimmed = content.trim();
  if (/^[\[{]/.test(trimmed)) {
    try { JSON.parse(content); return 'json'; } catch {}
  }

  // 4. XML / HTML
  if (/^</.test(trimmed)) {
    if (/<!DOCTYPE\s+html/i.test(head) || /<html[\s>]/i.test(head)) return 'html';
    if (/^<\?xml/.test(trimmed))                                      return 'xml';
    if (/<(html|head|body|div|span|p|a|script|style)[\s/>]/i.test(head)) return 'html';
    return 'xml';
  }

  // 5. Mermaid — raw diagram (bare .mmd content pasted into an untitled tab)
  if (MERMAID_START.test(trimmed) && !(/```/.test(trimmed))) return 'mermaid';

  // 6. YAML (must come before general key:value checks)
  if (/^---(\s|$)/m.test(content) || (/^[a-zA-Z_][\w-]*:\s/m.test(head) && !/{/.test(head.slice(0, 200)))) {
    return 'yaml';
  }

  // 6. Markdown — heading regex allows tabs/multiple spaces; require a non-whitespace
  // char after the space so bare "#" comment lines don't trigger this.
  if (/^#{1,6}[ \t]+\S/m.test(head) || /^```/m.test(head) || /^\*\*\w/.test(head) ||
      /^- \[[ xX]\]/m.test(head) || /^\*{1,2}[^*\n]+\*{1,2}/m.test(head)) return 'markdown';

  // 7. SQL
  if (/^\s*(SELECT|INSERT\s+INTO|UPDATE\s+\w+\s+SET|DELETE\s+FROM|CREATE\s+(TABLE|DATABASE|INDEX)|DROP\s+(TABLE|DATABASE)|ALTER\s+TABLE|WITH\s+\w+\s+AS)\b/im.test(head)) return 'sql';

  // 8. Dockerfile
  if (/^FROM\s+\S+/m.test(head) && /^(RUN|CMD|EXPOSE|COPY|ADD|ENV|ENTRYPOINT|WORKDIR)\s/m.test(head)) return 'dockerfile';

  // 9. PowerShell
  if (/\$\w+\s*=|Write-(Host|Output|Error)|Param\s*\(|\bfunction\s+\w+-\w+\b|-ErrorAction|-PassThru/.test(head)) return 'powershell';

  // 10. Shell
  if (/^(echo|export|alias|source|if\s+\[|for\s+\w+\s+in|while\s+\[|function\s+\w+\s*\(\))\s/m.test(head)) return 'shell';

  // 11. Python
  if (/^(import |from \w+ import |def |class |async def |@\w+(\(|$))/m.test(head)) return 'python';

  // 12. TypeScript (check before JS — TS is a superset)
  if (/\b(interface|type)\s+\w+[\s<{=]|:\s*(string|number|boolean|any|void|never|unknown)\b|<\w+>/.test(head) &&
      /\b(const|let|var|function|class|import|export)\b/.test(head)) return 'typescript';

  // 13. JavaScript
  if (/\b(const|let|var)\s+\w+\s*=|=>\s*[{(]|\brequire\s*\(|module\.exports|import\s+\w|export\s+(default|const|function|class)/.test(head)) return 'javascript';

  // 14. CSS / SCSS
  if (/[.#]?\w[\w-]*\s*\{[^}]*:[^}]*\}/.test(head) && !/</.test(trimmed.slice(0, 30))) {
    if (/\$\w+\s*:|@mixin|@include|@extend/.test(head)) return 'scss';
    if (/@\w+\s*\{|@import\s+'/.test(head))             return 'less';
    return 'css';
  }

  // 15. Java
  if (/^(package|import)\s+[\w.]+;/m.test(head) && /\b(public|private|protected)\s+(class|interface|enum)\b/.test(head)) return 'java';

  // 16. C#
  if (/^using\s+[\w.]+;/m.test(head) && /\bnamespace\s+\w+|\b(public|private)\s+(class|interface|struct|enum)\b/.test(head)) return 'csharp';

  // 17. Go
  if (/^package\s+\w+/m.test(head) && /^import\s+[\w("]/m.test(head)) return 'go';

  // 18. Rust
  if (/^(use |fn |pub fn |pub struct |impl |mod |enum |trait )\w/m.test(head) && /->|::|\blet\s+mut\b/.test(head)) return 'rust';

  // 19. C / C++
  if (/#include\s*[<"]/.test(head)) {
    if (/\b(cout|cin|std::|vector<|unique_ptr|template\s*<|namespace\s+\w+)/.test(head)) return 'cpp';
    return 'c';
  }

  // 20. Ruby
  if (/^(require|gem)\s+['"]|\.each\s+do\s*\||\battr_(accessor|reader|writer)\b/.test(head)) return 'ruby';

  // 21. PHP
  if (/^<\?php|<\?php/.test(trimmed)) return 'php';

  // 22. Lua
  if (/^(local\s+\w+\s*=|function\s+\w+\s*\(|require\s*\(|--\[\[)/m.test(head)) return 'lua';

  // 23. Kotlin
  if (/^(package|import)\s+[\w.]+$/m.test(head) && /\bfun\s+\w+\s*\(|\bval\s+\w+|\bvar\s+\w+/.test(head)) return 'kotlin';

  // 24. Swift
  if (/\bimport\s+(Foundation|UIKit|SwiftUI|AppKit)\b/.test(head) || /\b(var|let)\s+\w+\s*:\s*\w+\s*=/.test(head) && /\bfunc\s+\w+/.test(head)) return 'swift';

  return null; // genuinely unknown
}

function toggleComment() {
  editor.getAction('editor.action.commentLine')?.run();
}

function blockComment() {
  editor.getAction('editor.action.blockComment')?.run();
}

function toUpperCase() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  editor.executeEdits('case', [{ range: sel, text: text.toUpperCase() }]);
}

function toLowerCase() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  editor.executeEdits('case', [{ range: sel, text: text.toLowerCase() }]);
}

function toTitleCase() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  editor.executeEdits('case', [{ range: sel, text: text.replace(/\w\S*/g, w => w[0].toUpperCase() + w.substr(1).toLowerCase()) }]);
}

function base64Encode() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  editor.executeEdits('b64', [{ range: sel, text: btoa(unescape(encodeURIComponent(text))) }]);
}

function base64Decode() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  try { editor.executeEdits('b64', [{ range: sel, text: decodeURIComponent(escape(atob(text))) }]); }
  catch { showToast('Invalid Base64'); }
}

function jsonFormat() {
  const model = editor.getModel();
  const sel = editor.getSelection();
  const useSelection = sel && !sel.isEmpty();
  const text = useSelection ? model.getValueInRange(sel) : model.getValue();
  try {
    const formatted = JSON.stringify(JSON.parse(text), null, 2);
    if (useSelection) editor.executeEdits('json', [{ range: sel, text: formatted }]);
    else model.setValue(formatted);
    showToast('JSON formatted');
  } catch (e) { showToast('Invalid JSON: ' + e.message); }
}

function jsonMinify() {
  const model = editor.getModel();
  try {
    const minified = JSON.stringify(JSON.parse(model.getValue()));
    model.setValue(minified);
    showToast('JSON minified');
  } catch (e) { showToast('Invalid JSON: ' + e.message); }
}

function xmlFormat() {
  const model = editor.getModel();
  const text = model.getValue();
  try {
    let indent = 0;
    const formatted = text
      .replace(/(>)\s*(<)/g, '$1\n$2')
      .split('\n')
      .map(line => {
        const stripped = line.trim();
        if (!stripped) return '';
        if (stripped.startsWith('</')) indent = Math.max(0, indent - 1);
        const result = '  '.repeat(indent) + stripped;
        if (!stripped.startsWith('</') && !stripped.startsWith('<?') && !stripped.endsWith('/>') && stripped.includes('<') && !stripped.includes('</'))
          indent++;
        return result;
      })
      .filter(l => l !== '')
      .join('\n');
    model.setValue(formatted);
    showToast('XML formatted');
  } catch { showToast('XML format failed'); }
}

function sortLines(dir) {
  const model = editor.getModel();
  const sel = editor.getSelection();
  const s = sel && !sel.isEmpty() ? sel.startLineNumber : 1;
  const e2 = sel && !sel.isEmpty() ? sel.endLineNumber : model.getLineCount();
  const lines = [];
  for (let i = s; i <= e2; i++) lines.push(model.getLineContent(i));
  lines.sort((a, b) => dir * a.localeCompare(b));
  const range = new monaco.Range(s, 1, e2, model.getLineMaxColumn(e2));
  editor.executeEdits('sort', [{ range, text: lines.join('\n') }]);
}

function removeEmptyLines() {
  const model = editor.getModel();
  model.setValue(model.getValue().split('\n').filter(l => l.trim()).join('\n'));
}

function removeDuplicateLines() {
  const model = editor.getModel();
  const seen = new Set();
  model.setValue(model.getValue().split('\n').filter(l => { if (seen.has(l)) return false; seen.add(l); return true; }).join('\n'));
}

function joinLines() {
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  const text = editor.getModel().getValueInRange(sel);
  editor.executeEdits('join', [{ range: sel, text: text.replace(/\s*\n\s*/g, ' ') }]);
}

async function runCurrentFile() {
  const tab = getActiveTab();
  if (!tab) return;

  if (tab.dirty) await saveTabFile(tab);
  if (!tab.filePath) { showToast('Save the file first'); return; }

  const fp = tab.filePath;
  const ext = fp.split('.').pop().toLowerCase();
  const cwd = fp.replace(/[\\/][^\\/]+$/, '');

  // Per-extension runner. Each entry returns the command line to type into
  // the integrated PTY (PowerShell on Windows). We deliberately reuse the
  // existing terminal so the user sees live stdout/stderr in the same panel.
  const runners = {
    py: `python "${fp}"`, python: `python "${fp}"`,
    js: `node "${fp}"`, mjs: `node "${fp}"`,
    ts: `ts-node "${fp}"`,
    rb: `ruby "${fp}"`,
    php: `php "${fp}"`,
    go: `go run "${fp}"`,
    rs: `cargo run`,
    sh: `bash "${fp}"`,
    ps1: `& "${fp}"`,           // PowerShell call operator handles spaces in path
    bat: `& "${fp}"`,
    cmd: `& "${fp}"`,
    java: `java "${fp}"`,
    r: `Rscript "${fp}"`,
    lua: `lua "${fp}"`,
  };

  const cmd = runners[ext];
  if (!cmd) { showToast(`No runner configured for .${ext}`); return; }

  // Open the terminal panel and wait for the PTY to actually be alive
  // (terminalCreate is async — sending input before it resolves is a no-op).
  await openTerminal();
  // openTerminal returns immediately if the term already exists; if it just
  // created one, the create IPC was fired inside a 50 ms setTimeout, so give
  // it a beat before typing into the PTY on first launch.
  await new Promise(r => setTimeout(r, 80));

  // Sniff the host OS so we use the right shell-chain operator. PowerShell
  // (Windows default) accepts both `;` and `&&`; bash/zsh (macOS + Linux)
  // require `&&`. Sending the wrong one wedges the prompt with a parse error.
  const isWin = /windows/i.test(navigator.userAgent || '');
  const chain = isWin ? ';' : '&&';
  const line = `cd "${cwd}" ${chain} ${cmd}\r`;
  await window.electronAPI.terminalInput(terminalId, line);
}

// ===== Status Bar =====
function updateStatusBar() {
  if (!editor) return;
  const activeTab = getActiveTab();
  if (activeTab && activeTab.type === 'game') {
    statusLnCol.textContent = `🎮 Dev Arcade`;
    statusLines.textContent = 'lines: —';
    statusLength.textContent = 'length: —';
    return;
  }
  if (activeTab && activeTab.type === 'whiteboard') {
    statusLnCol.textContent = `🖼 Whiteboard`;
    statusLines.textContent = 'lines: —';
    statusLength.textContent = 'length: —';
    return;
  }
  const pos = editor.getPosition();
  const sel = editor.getSelection();
  const model = editor.getModel();
  if (!pos || !model) return;
  const allSels = editor.getSelections() || [];
  if (allSels.length > 1) {
    const totalChars = allSels.reduce((sum, s) => sum + model.getValueInRange(s).length, 0);
    statusLnCol.textContent = `Ln : ${pos.lineNumber}    Col : ${pos.column}    ${allSels.length} selections (${totalChars} chars)`;
  } else {
    const selCount = sel && !sel.isEmpty() ? model.getValueInRange(sel).length : 0;
    const selLines = sel && !sel.isEmpty() ? sel.endLineNumber - sel.startLineNumber + 1 : 0;
    statusLnCol.textContent = `Ln : ${pos.lineNumber}    Col : ${pos.column}    Sel : ${selCount} | ${selLines > 1 ? selLines + ' lines' : selCount}`;
  }
  statusLines.textContent = `lines: ${model.getLineCount()}`;
  statusLength.textContent = `length: ${model.getValueLength()}`;
}

function updateTitle() {
  const tab = getActiveTab();
  if (!tab) return;
  let title;
  if (tab.type === 'game') {
    title = '🎮 Dev Arcade - Note++';
  } else if (tab.type === 'whiteboard') {
    title = `${tab.dirty ? '* ' : ''}${tab.name} - Note++`;
  } else {
    title = `${tab.dirty ? '* ' : ''}${tab.filePath || tab.name} - Note++`;
  }
  window.electronAPI.setTitle(title);
  const tbEl = document.getElementById('title-bar-text');
  if (tbEl) tbEl.textContent = title;
}

function updateLanguageStatus() {
  const tab = getActiveTab();
  if (!tab) return;
  if (tab.type === 'game') {
    statusLang.textContent = '🎮 Game';
    document.getElementById('status-encoding').textContent = '—';
    document.getElementById('status-eol').textContent = '—';
    return;
  }
  if (tab.type === 'whiteboard') {
    statusLang.textContent = '🖼 Whiteboard';
    document.getElementById('status-encoding').textContent = '—';
    document.getElementById('status-eol').textContent = '—';
    return;
  }
  const names = { plaintext:'Normal Text',javascript:'JavaScript',typescript:'TypeScript',python:'Python',java:'Java',c:'C',cpp:'C++',csharp:'C#',go:'Go',rust:'Rust',ruby:'Ruby',php:'PHP',html:'HTML',css:'CSS',json:'JSON',xml:'XML',markdown:'Markdown',mermaid:'Mermaid',sql:'SQL',shell:'Shell Script',powershell:'PowerShell',bat:'Batch',yaml:'YAML',lua:'Lua',kotlin:'Kotlin',swift:'Swift',r:'R',dockerfile:'Dockerfile',scss:'SCSS' };
  statusLang.textContent = names[tab.language] || tab.language;
  document.getElementById('status-encoding').textContent = tab.encoding;
  document.getElementById('status-eol').textContent = tab.eol;
}

// ===== Terminal =====
function setupTerminal() {
  window.electronAPI.onMenu('terminal-output', (id, data) => {
    if (term) term.write(data);
  });
  window.electronAPI.onMenu('terminal-exit', (id, code) => {
    if (term) term.writeln(`\r\n\x1b[90mProcess exited with code ${code}\x1b[0m`);
  });

  document.getElementById('btn-term-close').addEventListener('click', () => closeTerminalPanel());
  document.getElementById('btn-term-kill').addEventListener('click', () => killTerminal());
  document.getElementById('btn-term-clear').addEventListener('click', () => { if (term) term.clear(); });
  document.getElementById('term-new-btn').addEventListener('click', () => openTerminal(true));
}

async function openTerminal(force = false) {
  const panel = document.getElementById('terminal-panel');
  const handle = document.getElementById('terminal-resize-handle');

  if (!terminalOpen || force) {
    panel.classList.remove('hidden');
    handle.classList.remove('hidden');
    terminalOpen = true;
    document.getElementById('btn-terminal').classList.add('active');
  }

  if (!term) {
    // Lazy-load xterm + addon (~270 KB) on first terminal open. Users who
    // never use the integrated terminal pay nothing for it at startup.
    try { await ensureXterm(); }
    catch (e) {
      console.error('xterm failed to load:', e);
      panel.classList.add('hidden');
      handle.classList.add('hidden');
      terminalOpen = false;
      document.getElementById('btn-terminal').classList.remove('active');
      showToast('Terminal failed to load: ' + (e?.message || 'unknown'));
      return;
    }
    term = new Terminal({
      fontFamily: "'Cascadia Code', 'Fira Code', Consolas, monospace",
      fontSize: 13,
      theme: {
        background: '#1e1e1e',
        foreground: '#d4d4d4',
        cursor: '#d4d4d4',
        selectionBackground: '#264f78',
        black: '#1e1e1e', red: '#f44747',
        green: '#6a9955', yellow: '#d7ba7d',
        blue: '#569cd6', magenta: '#c678dd',
        cyan: '#4ec9b0', white: '#d4d4d4',
        brightBlack: '#808080', brightRed: '#f14c4c',
        brightGreen: '#89d185', brightYellow: '#dcdcaa',
        brightBlue: '#75beff', brightMagenta: '#c586c0',
        brightCyan: '#23d18b', brightWhite: '#e5e5e5',
      },
      cursorBlink: true,
      scrollback: 5000,
      convertEol: true,
    });

    fitAddon = new FitAddon.FitAddon();
    term.loadAddon(fitAddon);
    term.open(document.getElementById('xterm-container'));

    // Apply font size from prefs if saved
    const savedFontSize = parseInt(document.getElementById('pref-term-fontsize')?.value) || 13;
    if (savedFontSize !== 13) term.options.fontSize = savedFontSize;

    setTimeout(() => {
      try { fitAddon.fit(); } catch {}
      // Pass actual cols/rows to main process so PTY is sized correctly
      const dims = fitAddon.proposeDimensions?.() || { cols: 80, rows: 24 };
      window.electronAPI.terminalCreate(terminalId, { cols: dims.cols || 80, rows: dims.rows || 24 })
        .then(res => {
          if (!res.success) {
            term.writeln('\x1b[31mFailed to start terminal: ' + (res.error || 'unknown error') + '\x1b[0m');
          } else {
            const shell = document.getElementById('pref-shell')?.value?.trim() || 'PowerShell';
            term.writeln(`\x1b[32mNote++ Terminal\x1b[0m — ${shell}`);
            term.writeln('');
          }
        });
    }, 50);

    // Handle user input
    term.onData(data => {
      window.electronAPI.terminalInput(terminalId, data);
    });

    term.onKey(({ key, domEvent }) => {
      // Ctrl+C in terminal
      if (domEvent.ctrlKey && domEvent.key === 'c') {
        window.electronAPI.terminalInput(terminalId, '\x03');
      }
    });
  } else {
    try { fitAddon.fit(); } catch {}
  }
}

function closeTerminalPanel() {
  document.getElementById('terminal-panel').classList.add('hidden');
  document.getElementById('terminal-resize-handle').classList.add('hidden');
  terminalOpen = false;
  document.getElementById('btn-terminal').classList.remove('active');
  editor.focus();
}

async function killTerminal() {
  await window.electronAPI.terminalKill(terminalId);
  if (term) { term.dispose(); term = null; fitAddon = null; }
  closeTerminalPanel();
}

function toggleTerminal() {
  if (terminalOpen) closeTerminalPanel();
  else openTerminal();
}

function setupTerminalResize() {
  const handle = document.getElementById('terminal-resize-handle');
  const panel = document.getElementById('terminal-panel');
  let dragging = false;
  let startY = 0;
  let startH = 0;

  handle.addEventListener('mousedown', (e) => {
    dragging = true;
    startY = e.clientY;
    startH = panel.offsetHeight;
    document.body.style.cursor = 'ns-resize';
    document.body.style.userSelect = 'none';
  });

  document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    const delta = startY - e.clientY;
    const newH = Math.max(80, Math.min(startH + delta, window.innerHeight * 0.7));
    panel.style.height = newH + 'px';
    fitTerminal();
  });

  document.addEventListener('mouseup', () => {
    if (dragging) fitTerminal();
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  });
}

// Fit the xterm viewport and relay new cols/rows to the PTY
function fitTerminal() {
  if (!fitAddon || !term) return;
  try {
    fitAddon.fit();
    const dims = fitAddon.proposeDimensions?.() || { cols: term.cols, rows: term.rows };
    const cols = dims?.cols || term.cols;
    const rows = dims?.rows || term.rows;
    window.electronAPI.terminalResize(terminalId, cols, rows);
  } catch {}
}

// ===== File Tree Resize =====
function setupFileTreeResize() {
  const handle = document.getElementById('file-tree-resize');
  const tree = document.getElementById('file-tree');
  let dragging = false;
  let startX = 0, startW = 0;

  handle.addEventListener('mousedown', (e) => {
    dragging = true; startX = e.clientX; startW = tree.offsetWidth;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  });

  document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    const w = Math.max(140, Math.min(startW + (e.clientX - startX), 500));
    tree.style.width = w + 'px';
  });

  document.addEventListener('mouseup', () => {
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  });
}

// ===== Find & Replace =====
function openFindReplace(mode = 'find') {
  findReplaceMode = mode;
  findPanel.classList.remove('hidden');
  document.querySelectorAll('.fr-tab').forEach(t => t.classList.remove('active'));
  document.querySelector(`.fr-tab[data-tab="${mode}"]`).classList.add('active');
  document.getElementById('replace-row').style.display = mode === 'replace' ? 'flex' : 'none';
  const sel = editor.getSelection();
  if (sel && !sel.isEmpty()) {
    document.getElementById('find-input').value = editor.getModel().getValueInRange(sel);
  }
  document.getElementById('find-input').focus();
  document.getElementById('find-input').select();
}

function closeFindReplace() {
  findPanel.classList.add('hidden');
  clearSearchDecorations();
  editor.focus();
}

function getSearchOpts() {
  return {
    searchString: document.getElementById('find-input').value,
    isRegex: document.getElementById('opt-regex').checked,
    matchCase: document.getElementById('opt-match-case').checked,
    wholeWord: document.getElementById('opt-whole-word').checked,
  };
}

function doFind(dir = 1) {
  const opts = getSearchOpts();
  if (!opts.searchString) return;
  const model = editor.getModel();
  const matches = model.findMatches(opts.searchString, true, opts.isRegex, opts.matchCase, opts.wholeWord ? ' \t\n.,!?' : null, true);
  if (!matches.length) { setFindStatus(`"${opts.searchString}" not found`, true); clearSearchDecorations(); return; }

  const pos = editor.getPosition();
  let idx = dir === 1
    ? matches.findIndex(m => m.range.startLineNumber > pos.lineNumber || (m.range.startLineNumber === pos.lineNumber && m.range.startColumn > pos.column))
    : (() => { for (let i = matches.length - 1; i >= 0; i--) if (matches[i].range.endLineNumber < pos.lineNumber || (matches[i].range.endLineNumber === pos.lineNumber && matches[i].range.endColumn < pos.column)) return i; return -1; })();

  if (idx === -1) idx = dir === 1 ? 0 : matches.length - 1;
  const m = matches[idx];
  editor.setSelection(m.range);
  editor.revealRangeInCenter(m.range);
  updateFindDecorations(matches, idx);
  setFindStatus(`${idx + 1} of ${matches.length} match${matches.length !== 1 ? 'es' : ''}`);
}

function findAll() {
  const opts = getSearchOpts();
  if (!opts.searchString) return;
  const model = editor.getModel();
  const matches = model.findMatches(opts.searchString, true, opts.isRegex, opts.matchCase, opts.wholeWord ? ' \t\n.,!?' : null, true);
  if (!matches.length) { setFindStatus(`"${opts.searchString}" not found`, true); return; }
  updateFindDecorations(matches, -1);
  setFindStatus(`${matches.length} match${matches.length !== 1 ? 'es' : ''} found`);
  showSearchResults(opts.searchString, matches, opts);
}

// ── Notepad++-style "Search results" panel ───────────────────────────────
// Header banner shows total hits; under it a per-file group lists every
// match with its line number and full line content (search term highlighted).
// Clicking a row jumps the editor to that line + reveals it.
function showSearchResults(searchString, matches, opts) {
  const panel = document.getElementById('search-results-panel');
  const body  = document.getElementById('search-results-body');
  const summary = document.getElementById('search-results-summary');
  if (!panel || !body) return;
  const model = editor.getModel();
  const tab = getActiveTab();
  const fileName = tab?.name || tab?.filePath?.split(/[\\/]/).pop() || 'Untitled';

  summary.textContent =
    `Search "${truncate(searchString, 40)}"  (${matches.length} hit${matches.length !== 1 ? 's' : ''} in 1 file of 1 searched)`;

  // Group rows by line — Notepad++ shows one row per match, with line content
  const escapeHtml = (s) => s.replace(/[&<>"']/g, c =>
    ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));

  // Build the search regex once for highlighting
  let pattern;
  try {
    pattern = opts.isRegex
      ? new RegExp(searchString, opts.matchCase ? 'g' : 'gi')
      : new RegExp(searchString.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
                   opts.matchCase ? 'g' : 'gi');
  } catch { pattern = null; }

  const fileHeader = `<div class="sr-file-header" data-file="${escapeHtml(fileName)}">${escapeHtml(fileName)}  (${matches.length} hit${matches.length !== 1 ? 's' : ''})</div>`;
  const rows = matches.map(m => {
    const line = m.range.startLineNumber;
    let text = model.getLineContent(line);
    if (text.length > 240) text = text.slice(0, 240) + '…';
    const safe = escapeHtml(text);
    const highlighted = pattern
      ? safe.replace(pattern, s => `<mark>${s}</mark>`)
      : safe;
    return `<div class="sr-row" data-line="${line}" data-col="${m.range.startColumn}">` +
             `<span class="sr-row-line">Line ${line}:</span>` +
             `<span class="sr-row-content">${highlighted}</span>` +
           `</div>`;
  }).join('');

  body.innerHTML = fileHeader + rows;
  panel.classList.remove('hidden');

  // Wire click-to-navigate. Also promote the clicked match to "current" so
  // the orange gutter band + dot move to that line.
  body.querySelectorAll('.sr-row').forEach((row, rowIdx) => {
    row.addEventListener('click', () => {
      const ln  = parseInt(row.dataset.line, 10);
      const col = parseInt(row.dataset.col, 10) || 1;
      body.querySelectorAll('.sr-row.active').forEach(r => r.classList.remove('active'));
      row.classList.add('active');
      const range = new monaco.Range(ln, col, ln, col + searchString.length);
      editor.setSelection(range);
      editor.revealRangeInCenter(range);
      // Re-paint decorations with this match flagged as current
      updateFindDecorations(matches, rowIdx);
      editor.focus();
    });
  });
  // Wire the file-header click → collapse/expand
  body.querySelector('.sr-file-header')?.addEventListener('click', (e) => {
    e.currentTarget.classList.toggle('collapsed');
    body.querySelectorAll('.sr-row').forEach(r => {
      r.style.display = e.currentTarget.classList.contains('collapsed') ? 'none' : '';
    });
  });
}

function truncate(s, n) { return s.length > n ? s.slice(0, n - 1) + '…' : s; }

function hideSearchResults() {
  const panel = document.getElementById('search-results-panel');
  if (panel) panel.classList.add('hidden');
}

// Paint match decorations:
//   - background highlight on each match (and a brighter one for "current")
//   - a yellow bar in the line-numbers gutter on every line with a match
//   - a ● marker in the glyph margin (further left) for current match
//   - ticks on the right-side overview ruler so you can see match density at a glance
function updateFindDecorations(matches, currentIdx) {
  if (!editor) return;
  const overviewRuler = monaco.editor.OverviewRulerLane
    ? { color: '#facc15', position: monaco.editor.OverviewRulerLane.Right }
    : undefined;
  const overviewRulerCurrent = monaco.editor.OverviewRulerLane
    ? { color: '#f97316', position: monaco.editor.OverviewRulerLane.Right }
    : undefined;
  const decos = matches.map((m, i) => {
    const isCurrent = i === currentIdx;
    return {
      range: m.range,
      options: {
        className: isCurrent ? 'find-highlight-current' : 'find-highlight',
        inlineClassName: isCurrent ? 'find-highlight-inline-current' : 'find-highlight-inline',
        linesDecorationsClassName: isCurrent ? 'find-line-marker-current' : 'find-line-marker',
        glyphMarginClassName: isCurrent ? 'find-glyph-current' : 'find-glyph',
        overviewRuler: isCurrent ? overviewRulerCurrent : overviewRuler,
        stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
      },
    };
  });
  searchDecorations = editor.deltaDecorations(searchDecorations, decos);
  // Cache for click-from-results-panel to highlight clicked match as "current"
  lastSearchMatches = matches;
}

// Cached matches from the most recent search (used by the results panel to
// promote the clicked row to "current" so the orange marker moves).
let lastSearchMatches = [];

function doReplace() {
  const opts = getSearchOpts();
  const repl = document.getElementById('replace-input').value;
  if (!opts.searchString) return;
  doFind(1);
  const sel = editor.getSelection();
  if (!sel || sel.isEmpty()) return;
  editor.executeEdits('replace', [{ range: sel, text: repl }]);
  doFind(1);
}

function doReplaceAll() {
  const opts = getSearchOpts();
  const repl = document.getElementById('replace-input').value;
  if (!opts.searchString) return;
  const model = editor.getModel();
  const matches = model.findMatches(opts.searchString, true, opts.isRegex, opts.matchCase, opts.wholeWord ? ' \t\n.,!?' : null, true);
  if (!matches.length) { setFindStatus(`Not found`, true); return; }
  editor.executeEdits('replace-all', matches.reverse().map(m => ({ range: m.range, text: repl })));
  setFindStatus(`${matches.length} replacement${matches.length !== 1 ? 's' : ''} made`);
}

function countMatches() {
  const opts = getSearchOpts();
  if (!opts.searchString) return;
  const n = editor.getModel().findMatches(opts.searchString, true, opts.isRegex, opts.matchCase, opts.wholeWord ? ' \t\n.,!?' : null, true).length;
  setFindStatus(`Found: ${n} match${n !== 1 ? 'es' : ''}`);
}

function clearSearchDecorations() { if (editor) searchDecorations = editor.deltaDecorations(searchDecorations, []); }

function setFindStatus(msg, err = false) {
  const el = document.getElementById('find-status-bar');
  el.textContent = msg;
  el.style.color = err ? '#c00000' : '#007000';
}

// ===== Word Wrap & Dark Mode =====
function toggleWordWrap() {
  isWordWrap = !isWordWrap;
  editor.updateOptions({ wordWrap: isWordWrap ? 'on' : 'off' });
  document.getElementById('btn-wordwrap').classList.toggle('active', isWordWrap);
  saveSetting('ui.wordWrap', isWordWrap);          // persist across sessions
}

function toggleColumnSelectMode() {
  isColumnSelectMode = !isColumnSelectMode;
  editor.updateOptions({ columnSelection: isColumnSelectMode });
  updateColSelectStatus();
  saveSetting('ui.columnSelectMode', isColumnSelectMode);
}

function updateColSelectStatus() {
  if (statusCol) statusCol.classList.toggle('active', isColumnSelectMode);
}

// Alt+drag → column/box selection (Notepad++ style).
// Registered on document in capture phase so it fires before Monaco's own
// mousedown handlers — Monaco then reads columnSelection:true when it sets
// up the drag, producing a box selection instead of a normal selection.
// Restored to false on mouseup unless persistent COL mode is active.
function setupAltMouseColumnSelect() {
  document.addEventListener('mousedown', (e) => {
    if (e.button !== 0 || !e.altKey || e.shiftKey || e.ctrlKey || e.metaKey) return;
    if (!editor?.getDomNode()?.contains(e.target)) return;
    editor.updateOptions({ columnSelection: true });
    document.addEventListener('mouseup', () => {
      if (!isColumnSelectMode) editor.updateOptions({ columnSelection: false });
    }, { once: true });
  }, { capture: true });
}

// ===== Theme System =====

// OS theme bridge. `prefers-color-scheme` is a standard Chromium/Electron media
// query that reflects the host OS theme on Windows, macOS AND Linux — no
// platform-specific code needed. Used to implement the "Follow Windows" option.
const systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)');

/** Resolve a user preference ('light'|'dark'|'system') to a concrete THEMES key. */
function resolveThemePref(pref) {
  if (pref === 'system') return systemThemeQuery.matches ? 'dark' : 'light';
  return pref in THEMES ? pref : 'light';
}

// When the OS theme changes while the user is on "Follow Windows", re-apply
// immediately so the switch is instant. Don't re-save (preference is unchanged).
systemThemeQuery.addEventListener('change', () => {
  if (themePref === 'system') applyTheme('system', false);
});

/**
 * Apply a theme by id ('light', 'dark', 'flower', …).
 * @param {string} id   - key in THEMES
 * @param {boolean} save - persist to settings (default true)
 */
// Toggle between custom themed title bar + HTML menu bar vs native OS chrome.
// No restart required — we show/hide #title-bar and toggle the native menu bar via IPC.
async function applyTitlebarMode(themed, save = true) {
  isThemedTitlebar = themed;
  document.documentElement.classList.toggle('native-titlebar-start', !themed);
  document.body.classList.toggle('native-titlebar', !themed);
  try { localStorage.setItem('notepp.ui.themedTitlebar', themed ? '1' : '0'); } catch {}
  const bar = document.getElementById('title-bar');
  if (bar) bar.style.display = themed ? '' : 'none';
  // Show/hide native OS menu bar (only meaningful with titleBarStyle:'hidden').
  window.electronAPI?.setMenuBarVisibility?.(!themed);
  // When switching back to themed, re-sync the WCO overlay colour.
  if (themed) {
    const t = THEMES[currentTheme] || THEMES.light;
    window.electronAPI?.setTitleBarOverlay?.(t.preview.tabbar, t.preview.text);
  }
  // Keep checkbox in sync if picker is open.
  const chk = document.getElementById('chk-themed-titlebar');
  if (chk) chk.checked = themed;
  if (save) {
    // Single read-modify-write captures both settings in one write, so a
    // concurrent applyTheme saveSetting can't race and lose either value.
    const s = await window.electronAPI.readSettings();
    (s.ui = s.ui || {}).themedTitlebar = themed;
    s.ui.theme = themePref;
    await window.electronAPI.writeSettings(s);
    // titleBarStyle is a BrowserWindow constructor option — only takes effect on restart.
    const r = await window.electronAPI?.messageDialog?.({
      type: 'info',
      title: 'Restart Required',
      message: 'Restart Note++ to apply the title bar change.',
      buttons: ['Restart Now', 'Later'],
      defaultId: 0,
      cancelId: 1,
    });
    if (r?.response === 0) window.electronAPI?.relaunchApp?.();
  }
}

async function applyTheme(pref, save = true) {
  // `pref` is the user preference: 'light' | 'dark' | 'system' (or any THEMES key).
  themePref = (pref === 'system' || pref in THEMES) ? pref : 'light';
  const id = resolveThemePref(themePref);
  const theme = THEMES[id] || THEMES.light;
  currentTheme = id in THEMES ? id : 'light';
  isDarkMode = theme.isDark;

  // Keep a synchronous cache for first-paint startup (prevents theme flash on
  // restart). Store the PREFERENCE so 'system' can be re-resolved on next boot.
  try { localStorage.setItem('notepp.ui.theme', themePref); } catch {}

  // Remove all theme-* classes, then add the one we want
  document.body.classList.forEach(cls => {
    if (cls.startsWith('theme-')) document.body.classList.remove(cls);
  });
  document.body.classList.add('theme-' + currentTheme);

  // Define the Monaco theme on demand right before applying it. Monaco only
  // registers the *active* theme upfront and the rest lazily (on idle), so a
  // theme switch that lands before that idle work (e.g. starting in dark via
  // "Follow Windows") could call setTheme() on an unregistered theme — which
  // Monaco silently ignores, leaving the editor pane light while the chrome
  // (pure CSS) goes dark. Defining first makes setTheme always succeed.
  const monacoThemeId = 'notepp-' + currentTheme;
  monaco.editor.defineTheme(monacoThemeId, {
    base: theme.monacoBase,
    inherit: true,
    rules: theme.monacoRules,
    colors: theme.monacoColors,
  });
  monaco.editor.setTheme(monacoThemeId);
  document.getElementById('btn-darkmode')?.classList.toggle('active', isDarkMode);
  syncMermaidThemeToAppMode();
  sendToWhiteboard({ type: 'wb-theme', dark: isDarkMode });
  sendToDrawio({ type: 'dw-theme', dark: isDarkMode });
  // Sync native Win32 window-control overlay colour (titleBarStyle:'hidden')
  window.electronAPI?.setTitleBarOverlay?.(theme.preview.tabbar, theme.preview.text);

  // Highlight the active card in the picker (if open)
  document.querySelectorAll('.theme-card').forEach(card => {
    card.classList.toggle('active', card.dataset.theme === currentTheme);
  });

  if (save) {
    // Single read-modify-write so the two keys can't race against each other
    // (or against a concurrent applyTitlebarMode save) and lose one of the values.
    const s = await window.electronAPI.readSettings();
    (s.ui = s.ui || {}).theme = themePref; // persist the PREFERENCE (incl. 'system')
    s.ui.darkMode = isDarkMode; // keep legacy key in sync
    await window.electronAPI.writeSettings(s);
  }
}

/** Legacy toggle used by toolbar dark-mode button */
function toggleDarkMode() {
  applyTheme(isDarkMode ? 'light' : 'dark');
}

// ===== Theme Picker =====

function openThemePicker() {
  const dlg = document.getElementById('theme-picker');
  if (!dlg) return;
  // Render cards each time so the active state is fresh
  renderThemeCards();
  // Sync the checkbox to current state
  const chk = document.getElementById('chk-themed-titlebar');
  if (chk) chk.checked = isThemedTitlebar;
  dlg.classList.remove('hidden');
}

function closeThemePicker() {
  document.getElementById('theme-picker')?.classList.add('hidden');
  editor.focus();
}

function renderThemeCards() {
  const grid = document.getElementById('theme-card-grid');
  if (!grid) return;
  grid.innerHTML = '';
  Object.entries(THEMES).forEach(([id, t]) => {
    const card = document.createElement('div');
    card.className = 'theme-card' + (id === currentTheme ? ' active' : '');
    card.dataset.theme = id;
    card.title = t.label;
    card.innerHTML = `
      <div class="theme-card-preview">
        <div class="tcp-tabbar" style="background:${t.preview.tabbar};color:${t.preview.text}">
          <span class="tcp-tab" style="background:${t.preview.editor};color:${t.preview.text}">file.js ×</span>
        </div>
        <div class="tcp-editor" style="background:${t.preview.editor};color:${t.preview.text}">
          <span style="opacity:.5">1</span> <span style="color:${t.preview.statusbar}">function</span> hello() {<br>
          <span style="opacity:.5">2</span>   return <span style="opacity:.7">"world"</span>;<br>
          <span style="opacity:.5">3</span> }
        </div>
        <div class="tcp-statusbar" style="background:${t.preview.statusbar};color:#fff">${t.label}</div>
      </div>
      <div class="theme-card-label">${t.label}</div>
    `;
    card.addEventListener('click', () => {
      applyTheme(id);
      document.querySelectorAll('.theme-card').forEach(c =>
        c.classList.toggle('active', c.dataset.theme === id));
    });
    grid.appendChild(card);
  });
}

function setupThemePicker() {
  const dlg = document.getElementById('theme-picker');
  if (!dlg) return;
  // Close button
  dlg.querySelector('.modal-close')?.addEventListener('click', closeThemePicker);
  dlg.querySelector('#btn-theme-close')?.addEventListener('click', closeThemePicker);
  // Title bar mode checkbox
  const chk = document.getElementById('chk-themed-titlebar');
  if (chk) chk.addEventListener('change', () => applyTitlebarMode(chk.checked));
  // Click outside the box closes
  dlg.addEventListener('click', e => { if (e.target === dlg) closeThemePicker(); });
}

// ===== Zoom =====
let currentFontSize = 13;
function zoomIn()    { currentFontSize = Math.min(currentFontSize + 2, 40); applyZoom(); }
function zoomOut()   { currentFontSize = Math.max(currentFontSize - 2,  8); applyZoom(); }
function zoomReset() { currentFontSize = 13; applyZoom(); }
function applyZoom() {
  editor.updateOptions({ fontSize: currentFontSize });
  saveSetting('ui.fontSize', currentFontSize);     // persist editor zoom across sessions
}

// ===== Zen Mode =====
function toggleZenMode() {
  isZenMode = !isZenMode;
  document.body.classList.toggle('zen-mode', isZenMode);
  window.electronAPI.setFullScreen(isZenMode);
  // Defer layout so the DOM reflows first (hiding chrome changes available height)
  requestAnimationFrame(() => editor?.layout());
  if (isZenMode) showToast('Zen mode — press F11 or Esc to exit');
}

// ===== Disk Auto-Save =====
// Writes dirty editor files (with a real saved path) back to disk on a
// fixed interval.  Unlike session auto-save (which writes to AppData), this
// keeps the actual file on disk current.  Opt-in; skips encrypted tabs.

function startDiskAutoSave(enabled, intervalSecs) {
  stopDiskAutoSave();
  if (enabled && intervalSecs > 0) {
    diskAutoSaveTimer = setInterval(runDiskAutoSave, intervalSecs * 1000);
  }
}

function stopDiskAutoSave() {
  if (diskAutoSaveTimer) { clearInterval(diskAutoSaveTimer); diskAutoSaveTimer = null; }
}

async function runDiskAutoSave() {
  const dirty = tabs.filter(t =>
    t.type === 'editor' && t.dirty && t.filePath && !t.encrypted
  );
  if (!dirty.length) return;
  let saved = 0;
  for (const tab of dirty) {
    try {
      // For the active tab use the editor model directly; others use tab.content.
      const content = (tab.id === activeTabId) ? editor.getValue() : tab.content;
      const res = await window.electronAPI.writeFile(tab.filePath, content);
      if (res.success) {
        tab.dirty = false;
        tab.content = content;
        try { window.electronAPI.fileSavedByApp(tab.filePath); } catch {}
        saved++;
      }
    } catch {}
  }
  if (saved > 0) {
    renderTabs();
    updateTitle();
    showToast(`Auto-saved ${saved} file${saved > 1 ? 's' : ''}`);
  }
}

async function loadDiskAutoSavePrefs() {
  const s = await window.electronAPI.readSettings();
  const das = s.diskAutoSave || {};
  const enableEl   = document.getElementById('pref-disk-autosave-enable');
  const intervalEl = document.getElementById('pref-disk-autosave-interval');
  if (enableEl)   enableEl.checked = !!das.enabled;
  if (intervalEl) intervalEl.value = das.intervalSecs || 30;
}

async function saveDiskAutoSavePrefs(settings) {
  settings.diskAutoSave = {
    enabled:     !!document.getElementById('pref-disk-autosave-enable')?.checked,
    intervalSecs: parseInt(document.getElementById('pref-disk-autosave-interval')?.value || '30'),
  };
}

// ===== Large-file Safe Mode =====
// Files over 10 MB disable expensive Monaco features (IntelliSense, minimap,
// folding, etc.) to keep the editor responsive.  The tab carries a
// `largeFileSafeMode` flag; options are restored when the user switches to a
// normal tab.

function _applyLargeFileMode() {
  editor.updateOptions({
    minimap:          { enabled: false },
    folding:          false,
    quickSuggestions: false,
    parameterHints:   { enabled: false },
    hover:            { enabled: false },
    codeLens:         false,
    inlayHints:       { enabled: 'off' },
    renderWhitespace: 'none',
    guides:           { indentation: false },
  });
}

function _restoreNormalEditorMode() {
  // Re-read current prefs from the (always-populated) pref dialog DOM so we
  // don't overwrite any setting the user changed while in the large-file tab.
  const g = id => document.getElementById(id);
  editor.updateOptions({
    minimap:          { enabled: g('pref-minimap')?.checked ?? true },
    folding:          true,
    quickSuggestions: (g('pref-intellisense')?.checked ?? true)
                        ? { other: 'on', comments: 'off', strings: 'off' } : false,
    parameterHints:   { enabled: g('pref-param-hints')?.checked ?? true },
    hover:            { enabled: g('pref-hover')?.checked ?? true, delay: 200 },
    codeLens:         g('pref-codelen')?.checked ?? true,
    inlayHints:       { enabled: (g('pref-inline-hints')?.checked ?? true) ? 'on' : 'off' },
    renderWhitespace: (g('pref-whitespace')?.checked ?? false) ? 'all' : 'selection',
    guides:           { indentation: true },
  });
}

function _checkLargeFile(tab, size) {
  if (!size || size <= LARGE_FILE_THRESHOLD) return;
  tab.largeFileSafeMode = true;
  tab.largeFileBytes = size;
  _applyLargeFileMode();
  const mb = (size / 1024 / 1024).toFixed(1);
  showToast(`⚠ Large file (${mb} MB) — IntelliSense & minimap disabled for performance`);
  // Very large files (> 50 MB): also strip syntax highlighting entirely
  if (size > LARGE_FILE_THRESHOLD * 5) {
    monaco.editor.setModelLanguage(tab.model, 'plaintext');
    tab.language = 'plaintext';
    updateLanguageStatus();
  }
}

// ===== Macro Recording & Playback =====
// Records editor operations (text input, cursor movement, deletions) as a
// replayable sequence.  Stored in settings.macros; keyed by name.

// Map from onKeyDown e.browserEvent.key → op shape
function _handleMacroKeydown(e) {
  const be   = e.browserEvent || e;
  const key  = be.key;
  const sh   = be.shiftKey;
  const ctrl = be.ctrlKey;

  switch (key) {
    case 'Backspace': {
      const last = currentMacroOps[currentMacroOps.length - 1];
      if (last?.op === 'backspace') { last.count++; return; }
      currentMacroOps.push({ op: 'backspace', count: 1 }); return;
    }
    case 'Delete': {
      const last = currentMacroOps[currentMacroOps.length - 1];
      if (last?.op === 'delete') { last.count++; return; }
      currentMacroOps.push({ op: 'delete', count: 1 }); return;
    }
    case 'ArrowLeft':  currentMacroOps.push({ op: 'cursorLeft',  sh, ctrl }); return;
    case 'ArrowRight': currentMacroOps.push({ op: 'cursorRight', sh, ctrl }); return;
    case 'ArrowUp':    currentMacroOps.push({ op: 'cursorUp',    sh }); return;
    case 'ArrowDown':  currentMacroOps.push({ op: 'cursorDown',  sh }); return;
    case 'Home':       currentMacroOps.push({ op: 'cursorHome',  sh, ctrl }); return;
    case 'End':        currentMacroOps.push({ op: 'cursorEnd',   sh, ctrl }); return;
    case 'PageUp':     currentMacroOps.push({ op: 'cursorPageUp',   sh }); return;
    case 'PageDown':   currentMacroOps.push({ op: 'cursorPageDown', sh }); return;
    // Tab / Enter come through onDidType — skip here
    case 'Tab': case 'Enter': return;
    // Modifier-only keys are noise
    case 'Control': case 'Shift': case 'Alt': case 'Meta': return;
    default:
      // Ctrl+key editor actions worth recording
      if (ctrl && !sh && !be.altKey) {
        const actions = {
          ']': { op: 'action', id: 'editor.action.indentLines' },
          '[': { op: 'action', id: 'editor.action.outdentLines' },
          '/': { op: 'action', id: 'editor.action.commentLine' },
        };
        if (actions[key]) { currentMacroOps.push(actions[key]); return; }
      }
      if (ctrl && sh) {
        const actions = {
          'K': { op: 'action', id: 'editor.action.deleteLines' },
        };
        if (actions[key.toUpperCase()]) { currentMacroOps.push(actions[key.toUpperCase()]); return; }
      }
      if (!ctrl && !sh && be.altKey) {
        if (key === 'ArrowUp')   { currentMacroOps.push({ op: 'action', id: 'editor.action.moveLinesUpAction' }); return; }
        if (key === 'ArrowDown') { currentMacroOps.push({ op: 'action', id: 'editor.action.moveLinesDownAction' }); return; }
      }
  }
}

function _playMacroOp(op) {
  if (!editor) return;
  switch (op.op) {
    case 'type':
      editor.trigger('macro', 'type', { text: op.text }); break;
    case 'backspace':
      for (let i = 0; i < (op.count || 1); i++) editor.trigger('macro', 'deleteLeft', null); break;
    case 'delete':
      for (let i = 0; i < (op.count || 1); i++) editor.trigger('macro', 'deleteRight', null); break;
    case 'cursorLeft': {
      const cmd = op.ctrl ? (op.sh ? 'cursorWordLeftSelect' : 'cursorWordLeft')
                          : (op.sh ? 'cursorLeftSelect'     : 'cursorLeft');
      editor.trigger('macro', cmd, null); break;
    }
    case 'cursorRight': {
      const cmd = op.ctrl ? (op.sh ? 'cursorWordRightSelect' : 'cursorWordRight')
                          : (op.sh ? 'cursorRightSelect'     : 'cursorRight');
      editor.trigger('macro', cmd, null); break;
    }
    case 'cursorUp':
      editor.trigger('macro', op.sh ? 'cursorUpSelect'   : 'cursorUp',   null); break;
    case 'cursorDown':
      editor.trigger('macro', op.sh ? 'cursorDownSelect' : 'cursorDown', null); break;
    case 'cursorHome': {
      const cmd = op.ctrl ? (op.sh ? 'cursorTopSelect'  : 'cursorTop')
                          : (op.sh ? 'cursorHomeSelect' : 'cursorHome');
      editor.trigger('macro', cmd, null); break;
    }
    case 'cursorEnd': {
      const cmd = op.ctrl ? (op.sh ? 'cursorBottomSelect' : 'cursorBottom')
                          : (op.sh ? 'cursorEndSelect'    : 'cursorEnd');
      editor.trigger('macro', cmd, null); break;
    }
    case 'cursorPageUp':
      editor.trigger('macro', op.sh ? 'cursorPageUpSelect'   : 'cursorPageUp',   null); break;
    case 'cursorPageDown':
      editor.trigger('macro', op.sh ? 'cursorPageDownSelect' : 'cursorPageDown', null); break;
    case 'action':
      editor.trigger('macro', op.id, null); break;
  }
}

function toggleMacroRecording() {
  if (isRecording) _stopMacroRecording(); else _startMacroRecording();
}

function _startMacroRecording() {
  isRecording = true;
  currentMacroOps = [];
  _updateMacroRecIndicator(true);
  showToast('⏺ Recording macro — press Ctrl+Shift+R to stop');
}

function _stopMacroRecording() {
  isRecording = false;
  _updateMacroRecIndicator(false);
  const ops = [...currentMacroOps];
  currentMacroOps = [];
  if (!ops.length) { showToast('No operations recorded'); return; }
  lastRecordedMacro = { name: 'Last Recording', ops };
  showToast(`⏹ Recorded ${ops.length} operation${ops.length > 1 ? 's' : ''} — open Macros menu to save or run`);
  // Prompt to save with a name
  openSaveMacroDialog(ops);
}

function _updateMacroRecIndicator(on) {
  const el  = document.getElementById('status-macro-rec');
  const sep = document.getElementById('status-macro-sep-rec');
  if (el)  el.classList.toggle('hidden', !on);
  if (sep) sep.style.display = on ? '' : 'none';
}

function runLastMacro(times) {
  const macro = lastRecordedMacro || (savedMacros.length ? savedMacros[0] : null);
  if (!macro) { showToast('No macro recorded yet — press Ctrl+Shift+R to start recording'); return; }
  runMacro(macro, times);
}

function runMacro(macro, times = 1) {
  const tab = getActiveTab();
  if (!tab || tab.type !== 'editor') { showToast('Switch to an editor tab to run a macro'); return; }
  editor.focus();
  for (let t = 0; t < times; t++) {
    for (const op of macro.ops) _playMacroOp(op);
  }
  if (times > 1) showToast(`Ran "${macro.name}" ×${times}`);
}

function openSaveMacroDialog(ops) {
  const dlg = document.getElementById('macro-save-dialog');
  if (!dlg) return;
  const input = document.getElementById('macro-save-name');
  input.value = `Macro ${savedMacros.length + 1}`;
  dlg._pendingOps = ops;
  dlg.classList.remove('hidden');
  requestAnimationFrame(() => { input.focus(); input.select(); });
}

async function saveMacroFromDialog() {
  const dlg = document.getElementById('macro-save-dialog');
  const name = document.getElementById('macro-save-name')?.value?.trim();
  if (!name) { showToast('Enter a name for the macro'); return; }
  const ops = dlg._pendingOps || [];
  const macro = { id: 'macro-' + Date.now(), name, ops };
  savedMacros.push(macro);
  // Update lastRecordedMacro to use the saved name too
  if (lastRecordedMacro) lastRecordedMacro.name = name;
  dlg.classList.add('hidden');
  await _persistMacros();
  showToast(`Macro "${name}" saved`);
}

function openRunNTimesDialog(macro) {
  const dlg = document.getElementById('macro-run-n-dialog');
  if (!dlg) return;
  document.getElementById('macro-run-n-name').textContent = macro?.name || 'Last Macro';
  dlg._macro = macro || lastRecordedMacro;
  document.getElementById('macro-run-n-count').value = '10';
  dlg.classList.remove('hidden');
  requestAnimationFrame(() => document.getElementById('macro-run-n-count').select());
}

function openManageMacrosDialog() {
  _renderMacroList();
  document.getElementById('macro-manage-dialog')?.classList.remove('hidden');
}

function _renderMacroList() {
  const list = document.getElementById('macro-list');
  if (!list) return;
  list.innerHTML = '';
  if (!savedMacros.length) {
    list.innerHTML = '<div class="macro-empty">No saved macros. Record one with Ctrl+Shift+R.</div>';
    return;
  }
  savedMacros.forEach((macro, idx) => {
    const row = document.createElement('div');
    row.className = 'macro-list-row';
    row.innerHTML =
      `<span class="macro-list-name" title="${_escHtml(macro.name)}">${_escHtml(macro.name)}</span>` +
      `<span class="macro-list-meta">${macro.ops.length} ops</span>` +
      `<button class="macro-btn macro-run-btn"   data-idx="${idx}" title="Run once">▶</button>` +
      `<button class="macro-btn macro-run-n-btn" data-idx="${idx}" title="Run N times">▶×N</button>` +
      `<button class="macro-btn macro-del-btn"   data-idx="${idx}" title="Delete">✕</button>`;
    list.appendChild(row);
  });
}

function _escHtml(s) {
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
}

async function _persistMacros() {
  try {
    const s = await window.electronAPI.readSettings();
    s.macros = savedMacros;
    await window.electronAPI.writeSettings(s);
  } catch (e) { console.error('[macros] persist failed', e); }
}

async function loadSavedMacros() {
  try {
    const s = await window.electronAPI.readSettings();
    savedMacros = Array.isArray(s.macros) ? s.macros : [];
  } catch {}
}

function setupMacroDialogs() {
  // Save dialog
  document.getElementById('btn-macro-save-ok')?.addEventListener('click', saveMacroFromDialog);
  document.getElementById('btn-macro-save-cancel')?.addEventListener('click', () => {
    document.getElementById('macro-save-dialog').classList.add('hidden');
  });
  document.getElementById('macro-save-name')?.addEventListener('keydown', e => {
    if (e.key === 'Enter') saveMacroFromDialog();
    if (e.key === 'Escape') document.getElementById('macro-save-dialog').classList.add('hidden');
  });

  // Run N times dialog
  document.getElementById('btn-macro-run-n-ok')?.addEventListener('click', () => {
    const count = parseInt(document.getElementById('macro-run-n-count')?.value || '1') || 1;
    const macro = document.getElementById('macro-run-n-dialog')?._macro;
    document.getElementById('macro-run-n-dialog').classList.add('hidden');
    if (macro) runMacro(macro, Math.max(1, Math.min(count, 10000)));
  });
  document.getElementById('btn-macro-run-n-cancel')?.addEventListener('click', () => {
    document.getElementById('macro-run-n-dialog').classList.add('hidden');
  });
  document.getElementById('macro-run-n-count')?.addEventListener('keydown', e => {
    if (e.key === 'Enter') document.getElementById('btn-macro-run-n-ok')?.click();
    if (e.key === 'Escape') document.getElementById('macro-run-n-dialog').classList.add('hidden');
  });

  // Manage dialog
  document.getElementById('btn-macro-manage-close')?.addEventListener('click', () => {
    document.getElementById('macro-manage-dialog').classList.add('hidden');
  });
  document.getElementById('macro-list')?.addEventListener('click', async e => {
    const btn = e.target.closest('button');
    if (!btn) return;
    const idx = parseInt(btn.dataset.idx);
    const macro = savedMacros[idx];
    if (!macro) return;
    if (btn.classList.contains('macro-run-btn')) {
      document.getElementById('macro-manage-dialog').classList.add('hidden');
      runMacro(macro, 1);
    } else if (btn.classList.contains('macro-run-n-btn')) {
      document.getElementById('macro-manage-dialog').classList.add('hidden');
      openRunNTimesDialog(macro);
    } else if (btn.classList.contains('macro-del-btn')) {
      savedMacros.splice(idx, 1);
      await _persistMacros();
      _renderMacroList();
    }
  });
}

// ===== Go To Line =====
function openGotoLine() {
  const dlg = document.getElementById('goto-dialog');
  document.getElementById('goto-max-line').textContent = editor.getModel()?.getLineCount() || 1;
  dlg.classList.remove('hidden');
  setTimeout(() => document.getElementById('goto-line-input').focus(), 50);
}
function gotoLine(n) {
  const max = editor.getModel()?.getLineCount() || 1;
  const line = Math.max(1, Math.min(n, max));
  editor.revealLineInCenter(line);
  editor.setPosition({ lineNumber: line, column: 1 });
  editor.focus();
}

// ===== Quick Open (Ctrl+P) =====
let quickSelectedIdx = 0;

function setupQuickOpen() {
  const overlay = document.getElementById('quick-open');
  const input = document.getElementById('quick-input');
  const results = document.getElementById('quick-results');

  input.addEventListener('input', () => renderQuickResults(input.value, results));

  input.addEventListener('keydown', (e) => {
    const items = results.querySelectorAll('.quick-item');
    if (e.key === 'ArrowDown') { quickSelectedIdx = Math.min(quickSelectedIdx + 1, items.length - 1); updateQuickSelection(items); e.preventDefault(); }
    else if (e.key === 'ArrowUp') { quickSelectedIdx = Math.max(quickSelectedIdx - 1, 0); updateQuickSelection(items); e.preventDefault(); }
    else if (e.key === 'Enter') {
      const sel = items[quickSelectedIdx];
      if (sel) { sel.click(); }
    }
    else if (e.key === 'Escape') closeQuickOpen();
  });

  overlay.addEventListener('click', (e) => { if (e.target === overlay) closeQuickOpen(); });
}

function openQuickOpen() {
  const overlay = document.getElementById('quick-open');
  const input = document.getElementById('quick-input');
  overlay.classList.remove('hidden');
  input.value = '';
  quickSelectedIdx = 0;
  renderQuickResults('', document.getElementById('quick-results'));
  input.focus();
}

function closeQuickOpen() {
  document.getElementById('quick-open').classList.add('hidden');
  editor.focus();
}

function renderQuickResults(query, container) {
  container.innerHTML = '';
  quickSelectedIdx = 0;
  const filtered = query
    ? tabs.filter(t => t.name.toLowerCase().includes(query.toLowerCase()) || (t.filePath && t.filePath.toLowerCase().includes(query.toLowerCase())))
    : tabs;

  filtered.forEach((tab, i) => {
    const item = document.createElement('div');
    item.className = 'quick-item' + (i === 0 ? ' selected' : '');
    item.innerHTML = `<span class="quick-item-icon">${getFileEmoji(tab.name)}</span><span class="quick-item-name">${highlight(tab.name, query)}</span><span class="quick-item-path">${tab.filePath || 'unsaved'}</span>`;
    item.addEventListener('click', () => { activateTab(tab.id); closeQuickOpen(); });
    container.appendChild(item);
  });
}

function updateQuickSelection(items) {
  items.forEach((el, i) => el.classList.toggle('selected', i === quickSelectedIdx));
  items[quickSelectedIdx]?.scrollIntoView({ block: 'nearest' });
}

function highlight(text, query) {
  if (!query) return text;
  return text.replace(new RegExp(`(${query.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')})`, 'gi'), '<span class="quick-match">$1</span>');
}

// ===== Command Palette (Ctrl+Shift+P) =====
const COMMANDS = [
  { label: 'Format Document', icon: '⎇', action: () => formatDocument() },
  { label: 'Toggle Comment', icon: '∥', action: () => toggleComment() },
  { label: 'Toggle Dark Mode', icon: '🌙', action: () => toggleDarkMode() },
  { label: 'Toggle Word Wrap', icon: '↩', action: () => toggleWordWrap() },
  { label: 'Toggle Terminal', icon: '⌨', action: () => toggleTerminal() },
  { label: 'New Tab', icon: '📄', action: () => newTab() },
  { label: 'Open File', icon: '📂', action: () => openFile() },
  { label: 'Save File', icon: '💾', action: () => saveFile() },
  { label: 'Save All', icon: '💾', action: () => saveAll() },
  { label: 'Close Tab', icon: '×', action: () => closeTab(activeTabId) },
  { label: 'Find in File', icon: '🔍', action: () => openFindReplace('find') },
  { label: 'Replace in File', icon: '🔄', action: () => openFindReplace('replace') },
  { label: 'Go to Line', icon: '→', action: () => openGotoLine() },
  { label: 'Fold All', icon: '▼', action: () => editor.getAction('editor.foldAll')?.run() },
  { label: 'Unfold All', icon: '▲', action: () => editor.getAction('editor.unfoldAll')?.run() },
  { label: 'Pretty Print JSON', icon: '{}', action: () => jsonFormat() },
  { label: 'Pretty Print XML', icon: '<>', action: () => xmlFormat() },
  { label: 'Minify JSON', icon: '{}', action: () => jsonMinify() },
  { label: 'Base64 Encode', icon: '🔡', action: () => base64Encode() },
  { label: 'Base64 Decode', icon: '🔤', action: () => base64Decode() },
  { label: 'Sort Lines Ascending', icon: '↑', action: () => sortLines(1) },
  { label: 'Sort Lines Descending', icon: '↓', action: () => sortLines(-1) },
  { label: 'Remove Duplicate Lines', icon: '⊘', action: () => removeDuplicateLines() },
  { label: 'Remove Empty Lines', icon: '⌫', action: () => removeEmptyLines() },
  { label: 'Run File', icon: '▶', action: () => runCurrentFile() },
  { label: 'Toggle Preview (HTML/MD)', icon: '👁', action: () => togglePreview() },
  { label: 'Open Preferences', icon: '⚙', action: () => openPreferences() },
  { label: 'Regex Tester', icon: '🔣', action: () => document.getElementById('regex-tester').classList.remove('hidden') },
  { label: 'UPPERCASE', icon: 'A', action: () => toUpperCase() },
  { label: 'lowercase', icon: 'a', action: () => toLowerCase() },
  { label: 'Title Case', icon: 'Aa', action: () => toTitleCase() },
  { label: 'Join Lines', icon: '⤵', action: () => joinLines() },
  { label: 'Zoom In', icon: '+', action: () => zoomIn() },
  { label: 'Zoom Out', icon: '-', action: () => zoomOut() },
  { label: 'Reset Zoom', icon: '=', action: () => zoomReset() },
];

let cmdSelectedIdx = 0;

function setupCmdPalette() {
  const overlay = document.getElementById('cmd-palette');
  const input = document.getElementById('cmd-input');
  const results = document.getElementById('cmd-results');

  input.addEventListener('input', () => renderCmdResults(input.value.replace(/^>\s*/, ''), results));

  input.addEventListener('keydown', (e) => {
    const items = results.querySelectorAll('.quick-item');
    if (e.key === 'ArrowDown') { cmdSelectedIdx = Math.min(cmdSelectedIdx + 1, items.length - 1); updateCmdSelection(items); e.preventDefault(); }
    else if (e.key === 'ArrowUp') { cmdSelectedIdx = Math.max(cmdSelectedIdx - 1, 0); updateCmdSelection(items); e.preventDefault(); }
    else if (e.key === 'Enter') { items[cmdSelectedIdx]?.click(); }
    else if (e.key === 'Escape') closeCmdPalette();
  });

  overlay.addEventListener('click', (e) => { if (e.target === overlay) closeCmdPalette(); });
}

function openCmdPalette() {
  const overlay = document.getElementById('cmd-palette');
  const input = document.getElementById('cmd-input');
  overlay.classList.remove('hidden');
  input.value = '> ';
  cmdSelectedIdx = 0;
  renderCmdResults('', document.getElementById('cmd-results'));
  setTimeout(() => { input.focus(); input.setSelectionRange(2, 2); }, 10);
}

function closeCmdPalette() {
  document.getElementById('cmd-palette').classList.add('hidden');
  editor.focus();
}

function renderCmdResults(query, container) {
  container.innerHTML = '';
  cmdSelectedIdx = 0;
  const q = query.toLowerCase();
  const filtered = q ? COMMANDS.filter(c => c.label.toLowerCase().includes(q)) : COMMANDS;
  filtered.forEach((cmd, i) => {
    const item = document.createElement('div');
    item.className = 'quick-item' + (i === 0 ? ' selected' : '');
    item.innerHTML = `<span class="quick-item-icon">${cmd.icon}</span><span class="quick-item-name">${highlight(cmd.label, query)}</span>`;
    item.addEventListener('click', () => { cmd.action(); closeCmdPalette(); });
    container.appendChild(item);
  });
}

function updateCmdSelection(items) {
  items.forEach((el, i) => el.classList.toggle('selected', i === cmdSelectedIdx));
  items[cmdSelectedIdx]?.scrollIntoView({ block: 'nearest' });
}

// ===== Regex Tester =====
function setupRegexTester() {
  document.getElementById('btn-rx-test').addEventListener('click', testRegex);
  document.getElementById('btn-rx-apply').addEventListener('click', applyRegex);
  document.getElementById('rx-pattern').addEventListener('input', testRegex);
  document.getElementById('rx-test').addEventListener('input', testRegex);
  document.getElementById('rx-flags').addEventListener('input', testRegex);
}

function testRegex() {
  const pattern = document.getElementById('rx-pattern').value;
  const flags = document.getElementById('rx-flags').value;
  const test = document.getElementById('rx-test').value;
  const replace = document.getElementById('rx-replace').value;
  const info = document.getElementById('rx-info');
  const result = document.getElementById('rx-result');

  if (!pattern) { result.textContent = test; info.textContent = ''; return; }

  try {
    const re = new RegExp(pattern, flags);
    const matches = [...test.matchAll(new RegExp(pattern, flags.includes('g') ? flags : flags + 'g'))];
    info.textContent = `${matches.length} match${matches.length !== 1 ? 'es' : ''}`;
    info.style.color = matches.length ? '#007000' : '#c00000';

    if (replace !== '') {
      result.textContent = test.replace(re, replace);
    } else {
      // Highlight matches
      let html = '';
      let last = 0;
      matches.forEach(m => {
        html += escHtml(test.slice(last, m.index));
        html += `<mark style="background:#f9c74f;color:#000">${escHtml(m[0])}</mark>`;
        last = m.index + m[0].length;
      });
      html += escHtml(test.slice(last));
      result.innerHTML = html;
    }
  } catch (e) {
    info.textContent = 'Error: ' + e.message;
    info.style.color = '#c00000';
    result.textContent = test;
  }
}

function applyRegex() {
  const pattern = document.getElementById('rx-pattern').value;
  const flags = document.getElementById('rx-flags').value;
  const replace = document.getElementById('rx-replace').value;
  if (!pattern) return;
  try {
    const model = editor.getModel();
    const text = model.getValue();
    const result = text.replace(new RegExp(pattern, flags), replace);
    model.setValue(result);
    document.getElementById('regex-tester').classList.add('hidden');
    editor.focus();
  } catch (e) { showToast('Regex error: ' + e.message); }
}

function escHtml(t) { return t.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }

// ===== Context Menu =====
function setupContextMenu() {
  document.getElementById('monaco-editor').addEventListener('contextmenu', async (e) => {
    e.preventDefault();
    dismissFloatingMenus();
    const menu = contextMenu;
    // Filter static items based on the active tab's language so the
    // menu only shows entries that make sense for the current file
    // type (e.g. no "Pretty Print JSON" in a Markdown tab).
    _applyContextMenuLanguageFilter();
    // Inject spell suggestions at the top of the menu if the user right-
    // clicked on a misspelled word. This is the path most users expect —
    // Monaco's bulb / quick-fix popup is in a separate UI layer that
    // we've intentionally disabled in favour of our own context menu.
    await _injectSpellSuggestionsIntoContextMenu(e.clientX, e.clientY);
    menu.classList.remove('hidden');
    menu.style.left = Math.min(e.clientX, window.innerWidth - 210) + 'px';
    menu.style.top = Math.min(e.clientY, window.innerHeight - 260) + 'px';
  });
  document.addEventListener('click', () => {
    contextMenu.classList.add('hidden');
    _clearSpellSuggestionsFromContextMenu();
  });
  // Right-clicking anywhere else (e.g., on a tab header) should also dismiss
  // the static editor menu — `click` doesn't fire for right-clicks.
  document.addEventListener('contextmenu', (e) => {
    if (!e.target.closest('#monaco-editor')) {
      contextMenu.classList.add('hidden');
      _clearSpellSuggestionsFromContextMenu();
    }
  }, true);
  document.querySelectorAll('.ctx-item[data-action]').forEach(item => {
    item.addEventListener('click', () => { handleContextAction(item.dataset.action); contextMenu.classList.add('hidden'); });
  });
}

// Dynamically-injected spell items live under this marker so we can
// strip them when the menu closes without disturbing the static entries.
const SPELL_CTX_CLASS = 'ctx-item-spell-dyn';

function _clearSpellSuggestionsFromContextMenu() {
  if (!contextMenu) return;
  contextMenu.querySelectorAll('.' + SPELL_CTX_CLASS).forEach(el => el.remove());
}

async function _injectSpellSuggestionsIntoContextMenu(clientX, clientY) {
  _clearSpellSuggestionsFromContextMenu();
  if (!editor) return;
  const tab = getActiveTab();
  if (!isSpellEligibleTab(tab)) return;

  // Map screen coords to a Monaco position.
  const target = editor.getTargetAtClientPoint?.(clientX, clientY);
  const pos = target?.position;
  if (!pos) return;

  // Find a misspelled token at the clicked position. Use containsPosition
  // for a precise hit-test — we only want to surface suggestions when the
  // user clicked exactly on a flagged word.
  const t = (_spellLastTokens || []).find(tok =>
    tok.range && tok.range.containsPosition(pos)
  );
  if (!t) return;

  // Resolve suggestions (cache-aware, single IPC at most).
  let suggestions = _spellSuggestCache.get(t.lookup);
  if (!suggestions) {
    try {
      const r = await window.electronAPI.spell.suggest(t.word, 6);
      if (r && r.success) suggestions = r.suggestions || [];
    } catch (e) { console.warn('[spell] suggest IPC failed', e); }
    suggestions = suggestions || [];
    _spellSuggestCache.set(t.lookup, suggestions);
  }

  const frag = document.createDocumentFragment();
  if (suggestions.length === 0) {
    const empty = document.createElement('div');
    empty.className = 'ctx-item ' + SPELL_CTX_CLASS;
    empty.style.opacity = '0.6';
    empty.style.fontStyle = 'italic';
    empty.textContent = 'No suggestions for "' + t.word + '"';
    frag.appendChild(empty);
  } else {
    suggestions.slice(0, 6).forEach(s => {
      const item = document.createElement('div');
      item.className = 'ctx-item ' + SPELL_CTX_CLASS;
      item.textContent = '✓ Replace with "' + s + '"';
      const replacement = _matchCase(t.word, s);
      item.addEventListener('click', () => {
        try {
          editor.executeEdits('spell-replace', [{ range: t.range, text: replacement }]);
          editor.focus();
        } catch (err) { showToast('Replace failed: ' + (err?.message || err)); }
        contextMenu.classList.add('hidden');
        _clearSpellSuggestionsFromContextMenu();
      });
      frag.appendChild(item);
    });
  }
  // "Add to dictionary" entry — always present so the user can teach
  // Note++ a name / acronym / domain term they use often.
  const addItem = document.createElement('div');
  addItem.className = 'ctx-item ' + SPELL_CTX_CLASS;
  addItem.textContent = '📚 Add "' + t.word + '" to dictionary';
  addItem.addEventListener('click', async () => {
    try {
      await window.electronAPI.spell.addWord(t.word);
      _spellCache.set(t.lookup, true);
      _spellSuggestCache.delete(t.lookup);
      scheduleSpellScan();
      showToast('Added "' + t.word + '" to dictionary');
    } catch (err) { showToast('Add failed: ' + (err?.message || err)); }
    contextMenu.classList.add('hidden');
    _clearSpellSuggestionsFromContextMenu();
  });
  frag.appendChild(addItem);

  // Separator between dynamic spell items and the static Cut/Copy/Paste
  const sep = document.createElement('div');
  sep.className = 'ctx-sep ' + SPELL_CTX_CLASS;
  frag.appendChild(sep);

  // Inject at the top of the menu.
  contextMenu.insertBefore(frag, contextMenu.firstChild);
}

function handleContextAction(action) {
  const acts = {
    'cut': () => editor.trigger('ctx', 'editor.action.clipboardCutAction', null),
    'copy': () => editor.trigger('ctx', 'editor.action.clipboardCopyAction', null),
    'paste': () => pasteFromClipboardClean(),
    'select-all': () => editor.trigger('ctx', 'selectAll', null),
    'format-doc': formatDocument,
    'toggle-comment': toggleComment,
    'toggle-preview': () => togglePreview(),
    'find': () => openFindReplace('find'),
    'replace': () => openFindReplace('replace'),
    'google-search': () => { const sel = editor.getSelection(); const text = sel && !sel.isEmpty() ? editor.getModel().getValueInRange(sel) : ''; if (text) window.open(`https://www.google.com/search?q=${encodeURIComponent(text)}`); },
    'b64-encode': base64Encode,
    'b64-decode': base64Decode,
    'json-format': jsonFormat,
    'json-minify': jsonMinify,
    'md-toggle-bold':   () => _mdWrapSelection('**', '**'),
    'md-toggle-italic': () => _mdWrapSelection('*', '*'),
    'md-insert-code':   () => _mdInsertCodeBlock(),
    'html-format':      formatDocument,
    'add-cursor-above':  () => editor.getAction('editor.action.insertCursorAbove')?.run(),
    'add-cursor-below':  () => editor.getAction('editor.action.insertCursorBelow')?.run(),
    'cursor-line-ends':  () => editor.getAction('editor.action.insertCursorAtEndOfEachLineSelected')?.run(),
    'select-all-occ':    () => editor.getAction('editor.action.selectHighlights')?.run(),
    'col-select-mode':   () => toggleColumnSelectMode(),
  };
  acts[action]?.();
  editor.focus();
}

// ── New context-menu helpers ───────────────────────────────────────────────
function jsonMinify() {
  try {
    const parsed = JSON.parse(editor.getValue());
    editor.setValue(JSON.stringify(parsed));
  } catch (e) {
    showToast('Invalid JSON: ' + (e?.message || e));
  }
}

// Markdown wrap: surround the current selection with `prefix` and
// `suffix`. If nothing is selected, insert the markers around the
// caret so the user can start typing in between (matches the bold/
// italic affordance most editors provide).
function _mdWrapSelection(prefix, suffix) {
  if (!editor) return;
  const sel = editor.getSelection();
  const model = editor.getModel();
  if (!sel || !model) return;
  if (sel.isEmpty()) {
    editor.executeEdits('md-wrap', [{ range: sel, text: prefix + suffix }]);
    // Move the caret between the markers
    const after = editor.getPosition();
    editor.setPosition({ lineNumber: after.lineNumber, column: after.column - suffix.length });
  } else {
    const text = model.getValueInRange(sel);
    editor.executeEdits('md-wrap', [{ range: sel, text: prefix + text + suffix }]);
  }
}

function _mdInsertCodeBlock() {
  if (!editor) return;
  const sel = editor.getSelection();
  const model = editor.getModel();
  if (!sel || !model) return;
  const text = sel.isEmpty() ? '' : model.getValueInRange(sel);
  const block = '```\n' + text + '\n```\n';
  editor.executeEdits('md-codeblock', [{ range: sel, text: block }]);
  if (!text) {
    // Drop caret into the empty block so the user can start typing
    const pos = editor.getPosition();
    editor.setPosition({ lineNumber: pos.lineNumber - 2, column: 1 });
  }
}

// Apply the data-langs / data-not-langs filter on the static context-
// menu items so only entries relevant to the active tab's language
// are visible. Also collapses runs of separators that end up adjacent
// after items are hidden (so the menu doesn't look gappy).
function _applyContextMenuLanguageFilter() {
  const menu = contextMenu;
  if (!menu) return;
  const tab = getActiveTab();
  const lang = (tab && tab.type === 'editor') ? tab.language : null;

  // Pass 1 — show/hide individual items based on language gates.
  menu.querySelectorAll('.ctx-item').forEach(item => {
    // Skip the dynamically-injected spell items — they manage their
    // own visibility lifecycle.
    if (item.classList.contains('ctx-item-spell-dyn')) return;
    const allowed = item.dataset.langs    ? item.dataset.langs.split(',').map(s => s.trim()) : null;
    const denied  = item.dataset.notLangs ? item.dataset.notLangs.split(',').map(s => s.trim()) : null;
    let visible = true;
    if (allowed && (!lang || !allowed.includes(lang))) visible = false;
    if (denied && lang && denied.includes(lang))      visible = false;
    item.style.display = visible ? '' : 'none';
  });

  // Pass 2 — collapse separators that no longer divide anything.
  // Walk the children: a separator is only visible if there's at least
  // one visible non-separator item between it and the previous visible
  // separator (or the start of the menu).
  let sawItemSinceLastSep = false;
  let lastVisibleSep = null;
  for (const child of menu.children) {
    if (child.classList.contains('ctx-sep')) {
      if (!sawItemSinceLastSep) {
        child.style.display = 'none';
      } else {
        child.style.display = '';
        lastVisibleSep = child;
        sawItemSinceLastSep = false;
      }
    } else if (child.style.display !== 'none') {
      sawItemSinceLastSep = true;
    }
  }
  // Trailing separator with nothing after it — also hide.
  if (lastVisibleSep && !sawItemSinceLastSep) {
    lastVisibleSep.style.display = 'none';
  }
}

// ===== Drag & Drop =====
// Skip the "Drop files to open" overlay when the active tab hosts an
// iframe (whiteboard / drawio). Those iframes handle their own file
// drops natively — Excalidraw imports the SVG as an image, drawio adds
// the file to the diagram. Intercepting them here strands the file on
// the parent overlay ("Drop files to open" stuck on screen) and never
// forwards it, because the parent's `preventDefault` on `dragover`
// eats the events before the iframe can see them.
function _dropShouldBypassIframe() {
  const t = getActiveTab();
  return t && (t.type === 'whiteboard' || t.type === 'drawio');
}
function setupDragDrop() {
  document.addEventListener('dragover', (e) => {
    if (_dropShouldBypassIframe()) return;   // Let the iframe handle it.
    e.preventDefault();
    document.body.classList.add('drag-over');
  });
  document.addEventListener('dragleave', (e) => { if (!e.relatedTarget) document.body.classList.remove('drag-over'); });
  document.addEventListener('drop', (e) => {
    if (_dropShouldBypassIframe()) return;   // Iframe already handled it.
    e.preventDefault(); document.body.classList.remove('drag-over');
    const files = Array.from(e.dataTransfer.files)
      .map(f => {
        try { return window.electronAPI.getPathForFile(f); } catch { return null; }
      })
      .filter(p => typeof p === 'string' && p.length > 0);
    if (files.length) openFile(files);
  });
}

// ===== Find/Replace setup =====
function setupFindReplace() {
  document.getElementById('find-close').addEventListener('click', closeFindReplace);
  document.getElementById('btn-close-find').addEventListener('click', closeFindReplace);
  document.getElementById('btn-find-next').addEventListener('click', () => doFind(1));
  document.getElementById('btn-find-prev').addEventListener('click', () => doFind(-1));
  document.getElementById('btn-find-all').addEventListener('click', findAll);

  // Search-results panel buttons
  document.getElementById('btn-sr-close')?.addEventListener('click', hideSearchResults);
  document.getElementById('btn-sr-clear')?.addEventListener('click', () => {
    document.getElementById('search-results-body').innerHTML = '';
    document.getElementById('search-results-summary').textContent = 'Search results';
  });
  document.getElementById('btn-sr-collapse')?.addEventListener('click', () => {
    const body = document.getElementById('search-results-body');
    const btn  = document.getElementById('btn-sr-collapse');
    const collapsed = body.style.display === 'none';
    body.style.display = collapsed ? '' : 'none';
    btn.textContent    = collapsed ? '▾' : '▸';
  });
  document.getElementById('btn-count').addEventListener('click', countMatches);
  document.getElementById('btn-replace').addEventListener('click', doReplace);
  document.getElementById('btn-replace-all').addEventListener('click', doReplaceAll);
  document.getElementById('find-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') e.shiftKey ? doFind(-1) : doFind(1);
    if (e.key === 'Escape') closeFindReplace();
  });
  // Live-update gutter + minimap markers as the user types so the matches
  // are visible immediately without pressing Enter.
  let findInputDebounce;
  document.getElementById('find-input').addEventListener('input', () => {
    clearTimeout(findInputDebounce);
    findInputDebounce = setTimeout(() => {
      const opts = getSearchOpts();
      if (!opts.searchString) { clearSearchDecorations(); setFindStatus(''); return; }
      const matches = editor.getModel().findMatches(
        opts.searchString, true, opts.isRegex, opts.matchCase,
        opts.wholeWord ? ' \t\n.,!?' : null, true
      );
      if (!matches.length) { clearSearchDecorations(); setFindStatus(`"${opts.searchString}" not found`, true); return; }
      updateFindDecorations(matches, -1);
      setFindStatus(`${matches.length} match${matches.length !== 1 ? 'es' : ''}`);
    }, 120);
  });
  document.getElementById('replace-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') doReplace();
    if (e.key === 'Escape') closeFindReplace();
  });
  document.querySelectorAll('.fr-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.fr-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      switchFindMode(tab.dataset.tab);
    });
  });

  // Find in Files
  document.getElementById('btn-fif-browse')?.addEventListener('click', async () => {
    const dir = await window.electronAPI.openFolderPicker();
    if (dir) document.getElementById('fif-directory').value = dir;
  });
  document.getElementById('btn-fif-search')?.addEventListener('click', () => doFindInFiles());
  document.getElementById('fif-directory')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doFindInFiles();
  });
  document.getElementById('fif-filter')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') doFindInFiles();
  });

  // Mark
  document.querySelectorAll('.mark-swatch').forEach(sw => {
    sw.addEventListener('click', () => {
      document.querySelectorAll('.mark-swatch').forEach(s => s.classList.remove('active'));
      sw.classList.add('active');
    });
  });
  document.getElementById('btn-mark-all')?.addEventListener('click', () => doMarkAll());
  document.getElementById('btn-mark-clear')?.addEventListener('click', () => clearAllMarks());
}

// Show / hide the per-mode rows + Find-action buttons in the find panel.
function switchFindMode(mode) {
  document.getElementById('replace-row').style.display       = mode === 'replace'        ? 'flex' : 'none';
  document.getElementById('find-in-files-row').style.display = mode === 'find-in-files'  ? 'flex' : 'none';
  document.getElementById('mark-row').style.display          = mode === 'mark'           ? 'flex' : 'none';
  // Find Next/Previous/All/Count are only meaningful in Find or Replace mode;
  // hide them in Mark + Find-in-Files (those tabs have their own action buttons).
  const actionsVisible = (mode === 'find' || mode === 'replace');
  const actions = document.getElementById('find-action-buttons');
  if (actions) actions.style.display = actionsVisible ? 'inline-flex' : 'none';
}

// ── Find in Files ─────────────────────────────────────────────────────────
async function doFindInFiles() {
  const opts = getSearchOpts();
  if (!opts.searchString) { setFindStatus('Type a search pattern', true); return; }
  let root = document.getElementById('fif-directory').value.trim();
  if (!root) {
    // Default: active file's folder, or the git repo root if set
    const tab = getActiveTab();
    if (activeGitRepo)       root = activeGitRepo;
    else if (tab?.filePath)  root = tab.filePath.replace(/[\\/][^\\/]+$/, '');
    if (!root)               { setFindStatus('Pick a directory first', true); return; }
    document.getElementById('fif-directory').value = root;
  }
  const filter = document.getElementById('fif-filter').value.trim() || '*.*';
  setFindStatus('Searching…');
  const r = await window.electronAPI.findInFiles({
    root, pattern: opts.searchString, filter,
    matchCase: opts.matchCase, isRegex: opts.isRegex, wholeWord: opts.wholeWord,
  });
  if (!r.success) { setFindStatus('Find in Files failed: ' + (r.error || 'unknown'), true); return; }
  if (!r.totalHits) { setFindStatus(`"${opts.searchString}" not found (scanned ${r.filesScanned} file${r.filesScanned !== 1 ? 's' : ''})`, true); return; }
  setFindStatus(`${r.totalHits} hit${r.totalHits !== 1 ? 's' : ''} in ${r.files.length} file${r.files.length !== 1 ? 's' : ''}${r.capped ? ' (capped)' : ''}`);
  showFifResults(opts.searchString, r, opts);
}

// Render results from Find-in-Files into the Notepad++-style results panel.
function showFifResults(searchString, fifResult, opts) {
  const panel   = document.getElementById('search-results-panel');
  const body    = document.getElementById('search-results-body');
  const summary = document.getElementById('search-results-summary');
  if (!panel || !body) return;

  summary.textContent =
    `Search "${truncate(searchString, 40)}"  (${fifResult.totalHits} hit${fifResult.totalHits !== 1 ? 's' : ''} in ${fifResult.files.length} file${fifResult.files.length !== 1 ? 's' : ''} of ${fifResult.filesScanned} searched)`;

  const escapeHtml = (s) => s.replace(/[&<>"']/g, c =>
    ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));

  let pattern;
  try {
    pattern = opts.isRegex
      ? new RegExp(searchString, opts.matchCase ? 'g' : 'gi')
      : new RegExp(searchString.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'),
                   opts.matchCase ? 'g' : 'gi');
  } catch { pattern = null; }

  const html = fifResult.files.map(f => {
    const shortName = f.path.split(/[\\/]/).pop();
    const dir = f.path.slice(0, f.path.length - shortName.length).replace(/[\\/]$/, '');
    const header = `<div class="sr-file-header" data-path="${escapeHtml(f.path)}" title="${escapeHtml(f.path)}">` +
                   `${escapeHtml(shortName)}  (${f.hits.length} hit${f.hits.length !== 1 ? 's' : ''})` +
                   `<span style="margin-left:8px;font-weight:400;color:#888;font-size:11px">${escapeHtml(dir)}</span>` +
                   `</div>`;
    const rows = f.hits.map(h => {
      let text = h.content || '';
      if (text.length > 240) text = text.slice(0, 240) + '…';
      const safe = escapeHtml(text);
      const highlighted = pattern ? safe.replace(pattern, s => `<mark>${s}</mark>`) : safe;
      return `<div class="sr-row" data-path="${escapeHtml(f.path)}" data-line="${h.line}" data-col="${h.col}">` +
               `<span class="sr-row-line">Line ${h.line}:</span>` +
               `<span class="sr-row-content">${highlighted}</span>` +
             `</div>`;
    }).join('');
    return header + rows;
  }).join('');

  body.innerHTML = html;
  panel.classList.remove('hidden');

  // Click a row → open the file (or activate existing tab) + jump to the line
  body.querySelectorAll('.sr-row').forEach(row => {
    row.addEventListener('click', async () => {
      const fp = row.dataset.path;
      const ln = parseInt(row.dataset.line, 10);
      const col = parseInt(row.dataset.col, 10) || 1;
      body.querySelectorAll('.sr-row.active').forEach(r => r.classList.remove('active'));
      row.classList.add('active');
      const existing = _findTabByPath(fp);
      if (existing) activateTab(existing.id);
      else await openFile([fp]);
      // Allow Monaco a tick to finish setting up the model before revealing
      setTimeout(() => {
        const range = new monaco.Range(ln, col, ln, col + searchString.length);
        editor.setSelection(range);
        editor.revealRangeInCenter(range);
        editor.focus();
      }, 50);
    });
  });
  // Collapse / expand per-file group
  body.querySelectorAll('.sr-file-header').forEach(h => {
    h.addEventListener('click', () => {
      h.classList.toggle('collapsed');
      let n = h.nextElementSibling;
      while (n && !n.classList.contains('sr-file-header')) {
        n.style.display = h.classList.contains('collapsed') ? 'none' : '';
        n = n.nextElementSibling;
      }
    });
  });
}

// ── Mark — persistent colored highlights, independent of Find ────────────
let markDecorations = [];   // array of decoration-id arrays, one per Mark All call

function doMarkAll() {
  const opts = getSearchOpts();
  if (!opts.searchString) { setFindStatus('Type a pattern to mark', true); return; }
  const model = editor.getModel();
  const matches = model.findMatches(
    opts.searchString, true, opts.isRegex, opts.matchCase,
    opts.wholeWord ? ' \t\n.,!?' : null, true
  );
  if (!matches.length) { setFindStatus(`"${opts.searchString}" not found`, true); return; }
  const color = document.querySelector('.mark-swatch.active')?.dataset.color || 'yellow';
  const cls = 'mark-' + color;
  // Each Mark All run produces its own decoration set so they stack across colors
  const ids = editor.deltaDecorations([], matches.map(m => ({
    range: m.range,
    options: { inlineClassName: cls, className: cls, stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges },
  })));
  markDecorations.push(ids);
  setFindStatus(`Marked ${matches.length} occurrence${matches.length !== 1 ? 's' : ''} (${color})`);
}

function clearAllMarks() {
  for (const ids of markDecorations) {
    try { editor.deltaDecorations(ids, []); } catch {}
  }
  markDecorations = [];
  setFindStatus('All marks cleared');
}

// ===== Modals =====
function setupModals() {
  document.querySelectorAll('[data-modal]').forEach(btn => {
    btn.addEventListener('click', () => document.getElementById(btn.dataset.modal).classList.add('hidden'));
  });
  document.querySelectorAll('.modal-overlay').forEach(overlay => {
    overlay.addEventListener('click', e => { if (e.target === overlay) overlay.classList.add('hidden'); });
  });
  document.getElementById('btn-goto-ok').addEventListener('click', () => {
    const n = parseInt(document.getElementById('goto-line-input').value);
    if (!isNaN(n)) gotoLine(n);
    document.getElementById('goto-dialog').classList.add('hidden');
    editor.focus();
  });
  document.getElementById('goto-line-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') document.getElementById('btn-goto-ok').click();
    if (e.key === 'Escape') { document.getElementById('goto-dialog').classList.add('hidden'); editor.focus(); }
  });
  document.querySelectorAll('.prefs-item').forEach(item => {
    item.addEventListener('click', () => {
      document.querySelectorAll('.prefs-item').forEach(i => i.classList.remove('active'));
      document.querySelectorAll('.pref-page').forEach(p => p.classList.remove('active'));
      item.classList.add('active');
      document.getElementById('pref-' + item.dataset.pref)?.classList.add('active');
    });
  });
  document.getElementById('btn-prefs-ok').addEventListener('click', async () => {
    applyPreferences();
    await saveCloudPrefs();
    document.getElementById('prefs-dialog').classList.add('hidden');
  });

  // Themes page: two-mode model — "Custom" (user picks a specific theme
  // via the dropdown) or "Follow Windows" (live OS theme tracking).
  // Each user gesture applies immediately so the change is visible
  // without waiting for OK.
  const _setCustomDropdownVisible = () => {
    const wrap = document.getElementById('pref-custom-theme-wrap');
    const isCustom = document.getElementById('pref-theme-mode-custom')?.checked;
    if (wrap) wrap.style.opacity = isCustom ? '1' : '0.4';
    const sel = document.getElementById('pref-custom-theme');
    if (sel) sel.disabled = !isCustom;
  };
  document.querySelectorAll('input[name="pref-theme-mode"]').forEach(radio => {
    radio.addEventListener('change', () => {
      if (!radio.checked) return;
      _setCustomDropdownVisible();
      if (radio.value === 'system') {
        applyTheme('system');
      } else {
        // Custom — apply whichever theme the dropdown has selected.
        const sel = document.getElementById('pref-custom-theme');
        const themeId = sel?.value || 'light';
        applyTheme(themeId);
      }
    });
  });
  document.getElementById('pref-custom-theme')?.addEventListener('change', (e) => {
    // Switching the dropdown implicitly means the user wants Custom mode.
    document.getElementById('pref-theme-mode-custom').checked = true;
    _setCustomDropdownVisible();
    applyTheme(e.target.value);
  });

  setupCloudPrefButtons();
  setupAiPrefsPage();
  setupNewDocPrefsPage();
  setupBackupPrefsPage();
  setupEncryptionPrefsPage();
  setupMacroDialogs();
}

function applyPreferences() {
  const tabSize = parseInt(document.getElementById('pref-tab-size').value) || 4;
  const useSpaces = document.getElementById('pref-tab-as-spaces').checked;
  const lineNumbers = document.getElementById('pref-show-linenumbers').checked;
  const minimap = document.getElementById('pref-minimap').checked;
  const ligatures = document.getElementById('pref-ligatures').checked;
  const intellisense = document.getElementById('pref-intellisense').checked;
  const bracketColor = document.getElementById('pref-bracket-color').checked;
  const paramHints = document.getElementById('pref-param-hints').checked;
  const hover = document.getElementById('pref-hover').checked;
  const whitespace = document.getElementById('pref-whitespace').checked;
  const font = document.getElementById('pref-font').value;

  editor.updateOptions({
    tabSize, insertSpaces: useSpaces,
    lineNumbers: lineNumbers ? 'on' : 'off',
    minimap: { enabled: minimap },
    fontLigatures: ligatures,
    fontFamily: font,
    quickSuggestions: intellisense ? { other: 'on', comments: 'off', strings: 'off' } : false,
    parameterHints: { enabled: paramHints },
    hover: { enabled: hover, delay: 200 },
    bracketPairColorization: { enabled: bracketColor },
    renderWhitespace: whitespace ? 'all' : 'selection',
  });

  // Themes — read the two-radio model. 'system' or the dropdown's
  // currently-selected THEMES key. applyTheme persists the preference.
  const mode = document.querySelector('input[name="pref-theme-mode"]:checked')?.value || 'custom';
  const theme = mode === 'system'
    ? 'system'
    : (document.getElementById('pref-custom-theme')?.value || 'light');
  if (theme !== themePref) applyTheme(theme);

  // Spell-check autocorrect (Editing tab). Only takes effect when
  // spell-check is also enabled via the toolbar — see _maybeAutoCorrect.
  const acCb = document.getElementById('pref-spell-autocorrect');
  if (acCb && acCb.checked !== spellAutocorrect) setSpellAutocorrect(acCb.checked);

  // New-document defaults
  newDocDefaults.encoding = document.querySelector('input[name="pref-encoding"]:checked')?.value || 'UTF-8';
  newDocDefaults.eol      = document.querySelector('input[name="pref-eol"]:checked')?.value || 'Windows (CR LF)';
  newDocDefaults.language = document.getElementById('pref-default-lang')?.value || 'plaintext';
  newDocDefaults.template = document.getElementById('pref-new-template')?.value || '';

  // Auto-backup settings — restart timer if changed
  const bkpEnabled  = document.getElementById('pref-backup-enable')?.checked || false;
  const bkpInterval = parseInt(document.getElementById('pref-backup-interval')?.value || '5') * 60 * 1000;
  startAutoBackup(bkpEnabled, bkpInterval);

  // Disk auto-save — restart timer if settings changed
  const dasEnabled  = document.getElementById('pref-disk-autosave-enable')?.checked || false;
  const dasInterval = parseInt(document.getElementById('pref-disk-autosave-interval')?.value || '30');
  startDiskAutoSave(dasEnabled, dasInterval);
}

async function openPreferences() {
  document.getElementById('prefs-dialog').classList.remove('hidden');
  loadThemePref();
  loadSpellAutocorrectPref();
  await loadStartupModePref();
  await loadCloudPrefs();
  await loadNewDocPrefs();
  await loadBackupPrefs();
  await loadDiskAutoSavePrefs();
  await loadTerminalPrefs();
  refreshEncryptionPrefsPage();
}

// Sync the Editing → "Enable autocorrect" checkbox to its persisted
// value whenever Preferences opens.
function loadSpellAutocorrectPref() {
  const cb = document.getElementById('pref-spell-autocorrect');
  if (cb) cb.checked = !!spellAutocorrect;
}

// Sync the Themes radios + custom-theme dropdown to the saved
// preference whenever Preferences opens, so the dialog reflects
// reality. Also populates the dropdown the first time it's opened.
function loadThemePref() {
  const sel = document.getElementById('pref-custom-theme');
  if (sel && !sel.options.length) {
    // Populate from the THEMES table on first open.
    for (const [id, t] of Object.entries(THEMES)) {
      const opt = document.createElement('option');
      opt.value = id;
      opt.textContent = t.label || id;
      sel.appendChild(opt);
    }
  }
  const isSystem = themePref === 'system';
  const sysRadio = document.getElementById('pref-theme-mode-system');
  const cusRadio = document.getElementById('pref-theme-mode-custom');
  if (sysRadio) sysRadio.checked = isSystem;
  if (cusRadio) cusRadio.checked = !isSystem;
  // For Custom mode, the dropdown should reflect the actively-applied
  // theme. For Follow-Windows mode, fall back to the resolved currentTheme
  // (still useful — it lets the user see what's currently in effect).
  if (sel) {
    sel.value = isSystem ? currentTheme : (themePref in THEMES ? themePref : 'light');
    sel.disabled = isSystem;
  }
  const wrap = document.getElementById('pref-custom-theme-wrap');
  if (wrap) wrap.style.opacity = isSystem ? '0.4' : '1';
}

// ── Launch-on-startup pref ─────────────────────────────────────────────────
// Lives on the General page. Reads the current state (both Note++'s own
// settings and the OS-level login item) so the checkbox reflects reality
// even if the user disabled the autostart entry through Task Manager.
async function loadStartupModePref() {
  const cb = document.getElementById('pref-startup-mode');
  if (!cb) return;
  try {
    const s = await window.electronAPI.startupMode.get();
    cb.checked = !!(s && (s.enabled || s.osConfigured));
  } catch { cb.checked = false; }
  // Attach the listener once. The flag is stored on the element itself
  // so re-opening the prefs dialog doesn't stack handlers.
  if (cb.dataset._wired === '1') return;
  cb.dataset._wired = '1';
  cb.addEventListener('change', async () => {
    const enabled = cb.checked;
    try {
      const r = await window.electronAPI.startupMode.set(enabled);
      if (!r?.success) {
        cb.checked = !enabled; // revert UI
        showToast('Could not change startup setting: ' + (r?.error || 'unknown'));
        return;
      }
      showToast(enabled
        ? 'Note++ will launch on system startup (and stay in the tray)'
        : 'Note++ will no longer launch on system startup');
    } catch (e) {
      cb.checked = !enabled;
      showToast('Startup setting change failed: ' + (e?.message || String(e)));
    }
  });
}

// ===== Cloud Storage Prefs =====
const CLOUD_PROVIDERS = [
  { key: 'googledrive', enableId: 'cloud-gd-enabled', pathId: 'cloud-gd-path', statusId: 'cloud-gd-status', detectBtn: 'btn-gd-detect', browseBtn: 'btn-gd-browse' },
  { key: 'onedrive',    enableId: 'cloud-od-enabled', pathId: 'cloud-od-path', statusId: 'cloud-od-status', detectBtn: 'btn-od-detect', browseBtn: 'btn-od-browse' },
  { key: 'dropbox',     enableId: 'cloud-db-enabled', pathId: 'cloud-db-path', statusId: 'cloud-db-status', detectBtn: 'btn-db-detect', browseBtn: 'btn-db-browse' },
];

async function loadCloudPrefs() {
  const settings = await window.electronAPI.readSettings();
  const cloud = settings.cloud || {};

  for (const p of CLOUD_PROVIDERS) {
    const cfg = cloud[p.key] || {};
    document.getElementById(p.enableId).checked = !!cfg.enabled;
    document.getElementById(p.pathId).value = cfg.path || '';
    updateCloudStatus(p, cfg.path || '');
  }
}

async function saveCloudPrefs() {
  const settings = await window.electronAPI.readSettings();
  const cloud = {};

  for (const p of CLOUD_PROVIDERS) {
    cloud[p.key] = {
      enabled: document.getElementById(p.enableId).checked,
      path:    document.getElementById(p.pathId).value.trim(),
    };
  }

  await saveNewDocPrefs(settings);
  await saveBackupPrefs(settings);
  await saveDiskAutoSavePrefs(settings);
  await saveTerminalPrefs(settings);
  await window.electronAPI.writeSettings({ ...settings, cloud });
}

function updateCloudStatus(provider, folderPath) {
  const el = document.getElementById(provider.statusId);
  if (!folderPath) {
    el.textContent = 'Not configured';
    el.className = 'cloud-status warn';
    return;
  }
  // Validate async; show immediately from last known state
  window.electronAPI.validatePath(folderPath).then(({ exists }) => {
    if (exists) {
      el.textContent = '✓ Folder found';
      el.className = 'cloud-status ok';
    } else {
      el.textContent = '✗ Folder missing';
      el.className = 'cloud-status err';
    }
  });
}

function setupCloudPrefButtons() {
  CLOUD_PROVIDERS.forEach(p => {
    // Auto-detect
    document.getElementById(p.detectBtn).addEventListener('click', async () => {
      const paths = await window.electronAPI.detectCloudPaths();
      const found = paths[p.key];
      if (found) {
        document.getElementById(p.pathId).value = found;
        document.getElementById(p.enableId).checked = true;
        updateCloudStatus(p, found);
        showToast(`${p.key} detected: ${found}`);
      } else {
        showToast(`Could not auto-detect ${p.key} folder`);
        updateCloudStatus(p, '');
      }
    });

    // Browse
    document.getElementById(p.browseBtn).addEventListener('click', async () => {
      const chosen = await window.electronAPI.openFolderPicker();
      if (chosen) {
        document.getElementById(p.pathId).value = chosen;
        document.getElementById(p.enableId).checked = true;
        updateCloudStatus(p, chosen);
      }
    });

    // Re-validate on enable toggle
    document.getElementById(p.enableId).addEventListener('change', () => {
      updateCloudStatus(p, document.getElementById(p.pathId).value);
    });
  });
}

// ===== Toolbar =====
function setupToolbar() {
  document.getElementById('tab-new-btn').addEventListener('click', newTab);

  // Tab scroll arrow buttons (visible only when tabs overflow)
  const scrollLeftBtn = document.getElementById('tab-scroll-left');
  const scrollRightBtn = document.getElementById('tab-scroll-right');
  const scrollStep = () => Math.max(120, Math.floor(tabBar.clientWidth * 0.6));
  scrollLeftBtn?.addEventListener('click', () => {
    tabBar.scrollBy({ left: -scrollStep(), behavior: 'smooth' });
  });
  scrollRightBtn?.addEventListener('click', () => {
    tabBar.scrollBy({ left: scrollStep(), behavior: 'smooth' });
  });
  tabBar.addEventListener('scroll', updateTabScrollButtons, { passive: true });
  window.addEventListener('resize', updateTabScrollButtons);
  if (typeof ResizeObserver !== 'undefined') {
    new ResizeObserver(updateTabScrollButtons).observe(tabBar);
  }

  // Double-click on empty tab bar space → new tab
  document.getElementById('tab-bar-container').addEventListener('dblclick', (e) => {
    if (e.target.closest('.tab') || e.target.id === 'tab-new-btn' || e.target.classList.contains('tab-scroll-btn')) return;
    newTab();
  });
  document.querySelectorAll('.tb-btn[data-action]').forEach(btn => {
    btn.addEventListener('click', () => {
      const a = btn.dataset.action;
      const map = {
        'new': newTab, 'open': () => openFile(), 'save': saveFile, 'save-all': saveAll,
        'close': () => closeTab(activeTabId), 'print': () => window.print(),
        'cut': () => editor.trigger('tb', 'editor.action.clipboardCutAction', null),
        'copy': () => editor.trigger('tb', 'editor.action.clipboardCopyAction', null),
        'paste': () => pasteFromClipboardClean(),
        'undo': () => editor.trigger('tb', 'undo', null),
        'redo': () => editor.trigger('tb', 'redo', null),
        'find': () => openFindReplace('find'), 'replace': () => openFindReplace('replace'),
        'zoom-in': zoomIn, 'zoom-out': zoomOut,
        'word-wrap': toggleWordWrap, 'dark-mode': toggleDarkMode,
        'format-doc': formatDocument,
        'toggle-comment': toggleComment,
        'quick-open': openQuickOpen,
        'terminal': toggleTerminal,
        'run-file': runCurrentFile,
        'preview': togglePreview,
        'games': openGameTab,
        'ai': toggleAiPanel,
        'encrypt-toggle': () => toggleTabEncryption(),
        'source-control': () => toggleSourceControlPanel(),
        'spell-toggle': () => cycleSpellMode(),
        'whiteboard-new': () => createWhiteboardTab(null, ''),
        'icons': () => toggleIconsPanel(),
      };
      map[a]?.();
      if (a !== 'games' && a !== 'ai' && a !== 'encrypt-toggle' && a !== 'source-control' && a !== 'spell-toggle' && a !== 'whiteboard-new' && a !== 'icons') editor.focus();
    });
  });

  // Status-bar lock indicator: click to toggle session lock. Only visible when
  // the active tab is encrypted (see updateEncryptionStatusIndicator).
  document.getElementById('status-enc')?.addEventListener('click', async () => {
    if (isEncUnlocked()) {
      lockEncryption();
      showToast('🔒 Encryption locked');
    } else {
      const ok = await promptUnlockDialog();
      if (ok) showToast('🔓 Encryption unlocked');
    }
  });

  statusLang.addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;

    // The three "new tab" actions are always available — they're the only
    // useful options when the active tab isn't a text editor (whiteboard,
    // drawio, game), so for those tab types we show ONLY these three and
    // skip the language list (which doesn't apply to non-editor tabs).
    const newTabActions = [
      ['🖼 New Whiteboard Tab', () => createWhiteboardTab(null, '')],
      ['📊 New Diagram Tab',    () => createDrawioTab(null, '')],
      ['📄 New Text Tab',       () => newTab()],
    ];

    let items;
    if (tab.type === 'whiteboard' || tab.type === 'drawio' || tab.type === 'game') {
      // Non-editor tab: only the new-tab actions make sense.
      items = newTabActions;
    } else {
      // Editor tab: new-tab actions PLUS the language switcher.
      const langs = ['plaintext','javascript','typescript','python','java','c','cpp','csharp','go','rust','ruby','php','html','css','json','xml','markdown','mermaid','sql','shell','powershell','bat','yaml','kotlin','swift','lua','r','dockerfile','scss'];
      items = [
        ...newTabActions,
        null,
        ...langs.map(lang => [lang, () => {
          tab.language = lang;
          monaco.editor.setModelLanguage(tab.model, lang);
          updateLanguageStatus();
          updateMermaidToolbar(lang === 'mermaid');
          if (lang === 'mermaid' && !previewOpen) openPreview();
          else if (previewOpen) updatePreview();
          editor.focus();
        }])
      ];
    }

    const langRect = statusLang.getBoundingClientRect();
    showFloatingMenu(langRect.left, langRect.bottom, items);
  });

  document.getElementById('file-tree-close').addEventListener('click', () => {
    document.getElementById('file-tree').classList.add('hidden');
    // User explicitly dismissed the tree — don't auto-restore it on
    // the next tab switch.
    fileTreeUserShown = false;
  });
}

// ===== Menu Listeners =====
function setupMenuListeners() {
  const api = window.electronAPI;
  // Shim: register for both IPC (native menu) and _menuActions (custom HTML menu bar).
  const m = (ch, fn) => { _menuActions[ch] = fn; api.onMenu(ch, fn); };

  m('menu-new', newTab);
  m('menu-close', () => closeTab(activeTabId));
  m('menu-close-all', closeAllTabs);
  m('menu-close-others', () => closeOtherTabs(activeTabId));
  m('menu-save', saveFile);
  m('menu-save-as', saveFileAs);
  m('menu-save-all', saveAll);
  m('menu-reload', reloadFile);
  m('menu-print', () => window.print());
  m('open-files', paths => openFile(paths));
  m('open-folder', folder => openFolderTree(folder));

  m('menu-undo', () => editor.trigger('m', 'undo', null));
  m('menu-redo', () => editor.trigger('m', 'redo', null));
  m('menu-uppercase', toUpperCase);
  m('menu-lowercase', toLowerCase);
  m('menu-titlecase', toTitleCase);
  m('menu-duplicate-line', () => editor.getAction('editor.action.copyLinesDownAction')?.run());
  m('menu-delete-line', () => editor.getAction('editor.action.deleteLines')?.run());
  m('menu-remove-dup-lines', removeDuplicateLines);
  m('menu-join-lines', joinLines);
  m('menu-remove-empty-lines', removeEmptyLines);
  m('menu-sort-asc', () => sortLines(1));
  m('menu-sort-desc', () => sortLines(-1));
  m('menu-move-line-up', () => editor.getAction('editor.action.moveLinesUpAction')?.run());
  m('menu-move-line-down', () => editor.getAction('editor.action.moveLinesDownAction')?.run());
  m('menu-b64-encode', base64Encode);
  m('menu-b64-decode', base64Decode);
  m('menu-readonly', () => editor.updateOptions({ readOnly: true }));
  m('menu-clear-readonly', () => editor.updateOptions({ readOnly: false }));

  m('menu-format-doc', formatWithAutoDetect);
  m('menu-format-sel', () => editor.getAction('editor.action.formatSelection')?.run());
  m('menu-toggle-comment', toggleComment);
  m('menu-block-comment', blockComment);
  m('menu-fold-all', () => editor.getAction('editor.foldAll')?.run());
  m('menu-unfold-all', () => editor.getAction('editor.unfoldAll')?.run());
  m('menu-toggle-fold', () => editor.getAction('editor.toggleFold')?.run());
  m('menu-goto-definition', () => editor.getAction('editor.action.revealDefinition')?.run());
  m('menu-goto-symbol', () => editor.getAction('editor.action.quickOutline')?.run());
  m('menu-rename-symbol', () => editor.getAction('editor.action.rename')?.run());
  m('menu-trigger-suggest', () => editor.trigger('m', 'editor.action.triggerSuggest', null));
  m('menu-trigger-hints', () => editor.trigger('m', 'editor.action.triggerParameterHints', null));
  m('menu-json-format', jsonFormat);
  m('menu-xml-format', xmlFormat);
  m('menu-json-minify', jsonMinify);
  m('menu-compare-files',       compareFilesFlow);
  m('menu-compare-with-saved',  compareWithSavedFlow);
  m('menu-compare-clipboard',   () => { if (activeTabId != null) compareWithClipboardFlow(activeTabId); else showToast('Open a text tab to compare'); });
  m('menu-compare-folders',     compareFoldersFlow);
  m('menu-indent-increase', () => editor.trigger('m', 'editor.action.indentLines', null));
  m('menu-indent-decrease', () => editor.trigger('m', 'editor.action.outdentLines', null));

  m('menu-find', () => openFindReplace('find'));
  m('menu-replace', () => openFindReplace('replace'));
  m('menu-find-next', () => doFind(1));
  m('menu-find-prev', () => doFind(-1));
  m('menu-find-all', findAll);
  m('menu-quick-open', openQuickOpen);
  m('menu-cmd-palette', openCmdPalette);
  m('menu-goto-line', openGotoLine);
  m('menu-regex-tester', () => document.getElementById('regex-tester').classList.remove('hidden'));
  m('menu-toggle-bookmark', () => editor.getAction('editor.action.toggleBookmark')?.run());
  m('menu-next-bookmark', () => editor.getAction('editor.action.nextBookmark')?.run());
  m('menu-prev-bookmark', () => editor.getAction('editor.action.previousBookmark')?.run());

  m('menu-word-wrap', checked => {
    isWordWrap = checked;
    editor.updateOptions({ wordWrap: checked ? 'on' : 'off' });
    document.getElementById('btn-wordwrap')?.classList.toggle('active', isWordWrap);
    saveSetting('ui.wordWrap', isWordWrap);
  });
  m('menu-zoom-in', zoomIn);
  m('menu-zoom-out', zoomOut);
  m('menu-zoom-reset', zoomReset);
  m('menu-minimap', checked => {
    editor.updateOptions({ minimap: { enabled: checked } });
    saveSetting('ui.minimap', checked);
  });
  m('menu-show-whitespace', checked => {
    editor.updateOptions({ renderWhitespace: checked ? 'all' : 'selection' });
    saveSetting('ui.renderWhitespace', checked);
  });
  m('menu-show-indent', checked => {
    editor.updateOptions({ guides: { indentation: checked } });
    saveSetting('ui.indentGuides', checked);
  });
  m('menu-zen-mode', toggleZenMode);
  m('menu-dark-mode', toggleDarkMode);   // legacy toolbar button
  // Macro
  m('menu-macro-record', toggleMacroRecording);
  m('menu-macro-run',    () => runLastMacro(1));
  m('menu-macro-run-n',  () => {
    const macro = lastRecordedMacro || savedMacros[0] || null;
    if (macro) openRunNTimesDialog(macro);
    else showToast('No macro recorded yet — press Ctrl+Shift+R to start recording');
  });
  m('menu-macro-manage', openManageMacrosDialog);
  // menu-theme-picker dispatch retired — Preferences → Themes is now
  // the single entry point for theme selection.
  // Individual theme items from the Settings > Theme submenu
  Object.keys(THEMES).forEach(id => m('menu-theme-' + id, () => applyTheme(id)));
  m('menu-toolbar', show => {
    document.getElementById('toolbar').style.display = show ? '' : 'none';
    saveSetting('ui.showToolbar', show);
  });
  m('menu-statusbar', show => {
    document.getElementById('status-bar').style.display = show ? '' : 'none';
    saveSetting('ui.showStatusbar', show);
  });
  m('menu-tabbar', show => {
    document.getElementById('tab-bar-container').style.display = show ? '' : 'none';
    saveSetting('ui.showTabbar', show);
  });

  m('menu-lang', lang => {
    const tab = getActiveTab(); if (!tab) return;
    tab.language = lang;
    monaco.editor.setModelLanguage(tab.model, lang);
    updateLanguageStatus();
    updateMermaidToolbar(lang === 'mermaid');
    if (lang === 'mermaid' && !previewOpen) openPreview();
    else if (previewOpen) updatePreview();
  });

  m('menu-encoding', enc => { const tab = getActiveTab(); if (tab) { tab.encoding = enc; updateLanguageStatus(); } });

  // ── Selection menu ──────────────────────────────────────────────────────
  m('menu-col-select-mode', checked => {
    isColumnSelectMode = checked;
    editor.updateOptions({ columnSelection: isColumnSelectMode });
    updateColSelectStatus();
    saveSetting('ui.columnSelectMode', isColumnSelectMode);
  });
  m('menu-select-line',        () => editor.trigger('keyboard', 'expandLineSelection', null));
  m('menu-expand-selection',   () => editor.getAction('editor.action.smartSelect.expand')?.run());
  m('menu-shrink-selection',   () => editor.getAction('editor.action.smartSelect.shrink')?.run());
  m('menu-cursor-above',       () => editor.getAction('editor.action.insertCursorAbove')?.run());
  m('menu-cursor-below',       () => editor.getAction('editor.action.insertCursorBelow')?.run());
  m('menu-cursor-line-ends',   () => editor.getAction('editor.action.insertCursorAtEndOfEachLineSelected')?.run());
  m('menu-select-all-occ',     () => editor.getAction('editor.action.selectHighlights')?.run());
  m('menu-select-all-word',    () => editor.getAction('editor.action.changeAll')?.run());
  m('menu-col-select-up',      () => editor.trigger('keyboard', 'cursorColumnSelectUp', null));
  m('menu-col-select-down',    () => editor.trigger('keyboard', 'cursorColumnSelectDown', null));
  m('menu-col-select-left',    () => editor.trigger('keyboard', 'cursorColumnSelectLeft', null));
  m('menu-col-select-right',   () => editor.trigger('keyboard', 'cursorColumnSelectRight', null));
  m('menu-col-select-home',    () => editor.trigger('keyboard', 'cursorColumnSelectHome', null));
  m('menu-col-select-end',     () => editor.trigger('keyboard', 'cursorColumnSelectEnd', null));
  m('menu-col-select-pgup',    () => editor.trigger('keyboard', 'cursorColumnSelectPageUp', null));
  m('menu-col-select-pgdn',    () => editor.trigger('keyboard', 'cursorColumnSelectPageDown', null));

  m('menu-toggle-preview', togglePreview);
  m('menu-toggle-terminal', toggleTerminal);
  m('menu-new-terminal', () => openTerminal(true));
  m('menu-kill-terminal', killTerminal);
  m('menu-clear-terminal', () => { if (term) term.clear(); });
  m('menu-run-file', runCurrentFile);
  m('menu-run-selection', () => {
    const sel = editor.getSelection();
    if (!sel || sel.isEmpty()) return;
    const text = editor.getModel().getValueInRange(sel);
    openTerminal().then(() => window.electronAPI.terminalInput(terminalId, text + '\r\n'));
  });

  m('menu-prev-tab', () => switchTab(-1));
  m('menu-next-tab', () => switchTab(1));
  m('menu-tab', idx => { if (tabs[idx]) activateTab(tabs[idx].id); });

  m('menu-preferences', openPreferences);
  m('menu-about', () => document.getElementById('about-dialog').classList.remove('hidden'));
  m('menu-boot-perf', () => showBootPerfDialog());
  m('menu-feature-tour', () => runFeatureTour());

  // ── draw.io ─────────────────────────────────────────────────────────────
  m('menu-new-drawio', () => createDrawioTab(null, ''));
  m('menu-new-drawio-template', (name) => createDrawioTabFromTemplate(name));
  m('menu-drawio-check-updates', () => checkDrawioForUpdates());

  m('menu-open-explorer', () => {
    const tab = getActiveTab();
    if (tab && tab.filePath) window.electronAPI.shellShowItem(tab.filePath);
  });
  m('menu-copy-path', () => { const tab = getActiveTab(); if (tab && tab.filePath) navigator.clipboard.writeText(tab.filePath); });

  m('menu-sha256-selection', async () => {
    const sel = editor.getSelection();
    if (!sel || sel.isEmpty()) return;
    const text = editor.getModel().getValueInRange(sel);
    const hash = await sha256(text);
    navigator.clipboard.writeText(hash);
    showToast('SHA-256 copied: ' + hash.slice(0, 16) + '…');
  });

  m('menu-md5-selection', async () => {
    showToast('MD5 requires a native module. Use SHA-256 instead.');
  });

  m('menu-shortcuts-ref', () => {
    const shortcuts = [
      'Ctrl+N — New Tab', 'Ctrl+O — Open File', 'Ctrl+S — Save', 'Ctrl+Shift+S — Save All',
      'Ctrl+W — Close Tab', 'Ctrl+F — Find', 'Ctrl+Shift+F — Format Document (auto-detect)', 'Ctrl+H — Replace', 'Ctrl+G — Go to Line',
      'Ctrl+P — Quick Open', 'Ctrl+Shift+P — Command Palette', 'Ctrl+` — Toggle Terminal',
      'F5 — Run File', 'Ctrl+Shift+V — Toggle Preview (HTML/MD)', 'F12 — Go to Definition', 'F2 — Rename Symbol', 'Shift+F12 — Go to References',
      'Shift+Alt+F — Format Document', 'Ctrl+/ — Toggle Comment', 'Ctrl+D — Duplicate Line',
      'Alt+Up/Down — Move Line', 'Ctrl+= — Zoom In', 'Ctrl+- — Zoom Out',
      'Ctrl+Alt+D — Toggle Dark Mode', 'Ctrl+Alt+W — Toggle Word Wrap',
      'Ctrl+PageUp/Down — Switch Tabs', 'Ctrl+1-5 — Switch to Tab',
    ].join('\n');
    window.electronAPI.messageDialog({ type: 'info', title: 'Keyboard Shortcuts', message: shortcuts, buttons: ['OK'] });
  });

  m('app-before-close', async () => {
    await saveSession();          // always auto-save before closing
    window.electronAPI.closeWindow();
  });

  // ── Extra registrations for the custom HTML menu bar ───────────────────
  // These channels weren't previously in m() because they use IPC paths,
  // but the custom menu bar needs them in _menuActions for direct dispatch.
  m('menu-open',          () => openFile());
  m('menu-open-folder',   () => window.electronAPI.openDialog({ properties: ['openDirectory'] }).then(r => { if (r) openFolderTree(r[0]); }));
  m('menu-toggle-sidebar',() => { const sb = document.getElementById('sidebar'); if (sb) sb.classList.toggle('hidden'); editor?.layout(); });
  m('menu-goto-refs',     () => editor?.getAction('editor.action.referenceSearch.trigger')?.run());
  m('menu-goto-brace',    () => editor?.getAction('editor.action.jumpToBracket')?.run());
  m('menu-check-updates', () => window.electronAPI.messageDialog({ type: 'info', title: 'Check for Updates', message: 'Use Help menu or restart the app to check for updates.', buttons: ['OK'] }));

  // Terminal output from run-command (id=0)
  api.onMenu('terminal-output', (id, data) => {
    if (term) term.write(data);
    if (!terminalOpen) openTerminal();
  });
}

// ===== File Tree =====
async function openFolderTree(folderPath) {
  fileTreeRootPath = folderPath;
  fileTreeUserShown = true;
  const tree = document.getElementById('file-tree');
  const content = document.getElementById('file-tree-content');
  document.getElementById('file-tree-title').textContent = folderPath.split(/[\\/]/).pop();
  tree.classList.remove('hidden');
  content.innerHTML = '';
  await renderTreeDir(content, folderPath, 0);
}

// Show / hide the tree based on whether the active tab's file lives
// inside the tree root. Called from activateTab. Idea: if you've opened
// a folder as your workspace and then click a tab whose file is in a
// totally different folder, the tree shouldn't be sticking around
// pretending it represents that tab too.
function updateFileTreeForActiveTab() {
  const tree = document.getElementById('file-tree');
  if (!tree || !fileTreeRootPath || !fileTreeUserShown) return;
  const tab = getActiveTab();
  const fp = tab?.filePath;
  // Untitled / scratch tab — leave the tree as-is; the user might be
  // about to save it under the workspace root.
  if (!fp) return;
  const normRoot = fileTreeRootPath.replace(/\\/g, '/').replace(/\/$/, '').toLowerCase();
  const normPath = fp.replace(/\\/g, '/').toLowerCase();
  const underRoot = normPath === normRoot || normPath.startsWith(normRoot + '/');
  if (underRoot) {
    tree.classList.remove('hidden');
  } else {
    tree.classList.add('hidden');
  }
}

async function renderTreeDir(container, dirPath, depth) {
  const entries = await window.electronAPI.listDir(dirPath);
  const dirs = entries.filter(e => e.isDir).sort((a, b) => a.name.localeCompare(b.name));
  const files = entries.filter(e => !e.isDir).sort((a, b) => a.name.localeCompare(b.name));
  const all = [...dirs, ...files];

  // Build a single row element (caller appends or chunks).
  const buildRow = (entry) => {
    const fullPath = dirPath.replace(/[/\\]$/, '') + '/' + entry.name;
    const row = document.createElement('div');
    row.className = 'tree-item';
    row.style.paddingLeft = (8 + depth * 16) + 'px';
    row.title = fullPath;

    const label = document.createElement('span');
    label.textContent = (entry.isDir ? '📁 ' : getFileEmoji(entry.name) + ' ') + entry.name;
    row.appendChild(label);

    if (!entry.isDir) {
      const dec = lookupGitDecoration(fullPath);
      if (dec) {
        const badge = document.createElement('span');
        badge.className = 'tree-git-badge sc-status-' + dec.cls;
        badge.textContent = dec.ch;
        badge.title = dec.label;
        row.appendChild(badge);
      }
    }

    row._fullPath = fullPath;
    if (entry.isDir) {
      row._expanded = false;
      row._child = null;
      row.addEventListener('click', async () => {
        if (!row._expanded) {
          row._child = document.createElement('div');
          row.after(row._child);
          await renderTreeDir(row._child, fullPath, depth + 1);
          row._expanded = true;
        } else { row._child?.remove(); row._child = null; row._expanded = false; }
      });
    } else {
      row.addEventListener('click', () => openFile([fullPath]));
    }
    return row;
  };

  // Small directories (≤ CHUNK_THRESHOLD entries) — render synchronously in
  // a DocumentFragment, single DOM insertion. Faster than the per-row append
  // we used to do, and the user never notices the difference at this scale.
  const CHUNK_THRESHOLD = 200;
  const CHUNK_SIZE = 100;

  if (all.length <= CHUNK_THRESHOLD) {
    const frag = document.createDocumentFragment();
    for (const entry of all) frag.appendChild(buildRow(entry));
    container.appendChild(frag);
    return;
  }

  // Large directories — chunk by CHUNK_SIZE in requestIdleCallback so the UI
  // stays responsive while monorepos with thousands of files at one level
  // (Maven, Gradle, big monorepos) render in.
  let cursor = 0;
  const renderChunk = () => {
    const end = Math.min(cursor + CHUNK_SIZE, all.length);
    const frag = document.createDocumentFragment();
    for (let i = cursor; i < end; i++) frag.appendChild(buildRow(all[i]));
    container.appendChild(frag);
    cursor = end;
    if (cursor < all.length) {
      if (typeof requestIdleCallback === 'function') {
        requestIdleCallback(renderChunk, { timeout: 100 });
      } else {
        setTimeout(renderChunk, 0);
      }
    }
  };
  renderChunk();
}

// Given an absolute file path, return { ch, cls, label } for the git decoration
// (or null if the file isn't inside the active repo / isn't dirty).
function lookupGitDecoration(absPath) {
  if (!activeGitRepo) return null;
  const s = gitRepos.get(activeGitRepo);
  if (!s || !s.files?.length) return null;
  // Normalise to forward slashes and strip the repo root prefix
  const norm = absPath.replace(/\\/g, '/');
  const rootNorm = activeGitRepo.replace(/\\/g, '/').replace(/\/$/, '');
  if (!norm.startsWith(rootNorm + '/')) return null;
  const rel = norm.slice(rootNorm.length + 1);
  const f = s.files.find(x => x.path === rel);
  if (!f) return null;
  const g = scStatusGlyph(f, f.x !== ' ' && f.x !== '?' ? 'staged' : 'unstaged');
  const labels = { M: 'Modified', A: 'Added', D: 'Deleted', R: 'Renamed', U: 'Conflict', Q: 'Untracked' };
  return { ch: g.ch, cls: g.cls, label: labels[g.cls] || 'Changed' };
}

// ===== Reveal in File Tree =====

// Return the depth of a tree row from its padding-left style.
function _treeRowDepth(row) {
  return (parseInt(row.style.paddingLeft || '8') - 8) / 16;
}

// Walk all .tree-item elements and return the one whose _fullPath matches.
function _findTreeRowByPath(fullPath) {
  const norm = fullPath.replace(/\\/g, '/');
  for (const row of document.querySelectorAll('.tree-item')) {
    if (row._fullPath && row._fullPath.replace(/\\/g, '/') === norm) return row;
  }
  return null;
}

// Scroll to a tree row and flash it with a highlight animation.
function _highlightTreeRow(row) {
  row.scrollIntoView({ behavior: 'smooth', block: 'center' });
  row.classList.remove('tree-item-reveal-flash'); // reset if already flashing
  // Force reflow so re-adding the class restarts the animation.
  void row.offsetWidth;
  row.classList.add('tree-item-reveal-flash');
  setTimeout(() => row.classList.remove('tree-item-reveal-flash'), 1600);
}

// Programmatically expand a directory row (if it isn't already).
async function _expandTreeRow(row) {
  if (row._expanded) return;
  row._child = document.createElement('div');
  row.after(row._child);
  await renderTreeDir(row._child, row._fullPath, _treeRowDepth(row) + 1);
  row._expanded = true;
}

// Reveal the file for a given tab in the file tree, expanding intermediate
// directories as needed.  Opens the containing folder if the tree isn't
// showing or the file lives outside the current root.
async function revealInFileTree(filePath) {
  if (!filePath) { showToast('Save the file first to reveal it in the tree'); return; }

  const normPath = p => p.replace(/\\/g, '/').replace(/\/$/, '');
  const target = normPath(filePath);

  // Determine if the file is under the current tree root.
  const root = fileTreeRootPath ? normPath(fileTreeRootPath) : null;
  const underRoot = root && target.startsWith(root + '/');

  if (!underRoot) {
    // Open the file's parent directory as the new tree root.
    const parentDir = filePath.replace(/[/\\][^/\\]+$/, '');
    await openFolderTree(parentDir);
    // Wait a tick for chunked rendering to flush.
    await new Promise(r => setTimeout(r, 60));
    const row = _findTreeRowByPath(target);
    if (row) _highlightTreeRow(row);
    else showToast('Could not locate file in tree');
    return;
  }

  // File is under the current root — ensure the tree panel is visible.
  document.getElementById('file-tree').classList.remove('hidden');

  // Walk from root, expanding each directory segment that needs it.
  const rel = target.slice(root.length + 1); // e.g. "src/renderer.js"
  const segments = rel.split('/');            // e.g. ["src", "renderer.js"]

  let currentPath = root;
  for (let i = 0; i < segments.length - 1; i++) {
    currentPath += '/' + segments[i];
    const dirRow = _findTreeRowByPath(currentPath);
    if (!dirRow) { showToast('Could not navigate to file in tree'); return; }
    await _expandTreeRow(dirRow);
    // Brief pause so chunked rendering can place child rows into the DOM.
    await new Promise(r => setTimeout(r, 30));
  }

  // Highlight the final file row.
  const fileRow = _findTreeRowByPath(target);
  if (fileRow) _highlightTreeRow(fileRow);
  else showToast('Could not locate file in tree');
}

// ===== Auto-Save Session =====
const MAX_INLINE_SIZE = 512 * 1024; // 512 KB — larger files saved by path only

function scheduleAutoSave() {
  clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(saveSession, AUTO_SAVE_DELAY);
}

async function saveSession() {
  const sessionTabs = tabs.filter(tab => tab.type !== 'game').map(tab => {
    // Whiteboard tabs store content directly (JSON state synced via postMessage)
    if (tab.type === 'whiteboard') {
      return {
        id: tab.id, name: tab.name, filePath: tab.filePath || null,
        content: tab.content || '',
        language: 'whiteboard', encoding: tab.encoding, eol: tab.eol,
        active: tab.id === activeTabId, type: 'whiteboard',
        dirty: !!tab.dirty,
        pinned: !!tab.pinned, color: tab.color || null,
      };
    }
    // draw.io tabs — same shape as whiteboard: content is the diagram XML
    if (tab.type === 'drawio') {
      return {
        id: tab.id, name: tab.name, filePath: tab.filePath || null,
        content: tab.content || '',
        language: 'drawio', encoding: tab.encoding, eol: tab.eol,
        active: tab.id === activeTabId, type: 'drawio',
        dirty: !!tab.dirty,
        pinned: !!tab.pinned, color: tab.color || null,
      };
    }
    const content = tab.model.getValue();
    // Encrypted tabs: NEVER persist plaintext to session.json. Save just the
    // path + flag; on restore we re-read the encrypted file from disk, which
    // re-triggers the unlock flow.
    if (tab.encrypted) {
      return {
        id: tab.id, name: tab.name, filePath: tab.filePath || null,
        content: null,
        language: tab.language, encoding: tab.encoding, eol: tab.eol,
        active: tab.id === activeTabId,
        encrypted: true, protectedBy: tab.protectedBy || null,
        dirty: !!tab.dirty,
        pinned: !!tab.pinned, color: tab.color || null,
      };
    }
    return {
      id: tab.id,
      name: tab.name,
      filePath: tab.filePath || null,
      content: (!tab.filePath || content.length <= MAX_INLINE_SIZE) ? content : null,
      language: tab.language,
      encoding: tab.encoding,
      eol: tab.eol,
      active: tab.id === activeTabId,
      dirty: !!tab.dirty,
      pinned: !!tab.pinned, color: tab.color || null,
    };
  });
  await window.electronAPI.writeSession({ tabs: sessionTabs });
}

async function restoreSession(opts = {}) {
  const res = await window.electronAPI.readSession();
  if (!res.success || !res.data?.tabs?.length) return false;

  const replaceId = opts.replacePlaceholderId;
  if (replaceId != null) {
    const idx = tabs.findIndex(t => t.id === replaceId);
    if (idx >= 0) {
      const t = tabs[idx];
      const isDisposablePlaceholder =
        t && t.type === 'editor' && !t.filePath && !t.dirty && /^new \d+$/.test(t.name) && (t.model?.getValue?.() || '') === '';
      if (isDisposablePlaceholder) {
        try { t.model?.dispose?.(); } catch {}
        tabs.splice(idx, 1);
        if (activeTabId === replaceId) activeTabId = null;
      }
    }
  }

  // If the user already opened a file via "Open with…" / command-line while
  // the session was still loading, keep focus on that tab instead of letting
  // the restored "active" tab steal it.
  const preExistingActiveId = (activeTabId != null && tabs.some(t => t.id === activeTabId))
    ? activeTabId : null;

  const { tabs: saved } = res.data;

  let activeId = null;

  for (const s of saved) {
    let content = s.content ?? '';

    // If the user already opened this file via "Open with…" / argv before
    // session restore got a chance to run, do not recreate a duplicate tab
    // for the same path. Carry over the saved "active" flag onto the
    // pre-existing tab so focus selection still respects the session.
    if (s.filePath) {
      const dupe = _findTabByPath(s.filePath);
      if (dupe) {
        if (s.active) activeId = dupe.id;
        continue;
      }
    }

    // Restore whiteboard tabs without a Monaco model
    if (s.type === 'whiteboard' || s.language === 'whiteboard') {
      tabCounter++;
      const id = tabCounter;
      // If the whiteboard was saved (has a real filePath), reload from disk —
      // it's the source of truth. For unsaved whiteboards (filePath: null)
      // we keep the session-cached content so the user's in-progress drawing
      // survives a restart.
      if (s.filePath) {
        const r = await window.electronAPI.readFile(s.filePath);
        if (r.success) content = r.content;
      }
      const tab = {
        id, name: s.name, filePath: s.filePath || null,
        content,
        dirty: !!s.dirty,
        language: 'whiteboard',
        encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
        model: null, viewState: null, type: 'whiteboard',
        pinned: !!s.pinned, color: s.color || null,
      };
      tabs.push(tab);
      if (s.active) activeId = id;
      continue;
    }

    // Restore drawio tabs (same pattern as whiteboard — no Monaco model)
    if (s.type === 'drawio' || s.language === 'drawio') {
      tabCounter++;
      const id = tabCounter;
      if (s.filePath) {
        const r = await window.electronAPI.readFile(s.filePath);
        if (r.success) content = r.content;
      }
      const tab = {
        id, name: s.name, filePath: s.filePath || null,
        content,
        dirty: !!s.dirty,
        language: 'drawio',
        encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
        model: null, viewState: null, type: 'drawio',
        pinned: !!s.pinned, color: s.color || null,
      };
      tabs.push(tab);
      if (s.active) activeId = id;
      continue;
    }

    // Encrypted tab: re-route through openFile so the user gets the unlock
    // prompt and the file is properly decrypted (or skipped on cancel).
    if (s.encrypted && s.filePath) {
      // Defer to next tick so all session tabs get processed before unlock
      // dialogs queue up. openFile reads + decrypts + creates the tab.
      await openFile([s.filePath]);
      const newTab = tabs[tabs.length - 1];
      if (newTab && s.active) activeId = newTab.id;
      continue;
    }

    // Re-read from disk if we only stored the path
    if (s.filePath && s.content === null) {
      const r = await window.electronAPI.readFile(s.filePath);
      if (r.success) {
        // Detect encrypted file even if the session record didn't flag it
        // (e.g., user encrypted the file in another app session).
        // Quick-check first so we skip the crypto.js load if it's plainly
        // not an encrypted envelope.
        if (_maybeEncrypted(r.content) && (await ensureCrypto()) && window.NotePPCrypto.detectEncrypted(r.content)) {
          await openFile([s.filePath]);
          const newTab = tabs[tabs.length - 1];
          if (newTab && s.active) activeId = newTab.id;
          continue;
        }
        content = r.content;
      }
    }

    tabCounter++;
    const id = tabCounter;
    const model = monaco.editor.createModel(content, s.language || 'plaintext');
    const tab = {
      id, name: s.name, filePath: s.filePath || null,
      content,
      dirty: !!s.dirty,
      language: s.language || 'plaintext',
      encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
      model, viewState: null, type: 'editor',
      encrypted: false, protectedBy: null,
      pinned: !!s.pinned, color: s.color || null,
    };
    tabs.push(tab);
    if (s.active) activeId = id;
    // Start watching restored files for external changes
    if (tab.filePath) {
      try { window.electronAPI.watchFile(tab.filePath); } catch {}
    }
  }

  if (tabs.length === 0) return false;

  activateTab(preExistingActiveId || activeId || tabs[tabs.length - 1].id);
  renderTabs();
  return true;
}

// ===== Utilities =====
let toastTimer = null;
function showToast(msg) {
  const el = document.getElementById('toast');
  el.textContent = msg; el.classList.add('show');
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), 2500);
}

// ===== AI Assistant Panel =====
let aiPanelOpen    = false;
let aiGenerating   = false;
let aiResponse     = '';          // last assistant message (used by Insert/Replace actions)
let aiModel        = '';          // currently selected model
let aiResizeActive = false;
let aiRefreshing   = false;       // prevents concurrent refreshAiModelList() calls

// Multi-turn chat history. The first entry (when present) is the system
// prompt that includes file context — set at the start of each conversation.
// Subsequent entries alternate user / assistant.
let aiMessages = [];               // [{ role: 'system'|'user'|'assistant', content }]

// Separate model for agent mode — lets users pick a smaller/faster model for
// chat and a stronger one for code-rewriting agent runs. Empty = fall back to
// `aiModel` (chat model).
let aiAgentModel = '';

// Agent mode — when true, AI's response is treated as the new file/selection
// content and shown in a Monaco diff modal for Apply/Reject. Per-session only.
let aiAgentMode = false;
// When agent mode is generating, we capture context here so we can wire the
// diff modal correctly when the response is done.
let aiAgentTurn = null;            // { originalContent, isSelection, selRange, snapshotText }

const RECOMMENDED_MODELS = [
  { name: 'phi3:mini',             size: '~2.3 GB', desc: 'Best for writing & markdown' },
  { name: 'llama3.2:1b',          size: '~1.3 GB', desc: 'Fastest, lightest' },
  { name: 'llama3.2:3b',          size: '~2.0 GB', desc: 'Good quality balance' },
  { name: 'qwen2.5-coder:1.5b',   size: '~1.0 GB', desc: 'Code-focused, small' },
  { name: 'deepseek-coder:1.3b',  size: '~776 MB', desc: 'Smallest code model' },
  { name: 'gemma2:2b',            size: '~1.6 GB', desc: 'Google Gemma' },
];

function setupAiPanel() {
  const panel     = document.getElementById('ai-panel');
  const resizeH   = document.getElementById('ai-resize-handle');
  const promptEl  = document.getElementById('ai-prompt');
  const sendBtn   = document.getElementById('btn-ai-send');
  const modelSel  = document.getElementById('ai-model-select');

  // Wire close/settings buttons
  document.getElementById('btn-ai-close').addEventListener('click', closeAiPanel);
  document.getElementById('btn-ai-new-chat').addEventListener('click', newAiConversation);
  document.getElementById('btn-ai-mode-toggle').addEventListener('click', toggleAgentMode);

  // Diff modal — Apply / Reject / Esc / × close
  document.getElementById('ai-diff-apply').addEventListener('click', applyAgentDiff);
  document.getElementById('ai-diff-reject').addEventListener('click', hideAgentDiff);
  document.getElementById('ai-diff-x').addEventListener('click', hideAgentDiff);
  document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape' && !document.getElementById('ai-diff-dialog').classList.contains('hidden')) {
      hideAgentDiff();
    }
  });

  document.getElementById('btn-ai-settings').addEventListener('click', () => {
    document.getElementById('prefs-dialog').classList.remove('hidden');
    // Switch to AI settings pane
    document.querySelectorAll('.prefs-item').forEach(i => i.classList.remove('active'));
    document.querySelectorAll('.pref-page').forEach(p => p.classList.remove('active'));
    document.querySelector('.prefs-item[data-pref="ai"]').classList.add('active');
    document.getElementById('pref-ai').classList.add('active');
    refreshAiSettingsPage();
  });

  // Model selector in panel header — saves to the right slot depending on
  // current mode. Agent mode picks `ai.modelAgent`; Chat mode picks `ai.model`.
  modelSel.addEventListener('change', () => {
    if (aiAgentMode) {
      aiAgentModel = modelSel.value;
      saveSetting('ai.modelAgent', aiAgentModel);
    } else {
      aiModel = modelSel.value;
      saveSetting('ai.model', aiModel);
    }
  });

  // Quick-action chips — pre-canned prompts that auto-toggle Agent mode where needed
  document.querySelectorAll('.ai-quick-btn[data-quick]').forEach(btn => {
    btn.addEventListener('click', () => runAiQuickAction(btn.dataset.quick));
  });

  // Send button + Enter key
  sendBtn.addEventListener('click', sendAiPrompt);
  promptEl.addEventListener('keydown', e => {
    if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendAiPrompt(); }
  });
  promptEl.addEventListener('input', () => {
    sendBtn.disabled = !promptEl.value.trim() || aiGenerating;
    // auto-grow textarea
    promptEl.style.height = 'auto';
    promptEl.style.height = Math.min(promptEl.scrollHeight, 120) + 'px';
  });

  // Action bar buttons
  document.getElementById('btn-ai-insert').addEventListener('click',      () => applyAiResponse('insert'));
  document.getElementById('btn-ai-replace').addEventListener('click',     () => applyAiResponse('replace'));
  document.getElementById('btn-ai-append').addEventListener('click',      () => applyAiResponse('append'));
  document.getElementById('btn-ai-replace-all').addEventListener('click', () => applyAiResponse('replace-all'));
  document.getElementById('btn-ai-discard').addEventListener('click', resetAiResponse);

  // Resize handle
  resizeH.addEventListener('mousedown', e => {
    aiResizeActive = true;
    e.preventDefault();
  });
  document.addEventListener('mousemove', e => {
    if (!aiResizeActive) return;
    const rect = document.getElementById('editor-container').getBoundingClientRect();
    const newH  = rect.bottom - e.clientY;
    if (newH >= 120 && newH <= window.innerHeight * 0.6)
      panel.style.height = newH + 'px';
  });
  document.addEventListener('mouseup', () => { aiResizeActive = false; });

  // Token streaming: append to the LAST assistant message in the chat thread
  window.electronAPI.onAiToken(token => {
    aiResponse += token;
    // Mirror into aiMessages so render reflects the live state
    if (aiMessages.length && aiMessages[aiMessages.length - 1].role === 'assistant') {
      aiMessages[aiMessages.length - 1].content = aiResponse;
    }
    renderAiConversation(true);
  });

  window.electronAPI.onAiDone(() => {
    aiGenerating = false;
    const lastMsg = aiMessages[aiMessages.length - 1];
    if (lastMsg && lastMsg.role === 'assistant') lastMsg.content = aiResponse;

    // Agent mode: route the response to the diff modal (don't show the
    // suggest-and-apply action bar; the diff modal handles Apply/Reject).
    if (lastMsg && lastMsg.agent && aiAgentTurn) {
      const tab = getActiveTab();
      const language = tab?.language || 'plaintext';
      // Snapshot the diff so the "Review diff →" link can re-open it later
      lastMsg.diffSnapshot = {
        originalContent: aiAgentTurn.originalContent,
        newContent: aiResponse,
        language,
        isSelection: aiAgentTurn.isSelection,
        selRange: aiAgentTurn.selRange,
      };
      renderAiConversation(false);
      showAgentDiff(lastMsg.diffSnapshot);
      // Action bar is for chat mode only — keep hidden in agent mode
      document.getElementById('ai-action-bar').classList.add('hidden');
    } else {
      renderAiConversation(false);
      document.getElementById('ai-action-bar').classList.remove('hidden');
    }
    document.getElementById('btn-ai-send').disabled = !document.getElementById('ai-prompt')?.value.trim();
    document.getElementById('btn-ai-send').textContent = 'Send ↵';
    // Refocus the input so user can immediately type a follow-up
    setTimeout(() => document.getElementById('ai-prompt')?.focus(), 50);
    const modelSel = document.getElementById('ai-model-select');
    if (modelSel?.value) {
      setAiStatus('online');
    } else {
      refreshAiModelList();
    }
  });

  // Load saved model on startup
  loadAiState();
}

async function loadAiState() {
  const settings = await window.electronAPI.readSettings();
  aiModel      = settings?.ai?.model      || '';
  aiAgentModel = settings?.ai?.modelAgent || '';
  if (settings?.ai?.systemPrompt !== undefined)
    document.getElementById('pref-ai-system').value = settings.ai.systemPrompt;
  await refreshAiModelList();
}

async function refreshAiModelList() {
  // Guard against concurrent calls — last caller wins
  if (aiRefreshing) return;
  aiRefreshing = true;
  try {
    const result = await window.electronAPI.aiCheck();
    const modelSel = document.getElementById('ai-model-select');
    modelSel.innerHTML = '';

    if (!result.running) {
      // Always set dot + dropdown together so they can never disagree
      setAiStatus('offline');
      modelSel.innerHTML = '<option value="">Ollama not running</option>';
      document.getElementById('btn-ai-send').disabled = true;
      return;
    }

    if (result.models.length === 0) {
      setAiStatus('online'); // Connected, just nothing installed yet
      modelSel.innerHTML = '<option value="">No models — download one in Settings ⚙</option>';
      document.getElementById('btn-ai-send').disabled = true;
      return;
    }

    // The active mode dictates which model is selected in the dropdown.
    // Agent mode picks from `aiAgentModel`; Chat mode picks from `aiModel`.
    const preferred = aiAgentMode ? (aiAgentModel || aiModel) : aiModel;
    result.models.forEach(m => {
      const opt = document.createElement('option');
      opt.value = m; opt.textContent = m;
      if (m === preferred || (!preferred && result.models.length === 1)) opt.selected = true;
      modelSel.appendChild(opt);
    });
    if (aiAgentMode) aiAgentModel = modelSel.value;
    else             aiModel      = modelSel.value;
    document.getElementById('btn-ai-send').disabled = false;
    // Set green dot AFTER dropdown is fully populated — never before
    setAiStatus('online');
  } finally {
    aiRefreshing = false;
  }
}

function setAiStatus(state) {
  const dot  = document.getElementById('ai-status-dot');
  const txt  = document.getElementById('ai-status-text');
  const pdot = document.getElementById('pref-ai-dot');
  const ptxt = document.getElementById('pref-ai-status-text');
  const map  = {
    online:  { cls: 'ai-dot-online',  label: 'Ollama connected' },
    offline: { cls: 'ai-dot-offline', label: 'Ollama not running' },
    busy:    { cls: 'ai-dot-busy',    label: 'Generating…' },
  };
  const info = map[state] || map.offline;
  [dot, pdot].forEach(el => { if (!el) return; el.className = info.cls; });
  [txt, ptxt].forEach(el => { if (!el) return; el.textContent = info.label; });
}

async function sendAiPrompt() {
  if (aiGenerating) return;
  const promptEl = document.getElementById('ai-prompt');
  const userPrompt = promptEl.value.trim();
  if (!userPrompt) return;

  const tab = getActiveTab();
  if (!tab || tab.type === 'game') { showToast('Open a file first'); return; }

  const modelSel = document.getElementById('ai-model-select');
  // Agent mode prefers its own model setting; falls back to the chat model
  // if the agent model isn't installed (or wasn't picked yet).
  const model = (aiAgentMode && aiAgentModel) ? aiAgentModel : modelSel.value;
  if (!model) { showToast('Select a model first'); return; }

  // Snapshot the current scope (selection or whole file) for agent mode so
  // the diff modal can compare against the user's CURRENT content, not the
  // version captured at conversation start (file may have changed since).
  if (aiAgentMode) {
    const editorModel = editor.getModel();
    const sel = editor.getSelection();
    const isSelection = sel && !sel.isEmpty();
    aiAgentTurn = {
      isSelection,
      selRange: isSelection ? sel : null,
      originalContent: isSelection ? editorModel.getValueInRange(sel) : editorModel.getValue(),
    };
  }

  // System message: agent mode rebuilds it every turn (current file may
  // have changed via a previous Apply); chat mode builds it once.
  if (aiAgentMode) {
    const sys = await buildAgentSystemPrompt(tab, aiAgentTurn);
    if (aiMessages.length && aiMessages[0].role === 'system') aiMessages[0].content = sys;
    else aiMessages.unshift({ role: 'system', content: sys });
  } else if (aiMessages.length === 0) {
    aiMessages.push({ role: 'system', content: await buildAiSystemPrompt(tab, userPrompt) });
  }

  // Append the user message and render the thread
  aiMessages.push({ role: 'user', content: userPrompt });
  renderAiConversation();

  // Clear the input so the user can immediately type a follow-up
  promptEl.value = '';
  promptEl.style.height = 'auto';

  // Start the assistant turn (empty placeholder; tokens will fill it in)
  aiResponse = '';
  aiMessages.push({ role: 'assistant', content: '', agent: aiAgentMode });
  aiGenerating = true;
  document.getElementById('btn-ai-send').disabled = true;
  document.getElementById('btn-ai-send').textContent = '…';
  document.getElementById('ai-action-bar').classList.add('hidden');
  setAiStatus('busy');
  renderAiConversation(true);

  // Send to Ollama via the multi-turn chat endpoint
  window.electronAPI.aiChat({ model, messages: aiMessages.slice(0, -1) /* exclude the empty assistant placeholder */ });
}

// Builds the per-conversation system prompt (file context, language hints, output rules).
// Called only on the FIRST turn of a conversation. The resulting string is
// frozen into aiMessages[0] so subsequent turns share the same context.
async function buildAiSystemPrompt(tab, firstUserPrompt) {
  const settings    = await window.electronAPI.readSettings();
  const userSystem  = settings?.ai?.systemPrompt || '';
  const langName    = tab.language || 'plaintext';
  const fileName    = tab.name || 'untitled';
  const editorModel = editor.getModel();
  const fullContent = editorModel.getValue();
  const selection   = editor.getSelection();
  const selText     = selection && !selection.isEmpty()
    ? editorModel.getValueInRange(selection) : '';
  const MAX_CONTENT = 6000;
  const fileContext = fullContent.length <= MAX_CONTENT
    ? fullContent
    : fullContent.slice(0, MAX_CONTENT) + '\n…[file truncated]';
  const projectCtx = await loadProjectContext(tab);

  return [
    userSystem,
    `You are an expert writing and coding assistant embedded in a text editor called Note++.`,
    `The user is editing a ${langName} file named "${fileName}".`,
    ``,
    `When the user asks you to GENERATE or REWRITE content for the file:`,
    `- Output the raw content only — ready to paste directly into the editor`,
    `- Do NOT add markdown code fences UNLESS the file is .md or .markdown`,
    `- For Markdown files: use correct Markdown + Mermaid syntax; Mermaid goes inside \`\`\`mermaid fences`,
    `- In Mermaid diagrams: use only valid Mermaid syntax — no if/else, no programming constructs`,
    `- Match the indentation and style of the file`,
    ``,
    `When the user asks a QUESTION or for a brief explanation, answer concisely in plain prose.`,
    `Use your judgement: if the user wants text to insert into the file, output just the text; if they want clarification, answer the question.`,
    ``,
    projectCtx,
    selText
      ? `The user has SELECTED this text (operate on it):\n\`\`\`\n${selText}\n\`\`\``
      : `Full file content (${fullContent.length} chars):\n\`\`\`\n${fileContext}\n\`\`\``,
  ].filter(Boolean).join('\n');
}

// Load project-level context the AI should always see for this tab:
//   - AGENTS.md  (vendor-neutral standard, walked up from the file's folder)
//   - .notepp/memory.md  (Note++-specific per-project instructions)
// Returns a string ready to inject into the system prompt, or '' if none.
async function loadProjectContext(tab) {
  if (!tab?.filePath) return '';
  try {
    const r = await window.electronAPI.projectContext.find(tab.filePath);
    const parts = [];
    if (r.agentsMd) {
      parts.push(`Project AGENTS.md (always-on instructions for AI in this repo):\n\`\`\`\n${r.agentsMd.slice(0, 4000)}\n\`\`\``);
    }
    if (r.memoryMd) {
      parts.push(`Project memory (.notepp/memory.md — user's notes for the AI):\n\`\`\`\n${r.memoryMd.slice(0, 4000)}\n\`\`\``);
    }
    return parts.length ? parts.join('\n\n') + '\n' : '';
  } catch (e) { return ''; }
}

// Build the system prompt for AGENT mode. Includes the entire current
// scope (selection or whole file) so the model has full context for the
// rewrite, and instructs it to output ONLY the replacement content.
async function buildAgentSystemPrompt(tab, turn) {
  const langName = tab.language || 'plaintext';
  const fileName = tab.name || 'untitled';
  const scope = turn.isSelection ? 'the selected text' : 'the entire file';
  const projectCtx = await loadProjectContext(tab);

  return [
    `You are an expert coding assistant in AGENT MODE inside the Note++ editor.`,
    ``,
    `RULES (critical):`,
    `- Your output IS the new content. It will REPLACE ${scope}.`,
    `- Output ONLY the complete updated content — no explanations, no preamble, no markdown code fences (unless the file is .md/.markdown and fences are part of its content).`,
    `- Preserve everything you weren't asked to change. Match indentation, style, and language conventions exactly.`,
    `- If the user asks a clarifying question, output the current content unchanged.`,
    ``,
    `File: ${fileName}    Language: ${langName}    Scope: ${scope}`,
    ``,
    projectCtx,
    `Current ${scope.toUpperCase()} (your output replaces this):`,
    '```',
    turn.originalContent,
    '```',
  ].filter(Boolean).join('\n');
}

// Toggle agent mode on/off. Visual: button text + colour change.
function toggleAgentMode() {
  aiAgentMode = !aiAgentMode;
  const btn = document.getElementById('btn-ai-mode-toggle');
  if (aiAgentMode) {
    btn.textContent = '⚡ Agent';
    btn.classList.add('agent-active');
  } else {
    btn.textContent = '🤖 Chat';
    btn.classList.remove('agent-active');
  }
  // Update the input placeholder so users know what to type
  const promptEl = document.getElementById('ai-prompt');
  if (promptEl) {
    promptEl.placeholder = aiAgentMode
      ? '⚡ Agent mode: tell AI what to change… (Enter to send · Shift+Enter for newline)'
      : 'Describe what to write… (Enter to send · Shift+Enter for newline)';
  }
  // Re-populate the model dropdown so it shows the active mode's preferred model
  refreshAiModelList();
}

// ── Quick actions (pre-canned prompts) ───────────────────────────────────
// Each chip auto-toggles into the right mode (Agent for editing actions,
// Chat for explanatory ones), fills the prompt textarea, and sends.
async function runAiQuickAction(action) {
  if (action === 'memory') return openProjectMemory();

  const tab = getActiveTab();
  if (!tab || tab.type !== 'editor') {
    showToast('Open a text file first');
    return;
  }
  const sel = editor.getSelection();
  const hasSel = sel && !sel.isEmpty();
  const target = hasSel ? 'the selected code' : 'this file';

  const prompts = {
    polish:   `Polish ${target}: fix typos, tighten wording, normalize formatting and style. Do NOT change meaning, structure, or behaviour.`,
    refactor: `Refactor ${target}: improve readability, naming, and structure. Preserve external behaviour exactly. Add no new features.`,
    comments: `Add concise, useful inline comments and (where appropriate) docstrings/JSDoc to ${target}. Do not change any code.`,
    tests:    `Write thorough unit tests for ${target}. Use the language's idiomatic test framework. Cover happy path, edge cases, and error cases.`,
    explain:  `Explain what ${target} does, in plain prose. Identify its purpose, key flows, dependencies, and anything non-obvious. Be concise.`,
  };
  const prompt = prompts[action];
  if (!prompt) return;

  // Editing actions go through Agent mode (so the diff modal gates the change).
  // "Explain" is a pure question — keep it in Chat mode.
  const wantsAgent = action !== 'explain';
  if (wantsAgent && !aiAgentMode) toggleAgentMode();
  if (!wantsAgent && aiAgentMode) toggleAgentMode();

  document.getElementById('ai-prompt').value = prompt;
  await sendAiPrompt();
}


// Open the .notepp/memory.md file for the active workspace as an editor tab.
// Creates it (with a friendly template) if it doesn't exist yet.
async function openProjectMemory() {
  const tab = getActiveTab();
  if (!tab?.filePath) {
    showToast('Open any file in the project first');
    return;
  }
  // Find the project root: prefer the active git repo, else the file's folder
  let root = activeGitRepo;
  if (!root) {
    // Fall back to the file's directory
    root = tab.filePath.replace(/[\\/][^\\/]+$/, '');
  }
  const r = await window.electronAPI.projectContext.ensureMemory(root);
  if (!r.success) { showToast('Failed: ' + (r.error || 'unknown')); return; }
  await openFile([r.path]);
}

// ── Diff modal (Monaco diff editor) ──────────────────────────────────────
let aiDiffEditor = null;            // Monaco diff editor instance (lazy)
let aiDiffOriginalModel = null;     // text models for left/right
let aiDiffModifiedModel = null;
let aiCurrentDiff = null;           // { originalContent, newContent, language, isSelection, selRange }

function ensureDiffEditor() {
  if (aiDiffEditor) return aiDiffEditor;
  const container = document.getElementById('ai-diff-container');
  aiDiffEditor = monaco.editor.createDiffEditor(container, {
    readOnly: true,
    automaticLayout: true,
    renderSideBySide: true,
    fontSize: 13,
    fontFamily: "'Cascadia Code', 'Fira Code', Consolas, monospace",
    minimap: { enabled: false },
    scrollBeyondLastLine: false,
    theme: isDarkMode ? 'vs-dark' : 'vs',
  });
  return aiDiffEditor;
}

function showAgentDiff({ originalContent, newContent, language, isSelection, selRange }) {
  aiCurrentDiff = { originalContent, newContent, language, isSelection, selRange };
  const dlg = document.getElementById('ai-diff-dialog');
  dlg.classList.remove('hidden');

  const ed = ensureDiffEditor();
  // Dispose old models then create fresh ones in the right language
  if (aiDiffOriginalModel) try { aiDiffOriginalModel.dispose(); } catch {}
  if (aiDiffModifiedModel) try { aiDiffModifiedModel.dispose(); } catch {}
  aiDiffOriginalModel = monaco.editor.createModel(originalContent || '', language || 'plaintext');
  aiDiffModifiedModel = monaco.editor.createModel(newContent || '',     language || 'plaintext');
  ed.setModel({ original: aiDiffOriginalModel, modified: aiDiffModifiedModel });

  // Summary line
  const before = (originalContent || '').split('\n').length;
  const after  = (newContent || '').split('\n').length;
  const delta  = after - before;
  const scopeStr = isSelection ? 'selection' : 'whole file';
  document.getElementById('ai-diff-summary').textContent =
    `Scope: ${scopeStr} · ${before} → ${after} lines (${delta >= 0 ? '+' : ''}${delta}) · review and Apply or Reject`;

  // Trigger a layout pass after the modal becomes visible
  setTimeout(() => { try { ed.layout(); } catch {} }, 50);
}

function hideAgentDiff() {
  document.getElementById('ai-diff-dialog').classList.add('hidden');
}

function applyAgentDiff() {
  if (!aiCurrentDiff) return;
  const { newContent, isSelection, selRange } = aiCurrentDiff;
  const tab = getActiveTab();
  if (!tab || tab.type === 'game' || tab.type === 'whiteboard') return;
  const model = editor.getModel();
  editor.pushUndoStop();
  if (isSelection && selRange) {
    editor.executeEdits('ai-agent', [{ range: selRange, text: newContent, forceMoveMarkers: true }]);
  } else {
    const fullRange = model.getFullModelRange();
    editor.executeEdits('ai-agent', [{ range: fullRange, text: newContent, forceMoveMarkers: true }]);
  }
  editor.pushUndoStop();
  showToast('✓ Applied AI change');
  hideAgentDiff();
  editor.focus();
}

// Render the chat thread inside #ai-response-area. Highlights the streaming
// assistant turn while in progress.
function renderAiConversation(isStreaming) {
  const ph  = document.getElementById('ai-response-placeholder');
  const out = document.getElementById('ai-response-text');
  // Hide the friendly intro, show the chat container
  ph.classList.add('hidden');
  out.classList.remove('hidden');

  // Skip the system message in the visible thread — it's just context for the model
  const visible = aiMessages.filter(m => m.role !== 'system');
  if (!visible.length) {
    out.innerHTML = '';
    out.classList.add('hidden');
    ph.classList.remove('hidden');
    return;
  }

  out.innerHTML = visible.map((m, i) => {
    const isLast = i === visible.length - 1;
    const role = m.role === 'user' ? 'You' : '🤖 Assistant';
    const cls = m.role === 'user' ? 'ai-msg-user' : 'ai-msg-assistant';
    const cursor = (isStreaming && m.role === 'assistant' && isLast)
      ? '<span class="ai-cursor"></span>' : '';
    // Agent-mode assistant turns are shown as a summary line — the raw
    // file-content reply would flood the chat thread otherwise.
    const isAgentReply = m.role === 'assistant' && (m.agent === true);
    let bodyHtml;
    if (isAgentReply) {
      const lines = (m.content || '').split('\n').length;
      const reviewable = (m.content || '').length > 0 && !isStreaming;
      bodyHtml = `<div class="ai-agent-summary">⚡ Proposed change — ${lines} lines${
        reviewable ? ` · <span class="ai-agent-summary-link" data-msg-idx="${i}">Review diff →</span>` : ' · generating…'
      }${cursor}</div>`;
    } else {
      bodyHtml = `<div class="ai-msg-content">${escapeHtml(m.content || (isStreaming && isLast ? '' : ''))}${cursor}</div>`;
    }
    return `
      <div class="ai-msg ${cls}">
        <div class="ai-msg-role">${role}</div>
        ${bodyHtml}
      </div>`;
  }).join('');
  // Wire the "Review diff →" links so users can re-open a closed diff modal
  out.querySelectorAll('.ai-agent-summary-link').forEach(link => {
    link.addEventListener('click', () => {
      const idx = parseInt(link.dataset.msgIdx, 10);
      const msg = visible[idx];
      if (!msg || !msg.diffSnapshot) return;
      showAgentDiff(msg.diffSnapshot);
    });
  });
  out.parentElement.scrollTop = out.parentElement.scrollHeight;
}

// Reset the conversation. Called from the "+" New chat button.
function newAiConversation() {
  if (aiGenerating) { window.electronAPI.aiAbort(); aiGenerating = false; }
  aiMessages = [];
  aiResponse = '';
  document.getElementById('ai-response-text').innerHTML = '';
  document.getElementById('ai-response-text').classList.add('hidden');
  document.getElementById('ai-action-bar').classList.add('hidden');
  const ph = document.getElementById('ai-response-placeholder');
  ph.classList.remove('hidden');
  ph.innerHTML = `
    <span>Ask AI to write, continue, or transform text in this file.</span><br>
    <span style="font-size:0.72rem;color:#484f58">e.g. "write a mermaid user-login flow" · "add JSDoc to this function" · "convert to TypeScript"</span>`;
  document.getElementById('btn-ai-send').disabled = !document.getElementById('ai-prompt').value.trim();
  document.getElementById('btn-ai-send').textContent = 'Send ↵';
  setAiStatus('online');
  document.getElementById('ai-prompt')?.focus();
}

function applyAiResponse(mode) {
  if (!aiResponse) return;
  const tab = getActiveTab();
  if (!tab || tab.type === 'game') return;

  editor.pushUndoStop();
  const model = editor.getModel();

  if (mode === 'insert') {
    const pos = editor.getPosition();
    editor.executeEdits('ai', [{
      range: new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column),
      text: aiResponse, forceMoveMarkers: true
    }]);
  } else if (mode === 'replace') {
    const sel = editor.getSelection();
    if (sel && !sel.isEmpty()) {
      editor.executeEdits('ai', [{ range: sel, text: aiResponse, forceMoveMarkers: true }]);
    } else {
      // Nothing selected — insert at cursor
      const pos = editor.getPosition();
      editor.executeEdits('ai', [{
        range: new monaco.Range(pos.lineNumber, pos.column, pos.lineNumber, pos.column),
        text: aiResponse, forceMoveMarkers: true
      }]);
    }
  } else if (mode === 'append') {
    const lastLine = model.getLineCount();
    const lastCol  = model.getLineMaxColumn(lastLine);
    const needsNewline = model.getValueLength() > 0 && model.getValue().slice(-1) !== '\n';
    editor.executeEdits('ai', [{
      range: new monaco.Range(lastLine, lastCol, lastLine, lastCol),
      text: (needsNewline ? '\n' : '') + aiResponse, forceMoveMarkers: true
    }]);
  } else if (mode === 'replace-all') {
    // Replace entire file content
    const lastLine = model.getLineCount();
    const lastCol  = model.getLineMaxColumn(lastLine);
    editor.executeEdits('ai', [{
      range: new monaco.Range(1, 1, lastLine, lastCol),
      text: aiResponse, forceMoveMarkers: true
    }]);
  }

  editor.pushUndoStop();
  editor.focus();
  resetAiResponse();
  showToast('AI text inserted ✓');
}

function resetAiResponse(clearText = true) {
  if (clearText) aiResponse = '';
  document.getElementById('ai-response-text').classList.add('hidden');
  document.getElementById('ai-response-text').textContent = '';
  document.getElementById('ai-response-placeholder').classList.remove('hidden');
  document.getElementById('ai-action-bar').classList.add('hidden');
}

function toggleAiPanel() {
  if (aiPanelOpen) { closeAiPanel(); } else { openAiPanel(); }
}

function openAiPanel() {
  aiPanelOpen = true;
  document.getElementById('ai-panel').classList.remove('hidden');
  document.getElementById('ai-resize-handle').classList.remove('hidden');
  document.getElementById('btn-ai').classList.add('active');
  // Run the auto-magic setup: detect Ollama → auto-start daemon → auto-pull
  // a default model if none installed → finally settle on a usable state.
  // refreshAiModelList() runs at the end of aiAutoSetup() to populate UI.
  aiAutoSetup();
  setTimeout(() => document.getElementById('ai-prompt').focus(), 50);
}

// One model auto-pulled on first launch when the user has no models. Picked
// for being small (~1 GB), code-aware, and present in our recommended list.
const AI_AUTO_PULL_MODEL = 'qwen2.5-coder:1.5b';

// Render a friendly status line in the AI placeholder area while we work.
function setAiSetupStatus(text, isError = false) {
  const ph = document.getElementById('ai-response-placeholder');
  if (!ph) return;
  ph.classList.remove('hidden');
  document.getElementById('ai-response-text').classList.add('hidden');
  ph.innerHTML = `
    <div style="text-align:center;padding:30px 16px">
      <div style="font-size:14px;${isError ? 'color:#e03131' : ''}">${escapeHtml(text)}</div>
    </div>`;
}

// Update progress for the qwen download — called repeatedly via onAiProgress.
let aiAutoPullActive = false;
function setAiPullProgress(model, completed, total, status) {
  const ph = document.getElementById('ai-response-placeholder');
  if (!ph) return;
  const pct = total > 0 ? Math.floor((completed / total) * 100) : 0;
  const mb  = (n) => (n / 1024 / 1024).toFixed(1) + ' MB';
  ph.classList.remove('hidden');
  document.getElementById('ai-response-text').classList.add('hidden');
  ph.innerHTML = `
    <div style="text-align:center;padding:24px 16px">
      <div style="font-size:13px;font-weight:600;margin-bottom:6px">⬇ Downloading ${escapeHtml(model)}…</div>
      <div style="font-size:11px;color:#888;margin-bottom:10px">${escapeHtml(status || '')}</div>
      <div style="height:6px;background:#2d2d2d;border-radius:3px;overflow:hidden;max-width:320px;margin:0 auto">
        <div style="height:100%;background:#3fb950;width:${pct}%;transition:width 0.2s"></div>
      </div>
      <div style="font-size:11px;color:#888;margin-top:6px">
        ${total > 0 ? `${mb(completed)} / ${mb(total)} (${pct}%)` : 'Preparing…'}
      </div>
    </div>`;
}

// The orchestrator. Runs every time the panel is opened — cheap when already
// healthy, helpful when not.
async function aiAutoSetup() {
  setAiSetupStatus('Checking Ollama…');
  let r = await window.electronAPI.aiCheck();

  // ── Step 1: ensure daemon is running ─────────────────────────────────
  if (!r.running) {
    setAiSetupStatus('Starting Ollama…');
    const detect = await window.electronAPI.aiDetectInstalled();
    if (!detect.installed) {
      // Ollama isn't installed — offer to open the download page
      const ph = document.getElementById('ai-response-placeholder');
      ph.classList.remove('hidden');
      document.getElementById('ai-response-text').classList.add('hidden');
      ph.innerHTML = `
        <div style="text-align:center;padding:30px 16px">
          <div style="font-size:14px;margin-bottom:8px">🤖 Ollama is not installed</div>
          <div style="font-size:12px;color:#888;margin-bottom:14px">
            Note++ uses Ollama to run AI locally on your machine. Install it once and Note++ will handle the rest.
          </div>
          <button class="ai-btn ai-btn-primary" id="btn-ai-install-ollama">Open Ollama download page</button>
          <div style="margin-top:10px;font-size:11px;color:#888">After installing, click 🤖 again.</div>
        </div>`;
      document.getElementById('btn-ai-install-ollama')?.addEventListener('click', () => {
        window.electronAPI.openUrl('https://ollama.com/download');
      });
      setAiStatus('offline');
      return;
    }
    // Ollama is installed but not running — start the daemon
    const start = await window.electronAPI.aiStartServer(detect.path);
    if (!start.success) {
      setAiSetupStatus('Failed to start Ollama: ' + (start.error || 'unknown'), true);
      setAiStatus('offline');
      return;
    }
    // Poll for the HTTP server to come up (up to 15 s)
    let waited = 0;
    while (waited < 15000) {
      await new Promise(res => setTimeout(res, 500));
      waited += 500;
      r = await window.electronAPI.aiCheck();
      if (r.running) break;
      setAiSetupStatus(`Starting Ollama… ${(waited / 1000).toFixed(1)}s`);
    }
    if (!r.running) {
      setAiSetupStatus('Ollama did not respond. Check that it started, then click 🤖 again.', true);
      setAiStatus('offline');
      return;
    }
  }

  // ── Step 2: ensure at least one model is installed ───────────────────
  if (!r.models || r.models.length === 0) {
    if (aiAutoPullActive) {
      setAiSetupStatus(`Already downloading ${AI_AUTO_PULL_MODEL}…`);
      return;
    }
    aiAutoPullActive = true;
    setAiPullProgress(AI_AUTO_PULL_MODEL, 0, 0, 'Starting download…');
    // Wire progress events so the user sees the bar move
    window.electronAPI.onAiProgress(d => {
      if (d.model !== AI_AUTO_PULL_MODEL) return;
      setAiPullProgress(d.model, d.completed || 0, d.total || 0, d.status);
    });
    const pull = await window.electronAPI.aiPull(AI_AUTO_PULL_MODEL);
    aiAutoPullActive = false;
    // Tear down the pull-specific listeners and reattach the streaming ones
    window.electronAPI.removeAiListeners();
    setupAiTokenListeners();
    if (!pull.success) {
      setAiSetupStatus(`Download failed: ${pull.error || 'unknown'}`, true);
      setAiStatus('offline');
      return;
    }
    // Re-check
    r = await window.electronAPI.aiCheck();
  }

  // ── Step 3: select a model and we're done ────────────────────────────
  if (r.models && r.models.length > 0) {
    if (!aiModel || !r.models.includes(aiModel)) {
      // Prefer the auto-pull model if present, else the first installed
      aiModel = r.models.includes(AI_AUTO_PULL_MODEL) ? AI_AUTO_PULL_MODEL : r.models[0];
      saveSetting('ai.model', aiModel);
    }
    await refreshAiModelList();
    // Reset placeholder back to the friendly intro
    const ph = document.getElementById('ai-response-placeholder');
    if (ph && !aiResponse) {
      ph.innerHTML = `
        <span>Ask AI to write, continue, or transform text in this file.</span><br>
        <span style="font-size:0.72rem;color:#484f58">e.g. "write a mermaid user-login flow" · "add JSDoc to this function" · "convert to TypeScript"</span>`;
    }
  } else {
    setAiSetupStatus('No models available.', true);
  }
}

function closeAiPanel() {
  aiPanelOpen = false;
  if (aiGenerating) { window.electronAPI.aiAbort(); aiGenerating = false; }
  document.getElementById('ai-panel').classList.add('hidden');
  document.getElementById('ai-resize-handle').classList.add('hidden');
  document.getElementById('btn-ai').classList.remove('active');
}

function escapeHtml(str) {
  return str.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

async function saveSetting(dotPath, value) {
  const settings = await window.electronAPI.readSettings();
  const parts = dotPath.split('.');
  let obj = settings;
  for (let i = 0; i < parts.length - 1; i++) { obj[parts[i]] = obj[parts[i]] || {}; obj = obj[parts[i]]; }
  obj[parts[parts.length - 1]] = value;
  await window.electronAPI.writeSettings(settings);
}

// ── AI Settings page (inside Preferences) ──────────────────────────────────
function refreshAiSettingsPage() {
  // Build model cards
  window.electronAPI.aiCheck().then(result => {
    const installed = new Set(result.models || []);

    setAiStatus(result.running ? 'online' : 'offline');

    // Populate default model dropdown in settings
    const prefSel = document.getElementById('pref-ai-model');
    prefSel.innerHTML = installed.size === 0
      ? '<option value="">— no models installed —</option>'
      : [...installed].map(m => `<option value="${m}"${m===aiModel?' selected':''}>${m}</option>`).join('');

    // Model cards
    const container = document.getElementById('ai-model-cards');
    container.innerHTML = '';
    RECOMMENDED_MODELS.forEach(m => {
      const card = document.createElement('div');
      card.className = 'ai-model-card' + (installed.has(m.name) ? ' mc-installed' : '');
      card.innerHTML = `<div class="mc-name">${m.name}</div><div class="mc-size">${m.size}</div><div class="mc-desc">${m.desc}</div>`;
      if (!installed.has(m.name)) {
        card.title = 'Click to download';
        card.addEventListener('click', () => pullAiModel(m.name));
      } else {
        card.title = 'Already installed';
        card.style.cursor = 'default';
      }
      container.appendChild(card);
    });
  });
}

async function pullAiModel(modelName) {
  const progress = document.getElementById('pref-ai-pull-progress');
  const label    = document.getElementById('pref-ai-pull-label');
  const bar      = document.getElementById('pref-ai-pull-bar');
  progress.style.display = 'block';
  label.textContent = `Downloading ${modelName}…`;
  bar.style.width = '0%';

  window.electronAPI.onAiProgress(d => {
    if (d.model !== modelName) return;
    const pct = d.total > 0 ? Math.round((d.completed / d.total) * 100) : 0;
    bar.style.width = pct + '%';
    label.textContent = `${d.status || 'Downloading'} ${modelName} — ${pct}%`;
    if (d.done) {
      bar.style.width = '100%';
      label.textContent = `✓ ${modelName} downloaded!`;
      setTimeout(() => {
        progress.style.display = 'none';
        refreshAiSettingsPage();
        refreshAiModelList();
        window.electronAPI.removeAiListeners();
        // Re-register streaming listeners
        setupAiTokenListeners();
      }, 1500);
    }
  });

  await window.electronAPI.aiPull(modelName);
}

// Re-attaches the same chat-thread listeners used in setupAiPanel().
// Called after pull-progress listeners temporarily replace them during auto-download.
function setupAiTokenListeners() {
  window.electronAPI.onAiToken(token => {
    aiResponse += token;
    if (aiMessages.length && aiMessages[aiMessages.length - 1].role === 'assistant') {
      aiMessages[aiMessages.length - 1].content = aiResponse;
    }
    renderAiConversation(true);
  });
  window.electronAPI.onAiDone(() => {
    aiGenerating = false;
    const lastMsg = aiMessages[aiMessages.length - 1];
    if (lastMsg && lastMsg.role === 'assistant') lastMsg.content = aiResponse;
    if (lastMsg && lastMsg.agent && aiAgentTurn) {
      const tab = getActiveTab();
      lastMsg.diffSnapshot = {
        originalContent: aiAgentTurn.originalContent,
        newContent: aiResponse,
        language: tab?.language || 'plaintext',
        isSelection: aiAgentTurn.isSelection,
        selRange: aiAgentTurn.selRange,
      };
      renderAiConversation(false);
      showAgentDiff(lastMsg.diffSnapshot);
      document.getElementById('ai-action-bar').classList.add('hidden');
    } else {
      renderAiConversation(false);
      document.getElementById('ai-action-bar').classList.remove('hidden');
    }
    document.getElementById('btn-ai-send').disabled = !document.getElementById('ai-prompt')?.value.trim();
    document.getElementById('btn-ai-send').textContent = 'Send ↵';
    setTimeout(() => document.getElementById('ai-prompt')?.focus(), 50);
    const modelSel = document.getElementById('ai-model-select');
    if (modelSel?.value) {
      setAiStatus('online');
    } else {
      refreshAiModelList();
    }
  });
}

// ===== New Document Preferences =====
function setupNewDocPrefsPage() {
  // loadNewDocPrefs() is called directly from openPreferences(); nothing else needed here.
}

async function loadNewDocPrefs() {
  const s = await window.electronAPI.readSettings();
  const nd = s.newDoc || {};
  const enc = nd.encoding || 'UTF-8';
  const eol = nd.eol || 'Windows (CR LF)';
  const lang = nd.language || 'plaintext';
  const tmpl = nd.template || '';

  document.querySelectorAll('input[name="pref-encoding"]').forEach(r => { r.checked = r.value === enc; });
  document.querySelectorAll('input[name="pref-eol"]').forEach(r => { r.checked = r.value === eol; });
  const langEl = document.getElementById('pref-default-lang');
  if (langEl) langEl.value = lang;
  const tmplEl = document.getElementById('pref-new-template');
  if (tmplEl) tmplEl.value = tmpl;

  // Apply into in-memory defaults
  newDocDefaults = { encoding: enc, eol, language: lang, template: tmpl };
}

async function saveNewDocPrefs(settings) {
  settings.newDoc = {
    encoding: document.querySelector('input[name="pref-encoding"]:checked')?.value || 'UTF-8',
    eol:      document.querySelector('input[name="pref-eol"]:checked')?.value || 'Windows (CR LF)',
    language: document.getElementById('pref-default-lang')?.value || 'plaintext',
    template: document.getElementById('pref-new-template')?.value || '',
  };
}

// ===== Backup Preferences =====
function setupBackupPrefsPage() {
  document.getElementById('btn-backup-now').addEventListener('click', async () => {
    const result = await runBackup();
    document.getElementById('backup-last-status').textContent =
      result.success ? `✓ Backed up ${result.count} file(s) at ${new Date().toLocaleTimeString()}` : `✗ Error: ${result.error}`;
  });

  document.getElementById('btn-backup-open').addEventListener('click', async () => {
    const customPath = document.getElementById('pref-backup-path')?.value?.trim() || '';
    const root = await window.electronAPI.getBackupRoot(customPath);
    window.electronAPI.shellOpen(root);
  });

  document.getElementById('btn-backup-browse').addEventListener('click', async () => {
    // openFolderPicker returns a string path or null (see main.js)
    const chosen = await window.electronAPI.openFolderPicker();
    if (chosen) {
      document.getElementById('pref-backup-path').value = chosen;
    }
  });

}

async function loadBackupPrefs() {
  const s = await window.electronAPI.readSettings();
  const bkp = s.backup || {};
  const enableEl   = document.getElementById('pref-backup-enable');
  const intervalEl = document.getElementById('pref-backup-interval');
  const versionsEl = document.getElementById('pref-backup-versions');
  const pathEl     = document.getElementById('pref-backup-path');
  if (enableEl)   enableEl.checked     = !!bkp.enabled;
  if (intervalEl) intervalEl.value     = bkp.intervalMin || 5;
  if (versionsEl) versionsEl.value     = bkp.versions    || 5;
  if (pathEl)     pathEl.value         = bkp.path        || '';
  startAutoBackup(!!bkp.enabled, (bkp.intervalMin || 5) * 60 * 1000);
}

async function saveBackupPrefs(settings) {
  settings.backup = {
    enabled:     document.getElementById('pref-backup-enable')?.checked || false,
    intervalMin: parseInt(document.getElementById('pref-backup-interval')?.value || '5'),
    versions:    parseInt(document.getElementById('pref-backup-versions')?.value  || '5'),
    path:        document.getElementById('pref-backup-path')?.value?.trim() || '',
  };
}

// ===== Terminal Preferences =====
// Pick the sensible per-OS default shell. The renderer doesn't know its
// platform directly, so we sniff via navigator.userAgent (Electron exposes
// the host OS there). Wrong-OS guess is harmless — main.js falls back to
// its own platform-aware getShell() anyway.
function defaultShellForPlatform() {
  const ua = (navigator.userAgent || '').toLowerCase();
  if (ua.includes('windows')) return 'powershell.exe';
  if (ua.includes('mac'))     return '/bin/zsh';
  return '/bin/bash';
}

async function loadTerminalPrefs() {
  const s = await window.electronAPI.readSettings();
  const t = s.terminal || {};
  const shellEl    = document.getElementById('pref-shell');
  const fontSizeEl = document.getElementById('pref-term-fontsize');
  if (shellEl)    shellEl.value    = t.shell    || defaultShellForPlatform();
  if (fontSizeEl) fontSizeEl.value = t.fontSize || 13;
}

async function saveTerminalPrefs(settings) {
  settings.terminal = {
    shell:    document.getElementById('pref-shell')?.value?.trim()        || defaultShellForPlatform(),
    fontSize: parseInt(document.getElementById('pref-term-fontsize')?.value || '13'),
  };
}

function startAutoBackup(enabled, intervalMs) {
  clearInterval(autoBackupTimer);
  if (enabled && intervalMs > 0) {
    autoBackupTimer = setInterval(runBackup, intervalMs);
  }
}

async function runBackup() {
  const s = await window.electronAPI.readSettings();
  const bkp = s.backup || {};
  // For encrypted tabs we must NOT back up the in-memory plaintext — instead
  // copy the encrypted form already on disk. For everything else, snapshot
  // from the Monaco model so unsaved changes get backed up too.
  const files = [];
  for (const t of tabs) {
    if (!t.filePath || t.type !== 'editor' || !t.model) continue;
    if (t.encrypted) {
      // Skip if the file isn't on disk yet (e.g., never saved)
      const r = await window.electronAPI.readFile(t.filePath);
      if (r.success) files.push({ filePath: t.filePath, content: r.content });
    } else {
      files.push({ filePath: t.filePath, content: t.model.getValue() });
    }
  }
  if (!files.length) return { success: true, count: 0 };
  return window.electronAPI.backupFiles(files, {
    versionsToKeep: bkp.versions || 5,
    backupPath: bkp.path || '',
  });
}

// =============================================================================
// Git integration — see features/GIT.md
// =============================================================================
// State model:
//   gitRepos = Map<repoRoot, { branch, upstream, ahead, behind, files: [...] }>
//   gitFileToRepo = Map<filePath, repoRoot>   // memoise per-file detection
//   activeGitRepo = the repoRoot for the active tab, or null
// Refresh triggers: tab activation, file save, after any git op, window focus,
// 3-min auto-fetch timer.

const gitRepos = new Map();
const gitFileToRepo = new Map();
let activeGitRepo = null;
let gitAutoFetchTimer = null;
let gitInstalled = null;          // null until first check; true/false after
// Bumped from 3 → 5 min: less network/CPU churn for long-running sessions.
// The window-focus listener still fires a fresh status refresh on every
// regain-focus, so the user sees up-to-date branch state every time they
// return to Note++.
const GIT_AUTO_FETCH_MS = 5 * 60 * 1000;
// Reentrancy guard — skip a scheduled fetch if the previous one is still in
// flight (matters on big repos where fetch can take 5+ seconds).
let gitFetchInFlight = false;

// Find (and cache) the repo root for a given file path.
async function detectGitRepoForFile(filePath) {
  if (!filePath) return null;
  if (gitFileToRepo.has(filePath)) return gitFileToRepo.get(filePath);
  if (gitInstalled === false) return null;
  const root = await window.electronAPI.git.findRepo(filePath);
  gitFileToRepo.set(filePath, root);
  return root;
}

// Refresh git status for a repo. Updates gitRepos and any UI that depends on it.
async function refreshGitStatus(repoRoot) {
  if (!repoRoot) return null;
  const s = await window.electronAPI.git.status(repoRoot);
  if (!s.success) {
    gitRepos.delete(repoRoot);
    if (activeGitRepo === repoRoot) renderGitStatusBar();
    return null;
  }
  gitRepos.set(repoRoot, s);
  if (activeGitRepo === repoRoot) {
    renderGitStatusBar();
    renderSourceControlPanel();
    refreshFileTreeDecorations();
  }
  // HEAD may have moved (commit / pull / switch / reset) — invalidate the
  // cached head-content on every tab in this repo so the gutter re-paints.
  for (const t of tabs) {
    if (t.type !== 'editor' || !t.filePath) continue;
    const tRoot = gitFileToRepo.get(t.filePath);
    if (tRoot === repoRoot) {
      t._gitHeadContent = undefined;
      if (t.id === activeTabId) scheduleGitDiffUpdate(t);
    }
  }
  return s;
}

// Re-render the visible file tree to update git badges. Cheap because the
// tree only renders expanded folders, and we just walk the already-built DOM.
function refreshFileTreeDecorations() {
  const content = document.getElementById('file-tree-content');
  if (!content || content.classList.contains('hidden')) return;
  // Strip existing badges, then re-attach for every visible row
  content.querySelectorAll('.tree-git-badge').forEach(b => b.remove());
  content.querySelectorAll('.tree-item').forEach(row => {
    const fp = row.title;
    if (!fp) return;
    const dec = lookupGitDecoration(fp);
    if (!dec) return;
    const badge = document.createElement('span');
    badge.className = 'tree-git-badge sc-status-' + dec.cls;
    badge.textContent = dec.ch;
    badge.title = dec.label;
    row.appendChild(badge);
  });
}

// Called whenever the active tab changes — figure out which repo (if any) it belongs to.
async function updateActiveGitRepo() {
  if (gitInstalled === false) return;
  const tab = getActiveTab();
  const fp = tab?.filePath;
  let root = null;
  if (fp) root = await detectGitRepoForFile(fp);
  activeGitRepo = root;
  window.activeGitRepo = root;  // expose to lsp-client.js (used for workspaceRoot)
  if (root) {
    if (!gitRepos.has(root)) await refreshGitStatus(root);
    renderGitStatusBar();
    renderSourceControlPanel();
  } else {
    renderGitStatusBar();    // hides the pill
    renderSourceControlPanel();
  }
  // Refresh the inline-diff gutter for the now-active tab
  scheduleGitDiffUpdate(tab);
}

// ── Inline Git diff gutter (vs HEAD) ─────────────────────────────────────
// For every editor tab inside a git repo, paint a green / blue / red marker
// in the lines-decorations gutter on each line whose content differs from
// the HEAD revision. Updates on tab activation, after save, after every git
// op, and 400 ms after the user stops typing.
const gitDiffDebounce = new Map();   // tabId → setTimeout handle
const GIT_DIFF_DEBOUNCE_MS = 400;
const GIT_DIFF_MAX_LINES   = 50000;  // skip diffing absurdly large files

function scheduleGitDiffUpdate(tab) {
  if (!tab) return;
  clearTimeout(gitDiffDebounce.get(tab.id));
  gitDiffDebounce.set(tab.id, setTimeout(() => updateGitDiffGutter(tab), GIT_DIFF_DEBOUNCE_MS));
}

// Clear any existing gutter decorations on a tab (called when repo is irrelevant)
function clearGitDiffGutter(tab) {
  if (!tab?.model) return;
  tab._gitDiffDecorations = tab.model.deltaDecorations(tab._gitDiffDecorations || [], []);
}

async function updateGitDiffGutter(tab) {
  // Bail-outs — many reasons this might not apply
  if (!tab || tab.type !== 'editor' || !tab.model || !tab.filePath) return;
  if (gitInstalled === false) return;
  // Lazy-load jsdiff (~50 KB) on first git-diff render. Most editor sessions
  // never touch a git-tracked file, and the gutter is the only consumer.
  if (typeof window.Diff?.diffLines !== 'function') {
    try { await ensureDiff(); } catch { return; }
    if (typeof window.Diff?.diffLines !== 'function') return;
  }

  // Only diff files inside the active repo (cheap path: reuse the cache we
  // already populate in detectGitRepoForFile)
  const repoRoot = gitFileToRepo.has(tab.filePath)
    ? gitFileToRepo.get(tab.filePath)
    : await detectGitRepoForFile(tab.filePath);
  if (!repoRoot) { clearGitDiffGutter(tab); return; }

  // Cache the file's HEAD content per (tab, headContentHash) so we don't
  // re-fetch on every keystroke. Fetched once at first call + after any
  // git op (refreshGitStatus calls scheduleGitDiffUpdate too).
  if (tab._gitHeadContent === undefined) {
    const rel = relativePathInRepo(tab.filePath, repoRoot);
    tab._gitHeadContent = await window.electronAPI.git.showHead(repoRoot, rel);
  }
  const headContent = tab._gitHeadContent;
  // If the file isn't tracked yet (new file) — mark ALL lines as added
  const editorContent = tab.model.getValue();
  if (headContent == null) {
    paintGitDiffDecos(tab, [{ kind: 'added', startLine: 1, endLine: tab.model.getLineCount() }]);
    return;
  }
  // Cheap noop check
  if (editorContent === headContent) { clearGitDiffGutter(tab); return; }

  // jsdiff line-level diff
  if (tab.model.getLineCount() > GIT_DIFF_MAX_LINES) { clearGitDiffGutter(tab); return; }
  let changes;
  try { changes = window.Diff.diffLines(headContent, editorContent); }
  catch { clearGitDiffGutter(tab); return; }

  // Walk the change list and convert to per-line decorations on the
  // EDITOR side (Monaco lines = "modified" side of the diff).
  //   - added chunk preceded by removed chunk → those added lines are MODIFIED
  //   - added chunk standing alone           → ADDED
  //   - removed chunk standing alone         → DELETED triangle on the next existing line
  const decos = [];
  let editorLine = 1;
  for (let i = 0; i < changes.length; i++) {
    const ch = changes[i];
    const nLines = ch.count != null ? ch.count : (ch.value || '').split('\n').length - 1;
    if (ch.added) {
      // Is the previous chunk a removed one of equal-ish span? → modified
      const prev = changes[i - 1];
      if (prev && prev.removed) {
        decos.push({ kind: 'modified', startLine: editorLine, endLine: editorLine + nLines - 1 });
      } else {
        decos.push({ kind: 'added',    startLine: editorLine, endLine: editorLine + nLines - 1 });
      }
      editorLine += nLines;
    } else if (ch.removed) {
      // If followed by an `added` chunk we already painted "modified" above.
      const next = changes[i + 1];
      if (next && next.added) continue; // it'll be handled by the added branch
      // Otherwise — pure deletion. Mark a triangle on the editor line where
      // the deletion happened (or line 1 if the deletion was at the top).
      const at = Math.max(1, editorLine);
      decos.push({ kind: 'deleted', startLine: at, endLine: at });
      // editorLine doesn't advance — removed lines aren't in the editor
    } else {
      editorLine += nLines;
    }
  }
  paintGitDiffDecos(tab, decos);
}

// Convert our compact `{kind, startLine, endLine}` array to Monaco delta-decorations.
function paintGitDiffDecos(tab, decos) {
  if (!tab.model) return;
  const newDecos = decos.map(d => ({
    range: new monaco.Range(d.startLine, 1, d.endLine, 1),
    options: {
      linesDecorationsClassName:
        d.kind === 'added'    ? 'git-gutter-added'    :
        d.kind === 'modified' ? 'git-gutter-modified' :
                                'git-gutter-deleted',
      stickiness: monaco.editor.TrackedRangeStickiness.NeverGrowsWhenTypingAtEdges,
    },
  }));
  tab._gitDiffDecorations = tab.model.deltaDecorations(tab._gitDiffDecorations || [], newDecos);
}

// Convert absolute file path to a forward-slash path relative to the repo root.
function relativePathInRepo(filePath, repoRoot) {
  const f = filePath.replace(/\\/g, '/');
  const r = repoRoot.replace(/\\/g, '/').replace(/\/$/, '');
  if (f.toLowerCase().startsWith(r.toLowerCase() + '/')) return f.slice(r.length + 1);
  return f; // best-effort
}

// Render the status-bar git pill: "⎇ branch ↑n ↓m ●k"
function renderGitStatusBar() {
  const el = document.getElementById('status-git');
  if (!el) return;
  const s = activeGitRepo ? gitRepos.get(activeGitRepo) : null;
  if (!s || !s.success) { el.classList.add('hidden'); return; }
  el.classList.remove('hidden');
  const dirty = (s.files || []).length;
  const aheadBehind = (s.ahead || s.behind)
    ? `  ↑${s.ahead} ↓${s.behind}` : '';
  const dirtyMark = dirty ? `  ●${dirty}` : '';
  el.textContent = `⎇ ${s.branch || 'detached'}${aheadBehind}${dirtyMark}`;
  el.title = `Branch ${s.branch}${s.upstream ? ` → ${s.upstream}` : ''}${dirty ? ` · ${dirty} change${dirty===1?'':'s'}` : ''} · click for Source Control`;
}

// Toggle the Source Control side panel. Visibility lives entirely in the
// `hidden` class — when opened we trigger a fresh status fetch.
function toggleSourceControlPanel() {
  const el = document.getElementById('sc-panel');
  if (!el) return;
  const willShow = el.classList.contains('hidden');
  el.classList.toggle('hidden');
  if (willShow) {
    renderSourceControlPanel();
    if (activeGitRepo) refreshGitStatus(activeGitRepo);
  }
}

// ═════════════════════════════════════════════════════════════════════════
// Azure Icons library — docked right pane
// ─────────────────────────────────────────────────────────────────────────
// Toolbar 🎨 → fetches the maskati.github.io/azure-icons manifest via IPC,
// renders a searchable grid of SVG thumbnails, lets the user drag any tile
// onto the whiteboard iframe to insert it as an image element.
//
// Data flow:
//   fetch manifest (IPC) → iconLibState.icons
//   thumbnail render: fetch each SVG on first paint, cache the data URL
//     in iconLibState.svgCache; subsequent grid renders reuse the cache
//     with no additional IPC round-trip
//   drag: dragstart on tile → we intercept via a transparent overlay
//     that catches drops over the whiteboard iframe (the iframe swallows
//     DOM drag events, so we sit above it during drag); on drop we
//     compute canvas coords + postMessage `wb-add-icon` to the iframe
// ═════════════════════════════════════════════════════════════════════════
const iconLibState = {
  icons: null,           // array of { name, type, keywords, description, svgPath }
  filtered: null,        // current search-filtered subset
  svgCache: new Map(),   // svgPath → { svg: string, dataURL: string }
  loading: false,
  dragIcon: null,        // { svgPath, name, dataURL } during drag
  overlayEl: null,
};

function toggleIconsPanel() {
  const panel = document.getElementById('icons-panel');
  const resize = document.getElementById('icons-panel-resize');
  if (!panel) return;
  const willShow = panel.classList.contains('hidden');
  panel.classList.toggle('hidden');
  resize?.classList.toggle('hidden');
  if (willShow) {
    // First open? Populate lazily.
    if (!iconLibState.icons) loadIconManifest(false);
    else renderIconsGrid();
    document.getElementById('icons-search-input')?.focus();
  }
}

async function loadIconManifest(force) {
  if (iconLibState.loading) return;
  iconLibState.loading = true;
  const statusEl = document.getElementById('icons-panel-status');
  const grid = document.getElementById('icons-panel-grid');
  if (grid) grid.innerHTML = '';
  if (statusEl) {
    statusEl.classList.remove('error');
    statusEl.textContent = force
      ? 'Refreshing manifest from maskati.github.io/azure-icons…'
      : 'Loading icon manifest…';
    statusEl.style.display = '';
  }
  try {
    const res = await window.electronAPI.iconLib.fetchManifest(!!force);
    if (!res?.success) {
      if (statusEl) {
        statusEl.classList.add('error');
        statusEl.innerHTML = `Failed to load manifest: ${escapeIconHtml(res?.error || 'unknown')}<br><br><em>Check your internet connection and click Refresh.</em>`;
      }
      return;
    }
    iconLibState.icons = res.icons;
    if (statusEl) statusEl.style.display = 'none';
    applyIconSearch();
  } catch (err) {
    if (statusEl) {
      statusEl.classList.add('error');
      statusEl.textContent = 'Failed: ' + (err.message || String(err));
    }
  } finally {
    iconLibState.loading = false;
  }
}

// Apply the current search string. Match against name + type + keywords.
function applyIconSearch() {
  if (!iconLibState.icons) return;
  const q = (document.getElementById('icons-search-input')?.value || '').trim().toLowerCase();
  if (!q) {
    iconLibState.filtered = iconLibState.icons;
  } else {
    const terms = q.split(/\s+/).filter(Boolean);
    iconLibState.filtered = iconLibState.icons.filter(icon => {
      const hay = (icon.name + ' ' + icon.type + ' ' + icon.keywords).toLowerCase();
      return terms.every(t => hay.includes(t));
    });
  }
  const countEl = document.getElementById('icons-panel-count');
  if (countEl) countEl.textContent = `${iconLibState.filtered.length} / ${iconLibState.icons.length}`;
  renderIconsGrid();
}

// Cap DOM size to keep the panel responsive when the search is broad.
// 500 tiles is enough to see full results after searching; scrolling to
// see all 1000+ icons in the unfiltered view is a niche need.
const ICONS_MAX_RENDER = 500;

function renderIconsGrid() {
  const grid = document.getElementById('icons-panel-grid');
  if (!grid) return;
  const list = iconLibState.filtered || [];
  const truncated = list.length > ICONS_MAX_RENDER;
  const view = truncated ? list.slice(0, ICONS_MAX_RENDER) : list;
  grid.innerHTML = '';
  const frag = document.createDocumentFragment();
  for (const icon of view) {
    const tile = document.createElement('div');
    tile.className = 'icon-tile';
    tile.title = `${icon.name}\n${icon.type}\n${icon.keywords || ''}`.trim();
    tile.draggable = true;
    tile.dataset.svgPath = icon.svgPath;

    const cached = iconLibState.svgCache.get(icon.svgPath);
    if (cached) {
      const img = document.createElement('img');
      img.src = cached.dataURL;
      img.alt = icon.name;
      tile.appendChild(img);
    } else {
      // Placeholder while we lazy-fetch. IntersectionObserver upgrades on scroll.
      const ph = document.createElement('div');
      ph.style.cssText = 'width:100%;height:100%;background:rgba(128,128,128,0.1);border-radius:2px';
      tile.appendChild(ph);
    }
    frag.appendChild(tile);
  }
  grid.appendChild(frag);
  if (truncated) {
    const more = document.createElement('div');
    more.style.cssText = 'grid-column: 1 / -1; padding: 10px; text-align: center; color: #888; font-size: 11px; font-style: italic';
    more.textContent = `Showing first ${ICONS_MAX_RENDER} of ${list.length}. Refine your search to narrow.`;
    grid.appendChild(more);
  }
  observeIconTiles();
}

// Lazy-load thumbnails as they scroll into view. Icons are ~1 KB each so
// downloading them all upfront would work — but on first open with a
// cold cache that's 1000+ concurrent IPC calls, which chokes the UI.
let _iconObserver = null;
function observeIconTiles() {
  if (_iconObserver) _iconObserver.disconnect();
  _iconObserver = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const tile = e.target;
      _iconObserver.unobserve(tile);
      const svgPath = tile.dataset.svgPath;
      if (!svgPath || iconLibState.svgCache.has(svgPath)) continue;
      window.electronAPI.iconLib.getSvg(svgPath).then(res => {
        if (!res?.success) return;
        const dataURL = 'data:image/svg+xml;base64,' + btoa(unescape(encodeURIComponent(res.svg)));
        iconLibState.svgCache.set(svgPath, { svg: res.svg, dataURL });
        // Only inject the img if this tile is still the same icon
        // (grid may have been re-rendered by a search change).
        if (tile.dataset.svgPath === svgPath && tile.isConnected) {
          tile.innerHTML = '';
          const img = document.createElement('img');
          img.src = dataURL;
          img.alt = tile.title.split('\n')[0];
          tile.appendChild(img);
        }
      }).catch(() => {});
    }
  }, { root: document.getElementById('icons-panel-body'), rootMargin: '200px' });
  document.querySelectorAll('.icon-tile').forEach(t => _iconObserver.observe(t));
}

function escapeIconHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ── Drag from panel → drop on whiteboard iframe ──────────────────────────
// The iframe swallows drag events on its own body, so the parent's drop
// handler never fires while the cursor is over the whiteboard canvas.
// Workaround: while dragging an icon, we lay a transparent overlay over
// the whiteboard-container so the parent catches the dragover / drop and
// can compute canvas-relative coords. On drop we postMessage the SVG to
// the iframe, which calls api.addFiles + creates the image element.
function setupIconsPanel() {
  const closeBtn = document.getElementById('icons-close-btn');
  const refreshBtn = document.getElementById('icons-refresh-btn');
  const searchInput = document.getElementById('icons-search-input');
  const grid = document.getElementById('icons-panel-grid');

  closeBtn?.addEventListener('click', toggleIconsPanel);
  refreshBtn?.addEventListener('click', () => loadIconManifest(true));

  let searchDebounce;
  searchInput?.addEventListener('input', () => {
    clearTimeout(searchDebounce);
    searchDebounce = setTimeout(applyIconSearch, 150);
  });

  // Delegated dragstart on the grid so we don't need to attach to every tile.
  grid?.addEventListener('dragstart', async (e) => {
    const tile = e.target.closest('.icon-tile');
    if (!tile) return;
    const svgPath = tile.dataset.svgPath;
    const cached = iconLibState.svgCache.get(svgPath);
    // If not cached yet, fetch synchronously-ish. In practice the tile
    // must already be visible to be draggable, so it's usually cached.
    if (!cached) {
      try {
        const res = await window.electronAPI.iconLib.getSvg(svgPath);
        if (res?.success) {
          const dataURL = 'data:image/svg+xml;base64,' + btoa(unescape(encodeURIComponent(res.svg)));
          iconLibState.svgCache.set(svgPath, { svg: res.svg, dataURL });
        }
      } catch {}
    }
    const now = iconLibState.svgCache.get(svgPath);
    iconLibState.dragIcon = now
      ? { svgPath, name: tile.title.split('\n')[0], dataURL: now.dataURL, svg: now.svg }
      : null;
    tile.classList.add('dragging');
    // Native drag ghost — set a small drag image so the cursor shows
    // what's being dragged.
    if (now) {
      const img = new Image();
      img.src = now.dataURL;
      try { e.dataTransfer.setDragImage(img, 24, 24); } catch {}
    }
    // Also stash a fallback plain-text payload so external drop targets
    // (e.g. Excalidraw's built-in drop-URL handler) get something usable.
    if (now) e.dataTransfer.setData('text/plain', now.svg);
    e.dataTransfer.effectAllowed = 'copy';
    showIconDragOverlay(true);
  });

  grid?.addEventListener('dragend', (e) => {
    const tile = e.target.closest('.icon-tile');
    tile?.classList.remove('dragging');
    iconLibState.dragIcon = null;
    showIconDragOverlay(false);
  });
}

function showIconDragOverlay(on) {
  const wbContainer = document.getElementById('whiteboard-container');
  if (!wbContainer || wbContainer.classList.contains('hidden')) return;
  let ov = document.getElementById('icons-drag-overlay');
  if (!ov && on) {
    ov = document.createElement('div');
    ov.id = 'icons-drag-overlay';
    wbContainer.appendChild(ov);
    iconLibState.overlayEl = ov;
    // Only enable pointer-events during drag so the overlay actually
    // catches dragover / drop instead of the iframe below.
    ov.style.pointerEvents = 'auto';
    ov.addEventListener('dragover', (e) => {
      if (!iconLibState.dragIcon) return;
      e.preventDefault();
      e.dataTransfer.dropEffect = 'copy';
    });
    ov.addEventListener('drop', (e) => {
      e.preventDefault();
      const drag = iconLibState.dragIcon;
      showIconDragOverlay(false);
      if (!drag) return;
      const iframe = document.getElementById('whiteboard-frame');
      if (!iframe) return;
      // Post drop coords in iframe-relative pixel space; the iframe
      // translates to canvas coords using its current viewport transform.
      const rect = iframe.getBoundingClientRect();
      const px = e.clientX - rect.left;
      const py = e.clientY - rect.top;
      sendToWhiteboard({
        type: 'wb-add-icon',
        dataURL: drag.dataURL,
        name: drag.name,
        svgPath: drag.svgPath,
        clientX: px,
        clientY: py,
      });
    });
  }
  if (ov) ov.classList.toggle('active', !!on);
}

// ── Source Control panel rendering ────────────────────────────────────────
// Pulls from gitRepos[activeGitRepo] and rebuilds the visible DOM.
function renderSourceControlPanel() {
  const panel = document.getElementById('sc-panel');
  if (!panel || panel.classList.contains('hidden')) return;

  const noRepoMsg = document.getElementById('sc-no-repo');
  const emptyMsg  = document.getElementById('sc-empty');
  const stagedSec = document.getElementById('sc-staged-section');
  const changesSec= document.getElementById('sc-changes-section');
  const branchName = document.getElementById('sc-branch-name');
  const branchArrows = document.getElementById('sc-branch-arrows');
  // Interactive blocks that are meaningless without an active repo. We
  // collapse them so the panel cleanly shows just the "no repo" message
  // when the active tab's file isn't inside any git checkout.
  const branchActions = document.getElementById('sc-branch-actions');
  const remoteRow     = document.getElementById('sc-remote-row');
  const commitBlock   = document.getElementById('sc-commit-block');
  const filesBlock    = document.getElementById('sc-files');

  const s = activeGitRepo ? gitRepos.get(activeGitRepo) : null;

  if (!activeGitRepo || !s || !s.success) {
    noRepoMsg.classList.remove('hidden');
    emptyMsg.classList.add('hidden');
    stagedSec.style.display = 'none';
    changesSec.style.display = 'none';
    if (branchActions) branchActions.style.display = 'none';
    if (remoteRow)     remoteRow.style.display     = 'none';
    if (commitBlock)   commitBlock.style.display   = 'none';
    if (filesBlock)    filesBlock.style.display    = 'none';
    branchName.textContent = '—';
    branchArrows.textContent = '';
    return;
  }
  noRepoMsg.classList.add('hidden');
  if (branchActions) branchActions.style.display = '';
  if (remoteRow)     remoteRow.style.display     = '';
  if (commitBlock)   commitBlock.style.display   = '';
  if (filesBlock)    filesBlock.style.display    = '';

  branchName.textContent = s.branch || 'detached';
  branchArrows.textContent = (s.ahead || s.behind)
    ? ` ↑${s.ahead} ↓${s.behind}` : '';

  // Partition files into staged (index status) and unstaged (worktree status).
  // A file can appear in BOTH sections if it has staged AND further worktree
  // changes (e.g. status code "MM").
  const staged = [];
  const changes = [];
  for (const f of (s.files || [])) {
    if (f.x !== ' ' && f.x !== '?') staged.push(f);
    if (f.y !== ' ' || (f.x === '?' && f.y === '?')) changes.push(f);
  }

  document.getElementById('sc-staged-count').textContent  = String(staged.length);
  document.getElementById('sc-changes-count').textContent = String(changes.length);

  if (staged.length + changes.length === 0) {
    emptyMsg.classList.remove('hidden');
    stagedSec.style.display = 'none';
    changesSec.style.display = 'none';
    return;
  }
  emptyMsg.classList.add('hidden');
  stagedSec.style.display = staged.length ? '' : 'none';
  changesSec.style.display = changes.length ? '' : 'none';

  renderScFileList('sc-staged-list',  staged,  'staged');
  renderScFileList('sc-changes-list', changes, 'unstaged');
}

// Status code → display char + class suffix (for colouring)
function scStatusGlyph(f, side) {
  // For staged side, look at f.x (index). For unstaged side, look at f.y.
  const c = side === 'staged' ? f.x : f.y;
  if (f.x === '?' && f.y === '?') return { ch: '?', cls: 'Q' }; // untracked
  if (c === 'M') return { ch: 'M', cls: 'M' };
  if (c === 'A') return { ch: 'A', cls: 'A' };
  if (c === 'D') return { ch: 'D', cls: 'D' };
  if (c === 'R') return { ch: 'R', cls: 'R' };
  if (c === 'C') return { ch: 'C', cls: 'R' };
  if (c === 'U' || f.x === 'U' || f.y === 'U' || (f.x === 'A' && f.y === 'A') || (f.x === 'D' && f.y === 'D')) {
    return { ch: '!', cls: 'U' }; // conflict
  }
  return { ch: c, cls: 'M' };
}

function renderScFileList(containerId, files, side) {
  const el = document.getElementById(containerId);
  el.innerHTML = '';
  for (const f of files) {
    const row = document.createElement('div');
    row.className = 'sc-file';
    row.title = f.path;

    const { ch, cls } = scStatusGlyph(f, side);
    const status = document.createElement('span');
    status.className = `sc-file-status sc-status-${cls}`;
    status.textContent = ch;

    const name = document.createElement('span');
    name.className = 'sc-file-path';
    name.textContent = f.path;
    name.addEventListener('click', () => openScFile(f.path));
    name.style.cursor = 'pointer';

    const actions = document.createElement('span');
    actions.className = 'sc-file-actions';

    if (side === 'staged') {
      const unstage = makeIconBtn('−', 'Unstage', () => doScAction('unstage', [f.path]));
      actions.appendChild(unstage);
    } else {
      const stage = makeIconBtn('+', 'Stage', () => doScAction('stage', [f.path]));
      const discard = makeIconBtn('↶', f.x === '?' ? 'Delete untracked' : 'Discard changes',
        () => doScAction(f.x === '?' && f.y === '?' ? 'clean' : 'discard', [f.path]));
      actions.appendChild(stage);
      actions.appendChild(discard);
    }

    row.appendChild(status);
    row.appendChild(name);
    row.appendChild(actions);
    el.appendChild(row);
  }
}

function makeIconBtn(label, title, fn) {
  const b = document.createElement('button');
  b.className = 'sc-icon-btn';
  b.textContent = label;
  b.title = title;
  b.addEventListener('click', (e) => { e.stopPropagation(); fn(); });
  return b;
}

// Open the file in the editor (or focus it if already open).
async function openScFile(relPath) {
  if (!activeGitRepo) return;
  const fullPath = activeGitRepo + (activeGitRepo.endsWith('\\') || activeGitRepo.endsWith('/') ? '' : '\\') + relPath.replace(/\//g, '\\');
  await openFile([fullPath]);
}

// Common dispatcher for stage/unstage/discard/clean. Refreshes status after.
async function doScAction(action, paths) {
  if (!activeGitRepo) return;
  const api = window.electronAPI.git;
  let r;
  if (action === 'stage')    r = await api.stage(activeGitRepo, paths);
  else if (action === 'unstage') r = await api.unstage(activeGitRepo, paths);
  else if (action === 'discard') r = await api.discard(activeGitRepo, paths);
  else if (action === 'clean')   r = await api.clean(activeGitRepo, paths);
  if (r && !r.success) showToast('git ' + action + ' failed: ' + (r.error || 'unknown'));
  await refreshGitStatus(activeGitRepo);
}

// Wire all the Source Control panel buttons. Called once at startup from setupSourceControl().
function setupSourceControlPanel() {
  const api = window.electronAPI.git;
  const onClick = (id, fn) => document.getElementById(id)?.addEventListener('click', fn);

  onClick('sc-close-btn',   () => document.getElementById('sc-panel').classList.add('hidden'));
  onClick('sc-refresh-btn', () => { if (activeGitRepo) refreshGitStatus(activeGitRepo); });

  onClick('sc-stage-all-btn',   async () => {
    const s = gitRepos.get(activeGitRepo);
    const paths = (s?.files || [])
      .filter(f => f.y !== ' ' || (f.x === '?' && f.y === '?'))
      .map(f => f.path);
    if (paths.length) await doScAction('stage', paths);
  });
  onClick('sc-unstage-all-btn', async () => {
    const s = gitRepos.get(activeGitRepo);
    const paths = (s?.files || [])
      .filter(f => f.x !== ' ' && f.x !== '?')
      .map(f => f.path);
    if (paths.length) await doScAction('unstage', paths);
  });
  onClick('sc-discard-all-btn', async () => {
    const s = gitRepos.get(activeGitRepo);
    if (!s || !s.files.length) return;
    const r = await window.electronAPI.messageDialog({
      type: 'warning', title: 'Discard all changes',
      message: `Discard all uncommitted changes in this repository?`,
      detail: 'This cannot be undone.',
      buttons: ['Discard All', 'Cancel'], defaultId: 1, cancelId: 1,
    });
    if (r.response !== 0) return;
    const tracked   = s.files.filter(f => f.y !== ' ' && !(f.x === '?' && f.y === '?')).map(f => f.path);
    const untracked = s.files.filter(f => f.x === '?' && f.y === '?').map(f => f.path);
    if (tracked.length)   await api.discard(activeGitRepo, tracked);
    if (untracked.length) await api.clean(activeGitRepo, untracked);
    await refreshGitStatus(activeGitRepo);
  });

  onClick('sc-commit-btn',      async () => doScCommit(false));
  onClick('sc-commit-amend-btn', async () => doScCommit(true));

  onClick('sc-push-btn',  async () => doScRemoteOp('push',  'Push'));
  onClick('sc-pull-btn',  async () => doScRemoteOp('pull',  'Pull'));
  onClick('sc-sync-btn',  async () => doScRemoteOp('sync',  'Sync'));
  onClick('sc-fetch-btn', async () => doScRemoteOp('fetch', 'Fetch'));

  onClick('sc-branch-switch-btn', () => openBranchPicker('switch'));
  onClick('sc-branch-create-btn', () => openBranchPicker('create'));

  // Commit on Ctrl+Enter inside the message textarea
  document.getElementById('sc-commit-msg')?.addEventListener('keydown', (e) => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      doScCommit(false);
    }
  });

  // Status-bar git pill → toggle panel
  document.getElementById('status-git')?.addEventListener('click', () => toggleSourceControlPanel());

  // Panel resize drag
  setupScPanelResize();
}

async function doScCommit(amend) {
  if (!activeGitRepo) return;
  const msgEl = document.getElementById('sc-commit-msg');
  const msg = (msgEl?.value || '').trim();
  if (!amend && !msg) { showToast('Enter a commit message'); msgEl?.focus(); return; }
  const r = amend
    ? await window.electronAPI.git.commitAmend(activeGitRepo, msg)
    : await window.electronAPI.git.commit(activeGitRepo, msg);
  if (!r || !r.success) { showToast('Commit failed: ' + (r?.error || 'unknown')); return; }
  msgEl.value = '';
  showToast(amend ? 'Amended last commit' : 'Committed');
  await refreshGitStatus(activeGitRepo);
}

async function doScRemoteOp(op, label) {
  if (!activeGitRepo) return;
  showToast(`${label}ing…`);
  const r = await window.electronAPI.git[op](activeGitRepo);
  if (!r || !r.success) {
    showToast(`${label} failed: ` + (r?.error || 'unknown'));
  } else {
    showToast(`${label} done`);
  }
  await refreshGitStatus(activeGitRepo);
}

// Generic single-line input prompt — Electron suppresses window.prompt(), so we
// roll our own modal. Resolves to the trimmed string, or null if cancelled.
function promptInput({ title = 'Input', message = '', placeholder = '', defaultValue = '', validate = null } = {}) {
  return new Promise((resolve) => {
    const dlg   = document.getElementById('prompt-input-dialog');
    const field = document.getElementById('prompt-input-field');
    const err   = document.getElementById('prompt-input-error');
    document.getElementById('prompt-input-title').textContent = title;
    document.getElementById('prompt-input-message').textContent = message;
    field.value = defaultValue || '';
    field.placeholder = placeholder || '';
    err.classList.add('hidden');
    dlg.classList.remove('hidden');
    setTimeout(() => field.focus(), 50);

    const cleanup = () => {
      dlg.classList.add('hidden');
      okBtn.removeEventListener('click', onOk);
      cancelBtn.removeEventListener('click', onCancel);
      xBtn.removeEventListener('click', onCancel);
      field.removeEventListener('keydown', onKey);
    };
    const onOk = () => {
      const v = (field.value || '').trim();
      if (validate) {
        const msg = validate(v);
        if (msg) { err.textContent = msg; err.classList.remove('hidden'); return; }
      }
      cleanup();
      resolve(v || null);
    };
    const onCancel = () => { cleanup(); resolve(null); };
    const onKey = (e) => {
      if (e.key === 'Enter') { e.preventDefault(); onOk(); }
      else if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
    };

    const okBtn     = document.getElementById('prompt-input-ok');
    const cancelBtn = document.getElementById('prompt-input-cancel');
    const xBtn      = document.getElementById('prompt-input-x');
    okBtn.addEventListener('click', onOk);
    cancelBtn.addEventListener('click', onCancel);
    xBtn.addEventListener('click', onCancel);
    field.addEventListener('keydown', onKey);
  });
}

// Branch picker: 'switch' lists local branches and switches; 'create' prompts for a name.
async function openBranchPicker(mode) {
  if (!activeGitRepo) return;
  if (mode === 'create') {
    const name = await promptInput({
      title: 'Create new branch',
      message: `Branch off ${gitRepos.get(activeGitRepo)?.branch || 'HEAD'}`,
      placeholder: 'feature/my-branch',
      validate: (v) => {
        if (!v) return 'Branch name cannot be empty';
        if (/[\s~^:?*\[\]\\]/.test(v)) return 'Branch name contains invalid characters';
        if (v.startsWith('-')) return 'Branch name cannot start with "-"';
        if (v.endsWith('.')) return 'Branch name cannot end with "."';
        return null;
      },
    });
    if (!name) return;
    const r = await window.electronAPI.git.branchCreate(activeGitRepo, name, null);
    if (!r.success) { showToast('Create branch failed: ' + (r.error || 'unknown')); return; }
    showToast('Switched to ' + name);
    await refreshGitStatus(activeGitRepo);
    return;
  }
  // mode === 'switch'
  const list = await window.electronAPI.git.branchList(activeGitRepo);
  if (!list.success) { showToast('Cannot list branches: ' + list.error); return; }
  const items = list.locals.map(b => [b + (b === list.current ? '  (current)' : ''), async () => {
    if (b === list.current) return;
    const r = await window.electronAPI.git.branchSwitch(activeGitRepo, b);
    if (!r.success) showToast('Switch failed: ' + (r.error || 'unknown'));
    else            showToast('Switched to ' + b);
    await refreshGitStatus(activeGitRepo);
  }]);
  if (!items.length) { showToast('No local branches found'); return; }
  const btn = document.getElementById('sc-branch-switch-btn');
  const rect = btn.getBoundingClientRect();
  showFloatingMenu(rect.left, rect.bottom, items);
}

// Panel resize handle (mirrors file-tree resize approach)
function setupScPanelResize() {
  const handle = document.getElementById('sc-panel-resize');
  const panel = document.getElementById('sc-panel');
  if (!handle || !panel) return;
  let dragging = false, startX = 0, startW = 0;
  handle.addEventListener('mousedown', (e) => {
    dragging = true;
    startX = e.clientX;
    startW = panel.offsetWidth;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  });
  document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    const w = Math.max(220, Math.min(600, startW + (e.clientX - startX)));
    panel.style.width = w + 'px';
  });
  document.addEventListener('mouseup', () => {
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  });
}

// Initial setup: check if git is installed, then wire periodic auto-fetch.
async function initGitIntegration() {
  try {
    gitInstalled = await window.electronAPI.git.installed();
  } catch { gitInstalled = false; }
  if (!gitInstalled) return; // No git on PATH — silently disable git UI
  await updateActiveGitRepo();
  if (gitAutoFetchTimer) clearInterval(gitAutoFetchTimer);
  gitAutoFetchTimer = setInterval(() => {
    // Skip if no repo, if already fetching, or if the window is hidden /
    // minimized. The user sees stale status when they come back focused,
    // and the focus handler below catches that case for a fresh refresh.
    if (!activeGitRepo) return;
    if (gitFetchInFlight) return;
    if (document.visibilityState !== 'visible') return;
    gitFetchInFlight = true;
    window.electronAPI.git.fetch(activeGitRepo)
      .then(() => refreshGitStatus(activeGitRepo))
      .finally(() => { gitFetchInFlight = false; });
  }, GIT_AUTO_FETCH_MS);

  // Refresh when window regains focus
  window.addEventListener('focus', () => {
    if (activeGitRepo) refreshGitStatus(activeGitRepo);
  });
}


// See features/ENCRYPTION.md for the full spec. The profile lives at
// `%AppData%\notepp\encryption\profile.json` and contains the DEK wrapped by
// each enabled auth method (password, recovery key). Plaintext DEK is held in
// memory only (`appEnc.rawDek`); never written to disk.

async function encryptionProfilePath() {
  const userData = await window.electronAPI.getUserDataPath();
  // Forward slashes work on Windows, macOS, and Linux. Node's fs APIs in
  // the main process normalise both separators correctly.
  return userData + '/encryption/profile.json';
}

// Read profile.json from disk into `appEnc.profile`. Returns the parsed profile,
// or null if not configured / unreadable / malformed.
async function loadEncryptionProfile() {
  try {
    const p = await encryptionProfilePath();
    const res = await window.electronAPI.readFile(p);
    if (!res.success) { appEnc.profile = null; return null; }
    await ensureCrypto();
    const parsed = window.NotePPCrypto.parseProfile(res.content);
    appEnc.profile = parsed;
    return parsed;
  } catch (e) {
    appEnc.profile = null;
    return null;
  }
}

async function saveEncryptionProfile(profile) {
  const p = await encryptionProfilePath();
  // write-file IPC auto-creates parent dirs (mkdirSync recursive)
  return await window.electronAPI.writeFile(p, JSON.stringify(profile, null, 2));
}

// First-time setup. Creates a new profile + DEK + recovery key and writes the
// profile file. The recovery key is returned to the caller to display once.
async function createEncryptionProfile(password) {
  await ensureCrypto();
  const { profile, rawDek, recoveryKey } = await window.NotePPCrypto.createProfile(password);
  const res = await saveEncryptionProfile(profile);
  if (!res.success) throw new Error('Failed to write profile: ' + (res.error || 'unknown'));
  appEnc.profile = profile;
  appEnc.rawDek = rawDek;       // session starts unlocked right after setup
  return { profile, recoveryKey };
}

// Unlock with password. Returns true on success, false on wrong password.
async function unlockEncryptionWithPassword(password) {
  if (!appEnc.profile) return false;
  try {
    await ensureCrypto();
    const dek = await window.NotePPCrypto.unlockWithPassword(appEnc.profile, password);
    appEnc.rawDek = dek;
    return true;
  } catch (e) {
    return false;
  }
}

// Unlock with recovery key. Returns true on success, false otherwise.
async function unlockEncryptionWithRecoveryKey(recoveryKey) {
  if (!appEnc.profile) return false;
  try {
    await ensureCrypto();
    const dek = await window.NotePPCrypto.unlockWithRecoveryKey(appEnc.profile, recoveryKey);
    appEnc.rawDek = dek;
    return true;
  } catch (e) {
    return false;
  }
}

// Best-effort: clear the DEK from memory. Doesn't guarantee the bytes are gone
// (V8 may have moved copies), but it's the most we can do from JS.
function lockEncryption() {
  if (appEnc.rawDek) { try { appEnc.rawDek.fill(0); } catch {} }
  appEnc.rawDek = null;
  updateEncryptionStatusIndicator();
}

// Change password. Requires the profile to already be unlocked (rawDek in memory).
// Re-wraps the DEK with a new password KEK — files on disk don't need to change.
async function changeEncryptionPassword(newPassword) {
  if (!isEncUnlocked()) throw new Error('Profile is locked');
  await ensureCrypto();
  await window.NotePPCrypto.setPasswordOnProfile(appEnc.profile, appEnc.rawDek, newPassword);
  const res = await saveEncryptionProfile(appEnc.profile);
  if (!res.success) throw new Error('Failed to save profile: ' + (res.error || 'unknown'));
  return true;
}

// Generate a fresh recovery key, replacing the existing one. Profile must be unlocked.
async function regenerateEncryptionRecoveryKey() {
  if (!isEncUnlocked()) throw new Error('Profile is locked');
  await ensureCrypto();
  const { recoveryKey } = await window.NotePPCrypto.regenerateRecoveryKey(appEnc.profile, appEnc.rawDek);
  const res = await saveEncryptionProfile(appEnc.profile);
  if (!res.success) throw new Error('Failed to save profile: ' + (res.error || 'unknown'));
  return recoveryKey;
}

// ── Unlock prompt (modal built in Phase 4) ────────────────────────────────
// Resolves to true if profile is unlocked after the call, false if user cancelled.
// If already unlocked, resolves true immediately.
function promptUnlockDialog() {
  if (isEncUnlocked()) return Promise.resolve(true);
  return new Promise((resolve) => {
    const dlg = document.getElementById('enc-unlock-dialog');
    if (!dlg) { // Modal not present yet — fall back to a synchronous prompt.
      const pw = window.prompt('Enter your Note++ encryption password:');
      if (!pw) return resolve(false);
      unlockEncryptionWithPassword(pw).then(ok => {
        if (!ok) showToast('Wrong password');
        else updateEncryptionStatusIndicator();
        resolve(ok);
      });
      return;
    }
    // Modal-based flow — pending resolver picked up by btn-enc-unlock-ok handler.
    document.getElementById('enc-unlock-error').classList.add('hidden');
    document.getElementById('enc-unlock-pw').value = '';
    dlg.classList.remove('hidden');
    setTimeout(() => document.getElementById('enc-unlock-pw').focus(), 50);
    appEnc._unlockResolver = resolve;
  });
}

// Status-bar indicator: only shown when the ACTIVE TAB is encrypted (so users
// who haven't applied encryption to any file don't see a session-level pill).
// When the encrypted tab is unlocked → "🔓 Unlocked"; when the session is
// locked → "🔒 Locked". Click toggles the session lock.
function updateEncryptionStatusIndicator() {
  const el = document.getElementById('status-enc');
  if (!el) return;
  const tab = getActiveTab();
  if (!tab || !tab.encrypted) {
    el.classList.add('hidden');
    return;
  }
  el.classList.remove('hidden');
  el.textContent = isEncUnlocked() ? '🔓 Unlocked' : '🔒 Locked';
  const fp = tab.protectedBy || appEnc.profile?.fingerprint || '—';
  el.title = `Encrypted (profile ${fp}) — click to ${isEncUnlocked() ? 'lock' : 'unlock'}`;
}

// ── Encryption settings page (refresh + button wiring) ────────────────────
function refreshEncryptionPrefsPage() {
  const statusText = document.getElementById('enc-status-text');
  const fpLine     = document.getElementById('enc-fingerprint-line');
  const fpText     = document.getElementById('enc-fingerprint-text');
  const notSection = document.getElementById('enc-not-configured-section');
  const okSection  = document.getElementById('enc-configured-section');
  if (!statusText) return; // page not in DOM yet
  if (!isEncConfigured()) {
    statusText.textContent = 'Not configured';
    fpLine.classList.add('hidden');
    notSection.classList.remove('hidden');
    okSection.classList.add('hidden');
    return;
  }
  statusText.textContent = isEncUnlocked() ? '🔓 Unlocked' : '🔒 Locked';
  fpText.textContent = appEnc.profile.fingerprint;
  fpLine.classList.remove('hidden');
  notSection.classList.add('hidden');
  okSection.classList.remove('hidden');
  // Toggle the relevant action buttons
  document.getElementById('btn-enc-unlock').classList.toggle('hidden', isEncUnlocked());
  document.getElementById('btn-enc-lock').classList.toggle('hidden', !isEncUnlocked());
  // Change-password and regen-recovery require unlocked profile
  document.getElementById('btn-enc-change-pw').disabled       = !isEncUnlocked();
  document.getElementById('btn-enc-regen-recovery').disabled  = !isEncUnlocked();
}

function setupEncryptionPrefsPage() {
  // Setup wizard
  document.getElementById('btn-enc-setup').addEventListener('click', () => openEncSetupDialog('setup'));
  // Unlock
  document.getElementById('btn-enc-unlock').addEventListener('click', () => openEncUnlockDialog());
  // Lock now
  document.getElementById('btn-enc-lock').addEventListener('click', () => {
    lockEncryption();
    refreshEncryptionPrefsPage();
    showToast('🔒 Encryption locked');
  });
  // Change password
  document.getElementById('btn-enc-change-pw').addEventListener('click', () => openEncChangePwDialog());
  // Regenerate recovery key
  document.getElementById('btn-enc-regen-recovery').addEventListener('click', async () => {
    if (!isEncUnlocked()) { showToast('Unlock the profile first'); return; }
    const r = await window.electronAPI.messageDialog({
      type: 'warning', title: 'Generate new recovery key',
      message: 'This invalidates your current recovery key.',
      detail: 'Anyone holding the old recovery key will no longer be able to decrypt your files. The new key replaces it.',
      buttons: ['Generate new key', 'Cancel'], defaultId: 0, cancelId: 1,
    });
    if (r.response !== 0) return;
    try {
      const newKey = await regenerateEncryptionRecoveryKey();
      openEncRecoveryDisplayDialog(newKey);
    } catch (e) {
      showToast('Failed: ' + (e.message || e));
    }
  });
  // Restore from recovery key (when not configured)
  document.getElementById('link-enc-restore').addEventListener('click', (e) => {
    e.preventDefault();
    showToast('Open the encrypted file directly — Note++ will offer to set up a profile from its recovery key.');
  });
  // Reset using recovery key
  document.getElementById('link-enc-use-recovery').addEventListener('click', (e) => {
    e.preventDefault();
    openEncRecoveryEntryDialog('reset');
  });

  // ── Setup dialog wiring ───────────────────────────────────────────────────
  document.getElementById('btn-enc-setup-ok').addEventListener('click', async () => {
    const pw  = document.getElementById('enc-setup-pw').value;
    const pw2 = document.getElementById('enc-setup-pw2').value;
    const err = document.getElementById('enc-setup-error');
    err.classList.add('hidden');
    if (pw.length < 8) { err.textContent = 'Password must be at least 8 characters.'; err.classList.remove('hidden'); return; }
    if (pw !== pw2)    { err.textContent = 'Passwords do not match.'; err.classList.remove('hidden'); return; }
    try {
      const { recoveryKey } = await createEncryptionProfile(pw);
      document.getElementById('enc-setup-dialog').classList.add('hidden');
      refreshEncryptionPrefsPage();
      updateEncryptionStatusIndicator();
      openEncRecoveryDisplayDialog(recoveryKey);
    } catch (e) {
      err.textContent = 'Failed: ' + (e.message || e);
      err.classList.remove('hidden');
    }
  });

  // ── Unlock dialog wiring ──────────────────────────────────────────────────
  document.getElementById('btn-enc-unlock-ok').addEventListener('click', async () => {
    const pw  = document.getElementById('enc-unlock-pw').value;
    const err = document.getElementById('enc-unlock-error');
    err.classList.add('hidden');
    if (!pw) { err.textContent = 'Enter your password.'; err.classList.remove('hidden'); return; }
    const ok = await unlockEncryptionWithPassword(pw);
    if (!ok) { err.textContent = 'Wrong password.'; err.classList.remove('hidden'); return; }
    document.getElementById('enc-unlock-dialog').classList.add('hidden');
    refreshEncryptionPrefsPage();
    updateEncryptionStatusIndicator();
    if (appEnc._unlockResolver) { appEnc._unlockResolver(true); appEnc._unlockResolver = null; }
  });
  document.getElementById('enc-unlock-pw').addEventListener('keydown', (e) => {
    if (e.key === 'Enter') document.getElementById('btn-enc-unlock-ok').click();
  });
  // Cancel buttons resolve the unlock-promise as false
  ['btn-enc-unlock-cancel', 'btn-enc-unlock-cancel-x'].forEach(id => {
    document.getElementById(id).addEventListener('click', () => {
      if (appEnc._unlockResolver) { appEnc._unlockResolver(false); appEnc._unlockResolver = null; }
    });
  });
  // "Use recovery key instead"
  document.getElementById('link-enc-unlock-recovery').addEventListener('click', (e) => {
    e.preventDefault();
    document.getElementById('enc-unlock-dialog').classList.add('hidden');
    if (appEnc._unlockResolver) { appEnc._unlockResolver(false); appEnc._unlockResolver = null; }
    openEncRecoveryEntryDialog('unlock');
  });

  // ── Change-password dialog wiring ─────────────────────────────────────────
  document.getElementById('btn-enc-changepw-ok').addEventListener('click', async () => {
    const oldPw  = document.getElementById('enc-changepw-old').value;
    const newPw  = document.getElementById('enc-changepw-new').value;
    const newPw2 = document.getElementById('enc-changepw-new2').value;
    const err    = document.getElementById('enc-changepw-error');
    err.classList.add('hidden');
    if (newPw.length < 8) { err.textContent = 'New password must be at least 8 characters.'; err.classList.remove('hidden'); return; }
    if (newPw !== newPw2) { err.textContent = 'New passwords do not match.'; err.classList.remove('hidden'); return; }
    // Verify old password
    const verifyOk = await unlockEncryptionWithPassword(oldPw);
    if (!verifyOk) { err.textContent = 'Current password is incorrect.'; err.classList.remove('hidden'); return; }
    try {
      await changeEncryptionPassword(newPw);
      document.getElementById('enc-changepw-dialog').classList.add('hidden');
      refreshEncryptionPrefsPage();
      showToast('Password changed');
    } catch (e) {
      err.textContent = 'Failed: ' + (e.message || e);
      err.classList.remove('hidden');
    }
  });

  // ── Recovery display dialog wiring (shows recovery key once) ──────────────
  document.getElementById('enc-recovery-saved-ack').addEventListener('change', (e) => {
    document.getElementById('btn-enc-recovery-done').disabled = !e.target.checked;
  });
  document.getElementById('btn-enc-recovery-done').addEventListener('click', () => {
    document.getElementById('enc-recovery-dialog').classList.add('hidden');
  });
  document.getElementById('btn-enc-recovery-copy').addEventListener('click', async () => {
    try {
      await navigator.clipboard.writeText(document.getElementById('enc-recovery-key-display').textContent.trim());
      showToast('Recovery key copied to clipboard');
    } catch { showToast('Copy failed'); }
  });
  document.getElementById('btn-enc-recovery-download').addEventListener('click', async () => {
    const key = document.getElementById('enc-recovery-key-display').textContent.trim();
    const fingerprint = appEnc.profile?.fingerprint || '';
    const json = JSON.stringify({
      _notepp_recovery: true, version: 1, fingerprint,
      createdAt: new Date().toISOString(), recoveryKey: key,
    }, null, 2);
    const r = await window.electronAPI.saveDialog({
      defaultPath: `notepp-recovery-${fingerprint}.json`,
      filters: [{ name: 'Recovery file', extensions: ['json'] }],
    });
    if (r.canceled) return;
    const w = await window.electronAPI.writeFile(r.filePath, json);
    if (w.success) showToast('Recovery file saved');
    else showToast('Save failed: ' + w.error);
  });

  // ── Recovery-entry dialog wiring (paste recovery key to unlock or reset) ──
  document.getElementById('btn-enc-recovery-entry-ok').addEventListener('click', async () => {
    const key   = document.getElementById('enc-recovery-entry-input').value.trim();
    const err   = document.getElementById('enc-recovery-entry-error');
    const mode  = document.getElementById('enc-recovery-entry-dialog').dataset.mode || 'unlock';
    err.classList.add('hidden');
    if (!key) { err.textContent = 'Paste your recovery key.'; err.classList.remove('hidden'); return; }
    const ok = await unlockEncryptionWithRecoveryKey(key);
    if (!ok) { err.textContent = 'Recovery key is invalid or does not match the active profile.'; err.classList.remove('hidden'); return; }
    if (mode === 'reset') {
      // Also require new password
      const npw  = document.getElementById('enc-recovery-entry-newpw').value;
      const npw2 = document.getElementById('enc-recovery-entry-newpw2').value;
      if (npw.length < 8) { err.textContent = 'New password must be at least 8 characters.'; err.classList.remove('hidden'); return; }
      if (npw !== npw2)   { err.textContent = 'New passwords do not match.'; err.classList.remove('hidden'); return; }
      try {
        await changeEncryptionPassword(npw);
      } catch (e) {
        err.textContent = 'Failed: ' + (e.message || e);
        err.classList.remove('hidden');
        return;
      }
      showToast('Password reset');
    }
    document.getElementById('enc-recovery-entry-dialog').classList.add('hidden');
    refreshEncryptionPrefsPage();
    updateEncryptionStatusIndicator();
  });
}

// ── Modal openers ─────────────────────────────────────────────────────────
function openEncSetupDialog() {
  document.getElementById('enc-setup-pw').value = '';
  document.getElementById('enc-setup-pw2').value = '';
  document.getElementById('enc-setup-error').classList.add('hidden');
  document.getElementById('enc-setup-dialog').classList.remove('hidden');
  setTimeout(() => document.getElementById('enc-setup-pw').focus(), 50);
}

function openEncUnlockDialog() {
  document.getElementById('enc-unlock-pw').value = '';
  document.getElementById('enc-unlock-error').classList.add('hidden');
  document.getElementById('enc-unlock-fingerprint').textContent = appEnc.profile?.fingerprint || '—';
  document.getElementById('enc-unlock-dialog').classList.remove('hidden');
  setTimeout(() => document.getElementById('enc-unlock-pw').focus(), 50);
}

function openEncChangePwDialog() {
  if (!isEncUnlocked()) { showToast('Unlock the profile first'); return; }
  document.getElementById('enc-changepw-old').value = '';
  document.getElementById('enc-changepw-new').value = '';
  document.getElementById('enc-changepw-new2').value = '';
  document.getElementById('enc-changepw-error').classList.add('hidden');
  document.getElementById('enc-changepw-dialog').classList.remove('hidden');
  setTimeout(() => document.getElementById('enc-changepw-old').focus(), 50);
}

function openEncRecoveryDisplayDialog(recoveryKey) {
  document.getElementById('enc-recovery-key-display').textContent = recoveryKey;
  document.getElementById('enc-recovery-fingerprint').textContent = appEnc.profile?.fingerprint || '—';
  document.getElementById('enc-recovery-saved-ack').checked = false;
  document.getElementById('btn-enc-recovery-done').disabled = true;
  document.getElementById('enc-recovery-dialog').classList.remove('hidden');
}

// mode = 'unlock' (just unlock) or 'reset' (unlock + set new password)
function openEncRecoveryEntryDialog(mode) {
  const dlg = document.getElementById('enc-recovery-entry-dialog');
  dlg.dataset.mode = mode;
  document.getElementById('enc-recovery-entry-input').value = '';
  document.getElementById('enc-recovery-entry-error').classList.add('hidden');
  document.getElementById('enc-recovery-entry-newpw-block').classList.toggle('hidden', mode !== 'reset');
  if (mode === 'reset') {
    document.getElementById('enc-recovery-entry-title').textContent = '🔑 Reset password using recovery key';
    document.getElementById('enc-recovery-entry-newpw').value = '';
    document.getElementById('enc-recovery-entry-newpw2').value = '';
  } else {
    document.getElementById('enc-recovery-entry-title').textContent = '🔑 Unlock with recovery key';
  }
  dlg.classList.remove('hidden');
  setTimeout(() => document.getElementById('enc-recovery-entry-input').focus(), 50);
}

// Preferences AI page events (wired up in setupModals)
function setupAiPrefsPage() {
  document.getElementById('btn-pref-ai-check').addEventListener('click', async () => {
    const r = await window.electronAPI.aiCheck();
    setAiStatus(r.running ? 'online' : 'offline');
    refreshAiSettingsPage();
  });
  document.getElementById('btn-pref-ai-install').addEventListener('click', () => {
    window.electronAPI.openUrl('https://ollama.com/download');
  });
  document.getElementById('pref-ai-model').addEventListener('change', e => {
    aiModel = e.target.value;
    saveSetting('ai.model', aiModel);
    // Sync panel selector
    const panelSel = document.getElementById('ai-model-select');
    if (panelSel) { [...panelSel.options].forEach(o => { o.selected = o.value === aiModel; }); }
  });
  document.getElementById('pref-ai-system').addEventListener('change', e => {
    saveSetting('ai.systemPrompt', e.target.value);
  });
}

// ===== Global keyboard shortcuts =====
// These fire regardless of which tab type is active (Monaco's addCommand
// only works while the editor has focus, so whiteboard / drawio / game
// tabs would miss them without a document-level listener).
//
// Uses the CAPTURE phase so it beats Monaco's own key handling — otherwise
// Monaco would swallow Ctrl+= etc. as its own default zoom before we saw
// them. When we handle a match we stopPropagation to keep everyone else
// from double-firing.
function setupGlobalShortcuts() {
  document.addEventListener('keydown', (e) => {
    if (!e.ctrlKey && !e.metaKey) return;
    const k = e.key;

    // Reopen the most recently closed tab — Ctrl+Shift+T (matches browsers /
    // VS Code). Handled *before* the text-field guard below so it fires even
    // while the Monaco editor (a hidden <textarea>) or any input has focus —
    // reopening a closed note should work no matter where the caret is, just
    // like in a browser.
    if (e.shiftKey && !e.altKey && (k === 't' || k === 'T')) {
      e.preventDefault(); e.stopPropagation();
      reopenClosedTab();
      return;
    }

    // Skip the remaining shortcuts when the user is typing in a form field —
    // don't want to steal Ctrl+- inside a numeric input, for example.
    const t = e.target;
    if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.isContentEditable)) return;

    // Zoom in — Ctrl+= / Ctrl++ / Ctrl+NumpadAdd
    if (!e.altKey && (k === '=' || k === '+' || e.code === 'NumpadAdd')) {
      e.preventDefault(); e.stopPropagation();
      zoomIn();
      return;
    }

    // Zoom out — Ctrl+- / Ctrl+NumpadSubtract
    if (!e.altKey && !e.shiftKey && (k === '-' || e.code === 'NumpadSubtract')) {
      e.preventDefault(); e.stopPropagation();
      zoomOut();
      return;
    }

    // Ctrl+0 / Ctrl+Numpad0 → reset zoom (bonus — matches VS Code / browsers)
    if (!e.altKey && !e.shiftKey && (k === '0' || e.code === 'Numpad0')) {
      // Only claim this if there's actually a zoom to reset — Ctrl+0 is
      // used by some other flows and we don't want to steal it universally.
      if (typeof currentFontSize === 'number' && currentFontSize !== 14) {
        e.preventDefault(); e.stopPropagation();
        currentFontSize = 14;
        applyZoom();
      }
    }
  }, true /* capture */);
}

// ═════════════════════════════════════════════════════════════════════
// Feature-callout system — powers BOTH the first-run tour and one-shot
// contextual tips. Shared primitive: a translucent overlay + spotlight
// ring around a target button + popover with title/body/buttons.
//
// Persistence: markers live in settings.json (onboarding.tourSeen and
// onboarding.tips.<id>) — NOT in localStorage. localStorage is stored
// under Chromium's Local Storage dir, which can be nuked by quota
// errors, dev-tools "clear site data", or some Electron upgrade paths.
// settings.json is where every other persistent user setting lives and
// survives all of those. The in-memory `onboardingState` mirror below
// is populated at boot from readSettings() and updated in lockstep with
// every write, so hot paths (fired 100+ times per session) don't need
// to hit disk to check whether a tip has been seen.
// ═════════════════════════════════════════════════════════════════════
let _fcCleanup = null;
const onboardingState = {
  ready: false,       // becomes true once settings.json has been read
  tourSeen: false,
  tips: {},           // { [tipId]: true }
};

// One-time migration from the previous localStorage-based markers so
// existing users don't re-see the tour or tips after this upgrade.
// Runs inside loadOnboardingState() on first read.
const LEGACY_TOUR_KEY = 'notepp.tour.v1.done';
const LEGACY_TIP_PREFIX = 'notepp.tip.';

async function loadOnboardingState() {
  try {
    const s = await window.electronAPI.readSettings();
    const ob = s.onboarding || {};
    onboardingState.tourSeen = !!ob.tourSeen;
    onboardingState.tips = ob.tips || {};
    // Migrate legacy localStorage markers → settings.json (one-time).
    let migrated = false;
    try {
      if (!onboardingState.tourSeen && localStorage.getItem(LEGACY_TOUR_KEY)) {
        onboardingState.tourSeen = true; migrated = true;
      }
      // Sweep every legacy tip marker into the new shape.
      for (let i = 0; i < localStorage.length; i++) {
        const k = localStorage.key(i);
        if (!k || !k.startsWith(LEGACY_TIP_PREFIX)) continue;
        const id = k.slice(LEGACY_TIP_PREFIX.length);
        if (!onboardingState.tips[id]) { onboardingState.tips[id] = true; migrated = true; }
      }
    } catch {}
    if (migrated) await persistOnboardingState();
  } catch (err) {
    console.warn('[onboarding] failed to read settings — treating as first-run', err);
  } finally {
    onboardingState.ready = true;
  }
}

async function persistOnboardingState() {
  try {
    await saveSetting('onboarding.tourSeen', !!onboardingState.tourSeen);
    await saveSetting('onboarding.tips', onboardingState.tips || {});
  } catch (err) {
    console.warn('[onboarding] failed to persist state', err);
  }
}

function markTourSeen() {
  onboardingState.tourSeen = true;
  persistOnboardingState();
}
function markTipSeen(id) {
  onboardingState.tips = onboardingState.tips || {};
  onboardingState.tips[id] = true;
  persistOnboardingState();
}

// Show a callout. `opts` shape:
//   { targetSelector?: string, target?: Element,
//     title, body,
//     nextLabel = 'Next', onNext, onSkip, skipLabel = 'Skip',
//     step?: number, totalSteps?: number,   // shown as "3 / 8"
//     side?: 'top'|'bottom'|'left'|'right'  // auto if omitted }
// Returns a `close()` function.
function showFeatureCallout(opts) {
  hideFeatureCallout();  // only one at a time

  const overlay  = document.createElement('div');
  overlay.id     = 'feature-callout-overlay';
  const spot     = document.createElement('div');
  spot.id        = 'feature-callout-spotlight';
  const popover  = document.createElement('div');
  popover.id     = 'feature-callout-popover';
  overlay.appendChild(spot);

  const target = opts.target || (opts.targetSelector ? document.querySelector(opts.targetSelector) : null);
  if (!target) overlay.classList.add('no-target');

  // Popover content — plain HTML because content is fully controlled by us.
  const stepHtml = (opts.step && opts.totalSteps)
    ? `<span class="fc-step">Step ${opts.step} of ${opts.totalSteps}</span>`
    : '<span class="fc-step"></span>';
  const skipBtn = opts.onSkip
    ? `<button class="fc-btn" data-fc-action="skip">${escapeFcHtml(opts.skipLabel || 'Skip')}</button>`
    : '';
  popover.innerHTML = `
    <h3>${escapeFcHtml(opts.title || '')}</h3>
    <p>${escapeFcHtml(opts.body || '')}</p>
    <div class="fc-actions">
      ${stepHtml}
      <div class="fc-btn-row">
        ${skipBtn}
        <button class="fc-btn fc-btn-primary" data-fc-action="next">${escapeFcHtml(opts.nextLabel || 'Next')}</button>
      </div>
    </div>`;

  document.body.appendChild(overlay);
  document.body.appendChild(popover);

  // Position spotlight + popover relative to the target. Recompute on
  // resize / scroll because the underlying app can reflow (tab switches,
  // panel toggles, etc.) while a callout is up.
  const layout = () => {
    if (!target) {
      // Centered popover, no spotlight.
      popover.style.top  = `calc(50% - ${popover.offsetHeight / 2}px)`;
      popover.style.left = `calc(50% - ${popover.offsetWidth / 2}px)`;
      popover.removeAttribute('data-arrow');
      return;
    }
    const r = target.getBoundingClientRect();
    // Spotlight around target with a small padding
    const pad = 6;
    spot.style.top    = (r.top - pad) + 'px';
    spot.style.left   = (r.left - pad) + 'px';
    spot.style.width  = (r.width + pad * 2) + 'px';
    spot.style.height = (r.height + pad * 2) + 'px';

    // Popover side: auto — prefer below the target if there's room, else above.
    // If the target is on the top-right of the toolbar we drop the popover
    // below so it doesn't clip off the window edge.
    const pw = popover.offsetWidth || 300;
    const ph = popover.offsetHeight || 140;
    const spaceBelow = window.innerHeight - r.bottom;
    const side = opts.side || (spaceBelow > ph + 24 ? 'bottom' : 'top');
    let top, left, arrow;
    if (side === 'bottom') {
      top   = r.bottom + 14;
      left  = Math.min(Math.max(r.left - 8, 12), window.innerWidth - pw - 12);
      arrow = 'top';
    } else if (side === 'top') {
      top   = r.top - ph - 14;
      left  = Math.min(Math.max(r.left - 8, 12), window.innerWidth - pw - 12);
      arrow = 'bottom';
    } else if (side === 'right') {
      top   = Math.max(r.top - 8, 12);
      left  = r.right + 14;
      arrow = 'left';
    } else {
      top   = Math.max(r.top - 8, 12);
      left  = r.left - pw - 14;
      arrow = 'right';
    }
    popover.style.top  = top  + 'px';
    popover.style.left = left + 'px';
    popover.setAttribute('data-arrow', arrow);
  };
  layout();
  requestAnimationFrame(layout);  // re-layout once dimensions are real
  const relayout = () => requestAnimationFrame(layout);
  window.addEventListener('resize', relayout);
  window.addEventListener('scroll', relayout, true);

  const cleanup = () => {
    window.removeEventListener('resize', relayout);
    window.removeEventListener('scroll', relayout, true);
    overlay.remove();
    popover.remove();
    if (_fcCleanup === cleanup) _fcCleanup = null;
  };
  _fcCleanup = cleanup;

  popover.querySelector('[data-fc-action="next"]')?.addEventListener('click', () => {
    cleanup();
    try { opts.onNext && opts.onNext(); } catch (e) { console.warn('[callout] onNext failed', e); }
  });
  popover.querySelector('[data-fc-action="skip"]')?.addEventListener('click', () => {
    cleanup();
    try { opts.onSkip && opts.onSkip(); } catch (e) { console.warn('[callout] onSkip failed', e); }
  });
  // Click on the dimmed area = skip (or dismiss if no skip handler)
  overlay.addEventListener('click', (e) => {
    if (e.target !== overlay) return;
    cleanup();
    try { (opts.onSkip || opts.onNext || (() => {}))(); } catch {}
  });

  return cleanup;
}

function hideFeatureCallout() {
  if (_fcCleanup) _fcCleanup();
}

function escapeFcHtml(s) {
  return String(s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
}

// ── First-run tour ──────────────────────────────────────────────────────
// Fires once on the very first launch (marker in localStorage). Also
// available on-demand from ? menu → Feature Tour.
function runFeatureTour() {
  const steps = [
    {
      title: '👋 Welcome to Note++',
      body: "Note++ has a lot more under the hood than a typical text editor. Let's take 45 seconds to point out the highlights — you can skip anytime.",
    },
    {
      target: '#btn-whiteboard',
      title: '🎨 Whiteboards',
      body: 'Click here to open an Excalidraw-powered hand-drawn whiteboard tab — great for quick sketches, wireframes, and diagrams. When a whiteboard is active, an 🖼️ icon library (1264 Azure icons) appears next to this button.',
    },
    {
      target: '#btn-preview',
      title: '👁 Live preview',
      body: 'Click while editing Markdown, HTML, or Mermaid to open a resizable preview pane alongside the editor. Updates as you type.',
    },
    {
      target: '#btn-terminal',
      title: '⌨ Integrated terminal',
      body: 'Full PowerShell / bash terminal embedded in the app — proper resize, colors, no cold-start.',
    },
    {
      target: '#btn-sc',
      title: '⎇ Source Control',
      body: 'Full git panel — stage, commit, branches, push/pull. Auto-detects any repo the active file lives in.',
    },
    {
      target: '#btn-ai',
      title: '🤖 AI Assistant',
      body: 'Local, private, free AI via Ollama. One-click setup installs the daemon and downloads a model. Chat mode for discussion, Agent mode to rewrite your file with a diff preview.',
    },
    {
      title: "✅ You're all set",
      body: "That's the tour. Ctrl+Shift+P opens the command palette — everything else is one keystroke away. Re-run this tour anytime from the ? menu → Show Feature Tour.",
    },
  ];

  let i = 0;
  const total = steps.length;
  const finish = () => markTourSeen();
  const next = () => {
    if (i >= steps.length) { finish(); return; }
    const s = steps[i]; i++;
    showFeatureCallout({
      targetSelector: s.target,
      title: s.title,
      body: s.body,
      step: i,
      totalSteps: total,
      nextLabel: i === total ? 'Done' : 'Next',
      onNext: next,
      onSkip: finish,
    });
  };
  next();
}

// ── One-shot contextual tips ────────────────────────────────────────────
// Fire once per user (marker in localStorage) when triggering condition
// is met. Skips silently if the tour is currently on screen so we don't
// stack overlays.
function showTipOnce(id, opts) {
  // Skip until the settings.json marker map has loaded — better to defer
  // one tip than fire a duplicate that we'd have suppressed if we'd waited.
  if (!onboardingState.ready) return;
  if (onboardingState.tips && onboardingState.tips[id]) return;
  if (_fcCleanup) return;  // tour or another tip already up — skip this trigger
  // Also skip if the target isn't visible (e.g., button hidden on non-whiteboard tabs).
  const target = opts.targetSelector ? document.querySelector(opts.targetSelector) : null;
  if (opts.targetSelector && (!target || target.classList.contains('hidden') || target.offsetParent === null)) return;
  showFeatureCallout({
    ...opts,
    target: target || opts.target,
    nextLabel: 'Got it',
    onNext: () => markTipSeen(id),
  });
}

// Called from various hot paths whenever a triggering condition is met.
// Each tip fires at most once per user; guard checks happen inside showTipOnce.
function maybeFireContextualTip(kind, ctx) {
  ctx = ctx || {};
  if (kind === 'previewable-file') {
    // Open .md / .html — hint about the 👁 preview button
    showTipOnce('preview', {
      targetSelector: '#btn-preview',
      title: '💡 Live preview available',
      body: `You just opened a ${ctx.lang || 'preview-able'} file — click the 👁 button in the toolbar to see it rendered live alongside the editor. Toggle with Ctrl+Shift+V.`,
    });
    return;
  }
  if (kind === 'whiteboard-open') {
    // First whiteboard tab activated — hint about the icons library
    // Slight delay so the 🖼️ button has time to un-hide via updateIconsButtonVisibility.
    setTimeout(() => showTipOnce('icons', {
      targetSelector: '#btn-icons',
      title: '🖼️ 1264 Azure icons',
      body: 'Click here to open the icon library — search by name/tag and drag any tile onto the whiteboard to insert it as an image.',
    }), 300);
    return;
  }
  if (kind === 'git-repo-file') {
    showTipOnce('git', {
      targetSelector: '#btn-sc',
      title: '⎇ Full git panel',
      body: 'This file lives in a git repo. Click ⎇ (or Ctrl+Shift+G) to stage, commit, and push without leaving the editor.',
    });
    return;
  }
  if (kind === 'python-no-lsp') {
    showTipOnce('lsp-install', {
      targetSelector: '#status-lsp',
      title: '💡 One-click Pyright install',
      body: 'This Python file could get real diagnostics, hover docs, and completion. Click the LSP pill in the status bar to install Pyright globally.',
    });
    return;
  }
}

// ═════════════════════════════════════════════════════════════════════
// End feature-callout system
// ═════════════════════════════════════════════════════════════════════

// ===== Global ESC Handler =====
function setupGlobalEscape() {
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;

    // Priority order: close the topmost visible layer first
    if (!document.getElementById('theme-picker')?.classList.contains('hidden')) {
      closeThemePicker(); return;
    }
    if (!document.getElementById('quick-open').classList.contains('hidden')) {
      closeQuickOpen(); return;
    }
    if (!document.getElementById('cmd-palette').classList.contains('hidden')) {
      closeCmdPalette(); return;
    }
    if (!document.getElementById('regex-tester').classList.contains('hidden')) {
      document.getElementById('regex-tester').classList.add('hidden'); editor.focus(); return;
    }
    if (!document.getElementById('about-dialog').classList.contains('hidden')) {
      document.getElementById('about-dialog').classList.add('hidden'); editor.focus(); return;
    }
    if (!document.getElementById('goto-dialog').classList.contains('hidden')) {
      document.getElementById('goto-dialog').classList.add('hidden'); editor.focus(); return;
    }
    if (!document.getElementById('prefs-dialog').classList.contains('hidden')) {
      document.getElementById('prefs-dialog').classList.add('hidden'); editor.focus(); return;
    }
    if (previewOpen && document.activeElement && document.getElementById('preview-panel')?.contains(document.activeElement)) {
      closePreview(); return;
    }
    if (!findPanel.classList.contains('hidden')) {
      closeFindReplace(); return;
    }
    if (!contextMenu.classList.contains('hidden')) {
      contextMenu.classList.add('hidden'); return;
    }
    // Zen mode — exit last so dialogs inside zen mode still close normally first
    if (isZenMode) { toggleZenMode(); return; }
  }, true); // capture phase so it fires before Monaco consumes the key
}

async function sha256(text) {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(buf)).map(b => b.toString(16).padStart(2, '0')).join('');
}

// ===== Preview Panel =====
function isPreviewable(lang) {
  return lang === 'html' || lang === 'markdown' || lang === 'mermaid' || lang === 'json';
}

// JSONEditor instance — one shared instance, recreated when preview opens on a new tab.
let _jsonEditorInstance = null;
// Guard flag: true while we're pushing a value INTO the JSONEditor so the
// JSONEditor's onChange doesn't echo the change back into Monaco.
let _jsonEditorUpdating = false;

// Preview-pane state outside the function so other code (zoom buttons, Ctrl+wheel)
// can read/write it. previewZoom is a multiplier applied to #preview-body via
// the Chromium `zoom` CSS property (reflows content cleanly).
let previewZoom = 1.0;
let previewMaximized = false;
const PREVIEW_ZOOM_MIN = 0.5;
const PREVIEW_ZOOM_MAX = 3.0;
const PREVIEW_ZOOM_STEP = 0.1;

// True when the active preview surface is the Mermaid diagram pane — in that
// case zoom is delegated to the existing mermaid-specific transform-scale
// logic (because the SVG ignores parent CSS `zoom` due to max-width:100%).
function isMermaidPreviewActive() {
  const mmd = document.getElementById('preview-mermaid-content');
  return mmd && !mmd.classList.contains('hidden');
}

function applyPreviewZoom() {
  const body = document.getElementById('preview-body');
  if (body) body.style.zoom = previewZoom;
  updatePreviewZoomLabel();
}
function updatePreviewZoomLabel() {
  const label = document.getElementById('preview-zoom-label');
  if (!label) return;
  // When Mermaid pane is active, the zoom % shown should reflect mermaidZoom
  const z = isMermaidPreviewActive() ? mermaidZoom : previewZoom;
  label.textContent = Math.round(z * 100) + '%';
}
function previewZoomBy(delta) {
  if (isMermaidPreviewActive()) {
    // Delegate to mermaid's transform-scale zoom (works through the SVG's max-width:100%)
    delta > 0 ? mermaidZoomIn() : mermaidZoomOut();
    updatePreviewZoomLabel();
    return;
  }
  const next = Math.max(PREVIEW_ZOOM_MIN, Math.min(PREVIEW_ZOOM_MAX, previewZoom + delta));
  if (Math.abs(next - previewZoom) < 0.001) return;
  previewZoom = Math.round(next * 100) / 100;
  applyPreviewZoom();
}
function previewZoomReset() {
  if (isMermaidPreviewActive()) {
    mermaidZoomFit();
    updatePreviewZoomLabel();
    return;
  }
  previewZoom = 1.0;
  applyPreviewZoom();
}
function togglePreviewMaximize() {
  const row = document.getElementById('editor-preview-row');
  if (!row) return;
  previewMaximized = !previewMaximized;
  row.classList.toggle('preview-maximized', previewMaximized);
  const btn = document.getElementById('btn-preview-maximize');
  if (btn) {
    btn.classList.toggle('active', previewMaximized);
    btn.title = previewMaximized ? 'Restore split view' : 'Maximise preview (hide editor)';
    // Restore uses the "overlapping windows" glyph — visually distinct
    // from the close ✕ so users can tell them apart at a glance.
    // Maximise uses the corner-frame ⛶.
    btn.textContent = previewMaximized ? '❐' : '⛶';
  }
  editor?.layout();
}

// ── Preview ↔ editor scroll sync ───────────────────────────────────────────
// Two-way binding: scrolling the editor scrolls the preview to the
// matching block (found via data-source-line), and vice versa. A shared
// "suppress" flag stops the two listeners from ping-ponging each other
// after the first jump.
let previewScrollSync = true;       // user toggle (persisted)
let _syncSuppressUntil = 0;         // timestamp: ignore scroll events before this
const SYNC_SUPPRESS_MS = 120;

function _isMdPreviewActive() {
  const mdEl = document.getElementById('preview-md-content');
  return mdEl && !mdEl.classList.contains('hidden');
}

// Find the last `.md-block` whose source-line is at or before `line`.
// Binary search — the preview can have hundreds of blocks in a long doc.
function _findBlockForLine(line) {
  const blocks = document.querySelectorAll('#preview-md-content .md-block');
  if (!blocks.length) return null;
  let lo = 0, hi = blocks.length - 1, best = blocks[0];
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const bLine = +blocks[mid].dataset.sourceLine || 0;
    if (bLine <= line) { best = blocks[mid]; lo = mid + 1; }
    else hi = mid - 1;
  }
  return best;
}

// Find the first `.md-block` whose top edge is at or below the scroll
// container's top edge — i.e. the block currently at the top of the
// preview viewport.
function _findTopmostVisibleBlock() {
  const scroller = document.getElementById('preview-md-content');
  const blocks = scroller?.querySelectorAll('.md-block');
  if (!scroller || !blocks?.length) return null;
  const anchorY = scroller.getBoundingClientRect().top;
  for (const b of blocks) {
    if (b.getBoundingClientRect().bottom >= anchorY) return b;
  }
  return blocks[blocks.length - 1];
}

function setupPreviewScrollSync() {
  const scroller = document.getElementById('preview-md-content');
  if (!scroller || !editor) return;

  // Editor → preview
  editor.onDidScrollChange?.(() => {
    if (!previewScrollSync || !previewOpen) return;
    if (!_isMdPreviewActive()) return;
    if (Date.now() < _syncSuppressUntil) return;
    const vis = editor.getVisibleRanges?.()[0];
    if (!vis) return;
    const block = _findBlockForLine(vis.startLineNumber - 1);
    if (!block) return;
    _syncSuppressUntil = Date.now() + SYNC_SUPPRESS_MS;
    const anchorY = scroller.getBoundingClientRect().top;
    const blockY  = block.getBoundingClientRect().top;
    scroller.scrollTop += (blockY - anchorY);
  });

  // Preview → editor (debounced via rAF so drag-scrolling doesn't
  // fire hundreds of setPosition calls per second)
  let pending = false;
  scroller.addEventListener('scroll', () => {
    if (!previewScrollSync || !previewOpen || pending) return;
    if (!_isMdPreviewActive()) return;
    if (Date.now() < _syncSuppressUntil) return;
    pending = true;
    requestAnimationFrame(() => {
      pending = false;
      if (Date.now() < _syncSuppressUntil) return;
      const block = _findTopmostVisibleBlock();
      if (!block) return;
      const line = +block.dataset.sourceLine + 1; // Monaco is 1-indexed
      _syncSuppressUntil = Date.now() + SYNC_SUPPRESS_MS;
      editor.revealLine(line, monaco.editor.ScrollType.Immediate);
    });
  }, { passive: true });
}

function setupPreview() {
  document.getElementById('btn-preview-close').addEventListener('click', closePreview);
  document.getElementById('btn-preview-refresh').addEventListener('click', () => updatePreview());

  // Zoom controls
  document.getElementById('btn-preview-zoomin') ?.addEventListener('click', () => previewZoomBy( PREVIEW_ZOOM_STEP));
  document.getElementById('btn-preview-zoomout')?.addEventListener('click', () => previewZoomBy(-PREVIEW_ZOOM_STEP));
  document.getElementById('preview-zoom-label')?.addEventListener('click', () => previewZoomReset());

  // Maximise toggle (preview takes full editor row)
  document.getElementById('btn-preview-maximize')?.addEventListener('click', () => togglePreviewMaximize());

  // Export the rendered Markdown preview as HTML / PDF / Word
  document.getElementById('btn-preview-export')?.addEventListener('click', (e) => {
    e.stopPropagation();
    const r = e.currentTarget.getBoundingClientRect();
    showFloatingMenu(r.left, r.bottom + 2, [
      ['📄 Export as HTML',         () => exportMarkdownPreviewAs('html')],
      ['📕 Export as PDF',          () => exportMarkdownPreviewAs('pdf')],
      ['📘 Export as Word (.docx)', () => exportMarkdownPreviewAs('docx')],
    ]);
  });

  // Ctrl+wheel inside the preview body → zoom
  document.getElementById('preview-body')?.addEventListener('wheel', (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    previewZoomBy(e.deltaY < 0 ? PREVIEW_ZOOM_STEP : -PREVIEW_ZOOM_STEP);
  }, { passive: false });

  setupMermaidToolbar();
  setupPreviewScrollSync();

  // Horizontal resize handle
  const handle = document.getElementById('preview-resize-handle');
  const panel = document.getElementById('preview-panel');
  let dragging = false, startX = 0, startW = 0;

  handle.addEventListener('mousedown', (e) => {
    dragging = true;
    startX = e.clientX;
    startW = panel.offsetWidth;
    document.body.style.cursor = 'col-resize';
    document.body.style.userSelect = 'none';
  });
  document.addEventListener('mousemove', (e) => {
    if (!dragging) return;
    const delta = startX - e.clientX; // panel is on the right, drag left = wider
    const newW = Math.max(200, Math.min(startW + delta, window.innerWidth * 0.8));
    panel.style.width = newW + 'px';
  });
  document.addEventListener('mouseup', () => {
    if (!dragging) return;
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
    editor?.layout();
  });
}

function togglePreview() {
  const tab = getActiveTab();
  if (previewOpen) {
    closePreview();
  } else {
    if (!tab || !isPreviewable(tab.language)) {
      showToast('Preview available for HTML, Markdown, Mermaid (.mmd) and JSON files');
      return;
    }
    openPreview();
  }
}

function openPreview() {
  const tab = getActiveTab();
  if (!tab || !isPreviewable(tab.language)) return;

  document.getElementById('preview-panel').classList.remove('hidden');
  document.getElementById('preview-resize-handle').classList.remove('hidden');
  previewOpen = true;
  document.getElementById('btn-preview').classList.add('active');

  updatePreview();
  editor?.layout();
}

function closePreview() {
  // Drop maximised state so editor isn't still hidden next time preview opens
  if (previewMaximized) {
    previewMaximized = false;
    document.getElementById('editor-preview-row')?.classList.remove('preview-maximized');
    const btn = document.getElementById('btn-preview-maximize');
    if (btn) { btn.classList.remove('active'); btn.textContent = '⛶'; }
  }
  document.getElementById('preview-panel').classList.add('hidden');
  document.getElementById('preview-resize-handle').classList.add('hidden');
  previewOpen = false;
  document.getElementById('btn-preview').classList.remove('active');
  if (_jsonEditorInstance) { _jsonEditorInstance.destroy(); _jsonEditorInstance = null; }
  // If live view was active, reset it so it doesn't linger on next open
  if (mmdLiveViewActive) {
    mmdLiveViewActive = false;
    const lv = document.getElementById('mmd-live-view');
    if (lv) { lv.classList.add('hidden'); }
    const btn = document.getElementById('btn-mmde-live');
    if (btn) { btn.classList.remove('active'); btn.title = '🌐 Embed live preview in-app (auto-updates)'; }
  }
  editor?.layout();
  editor?.focus();
}

function schedulePreviewUpdate() {
  if (!previewOpen) return;
  clearTimeout(previewTimer);
  previewTimer = setTimeout(updatePreview, PREVIEW_DELAY);
}

function updatePreview() {
  if (!previewOpen) return;
  const tab = getActiveTab();
  if (!tab) return;

  if (!isPreviewable(tab.language)) {
    showPreviewPlaceholder();
    return;
  }

  const content = editor.getValue();
  const mdEl    = document.getElementById('preview-md-content');
  const frame   = document.getElementById('preview-html-frame');
  const mmdEl   = document.getElementById('preview-mermaid-content');
  const jsonEl  = document.getElementById('preview-json-content');

  // Hide all, then show the right one
  mdEl.classList.add('hidden');
  frame.classList.add('hidden');
  mmdEl.classList.add('hidden');
  jsonEl.classList.add('hidden');

  // Export button is only meaningful when there's rendered content to
  // save out. Show it for Markdown previews; hide for everything else.
  const exportBtn = document.getElementById('btn-preview-export');
  if (exportBtn) exportBtn.classList.toggle('hidden', tab.language !== 'markdown');

  if (tab.language === 'markdown') {
    mdEl.classList.remove('hidden');
    renderMarkdownPreview(content, mdEl);
  } else if (tab.language === 'html') {
    frame.classList.remove('hidden');
    renderHtmlPreview(content, frame, tab);
  } else if (tab.language === 'mermaid') {
    mmdEl.classList.remove('hidden');
    updateMermaidToolbar(true);
    renderMermaidPreview(content);
  } else if (tab.language === 'json') {
    jsonEl.classList.remove('hidden');
    renderJsonPreview(content);
  }
  // Refresh the shared zoom label — it shows different value for Mermaid vs others
  if (typeof updatePreviewZoomLabel === 'function') updatePreviewZoomLabel();
}

function showPreviewPlaceholder() {
  const mdEl   = document.getElementById('preview-md-content');
  const frame  = document.getElementById('preview-html-frame');
  const mmdEl  = document.getElementById('preview-mermaid-content');
  const jsonEl = document.getElementById('preview-json-content');
  frame.classList.add('hidden');
  mmdEl.classList.add('hidden');
  jsonEl.classList.add('hidden');
  updateMermaidToolbar(false);
  mdEl.classList.remove('hidden');
  mdEl.innerHTML = '<div style="text-align:center;padding:40px;opacity:0.5;font-size:13px;">No preview available for this file type.<br>Open an HTML, Markdown, Mermaid (.mmd) or JSON file to use preview.</div>';
}

// ---------------------------------------------------------------------------
// JSON path → character offset
// Walks the raw JSON text following a path array (same shape as JSONEditor's
// node.path: string keys for objects, number indices for arrays).
// Returns the character offset of the opening quote of the last key, or the
// start of the value for array elements. Returns -1 on any failure.
// ---------------------------------------------------------------------------
function _jsonPathToOffset(text, path) {
  if (!path || path.length === 0) return 0;
  let i = 0;
  const n = text.length;

  function ws() { while (i < n && (text[i] === ' ' || text[i] === '\t' || text[i] === '\r' || text[i] === '\n')) i++; }

  function readStr() {
    if (text[i] !== '"') return null;
    i++;
    let s = '';
    while (i < n && text[i] !== '"') {
      if (text[i] === '\\') {
        i++;
        const c = text[i];
        if      (c === 'n') s += '\n';
        else if (c === 't') s += '\t';
        else if (c === 'r') s += '\r';
        else if (c === 'b') s += '\b';
        else if (c === 'f') s += '\f';
        else if (c === 'u') { s += String.fromCharCode(parseInt(text.slice(i+1,i+5),16)); i+=4; }
        else s += c;
      } else { s += text[i]; }
      i++;
    }
    i++; // closing "
    return s;
  }

  function skipStr() {
    if (text[i] !== '"') return;
    i++;
    while (i < n && text[i] !== '"') { if (text[i] === '\\') i++; i++; }
    i++;
  }

  function skipVal() {
    ws();
    const c = text[i];
    if      (c === '"') skipStr();
    else if (c === '{') skipObj();
    else if (c === '[') skipArr();
    else if (c === 't') i += 4;
    else if (c === 'f') i += 5;
    else if (c === 'n') i += 4;
    else { while (i < n && /[0-9.eE+\-]/.test(text[i])) i++; }
  }

  function skipObj() {
    i++; ws();
    if (text[i] === '}') { i++; return; }
    while (i < n) {
      ws(); skipStr(); ws();
      if (text[i] === ':') i++; ws();
      skipVal(); ws();
      if (text[i] === '}') { i++; return; }
      if (text[i] === ',') i++;
    }
  }

  function skipArr() {
    i++; ws();
    if (text[i] === ']') { i++; return; }
    while (i < n) {
      ws(); skipVal(); ws();
      if (text[i] === ']') { i++; return; }
      if (text[i] === ',') i++;
    }
  }

  function follow(depth) {
    ws();
    if (depth >= path.length) return i;
    const key = path[depth];
    const isLast = depth === path.length - 1;

    if (text[i] === '{') {
      i++; ws();
      if (text[i] === '}') return -1;
      while (i < n) {
        ws();
        const keyStart = i;
        const k = readStr();
        if (k === null) return -1;
        ws();
        if (text[i] === ':') i++; ws();
        if (String(k) === String(key)) {
          return isLast ? keyStart : follow(depth + 1);
        }
        skipVal(); ws();
        if (text[i] === '}') return -1;
        if (text[i] === ',') i++;
      }
      return -1;
    }

    if (text[i] === '[') {
      i++; ws();
      if (text[i] === ']') return -1;
      const idx = Number(key);
      let cur = 0;
      while (i < n) {
        ws();
        if (cur === idx) return isLast ? i : follow(depth + 1);
        skipVal(); ws();
        if (text[i] === ']') return -1;
        if (text[i] === ',') i++;
        cur++;
      }
      return -1;
    }

    return -1;
  }

  try { return follow(0); } catch { return -1; }
}

// Decoration handle for the "flash" highlight when syncing to Monaco
let _jsonNavDecoration = [];

// Scroll Monaco editor to the position described by a JSONEditor path array.
function _syncJsonEditorToMonaco(path) {
  if (!editor || !path) return;
  const text = editor.getValue();
  const offset = _jsonPathToOffset(text, path);
  if (offset < 0) return;
  const model = editor.getModel();
  if (!model) return;
  const pos = model.getPositionAt(offset);
  editor.revealLineInCenter(pos.lineNumber, 1 /* immediate */);
  editor.setPosition(pos);
  // Flash-highlight the line for ~800 ms so the eye knows where to look
  _jsonNavDecoration = editor.deltaDecorations(_jsonNavDecoration, [{
    range: new monaco.Range(pos.lineNumber, 1, pos.lineNumber, model.getLineMaxColumn(pos.lineNumber)),
    options: { isWholeLine: true, className: 'json-nav-highlight', overviewRuler: { color: '#f0c040', position: 1 } },
  }]);
  clearTimeout(_jsonNavDecoration._timer);
  _jsonNavDecoration._timer = setTimeout(() => {
    _jsonNavDecoration = editor.deltaDecorations(_jsonNavDecoration, []);
  }, 800);
}

async function renderJsonPreview(content) {
  const container = document.getElementById('preview-json-content');
  if (!container) return;

  let parsed;
  try {
    parsed = JSON.parse(content);
  } catch (e) {
    if (_jsonEditorInstance) { _jsonEditorInstance.destroy(); _jsonEditorInstance = null; }
    // Recovery cascade. Each layer tries one more transformation and
    // hands off if its parse still fails. We surface an Auto-fix button
    // whenever ANY layer succeeds, plus a short label of what we did.
    let recoverable = null;
    let recoveryNote = '';
    try {
      const JSON5 = await ensureJson5();
      // Pass 1: JSON5 alone — handles trailing commas, single quotes,
      // comments, unquoted keys, hex/Infinity/NaN, etc.
      try {
        recoverable = JSON5.parse(content);
        recoveryNote = 'trailing commas, single quotes, comments, unquoted keys';
      } catch {}
      // Pass 2: balance missing closing braces/brackets, then JSON5.
      // Catches the common "file got truncated / user forgot a `}`" case.
      if (recoverable === null) {
        const balanced = _balanceJsonBraces(content);
        if (balanced && balanced !== content) {
          try {
            recoverable = JSON5.parse(balanced);
            recoveryNote = 'appended missing closing brace(s)/bracket(s)';
          } catch {}
        }
      }
    } catch { /* ensureJson5 failed — show error without fix offer */ }

    const errMsg  = escHtml(e.message || String(e));
    const fixHtml = recoverable !== null
      ? `<div style="margin-top:12px;padding:10px 12px;background:var(--find-input-bg);border:1px solid var(--statusbar-bg);border-radius:4px">
           <div style="font-size:12px;margin-bottom:8px">
             We found a likely fix — ${escHtml(recoveryNote)}.
           </div>
           <button id="btn-json-autofix" class="modal-btn modal-btn-primary" style="font-size:12px">✨ Auto-fix syntax</button>
         </div>`
      : '';

    container.innerHTML =
      `<div style="padding:16px;font-size:12px">
         <div style="color:var(--error-fg,#c00000);font-family:monospace;white-space:pre-wrap;font-weight:500">JSON parse error</div>
         <div style="margin-top:6px;color:var(--error-fg,#c00000);font-family:monospace;white-space:pre-wrap">${errMsg}</div>
         ${fixHtml}
       </div>`;

    if (recoverable !== null) {
      document.getElementById('btn-json-autofix')?.addEventListener('click', () => {
        applyJsonAutoFix(recoverable);
      });
    }
    return;
  }

  try {
    const JSONEditorClass = await ensureJsonEditor();

    if (!_jsonEditorInstance) {
      container.innerHTML = '';
      _jsonEditorInstance = new JSONEditorClass(container, {
        mode: 'tree',
        modes: ['tree', 'view', 'form', 'code', 'text'],
        onChange() {
          if (_jsonEditorUpdating) return;
          try {
            const val = _jsonEditorInstance.get();
            const str = JSON.stringify(val, null, 2);
            if (str !== editor.getValue()) {
              _jsonEditorUpdating = true;
              editor.setValue(str);
              _jsonEditorUpdating = false;
            }
          } catch { /* ignore — user may have invalid state mid-edit */ }
        },
        // When the user clicks or keyboards to a node in the tree, scroll
        // Monaco to the corresponding line in the source JSON.
        onEvent(node, event) {
          if (!node || !node.path) return;
          if (event.type === 'click' || (event.type === 'keydown' && (event.key === 'ArrowUp' || event.key === 'ArrowDown'))) {
            _syncJsonEditorToMonaco(node.path);
          }
        },
      });
    }

    _jsonEditorUpdating = true;
    _jsonEditorInstance.set(parsed);
    _jsonEditorUpdating = false;

  } catch (e) {
    container.innerHTML = `<div style="color:var(--error-fg,#c00000);padding:16px;font-size:12px">Failed to load JSONEditor: ${e.message}</div>`;
  }
}

// ── Markdown preview → HTML / PDF / DOCX export ─────────────────────────────
// Serialises whatever is currently rendered into #preview-md-content
// alongside a minimal print-friendly stylesheet so the exported file
// stands alone (no external font/CSS deps).
function _buildExportableHtml(tab) {
  const mdEl = document.getElementById('preview-md-content');
  const innerHtml = mdEl ? mdEl.innerHTML : '';
  const title = escHtml((tab?.name || 'document').replace(/\.[^.]+$/, ''));

  // Print-friendly inline CSS. Keep it self-contained — once exported,
  // the file has no access to Note++'s theme variables.
  const css = `
    body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
           line-height: 1.55; color: #222; max-width: 880px; margin: 32px auto; padding: 0 16px; }
    h1, h2, h3, h4, h5, h6 { color: #111; line-height: 1.25; margin-top: 1.6em; margin-bottom: 0.5em; }
    h1 { border-bottom: 1px solid #ddd; padding-bottom: 0.3em; }
    h2 { border-bottom: 1px solid #eee; padding-bottom: 0.2em; }
    p, ul, ol, blockquote, table, pre { margin: 0.6em 0; }
    code { background: #f4f4f4; padding: 1px 5px; border-radius: 3px;
           font-family: "Cascadia Code", "Fira Code", Consolas, Menlo, monospace; font-size: 0.92em; }
    pre  { background: #f6f8fa; padding: 12px 14px; border-radius: 5px; overflow-x: auto;
           font-family: "Cascadia Code", "Fira Code", Consolas, Menlo, monospace; font-size: 0.9em; line-height: 1.45; }
    pre code { background: none; padding: 0; }
    blockquote { border-left: 4px solid #ddd; padding: 0.2em 0.9em; color: #555; }
    table { border-collapse: collapse; }
    th, td { border: 1px solid #ddd; padding: 6px 10px; }
    th { background: #f4f4f4; }
    img { max-width: 100%; height: auto; }
    a { color: #0366d6; text-decoration: none; }
    a:hover { text-decoration: underline; }
    hr { border: 0; border-top: 1px solid #ddd; margin: 1.5em 0; }
    .mermaid svg { max-width: 100%; height: auto; }
  `;

  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>${title}</title>
<style>${css}</style>
</head>
<body>
${innerHtml}
</body>
</html>`;
}

async function exportMarkdownPreviewAs(format) {
  const tab = getActiveTab();
  if (!tab || tab.language !== 'markdown') {
    showToast('Open a Markdown preview first');
    return;
  }
  // Wait one frame so the preview has settled (e.g. just after a refresh)
  await new Promise(r => requestAnimationFrame(r));

  const html = _buildExportableHtml(tab);
  const baseName = (tab.name || 'document').replace(/\.[^.]+$/, '');

  const formatMeta = {
    html: { ext: 'html', name: 'HTML',      filterName: 'HTML' },
    pdf:  { ext: 'pdf',  name: 'PDF',       filterName: 'PDF' },
    docx: { ext: 'docx', name: 'Word DOCX', filterName: 'Word document' },
  }[format];
  if (!formatMeta) return;

  const r = await window.electronAPI.saveDialog({
    defaultPath: baseName + '.' + formatMeta.ext,
    filters: [{ name: formatMeta.filterName, extensions: [formatMeta.ext] }],
  });
  if (r.canceled || !r.filePath) return;

  showToast(`Exporting ${formatMeta.name}…`);

  try {
    if (format === 'html') {
      const writeRes = await window.electronAPI.writeFile(r.filePath, html);
      if (!writeRes.success) throw new Error(writeRes.error || 'write failed');
    } else if (format === 'pdf') {
      const res = await window.electronAPI.previewExport.toPdf(html);
      if (!res.success) throw new Error(res.error || 'PDF render failed');
      const writeRes = await window.electronAPI.writeFileBinary(r.filePath, res.base64);
      if (!writeRes.success) throw new Error(writeRes.error || 'write failed');
    } else if (format === 'docx') {
      const res = await window.electronAPI.previewExport.toDocx(html);
      if (!res.success) throw new Error(res.error || 'DOCX render failed');
      const writeRes = await window.electronAPI.writeFileBinary(r.filePath, res.base64);
      if (!writeRes.success) throw new Error(writeRes.error || 'write failed');
    }
    showToast(`Exported to ${r.filePath}`);
  } catch (err) {
    showToast(`${formatMeta.name} export failed: ${err.message || err}`);
  }
}

// Walk the JSON-ish content tracking open `{` `[` (skipping over strings
// and escapes) and return `content + missingClosers` if the nesting was
// left dangling at EOF. Returns `null` when nothing's missing OR when we
// detect a mismatched closer (e.g. `}` where `]` was expected) — the
// latter is too ambiguous to auto-fix without risk.
function _balanceJsonBraces(content) {
  const stack = [];
  let inString = false; // 'false' or the opening quote char ('"' or "'")
  let escape = false;
  let inLineComment = false, inBlockComment = false;
  for (let i = 0; i < content.length; i++) {
    const ch = content[i];
    const next = content[i + 1];
    if (inLineComment) { if (ch === '\n') inLineComment = false; continue; }
    if (inBlockComment) { if (ch === '*' && next === '/') { inBlockComment = false; i++; } continue; }
    if (escape) { escape = false; continue; }
    if (inString) {
      if (ch === '\\') escape = true;
      else if (ch === inString) inString = false;
      continue;
    }
    if (ch === '"' || ch === "'") { inString = ch; continue; }
    if (ch === '/' && next === '/') { inLineComment = true; i++; continue; }
    if (ch === '/' && next === '*') { inBlockComment = true; i++; continue; }
    if (ch === '{') stack.push('}');
    else if (ch === '[') stack.push(']');
    else if (ch === '}' || ch === ']') {
      if (stack.length === 0) return null;            // extra closer at root
      if (stack[stack.length - 1] !== ch) return null; // mismatched closer
      stack.pop();
    }
  }
  if (stack.length === 0) return null; // nothing to add
  // Close inside-out (last opened, first closed).
  return content + stack.reverse().join('');
}

// Replace the active editor content with a re-formatted strict-JSON
// dump of `parsed`. Wrapped in a single Monaco edit so Ctrl+Z reverts
// the fix cleanly. Re-runs the preview so the user sees the result.
function applyJsonAutoFix(parsed) {
  if (!editor) return;
  const tab = getActiveTab();
  if (!tab) return;
  // Try to preserve the source file's indentation hint — match what
  // Monaco's detected indentation says, defaulting to 2 spaces (json
  // convention) when nothing's detected.
  const model = tab.model;
  let indent = '  ';
  try {
    const opts = model?.getOptions?.();
    if (opts) indent = opts.insertSpaces ? ' '.repeat(opts.tabSize || 2) : '\t';
  } catch {}
  const fixed = JSON.stringify(parsed, null, indent);
  const fullRange = model.getFullModelRange();
  editor.executeEdits('json-autofix', [{ range: fullRange, text: fixed }]);
  editor.focus();
  showToast('JSON auto-fixed — Ctrl+Z to undo');
  schedulePreviewUpdate();
}

// Detect if content is a raw Mermaid diagram (no markdown code fences)
const MERMAID_START = /^\s*(graph|flowchart|sequenceDiagram|classDiagram|stateDiagram|erDiagram|gantt|pie|journey|gitGraph|mindmap|timeline|xychart|block|packet|architecture|requirementDiagram|C4Context|C4Container|C4Component|C4Dynamic|C4Deployment)\b/i;

function isBareRawMermaid(text) {
  return MERMAID_START.test(text.trim()) && !(/```/.test(text));
}

async function renderMarkdownPreview(content, container) {
  // Lazy-load marked on first Markdown preview render. The library is
  // small (~43 KB) but parsing it eagerly hits every cold launch.
  if (!window.marked) {
    try { await ensureMarked(); }
    catch (e) {
      container.innerHTML = '<div style="color:#c00000;padding:12px;font-size:12px">Failed to load Markdown renderer: ' + escHtml(e.message || String(e)) + '</div>';
      return;
    }
  }

  // If the whole file is a raw Mermaid diagram (no code fences), render it directly
  if (isBareRawMermaid(content)) {
    renderBareMarkdown(content, container);
    return;
  }

  // Build a one-time renderer that converts ```mermaid fences to div.mermaid
  let renderer, _code;
  try {
    renderer = new marked.Renderer();
    _code = renderer.code.bind(renderer);
    renderer.code = function(token) {
      const lang = (token.lang || '').toLowerCase().trim();
      if (lang === 'mermaid') {
        // HTML-encode so textContent is the raw diagram code mermaid reads
        const safe = token.text
          .replace(/&/g, '&amp;')
          .replace(/</g, '&lt;')
          .replace(/>/g, '&gt;');
        return `<div class="mermaid">${safe}</div>`;
      }
      return _code(token);
    };
  } catch (e) {
    container.innerHTML = `<div style="color:#c00000;padding:12px;font-size:12px">Renderer error: ${escHtml(e.message)}</div>`;
    return;
  }

  // Parse with one-time options. We use the lexer + parser split
  // instead of `marked.parse` so we can tag each top-level block with
  // its source line number for scroll-sync. Wraps each block in a
  // `<div class="md-block" data-source-line="N">` so scroll-sync can
  // map Monaco lines ↔ preview positions.
  let html;
  try {
    const tokens = marked.lexer(content);
    let lineCursor = 0;
    const parts = [];
    for (const tok of tokens) {
      const startLine = lineCursor;
      const rendered = marked.parser([tok], { renderer, gfm: true, breaks: false, pedantic: false });
      parts.push(`<div class="md-block" data-source-line="${startLine}">${rendered}</div>`);
      // Advance the cursor by the number of source lines this token
      // consumed. `raw` includes trailing whitespace / newlines when
      // marked has any (e.g. paragraph tokens end with \n\n).
      const rawLines = (tok.raw ? tok.raw.split('\n').length - 1 : 0);
      lineCursor += rawLines;
    }
    html = parts.join('');
  } catch (e) {
    container.innerHTML = `<div style="color:#c00000;padding:12px;font-family:monospace;font-size:12px">Markdown parse error: ${escHtml(e.message)}</div>`;
    return;
  }

  container.innerHTML = html;
  runMermaidInContainer(container);
}

// Render a bare Mermaid file (not wrapped in markdown fences)
function renderBareMarkdown(content, container) {
  const safe = content.trim()
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;');
  container.innerHTML = `<div class="mermaid" style="padding:16px">${safe}</div>`;
  runMermaidInContainer(container);
}

// Lazy-load helpers (ensureMermaid / ensureXterm / ensureDiff /
// _loadUmdScript) now live in src/lazy-loaders.js, loaded as a classic
// <script> tag before renderer.js. They remain global functions so the
// call sites below need no changes — they invoke ensure*() directly.

async function runMermaidInContainer(container) {
  const diagrams = container.querySelectorAll('.mermaid');
  if (diagrams.length === 0) return;
  try {
    await ensureMermaid();
    mermaid.initialize({
      startOnLoad: false,
      theme: isDarkMode ? 'dark' : 'default',
      securityLevel: 'loose',
      fontFamily: "'Segoe UI', sans-serif",
    });
    mermaid.run({ nodes: Array.from(diagrams) }).catch(err => {
      console.warn('Mermaid render error:', err);
    });
  } catch (e) {
    console.warn('Mermaid load/error:', e);
  }
}

// ===== Mermaid Live Editor =====
let mermaidZoom        = 1.0;
let mermaidRenderId    = 0;
let mermaidLastContent = '';      // tracks last successfully rendered content
let mermaidTheme       = 'default'; // current diagram theme
let mermaidSketch      = false;   // hand-drawn look toggle
let mmdLiveViewActive  = false;   // true = fullscreen live-view div showing instead of zoom/edit panel

// Guard against Mermaid bomb-SVGs leaking into document.body.
// Mermaid appends temporary render containers (e.g. #dmmd-r1, #mmd-r1) directly
// IMPORTANT: We HIDE but do NOT remove — Mermaid still needs to access these nodes
// during its async render. Removal happens in the renderMermaidPreview finally block.
// to document.body. On parse/render errors it may not clean them up, causing the
// huge "Syntax error in text / mermaid version X" bomb icons to appear on screen.
// A MutationObserver fires synchronously for each childList change so we can
// remove stale containers before they are ever painted.
(function installMermaidBodyGuard() {
  // Matches Mermaid's temporary render roots: dmmd-r1, mmd-r1, etc.
  // We intentionally do NOT schedule removal here — removing elements while
  // mermaid.render() is still running (awaiting internally) causes
  // "Cannot read properties of null (reading 'getAttribute')" errors.
  // Instead we just hide them visually; renderMermaidPreview's finally block
  // removes them after the render promise settles.
  const MMD_ID_RE = /^d?mmd-r\d/;
  const obs = new MutationObserver(mutations => {
    for (const mut of mutations) {
      for (const node of mut.addedNodes) {
        if (node.nodeType === 1 && MMD_ID_RE.test(node.id || '')) {
          // Use visibility:hidden + position:fixed (off-screen) NOT display:none.
          // display:none removes the element from layout → mermaid's coordinate
          // calculations produce NaN → "translate(undefined,NaN)" SVG errors.
          // visibility:hidden keeps the element laid out (with real dimensions)
          // while preventing it from painting on screen.
          node.style.cssText += ';position:fixed!important;top:-9999px!important;' +
            'left:-9999px!important;visibility:hidden!important;pointer-events:none!important;';
        }
      }
    }
  });
  obs.observe(document.body, { childList: true });
})();
let mermaidPanning     = false;   // pan-drag active
let mermaidPanStart    = { x: 0, y: 0, sl: 0, st: 0 }; // pan anchor

const MERMAID_TEMPLATES = {
  flowchart:
    'flowchart LR\n    A[Start] --> B{Decision?}\n    B -- Yes --> C[Do this]\n    B -- No  --> D[Do that]\n    C --> E[End]\n    D --> E',
  sequence:
    'sequenceDiagram\n    participant A as Client\n    participant B as Server\n    A->>+B: GET /api/data\n    B-->>-A: 200 OK { data }',
  class:
    'classDiagram\n    class Animal {\n        +String name\n        +makeSound() void\n    }\n    class Dog {\n        +fetch() void\n    }\n    Animal <|-- Dog',
  er:
    'erDiagram\n    CUSTOMER ||--o{ ORDER : places\n    ORDER ||--|{ LINE-ITEM : contains\n    CUSTOMER {\n        string name\n        string email\n    }',
  state:
    'stateDiagram-v2\n    [*] --> Idle\n    Idle --> Running : start\n    Running --> Idle : stop\n    Running --> [*] : finish',
  gantt:
    'gantt\n    title Project Timeline\n    dateFormat YYYY-MM-DD\n    section Planning\n        Research      :a1, 2024-01-01, 7d\n        Design        :a2, after a1, 5d\n    section Development\n        Coding        :a3, after a2, 14d\n        Testing       :a4, after a3, 7d',
  pie:
    'pie title Distribution\n    "Frontend"  : 35\n    "Backend"   : 45\n    "DevOps"    : 20',
  mindmap:
    'mindmap\n  root((Project))\n    Requirements\n      Functional\n      Non-Functional\n    Design\n      UX\n      Architecture\n    Development\n      Frontend\n      Backend',
  git:
    'gitGraph\n   commit id: "init"\n   branch develop\n   checkout develop\n   commit id: "feature-A"\n   commit id: "feature-B"\n   checkout main\n   merge develop\n   commit id: "v1.0"',
  timeline:
    'timeline\n    title Company History\n    section Founded\n        2010 : Company started\n    section Growth\n        2015 : First product\n        2018 : Series A\n    section Present\n        2024 : 100 employees',
  xychart:
    'xychart-beta\n    title "Monthly Revenue"\n    x-axis [jan, feb, mar, apr, may, jun]\n    y-axis "Revenue ($k)" 4000 --> 11000\n    bar [5000, 6000, 7500, 8200, 9100, 10200]\n    line [5000, 6000, 7500, 8200, 9100, 10200]',
  quadrant:
    'quadrantChart\n    title Prioritization Matrix\n    x-axis Low Effort --> High Effort\n    y-axis Low Impact --> High Impact\n    quadrant-1 Quick Wins\n    quadrant-2 Major Projects\n    quadrant-3 Fill Ins\n    quadrant-4 Hard Slogs\n    Task A: [0.3, 0.8]\n    Task B: [0.7, 0.7]\n    Task C: [0.2, 0.3]',
};

async function renderMermaidPreview(content) {
  // If the fullscreen live view is active, render there instead
  if (mmdLiveViewActive) { renderMmdLiveView(content); return; }

  const diagramEl  = document.getElementById('mermaid-diagram');
  const errorBox   = document.getElementById('mermaid-error-box');
  const errorText  = document.getElementById('mermaid-error-text');
  if (!diagramEl) return;

  const text = (content || '').trim();

  if (!text) {
    diagramEl.innerHTML = '<div class="mmd-empty">Start typing a Mermaid diagram…<br><span style="font-size:11px;opacity:0.6">e.g. <code>flowchart LR</code></span></div>';
    errorBox.classList.add('hidden');
    return;
  }

  // Lazy-load on first preview render. If the network/disk hiccup fails,
  // show a friendly message — but most loads succeed instantly because the
  // script is a local file://.
  try { await ensureMermaid(); }
  catch (e) {
    diagramEl.innerHTML = '<div style="padding:20px;color:#c00000">Mermaid failed to load: ' + (e?.message || 'unknown') + '</div>';
    return;
  }

  mermaid.initialize({
    startOnLoad: false,
    theme: mermaidTheme,
    look: mermaidSketch ? 'handDrawn' : 'classic',
    securityLevel: 'loose',
    fontFamily: "'Segoe UI', Tahoma, sans-serif",
    // Prevent Mermaid from rendering its own error-bomb SVGs into document.body.
    // We handle parse/render errors ourselves in the catch block below.
    suppressErrorRendering: true,
  });

  const id = `mmd-r${++mermaidRenderId}`;

  // Note: mermaid.min.js does NOT support the optional 3rd arg (svgContainingElement),
  // so we call render with two args only. The MutationObserver (above) hides any
  // temp elements Mermaid appends to document.body; the finally block removes them
  // AFTER the promise settles (removing earlier causes 'getAttribute of null' errors).
  try {
    const { svg, bindFunctions } = await mermaid.render(id, text);
    diagramEl.innerHTML = svg;
    if (typeof bindFunctions === 'function') bindFunctions(diagramEl);
    errorBox.classList.add('hidden');
    mermaidLastContent = text;
    applyMermaidZoom();
  } catch (err) {
    // Keep the last valid SVG; just surface the error in the error box
    const raw = err?.message || String(err);
    errorText.textContent = raw.replace(/<[^>]*>/g, '').slice(0, 400);
    errorBox.classList.remove('hidden');
  } finally {
    // Now that the render promise has settled, safe to sweep temp body elements.
    document.querySelectorAll('[id^="mmd-r"],[id^="dmmd-r"]').forEach(el => {
      if (el.parentElement === document.body) el.remove();
    });
  }
}

function setupMermaidToolbar() {
  // Templates
  document.getElementById('mmd-template-select').addEventListener('change', e => {
    const type = e.target.value;
    e.target.value = '';
    if (!type) return;
    insertMermaidTemplate(type);
  });

  // Theme selector
  const themeSel = document.getElementById('mmd-theme-select');
  themeSel.addEventListener('change', e => {
    mermaidTheme = e.target.value;
    if (mmdLiveViewActive) renderMmdLiveView(editor.getValue());
    else renderMermaidPreview(editor.getValue());
  });
  // Initialise theme to match app dark mode
  mermaidTheme = isDarkMode ? 'dark' : 'default';
  themeSel.value = mermaidTheme;

  // Sketch / hand-drawn toggle
  document.getElementById('btn-mmde-sketch').addEventListener('click', () => {
    mermaidSketch = !mermaidSketch;
    document.getElementById('btn-mmde-sketch').classList.toggle('active', mermaidSketch);
    renderMermaidPreview(editor.getValue());
  });

  // Download buttons
  document.getElementById('btn-mmde-svg').addEventListener('click', exportMermaidSvg);
  document.getElementById('btn-mmde-png').addEventListener('click', exportMermaidPng);

  // Copy-to-clipboard buttons
  document.getElementById('btn-mmde-copy-svg').addEventListener('click', copyMermaidSvgToClipboard);
  document.getElementById('btn-mmde-copy-png').addEventListener('click', copyMermaidPngToClipboard);

  // Toggle embedded mermaid live view (webview)
  document.getElementById('btn-mmde-live').addEventListener('click', toggleMmdLiveView);
  // Open mermaid.live /edit in the system browser
  document.getElementById('btn-mmde-open-browser').addEventListener('click', openInMermaidLive);


  // Zoom controls
  document.getElementById('btn-mmde-zoomin').addEventListener('click',  mermaidZoomIn);
  document.getElementById('btn-mmde-zoomout').addEventListener('click', mermaidZoomOut);
  document.getElementById('btn-mmde-zoomfit').addEventListener('click', mermaidZoomFit);

  const zoomArea = document.getElementById('mermaid-zoom-area');

  // Ctrl+scroll → zoom
  zoomArea.addEventListener('wheel', e => {
    if (!e.ctrlKey) return;
    e.preventDefault();
    e.deltaY < 0 ? mermaidZoomIn() : mermaidZoomOut();
  }, { passive: false });

  // Pan controls
  //  • Middle-mouse drag    → always pans (any mode)
  //  • Alt + left-drag      → always pans (legacy power-user gesture)
  //  • Plain left-drag      → pans, UNLESS visual edit mode is on (in
  //                            which case clicks need to reach the
  //                            nodes for the node-editor popup).
  // A small movement threshold ensures a plain click still registers
  // as a click (no scroll jitter from a release-without-move).
  let mermaidPanArmed = false;        // mousedown registered, waiting to see if it's a drag or a click
  const PAN_THRESHOLD = 4;            // px before we commit to panning
  zoomArea.addEventListener('mousedown', e => {
    const isMiddle = e.button === 1;
    const isAltLeft = e.button === 0 && e.altKey;
    const isPlainLeft = e.button === 0 && !e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey;
    if (!isMiddle && !isAltLeft && !isPlainLeft) return;
    // In visual edit mode plain left-clicks belong to the node editor,
    // not the pan handler. Modifier-based pans still work.
    if (isPlainLeft && zoomArea.classList.contains('mmd-edit-mode')) return;
    // Don't hijack interactions on the node-editor popup itself
    if (e.target.closest && e.target.closest('#mmd-node-editor')) return;
    mermaidPanArmed = true;
    mermaidPanning = false; // not yet — wait for movement past threshold
    mermaidPanStart = { x: e.clientX, y: e.clientY, sl: zoomArea.scrollLeft, st: zoomArea.scrollTop };
    if (isMiddle || isAltLeft) {
      // These gestures are unambiguous — engage immediately, no threshold.
      e.preventDefault();
      mermaidPanning = true;
      zoomArea.style.cursor = 'grabbing';
    }
  });
  document.addEventListener('mousemove', e => {
    if (!mermaidPanArmed && !mermaidPanning) return;
    // Promote armed→panning once the cursor has moved far enough that
    // this clearly isn't just a click.
    if (mermaidPanArmed && !mermaidPanning) {
      const dx = e.clientX - mermaidPanStart.x;
      const dy = e.clientY - mermaidPanStart.y;
      if (Math.hypot(dx, dy) < PAN_THRESHOLD) return;
      mermaidPanning = true;
      zoomArea.style.cursor = 'grabbing';
      // Prevent text-selection drift inside the SVG once panning kicks in
      e.preventDefault();
    }
    if (mermaidPanning) {
      zoomArea.scrollLeft = mermaidPanStart.sl - (e.clientX - mermaidPanStart.x);
      zoomArea.scrollTop  = mermaidPanStart.st - (e.clientY - mermaidPanStart.y);
    }
  });
  document.addEventListener('mouseup', e => {
    if (!mermaidPanArmed && !mermaidPanning) return;
    mermaidPanArmed = false;
    mermaidPanning = false;
    zoomArea.style.cursor = '';
  });
  // Re-evaluate the cursor whenever the zoom changes, the diagram
  // re-renders, or the pane is resized. The function itself lives at
  // module scope (see updateMermaidPanCursor below) so applyMermaidZoom
  // can call it directly without us monkey-patching applyMermaidZoom.
  const _resizeObs = new ResizeObserver(updateMermaidPanCursor);
  _resizeObs.observe(zoomArea);
  _resizeObs.observe(document.getElementById('mermaid-diagram'));
  updateMermaidPanCursor();

  // Wire up visual node editor
  setupMermaidVisualEditor();
}

// ── Theme sync with dark mode toggle ─────────────────────────────────────────
// Called from toggleDarkMode so the diagram re-renders with the matching theme
function syncMermaidThemeToAppMode() {
  const themeSel = document.getElementById('mmd-theme-select');
  if (!themeSel) return;
  // Only auto-sync if the user is still on default or dark (not a custom pick like forest)
  if (mermaidTheme === 'default' || mermaidTheme === 'dark') {
    mermaidTheme = isDarkMode ? 'dark' : 'default';
    themeSel.value = mermaidTheme;
    if (previewOpen) {
      if (mmdLiveViewActive) renderMmdLiveView(editor.getValue());
      else renderMermaidPreview(editor.getValue());
    }
  }
}

// ── Mermaid Visual Node Editor ───────────────────────────────────────────────
let mmdEditMode         = false;
let mmdSelectedNodeId   = null;
let mmdSelectedShape    = 'rect';

// Maps shape-key → open/close delimiters (longest first for safe parsing)
const MMD_SHAPES = {
  circle:     { open: '((',  close: '))'  },
  stadium:    { open: '([',  close: '])'  },
  hex:        { open: '{{',  close: '}}'  },
  subroutine: { open: '[[',  close: ']]'  },
  rect:       { open: '[',   close: ']'   },
  round:      { open: '(',   close: ')'   },
  diamond:    { open: '{',   close: '}'   },
};

function escapeRe(s) { return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'); }

// Parse current label + shape of a node from mermaid flowchart code
function parseMmdNodeDef(code, nodeId) {
  const eid = escapeRe(nodeId);
  const tries = [
    { re: new RegExp(`\\b${eid}\\(\\(([^)]+?)\\)\\)`),     shape: 'circle'     },
    { re: new RegExp(`\\b${eid}\\(\\[([^\\]]+?)\\]\\)`),   shape: 'stadium'    },
    { re: new RegExp(`\\b${eid}\\{\\{([^}]+?)\\}\\}`),     shape: 'hex'        },
    { re: new RegExp(`\\b${eid}\\[\\[([^\\]]+?)\\]\\]`),   shape: 'subroutine' },
    { re: new RegExp(`\\b${eid}\\[([^\\]]+?)\\]`),         shape: 'rect'       },
    { re: new RegExp(`\\b${eid}\\(([^)]+?)\\)`),           shape: 'round'      },
    { re: new RegExp(`\\b${eid}\\{([^}]+?)\\}`),           shape: 'diamond'    },
  ];
  for (const { re, shape } of tries) {
    const m = re.exec(code);
    if (m) return { label: m[1].trim(), shape };
  }
  return null;
}

// Replace a node definition in the code (longest pattern wins to avoid partial match)
function replaceMmdNodeDef(code, nodeId, newDef) {
  const eid = escapeRe(nodeId);
  const pats = [
    new RegExp(`\\b${eid}\\(\\([^)]*?\\)\\)`, 'g'),
    new RegExp(`\\b${eid}\\(\\[[^\\]]*?\\]\\)`, 'g'),
    new RegExp(`\\b${eid}\\{\\{[^}]*?\\}\\}`, 'g'),
    new RegExp(`\\b${eid}\\[\\[[^\\]]*?\\]\\]`, 'g'),
    new RegExp(`\\b${eid}\\[[^\\]]*?\\]`, 'g'),
    new RegExp(`\\b${eid}\\([^)]*?\\)`, 'g'),
    new RegExp(`\\b${eid}\\{[^}]*?\\}`, 'g'),
  ];
  for (const re of pats) {
    if (re.test(code)) return code.replace(re, newDef);
  }
  return code;
}

function showMmdNodeEditor(nodeId, nodeEl) {
  const tab = getActiveTab();
  if (!tab) return;
  const def = parseMmdNodeDef(tab.model.getValue(), nodeId);
  if (!def) return; // not a shaped node (bare ID used only in edges)

  mmdSelectedNodeId  = nodeId;
  mmdSelectedShape   = def.shape;

  // Populate popup
  document.getElementById('mmd-ne-label').value = def.label;
  document.querySelectorAll('.mmd-ne-shape').forEach(b =>
    b.classList.toggle('active', b.dataset.s === def.shape));

  // Highlight node
  document.querySelectorAll('.mmd-ve-selected').forEach(el => el.classList.remove('mmd-ve-selected'));
  nodeEl.classList.add('mmd-ve-selected');

  // Position popup below the clicked node
  const zoomArea = document.getElementById('mermaid-zoom-area');
  const zRect    = zoomArea.getBoundingClientRect();
  const nRect    = nodeEl.getBoundingClientRect();
  const popup    = document.getElementById('mmd-node-editor');

  popup.classList.remove('hidden');
  const popW = popup.offsetWidth || 220;

  let left = (nRect.left - zRect.left) + zoomArea.scrollLeft;
  let top  = (nRect.bottom - zRect.top) + zoomArea.scrollTop + 8;
  left = Math.max(4, Math.min(left, zRect.width - popW - 4));

  popup.style.left = left + 'px';
  popup.style.top  = top  + 'px';

  const inp = document.getElementById('mmd-ne-label');
  inp.focus(); inp.select();
}

function hideMmdNodeEditor() {
  document.getElementById('mmd-node-editor').classList.add('hidden');
  document.querySelectorAll('.mmd-ve-selected').forEach(el => el.classList.remove('mmd-ve-selected'));
  mmdSelectedNodeId = null;
}

function applyMmdNodeEdit() {
  if (!mmdSelectedNodeId) return;
  const tab = getActiveTab();
  if (!tab) return;
  const newLabel = document.getElementById('mmd-ne-label').value.trim();
  if (!newLabel) return;

  const d       = MMD_SHAPES[mmdSelectedShape] || MMD_SHAPES.rect;
  const newDef  = `${mmdSelectedNodeId}${d.open}${newLabel}${d.close}`;
  const newCode = replaceMmdNodeDef(tab.model.getValue(), mmdSelectedNodeId, newDef);

  if (newCode !== tab.model.getValue()) {
    tab.model.pushEditOperations([], [{ range: tab.model.getFullModelRange(), text: newCode }], () => null);
  }
  hideMmdNodeEditor();
}

function deleteMmdNode() {
  if (!mmdSelectedNodeId) return;
  const tab = getActiveTab();
  if (!tab) return;
  const eid = escapeRe(mmdSelectedNodeId);
  // Remove lines where this node is a source/standalone definition;
  // lines where it appears only as a target are also cleaned to avoid dangling refs.
  const newCode = tab.model.getValue().split('\n')
    .filter(line => !new RegExp(`\\b${eid}\\b`).test(line))
    .join('\n');
  tab.model.pushEditOperations([], [{ range: tab.model.getFullModelRange(), text: newCode }], () => null);
  hideMmdNodeEditor();
}

function setupMermaidVisualEditor() {
  const zoomArea = document.getElementById('mermaid-zoom-area');

  // Click on a diagram node
  zoomArea.addEventListener('click', e => {
    if (!mmdEditMode) return;

    // Walk up to the node <g> whose id matches the mermaid node pattern
    let el = e.target;
    let nodeGroupEl = null;
    while (el && el !== zoomArea) {
      if (el.id && /-(flowchart)-([^-]+)-\d+$/.test(el.id)) {
        nodeGroupEl = el;
        break;
      }
      el = el.parentElement;
    }

    if (!nodeGroupEl) { hideMmdNodeEditor(); return; }

    const m = nodeGroupEl.id.match(/-(flowchart)-([^-]+)-\d+$/);
    if (!m) { hideMmdNodeEditor(); return; }

    showMmdNodeEditor(m[2], nodeGroupEl);
    e.stopPropagation();
  });

  // Dismiss popup when clicking outside
  document.addEventListener('click', e => {
    const popup = document.getElementById('mmd-node-editor');
    if (popup.classList.contains('hidden')) return;
    if (!popup.contains(e.target) && !e.target.closest('#mermaid-zoom-area')) {
      hideMmdNodeEditor();
    }
  });

  // Toggle visual edit mode button
  document.getElementById('btn-mmde-edit').addEventListener('click', () => {
    mmdEditMode = !mmdEditMode;
    document.getElementById('btn-mmde-edit').classList.toggle('active', mmdEditMode);
    zoomArea.classList.toggle('mmd-edit-mode', mmdEditMode);
    if (!mmdEditMode) hideMmdNodeEditor();
    showToast(mmdEditMode ? '✏ Visual editing ON — click a node to edit' : 'Visual editing OFF');
  });

  // Popup: Apply button
  document.getElementById('btn-mmd-ne-apply').addEventListener('click', applyMmdNodeEdit);

  // Popup: Delete button
  document.getElementById('btn-mmd-ne-delete').addEventListener('click', deleteMmdNode);

  // Popup: Close button
  document.getElementById('btn-mmd-ne-close').addEventListener('click', hideMmdNodeEditor);

  // Popup: Enter = apply, Escape = close
  document.getElementById('mmd-ne-label').addEventListener('keydown', e => {
    if (e.key === 'Enter')  { e.preventDefault(); applyMmdNodeEdit(); }
    if (e.key === 'Escape') hideMmdNodeEditor();
  });

  // Popup: Shape selector
  document.querySelectorAll('.mmd-ne-shape').forEach(btn => {
    btn.addEventListener('click', () => {
      mmdSelectedShape = btn.dataset.s;
      document.querySelectorAll('.mmd-ne-shape').forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
    });
  });
}

// ── Copy to clipboard ─────────────────────────────────────────────────────────
function copyMermaidSvgToClipboard() {
  const svgEl = document.querySelector('#mermaid-diagram svg');
  if (!svgEl) { showToast('No diagram to copy'); return; }
  const clone = svgEl.cloneNode(true);
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  const text = clone.outerHTML;
  navigator.clipboard.writeText(text)
    .then(() => showToast('SVG copied to clipboard ✓'))
    .catch(() => showToast('Copy failed — clipboard permission denied'));
}

async function copyMermaidPngToClipboard() {
  const svgEl = document.querySelector('#mermaid-diagram svg');
  if (!svgEl) { showToast('No diagram to copy'); return; }

  try {
    const canvas = await mermaidSvgToCanvas(svgEl, 2);
    await new Promise((resolve, reject) => {
      canvas.toBlob(blob => {
        if (!blob) { reject(new Error('canvas.toBlob returned null')); return; }
        navigator.clipboard.write([new ClipboardItem({ 'image/png': blob })])
          .then(() => { showToast('PNG copied to clipboard ✓'); resolve(); })
          .catch(err => reject(err));
      }, 'image/png');
    });
  } catch (err) {
    showToast('PNG copy failed: ' + (err?.message || 'clipboard permission denied'));
  }
}

// ── Encode Mermaid state → pako: URL hash (used by open-in-browser ↗ button) ─────────
// Uses the browser's built-in CompressionStream (deflate-raw ≡ RFC 1951 raw deflate),
// the same algorithm as pako.deflateRaw that mermaid.live's serde.ts expects.
async function encodeMermaidState(code) {
  const state = JSON.stringify({
    code: (code || '').trim(),
    mermaid: JSON.stringify({ theme: mermaidTheme }),
    updateEditor: false,
    autoSync: true,
    updateDiagram: true,
  });
  const ds = new CompressionStream('deflate-raw');
  const w  = ds.writable.getWriter();
  w.write(new TextEncoder().encode(state));
  w.close();
  const buf = await new Response(ds.readable).arrayBuffer();
  const bytes = new Uint8Array(buf);
  let binary = '';
  bytes.forEach(b => binary += String.fromCharCode(b));
  return 'pako:' + btoa(binary);
}

// ── Open in mermaid.live (external browser, /edit route) ─────────────────────
async function openInMermaidLive() {
  const content = editor?.getValue().trim();
  if (!content) { showToast('Nothing to open'); return; }
  try {
    const hash = await encodeMermaidState(content);
    window.electronAPI.openUrl(`https://mermaid.live/edit#${hash}`);
    showToast('Opening in mermaid.live…');
  } catch {
    window.electronAPI.openUrl('https://mermaid.live');
    showToast('Opened mermaid.live (encoding unavailable)');
  }
}

// ── Mermaid Live (Fullscreen) View ────────────────────────────────────────────
// Renders the diagram into a fullscreen overlay div using the already-loaded
// global mermaid library — no webview, no IPC, no timing issues.

let mmdLiveRenderId = 0;

async function renderMmdLiveView(content) {
  if (!mmdLiveViewActive) return;

  const diagramDiv = document.getElementById('mmd-live-diagram');
  const errorDiv   = document.getElementById('mmd-live-error');
  const emptyDiv   = document.getElementById('mmd-live-empty');
  if (!diagramDiv) return;

  const text = (content ?? editor?.getValue() ?? '').trim();

  if (!text) {
    diagramDiv.innerHTML = '';
    if (errorDiv) { errorDiv.classList.add('hidden'); }
    if (emptyDiv) { emptyDiv.style.display = 'block'; }
    return;
  }
  if (emptyDiv) emptyDiv.style.display = 'none';

  try { await ensureMermaid(); }
  catch (e) {
    if (errorDiv) { errorDiv.textContent = 'Mermaid failed to load: ' + (e?.message || ''); errorDiv.classList.remove('hidden'); }
    return;
  }
  mermaid.initialize({ startOnLoad: false, theme: mermaidTheme || 'default', securityLevel: 'loose' });

  const id = 'mmlv-' + (++mmdLiveRenderId);
  try {
    const { svg, bindFunctions } = await mermaid.render(id, text);
    diagramDiv.innerHTML = svg;
    if (typeof bindFunctions === 'function') bindFunctions(diagramDiv);
    // Override Mermaid's hard-coded pixel dimensions so the SVG scales to
    // fill the panel width (CSS on #mmd-live-diagram svg handles the rest).
    const svgEl = diagramDiv.querySelector('svg');
    if (svgEl) { svgEl.style.width = '100%'; svgEl.style.height = 'auto'; }
    if (errorDiv) errorDiv.classList.add('hidden');
  } catch (err) {
    const msg = String(err?.message || err).replace(/<[^>]*>/g, '').slice(0, 600);
    if (errorDiv) { errorDiv.textContent = msg; errorDiv.classList.remove('hidden'); }
    diagramDiv.innerHTML = '';
  } finally {
    document.querySelectorAll('[id^="mmd-r"],[id^="dmmd-r"]').forEach(el => {
      if (el.parentElement === document.body) el.remove();
    });
  }
}

// Toggle the Mermaid live fullscreen view.
async function toggleMmdLiveView() {
  mmdLiveViewActive = !mmdLiveViewActive;
  const liveView = document.getElementById('mmd-live-view');
  const zoomArea = document.getElementById('mermaid-zoom-area');
  const errorBox = document.getElementById('mermaid-error-box');
  const nodeEd   = document.getElementById('mmd-node-editor');
  const btn      = document.getElementById('btn-mmde-live');

  if (mmdLiveViewActive) {
    // ── Switch TO live view ─────────────────────────────────────────────
    btn.classList.add('active');
    btn.title = '🖥 Switch back to local preview';
    zoomArea.classList.add('hidden');
    errorBox.classList.add('hidden');
    if (nodeEd) nodeEd.classList.add('hidden');
    liveView.classList.remove('hidden');
    await renderMmdLiveView(editor?.getValue() ?? '');
    showToast('🖥 Fullscreen live preview — auto-updates as you type');
  } else {
    // ── Switch BACK to local preview ─────────────────────────────────────
    btn.classList.remove('active');
    btn.title = '🌐 Embed live preview in-app (auto-updates)';
    liveView.classList.add('hidden');
    zoomArea.classList.remove('hidden');
    hideMmdNodeEditor();
    renderMermaidPreview(editor?.getValue() ?? '');
    showToast('Returned to local preview');
  }
}

function updateMermaidToolbar(show) {
  const tb = document.getElementById('mermaid-toolbar');
  if (!tb) return;
  if (show) tb.classList.remove('hidden');
  else       tb.classList.add('hidden');
}

function insertMermaidTemplate(type) {
  const text = MERMAID_TEMPLATES[type];
  if (!text) return;
  const model     = editor.getModel();
  const lastLine  = model.getLineCount();
  const lastCol   = model.getLineMaxColumn(lastLine);
  const isEmpty   = model.getValue().trim() === '';
  if (isEmpty || confirm('Replace the current diagram with this template? (Ctrl+Z to undo)')) {
    editor.executeEdits('mermaid-template', [{
      range: new monaco.Range(1, 1, lastLine, lastCol),
      text,
    }]);
    editor.focus();
    showToast(`Template inserted — Ctrl+Z to undo`);
  }
}

function applyMermaidZoom() {
  const el    = document.getElementById('mermaid-diagram');
  const label = document.getElementById('mmde-zoom-label');
  if (!el) return;
  // Drive the SVG's own width/height instead of a CSS `transform: scale`.
  // Transform-scale doesn't update the layout box, so when the diagram
  // grows past the container size no scrollbars appear and the rest of
  // the diagram is unreachable. Sizing the SVG directly makes the
  // overflow flow through #mermaid-zoom-area's `overflow: auto`.
  const svg = el.querySelector('svg');
  if (svg) {
    // Resolve natural size: prefer the viewBox (Mermaid always emits one),
    // fall back to the original width/height attributes captured on first
    // render. Cache once on the element so subsequent zoom calls don't
    // chase a width that we've already mutated.
    let naturalW = parseFloat(svg.dataset._origW || '');
    let naturalH = parseFloat(svg.dataset._origH || '');
    if (!naturalW || !naturalH) {
      const vb = svg.getAttribute('viewBox');
      if (vb) {
        const parts = vb.trim().split(/\s+/).map(Number);
        if (parts.length === 4 && parts[2] > 0 && parts[3] > 0) {
          naturalW = parts[2];
          naturalH = parts[3];
        }
      }
      if (!naturalW || !naturalH) {
        naturalW = parseFloat(svg.getAttribute('width')  || '') || svg.clientWidth  || 0;
        naturalH = parseFloat(svg.getAttribute('height') || '') || svg.clientHeight || 0;
      }
      if (naturalW && naturalH) {
        svg.dataset._origW = String(naturalW);
        svg.dataset._origH = String(naturalH);
      }
    }
    if (naturalW && naturalH) {
      svg.style.width    = (naturalW * mermaidZoom) + 'px';
      svg.style.height   = (naturalH * mermaidZoom) + 'px';
      svg.style.maxWidth = 'none'; // override the global "fit to parent" rule
    }
  }
  // Clear any leftover transform from the old approach so we don't double-scale.
  el.style.transform = '';
  el.style.transformOrigin = '';
  if (label) label.textContent = Math.round(mermaidZoom * 100) + '%';
  // Keep the shared preview-header zoom label in sync when Mermaid pane is active
  if (typeof updatePreviewZoomLabel === 'function') updatePreviewZoomLabel();
  // The diagram likely just grew/shrank past the scroll threshold, so
  // refresh the grab/default cursor hint immediately.
  updateMermaidPanCursor();
}

// Show a `grab` cursor over the mermaid zoom area whenever the diagram
// visually overflows it (i.e. there's somewhere to pan to); fall back to
// the default cursor when everything fits and in edit mode (where
// left-click is reserved for the node-editor popup).
function updateMermaidPanCursor() {
  const zoomArea = document.getElementById('mermaid-zoom-area');
  if (!zoomArea) return;
  if (zoomArea.classList.contains('mmd-edit-mode')) {
    zoomArea.style.cursor = '';
    return;
  }
  const overflows =
    zoomArea.scrollWidth  > zoomArea.clientWidth  + 1 ||
    zoomArea.scrollHeight > zoomArea.clientHeight + 1;
  zoomArea.style.cursor = overflows ? 'grab' : 'default';
}

// Zoom range: 10 % – 800 %. The previous 25 %–400 % was hit by users zooming
// in to inspect labels on large sequence/architecture diagrams. Step is
// adaptive so the same button-press feels right across the full range:
// 25 % below 1.0×, 50 % up to 4.0×, then 100 % up to 8.0×.
const MERMAID_ZOOM_MIN = 0.1;
const MERMAID_ZOOM_MAX = 8.0;
function _mermaidZoomStep(z) {
  if (z >= 4.0) return 1.0;
  if (z >= 1.0) return 0.5;
  return 0.25;
}
function mermaidZoomIn() {
  const step = _mermaidZoomStep(mermaidZoom);
  mermaidZoom = Math.min(+(mermaidZoom + step).toFixed(2), MERMAID_ZOOM_MAX);
  applyMermaidZoom();
}
function mermaidZoomOut() {
  // Use the step that would land us at this zoom level, not the next one up
  const step = _mermaidZoomStep(mermaidZoom - 0.001);
  mermaidZoom = Math.max(+(mermaidZoom - step).toFixed(2), MERMAID_ZOOM_MIN);
  applyMermaidZoom();
}
function mermaidZoomFit() { mermaidZoom = 1.0; applyMermaidZoom(); }

async function exportMermaidSvg() {
  const svgEl = document.querySelector('#mermaid-diagram svg');
  if (!svgEl) { showToast('No diagram rendered yet'); return; }

  // Ensure SVG namespace is present
  const clone = svgEl.cloneNode(true);
  clone.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  const svgText = '<?xml version="1.0" encoding="UTF-8"?>\n' + clone.outerHTML;

  const tab = getActiveTab();
  const defaultName = (tab?.name || 'diagram').replace(/\.(mmd|mermaid)$/i, '') + '.svg';

  const result = await window.electronAPI.saveDialog({
    title: 'Export as SVG',
    defaultPath: defaultName,
    filters: [{ name: 'SVG Image', extensions: ['svg'] }, { name: 'All Files', extensions: ['*'] }]
  });
  if (result.canceled || !result.filePath) return;

  const wr = await window.electronAPI.writeFile(result.filePath, svgText);
  showToast(wr.success ? 'SVG exported ✓' : 'Export failed: ' + wr.error);
}

// Shared helper: renders the Mermaid SVG to an offscreen canvas and resolves with it.
// Uses a data: URI instead of blob: to avoid the tainted-canvas SecurityError
// that Chromium throws when an SVG containing <foreignObject> is drawn via blob: URL.
function mermaidSvgToCanvas(svgEl, scale = 2) {
  return new Promise((resolve, reject) => {
    const vb = svgEl.viewBox.baseVal;
    const W  = vb.width  || svgEl.getBoundingClientRect().width  || 800;
    const H  = vb.height || svgEl.getBoundingClientRect().height || 600;

    const svgData = new XMLSerializer().serializeToString(svgEl);
    // data: URI avoids the cross-origin tainted-canvas issue caused by <foreignObject>
    const svgUrl = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svgData);

    const img = new Image();
    img.onload = () => {
      const canvas = Object.assign(document.createElement('canvas'), {
        width: W * scale, height: H * scale
      });
      const ctx = canvas.getContext('2d');
      ctx.scale(scale, scale);
      ctx.fillStyle = isDarkMode ? '#1e1e1e' : '#ffffff';
      ctx.fillRect(0, 0, W, H);
      ctx.drawImage(img, 0, 0, W, H);
      resolve(canvas);
    };
    img.onerror = () => reject(new Error('SVG image failed to load'));
    img.src = svgUrl;
  });
}

async function exportMermaidPng() {
  const svgEl = document.querySelector('#mermaid-diagram svg');
  if (!svgEl) { showToast('No diagram rendered yet'); return; }

  const tab = getActiveTab();
  const defaultName = (tab?.name || 'diagram').replace(/\.(mmd|mermaid)$/i, '') + '.png';

  const result = await window.electronAPI.saveDialog({
    title: 'Export as PNG',
    defaultPath: defaultName,
    filters: [{ name: 'PNG Image', extensions: ['png'] }, { name: 'All Files', extensions: ['*'] }]
  });
  if (result.canceled || !result.filePath) return;

  try {
    const canvas  = await mermaidSvgToCanvas(svgEl, 2);
    const dataUrl = canvas.toDataURL('image/png');
    const base64  = dataUrl.replace(/^data:image\/png;base64,/, '');
    const wr = await window.electronAPI.writeFileBinary(result.filePath, base64);
    showToast(wr.success ? 'PNG exported at 2× resolution ✓' : 'Export failed: ' + wr.error);
  } catch (err) {
    showToast('PNG export failed: ' + err.message);
  }
}

function renderHtmlPreview(content, frame, tab) {
  // Determine base path for relative assets
  const appBase = window.location.href.replace(/[^/]*$/, ''); // e.g. file:///D:/NewNotepad/
  const fileBase = tab.filePath
    ? 'file:///' + tab.filePath.replace(/\\/g, '/').replace(/\/[^/]+$/, '/')
    : appBase;

  const mermaidSrc = appBase + '../node_modules/mermaid/dist/mermaid.min.js';

  // Scripts to inject for mermaid support
  const injectHead = `<base href="${fileBase}">
<script src="${mermaidSrc}"><\/script>
<script>if(window.mermaid){mermaid.initialize({startOnLoad:true,securityLevel:'loose'});}<\/script>`;

  let html = content;

  // Inject into <head> if present, otherwise prepend a minimal head
  if (/<head[\s>]/i.test(html)) {
    html = html.replace(/(<head(?:\s[^>]*)?>)/i, `$1\n${injectHead}`);
  } else if (/<html[\s>]/i.test(html)) {
    html = html.replace(/(<html(?:\s[^>]*)?>)/i, `$1\n<head>${injectHead}</head>`);
  } else {
    html = `<!DOCTYPE html><html><head>${injectHead}</head><body>${html}</body></html>`;
  }

  frame.srcdoc = html;
}

// ── Whiteboard postMessage bridge ─────────────────────────────────────────────
window.addEventListener('message', (e) => {
  const m = e.data;
  if (!m || !m.type) return;

  // draw.io tab bridge — same shape as whiteboard, dw- prefix
  if (m.type === 'dw-ready') {
    drawioReady = true;
    if (drawioPendingContent !== null) {
      sendToDrawio({ type: 'dw-load', content: drawioPendingContent });
      drawioPendingContent = null;
    }
    sendToDrawio({ type: 'dw-theme', dark: isDarkMode });
    return;
  }
  if (m.type === 'dw-state') {
    const tab = tabs.find(t => t.type === 'drawio' && t.id === activeTabId);
    if (tab) {
      const wasClean = !tab.dirty;
      tab.content = m.content || '';
      tab.dirty = true;
      if (wasClean) { renderTabs(); updateTitle(); }
      scheduleAutoSave();
    }
    return;
  }

  if (!m.type.startsWith('wb-')) return;

  if (m.type === 'wb-ready') {
    // iframe finished initialising — send pending content + current theme
    wbReady = true;
    if (wbPendingContent) {
      sendToWhiteboard({ type: 'wb-load', content: wbPendingContent });
      wbPendingContent = null;
    }
    sendToWhiteboard({ type: 'wb-theme', dark: isDarkMode });
  }

  if (m.type === 'wb-state') {
    // Whiteboard pushes full serialised state after every committed action.
    // We mirror text-tab behaviour: mark the tab dirty (red dot in the tab
    // header), persist the content to the session file for crash recovery,
    // and leave the actual on-disk file untouched until the user explicitly
    // saves with Ctrl+S / File → Save. (Previously this auto-wrote to
    // %AppData%\notepp\Whiteboards\whiteboard-N.json every 1.5 s, which made
    // whiteboards behave differently from every other tab type.)
    const tab = tabs.find(t => t.type === 'whiteboard' && t.id === activeTabId);
    if (tab) {
      const wasClean = !tab.dirty;
      tab.content = m.content || '';
      tab.dirty = true;
      if (wasClean) { renderTabs(); updateTitle(); }
      scheduleAutoSave();       // session file only — survives crashes
    }
  }

  if (m.type === 'wb-save-request') {
    // Whiteboard requested an explicit save (Ctrl+S inside iframe)
    const tab = tabs.find(t => t.type === 'whiteboard' && t.id === activeTabId);
    if (tab) saveTabFile(tab);
  }

});
