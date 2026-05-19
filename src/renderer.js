'use strict';

// ===== State =====
const tabs = [];
let activeTabId = null;
let editor = null;
let isDarkMode = false;
let isWordWrap = false;
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
const contextMenu  = document.getElementById('context-menu');

// ===== Monaco Loader =====
require.config({ paths: { vs: '../node_modules/monaco-editor/min/vs' } });

require(['vs/editor/editor.main'], () => {
  // Define a custom Note++ dark theme
  monaco.editor.defineTheme('notepp-dark', {
    base: 'vs-dark',
    inherit: true,
    rules: [
      { token: 'comment', foreground: '6A9955', fontStyle: 'italic' },
      { token: 'keyword', foreground: '569CD6', fontStyle: 'bold' },
      { token: 'string', foreground: 'CE9178' },
      { token: 'number', foreground: 'B5CEA8' },
      { token: 'type', foreground: '4EC9B0' },
      { token: 'function', foreground: 'DCDCAA' },
      { token: 'variable', foreground: '9CDCFE' },
    ],
    colors: {
      'editor.background': '#1E1E1E',
      'editor.lineHighlightBackground': '#2A2D2E',
      'editorLineNumber.foreground': '#858585',
      'editorLineNumber.activeForeground': '#C6C6C6',
      'editor.selectionBackground': '#264F78',
      'editor.findMatchBackground': '#515C6A',
      'editor.findMatchHighlightBackground': '#EA5C0055',
    }
  });

  monaco.editor.defineTheme('notepp-light', {
    base: 'vs',
    inherit: true,
    rules: [],
    colors: {
      'editor.background': '#FFFFFE',
      'editor.lineHighlightBackground': '#F0F7FF',
    }
  });

  editor = monaco.editor.create(document.getElementById('monaco-editor'), {
    value: '',
    language: 'plaintext',
    theme: 'notepp-light',
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
    minimap: { enabled: true, scale: 1, showSlider: 'mouseover' },
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
    bracketPairColorization: { enabled: true, independentColorPoolPerBracketType: true },
    guides: { bracketPairs: 'active', indentation: true },
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

  // Expose for lsp-client.js (finds tab by Monaco model + reads activeGitRepo)
  window.tabs = tabs;
  window.NotePPLsp?.attachEditor(editor);

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
    // Pre-populate terminal pref elements so they're ready when the terminal opens
    const t = s.terminal || {};
    const shellEl    = document.getElementById('pref-shell');
    const fontSizeEl = document.getElementById('pref-term-fontsize');
    if (shellEl    && t.shell)    shellEl.value    = t.shell;
    if (fontSizeEl && t.fontSize) fontSizeEl.value = t.fontSize;

    // ── Persistent UI toggles: dark mode, word wrap, editor zoom ─────────
    const ui = s.ui || {};
    if (ui.darkMode === true  && !isDarkMode)  toggleDarkMode();
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
      editor.updateOptions({ minimap: { enabled: ui.minimap } });
    }
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
        case 'none':
        case 'checking':
        default:
          // Don't show the pill when there's nothing actionable.
          pill.classList.add('hidden');
          break;
      }
    });
  })();

  // Load encryption profile (if previously configured). Doesn't unlock — just
  // detects "is this install set up?". Unlocking happens on demand.
  loadEncryptionProfile().then(() => updateEncryptionStatusIndicator());

  // Initialise git integration (detects `git` on PATH, attaches focus + fetch timers)
  setupSourceControlPanel();
  initGitIntegration();
  restoreSession().then(restored => { if (!restored) createTab(); });

  // Events
  editor.onDidChangeCursorPosition(updateStatusBar);
  editor.onDidChangeCursorSelection(updateStatusBar);
  editor.onDidChangeModelContent(() => {
    const tab = getActiveTab();
    if (tab && !tab.dirty) { tab.dirty = true; renderTabs(); }
    updateStatusBar();
    updateTitle();
    scheduleAutoSave();
    schedulePreviewUpdate();
    scheduleGitDiffUpdate(tab);   // re-paint inline git-diff gutter
    try { window.NotePPLsp?.onTabContentChange(tab); } catch {}
  });

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

  setupMenuListeners();
  setupExternalChangeWatcher();
  setupContextMenu();
  setupDragDrop();
  setupFindReplace();
  setupModals();
  setupToolbar();
  setupTerminal();
  setupTerminalResize();
  setupFileTreeResize();
  setupQuickOpen();
  setupCmdPalette();
  setupRegexTester();
  setupPreview();
  setupAiPanel();
  setupDiffToolbars();
  setupGlobalEscape();
  updateStatusBar();

  // Signal main that all our IPC listeners (especially 'open-files') are now
  // wired up. Main will flush any files queued from double-click / "Open with".
  try { window.electronAPI.rendererReady(); } catch {}
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
function nextWbTabNumber() {
  const used = new Set(
    tabs
      .filter(t => t.type === 'whiteboard')
      .map(t => {
        const m = t.name.match(/^whiteboard-(\d+)(?:\.json)?$/);
        return m ? parseInt(m[1], 10) : null;
      })
      .filter(n => n !== null)
  );
  for (let n = 1; ; n++) if (!used.has(n)) return n;
}

function sendToWhiteboard(msg) {
  const frame = document.getElementById('whiteboard-frame');
  if (frame && frame.contentWindow) {
    frame.contentWindow.postMessage(msg, '*');
  }
}

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
function nextDrawioTabNumber() {
  const used = new Set(
    tabs
      .filter(t => t.type === 'drawio')
      .map(t => { const mm = t.name.match(/^drawing-(\d+)(?:\.drawio)?$/); return mm ? parseInt(mm[1], 10) : null; })
      .filter(n => n !== null)
  );
  for (let n = 1; ; n++) if (!used.has(n)) return n;
}

function sendToDrawio(msg) {
  // The renderer-side iframe is #drawio-frame-host; #drawio-frame is the
  // INNER iframe that exists only inside drawio.html. Targeting the wrong
  // one made every dw-load after the first one a silent no-op — so the
  // shared drawio iframe kept displaying tab #1's diagram regardless of
  // which tab the user activated.
  const frame = document.getElementById('drawio-frame-host');
  if (frame && frame.contentWindow) frame.contentWindow.postMessage(msg, '*');
}

// Show a modal that downloads drawio. Returns true on success, false on
// cancel / network failure. Resolves only after the download is fully done
// (or the user dismissed it).
async function showDrawioDownloadModal() {
  // Lazy-create the modal markup on first use
  let modal = document.getElementById('drawio-download-modal');
  if (!modal) {
    modal = document.createElement('div');
    modal.id = 'drawio-download-modal';
    modal.className = 'modal-overlay hidden';
    modal.innerHTML = `
      <div class="modal-box" style="max-width:460px">
        <div class="modal-header">
          <span>Download draw.io</span>
        </div>
        <div class="modal-body" style="padding:24px;text-align:center">
          <div style="font-size:13px;color:var(--fg-secondary);margin-bottom:14px">
            draw.io isn't bundled with Note++. We'll download it once
            (≈40 MB) and reuse it across upgrades.
          </div>
          <div id="dw-dl-phase" style="font-size:13px;font-weight:600;margin-bottom:8px">Starting…</div>
          <div style="width:100%;height:6px;background:#374151;border-radius:3px;overflow:hidden">
            <div id="dw-dl-bar" style="width:0%;height:100%;background:linear-gradient(90deg,#3b82f6,#60a5fa);transition:width 0.15s linear"></div>
          </div>
          <div id="dw-dl-detail" style="font-size:11px;color:var(--fg-secondary);margin-top:6px">&nbsp;</div>
          <div id="dw-dl-error" style="font-size:12px;color:#dc2626;margin-top:10px;display:none"></div>
        </div>
        <div class="modal-footer">
          <button class="modal-btn" id="dw-dl-cancel">Cancel</button>
          <button class="modal-btn primary hidden" id="dw-dl-retry">Retry</button>
        </div>
      </div>`;
    document.body.appendChild(modal);
  }

  const phaseEl  = modal.querySelector('#dw-dl-phase');
  const barEl    = modal.querySelector('#dw-dl-bar');
  const detailEl = modal.querySelector('#dw-dl-detail');
  const errEl    = modal.querySelector('#dw-dl-error');
  const cancelEl = modal.querySelector('#dw-dl-cancel');
  const retryEl  = modal.querySelector('#dw-dl-retry');

  modal.classList.remove('hidden');
  errEl.style.display = 'none';
  retryEl.classList.add('hidden');
  barEl.style.width = '0%';
  detailEl.textContent = '';

  return new Promise((resolve) => {
    let settled = false;
    function finish(result) {
      if (settled) return;
      settled = true;
      window.electronAPI.drawio.removeProgressListener();
      modal.classList.add('hidden');
      resolve(result);
    }

    cancelEl.onclick = () => finish(false);

    window.electronAPI.drawio.onProgress((p) => {
      if (p.phase === 'download') {
        phaseEl.textContent = 'Downloading draw.io…';
        barEl.style.width = (p.percent || 0) + '%';
        if (p.totalBytes) {
          const mb = (b) => (b / (1024 * 1024)).toFixed(1);
          detailEl.textContent = `${mb(p.bytes)} / ${mb(p.totalBytes)} MB`;
        }
      } else if (p.phase === 'extract') {
        phaseEl.textContent = 'Extracting…'; barEl.style.width = '95%'; detailEl.textContent = '';
      } else if (p.phase === 'install') {
        phaseEl.textContent = 'Installing…'; barEl.style.width = '98%';
      } else if (p.phase === 'done') {
        phaseEl.textContent = `draw.io ${p.version} installed`; barEl.style.width = '100%';
      }
    });

    async function attempt() {
      errEl.style.display = 'none';
      retryEl.classList.add('hidden');
      const res = await window.electronAPI.drawio.download();
      if (res?.success) {
        // Brief pause so the user sees "installed" before the modal closes
        setTimeout(() => finish(true), 700);
      } else {
        errEl.textContent = 'Download failed: ' + (res?.error || 'unknown error');
        errEl.style.display = 'block';
        retryEl.classList.remove('hidden');
        retryEl.onclick = attempt;
      }
    }
    attempt();
  });
}

// Ensures the drawio bundle is installed. Returns true if available, false
// if the user cancelled or the download failed.
async function ensureDrawioInstalled() {
  const s = await window.electronAPI.drawio.status();
  if (s.installed && !s.needsUpdate) return true;
  return await showDrawioDownloadModal();
}

// Manual update check — invoked from "? → Check for draw.io updates"
async function checkDrawioForUpdates() {
  const s = await window.electronAPI.drawio.status();
  if (!s.installed) {
    showToast('draw.io isn\'t installed yet — open a draw.io tab to download it.');
    return;
  }
  if (s.installedVersion === s.requestedVersion) {
    showToast(`draw.io is up to date (v${s.installedVersion}).`);
    return;
  }
  const r = await window.electronAPI.messageDialog({
    type: 'question', title: 'Update draw.io',
    message: `draw.io update available: v${s.installedVersion} → v${s.requestedVersion}. Download now?`,
    buttons: ['Update', 'Later'], defaultId: 0, cancelId: 1,
  });
  if (r.response === 0) await showDrawioDownloadModal();
}

// ── Starter templates for "Tools → New Diagram from Template" ──────────────
// Each value is a minimal but well-formed mxfile XML so drawio renders the
// scaffold and the user can build on it. Kept intentionally small (≤ ~8
// shapes per template) — the goal is a head-start, not a finished diagram.
const DRAWIO_TEMPLATES = {
  flowchart:
    '<mxfile host="notepp" version="1"><diagram id="flow" name="Flowchart">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="s" value="Start" style="ellipse;whiteSpace=wrap;html=1;fillColor=#d5e8d4;strokeColor=#82b366;" vertex="1" parent="1"><mxGeometry x="360" y="40" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="p" value="Process step" style="rounded=0;whiteSpace=wrap;html=1;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="360" y="140" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="d" value="Decision?" style="rhombus;whiteSpace=wrap;html=1;fillColor=#fff2cc;strokeColor=#d6b656;" vertex="1" parent="1"><mxGeometry x="360" y="240" width="120" height="80" as="geometry"/></mxCell>' +
          '<mxCell id="e" value="End" style="ellipse;whiteSpace=wrap;html=1;fillColor=#f8cecc;strokeColor=#b85450;" vertex="1" parent="1"><mxGeometry x="360" y="380" width="120" height="50" as="geometry"/></mxCell>' +
          '<mxCell id="e1" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="s" target="p"><mxGeometry relative="1" as="geometry"/></mxCell>' +
          '<mxCell id="e2" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="p" target="d"><mxGeometry relative="1" as="geometry"/></mxCell>' +
          '<mxCell id="e3" value="Yes" style="edgeStyle=orthogonalEdgeStyle;rounded=0;html=1;exitX=0.5;exitY=1;entryX=0.5;entryY=0;" edge="1" parent="1" source="d" target="e"><mxGeometry relative="1" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  sequence:
    '<mxfile host="notepp" version="1"><diagram id="seq" name="Sequence">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="a" value="Actor A" style="shape=umlLifeline;perimeter=lifelinePerimeter;whiteSpace=wrap;html=1;container=1;collapsible=0;recursiveResize=0;outlineConnect=0;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="120" y="40" width="120" height="360" as="geometry"/></mxCell>' +
          '<mxCell id="b" value="Actor B" style="shape=umlLifeline;perimeter=lifelinePerimeter;whiteSpace=wrap;html=1;container=1;collapsible=0;recursiveResize=0;outlineConnect=0;fillColor=#dae8fc;strokeColor=#6c8ebf;" vertex="1" parent="1"><mxGeometry x="400" y="40" width="120" height="360" as="geometry"/></mxCell>' +
          '<mxCell id="m1" value="request()" style="html=1;verticalAlign=bottom;startSize=8;endSize=8;" edge="1" parent="1" source="a" target="b"><mxGeometry relative="1" as="geometry"><mxPoint x="180" y="140" as="sourcePoint"/><mxPoint x="460" y="140" as="targetPoint"/></mxGeometry></mxCell>' +
          '<mxCell id="m2" value="response" style="html=1;verticalAlign=bottom;startSize=8;endSize=8;endArrow=open;dashed=1;" edge="1" parent="1" source="b" target="a"><mxGeometry relative="1" as="geometry"><mxPoint x="460" y="240" as="sourcePoint"/><mxPoint x="180" y="240" as="targetPoint"/></mxGeometry></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  classDiagram:
    '<mxfile host="notepp" version="1"><diagram id="cls" name="Class">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          '<mxCell id="c" value="ClassName" style="swimlane;fontStyle=1;align=center;verticalAlign=top;childLayout=stackLayout;horizontal=1;startSize=26;horizontalStack=0;resizeParent=1;resizeParentMax=0;collapsible=0;marginBottom=0;fillColor=#dae8fc;strokeColor=#6c8ebf;swimlaneFillColor=#ffffff;" vertex="1" parent="1"><mxGeometry x="200" y="80" width="240" height="170" as="geometry"/></mxCell>' +
          '<mxCell id="a1" value="+ name: String" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="26" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="a2" value="+ age: int" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="48" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="sep" value="" style="line;strokeWidth=1;fillColor=none;align=left;verticalAlign=middle;spacingTop=-1;spacingLeft=3;spacingRight=3;rotatable=0;labelPosition=right;points=[];portConstraint=eastwest;" vertex="1" parent="c"><mxGeometry y="70" width="240" height="8" as="geometry"/></mxCell>' +
          '<mxCell id="m1" value="+ getName(): String" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="78" width="240" height="22" as="geometry"/></mxCell>' +
          '<mxCell id="m2" value="+ setAge(age: int): void" style="text;strokeColor=none;fillColor=none;align=left;verticalAlign=middle;spacingLeft=4;spacingRight=4;overflow=hidden;rotatable=0;points=[[0,0.5],[1,0.5]];portConstraint=eastwest;fontSize=12;" vertex="1" parent="c"><mxGeometry y="100" width="240" height="22" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',

  erDiagram:
    '<mxfile host="notepp" version="1"><diagram id="er" name="ER">' +
      '<mxGraphModel dx="900" dy="700" grid="1" gridSize="10" guides="1" tooltips="1" connect="1" arrows="1" fold="1" page="1" pageScale="1" pageWidth="850" pageHeight="1100" math="0" shadow="0">' +
        '<root>' +
          '<mxCell id="0"/><mxCell id="1" parent="0"/>' +
          // Customer entity (rectangle with header + rows so it looks like an ER table)
          '<mxCell id="ec" value="Customer" style="rounded=0;whiteSpace=wrap;html=1;verticalAlign=top;fontStyle=1;fillColor=#dae8fc;strokeColor=#6c8ebf;align=center;" vertex="1" parent="1"><mxGeometry x="80" y="120" width="200" height="120" as="geometry"/></mxCell>' +
          '<mxCell id="ec1" value="🔑 id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="30" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="ec2" value="name: string" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="58" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="ec3" value="email: string" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="ec"><mxGeometry y="86" width="200" height="24" as="geometry"/></mxCell>' +
          // Order entity
          '<mxCell id="eo" value="Order" style="rounded=0;whiteSpace=wrap;html=1;verticalAlign=top;fontStyle=1;fillColor=#d5e8d4;strokeColor=#82b366;align=center;" vertex="1" parent="1"><mxGeometry x="440" y="120" width="200" height="120" as="geometry"/></mxCell>' +
          '<mxCell id="eo1" value="🔑 id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="30" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="eo2" value="🔗 customer_id: int" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="58" width="200" height="24" as="geometry"/></mxCell>' +
          '<mxCell id="eo3" value="total: decimal" style="text;align=left;verticalAlign=middle;spacingLeft=8;html=1;" vertex="1" parent="eo"><mxGeometry y="86" width="200" height="24" as="geometry"/></mxCell>' +
          // Relationship with cardinality labels
          '<mxCell id="rel" value="places" style="edgeStyle=entityRelationEdgeStyle;fontSize=12;html=1;endArrow=ERmany;startArrow=ERone;rounded=0;exitX=1;exitY=0.5;entryX=0;entryY=0.5;" edge="1" parent="1" source="ec" target="eo"><mxGeometry relative="1" as="geometry"/></mxCell>' +
        '</root>' +
      '</mxGraphModel>' +
    '</diagram></mxfile>',
};

function createDrawioTabFromTemplate(name) {
  const xml = DRAWIO_TEMPLATES[name];
  if (!xml) { showToast('Unknown template: ' + name); return; }
  return createDrawioTab(null, xml);
}

function createDrawioTab(filePath, content) {
  if (filePath) {
    const existing = tabs.find(t => t.filePath === filePath && t.type === 'drawio');
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  const name = filePath
    ? filePath.split(/[\\/]/).pop()
    : `drawing-${nextDrawioTabNumber()}`;
  const tab = {
    id, name,
    filePath: filePath || null,
    content: content || '',
    dirty: false,
    language: 'drawio',
    encoding: 'UTF-8',
    eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'drawio',
  };
  tabs.push(tab);
  // Activate after install check so we don't show a blank iframe.
  ensureDrawioInstalled().then(ok => {
    if (!ok) {
      // User cancelled or download failed — close the placeholder tab.
      const idx = tabs.findIndex(t => t.id === id);
      if (idx >= 0) tabs.splice(idx, 1);
      renderTabs();
      // Re-activate something useful
      if (tabs.length === 0) createTab();
      else activateTab(tabs[tabs.length - 1].id);
      return;
    }
    activateTab(id);
    renderTabs();
  });
  return tab;
}

// =============================================================================
// Compare (File diff + Folder diff) — see features/DIFF.md
// =============================================================================

// One Monaco DiffEditor instance reused across diff tabs. Tabs swap their
// (originalModel, modifiedModel) into it on activation.
let diffEditor = null;
let diffEditorSideBySide = true;

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

// Tiny HTML escape — shared with other helpers but defined here too to keep
// the diff module self-contained.
function escapeHtml(s) {
  return String(s == null ? '' : s).replace(/[&<>"']/g, c =>
    ({ '&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&#39;' }[c]));
}

function createWhiteboardTab(filePath, content) {
  // If a whiteboard for this filePath already exists, just activate it
  if (filePath) {
    const existing = tabs.find(t => t.filePath === filePath && t.type === 'whiteboard');
    if (existing) { activateTab(existing.id); return existing; }
  }
  tabCounter++;
  const id = tabCounter;
  // Match text-tab UX: new (unsaved) whiteboards get a "whiteboard-N" display
  // name (no .json suffix until the user picks a real save location), so the
  // tab reads cleanly as "untitled" rather than as a real file on disk.
  const name = filePath
    ? filePath.split(/[\\/]/).pop()
    : `whiteboard-${nextWbTabNumber()}`;
  const tab = {
    id, name,
    filePath: filePath || null,
    content: content || '',
    // New whiteboards mirror "new N" text tabs: not dirty yet (nothing drawn),
    // but the absence of filePath means Ctrl+S → Save As prompt.
    dirty: false,
    language: 'whiteboard',
    encoding: 'UTF-8',
    eol: 'Windows (CR LF)',
    model: null, viewState: null,
    type: 'whiteboard'
  };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function activateTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  const prev = getActiveTab();
  if (prev && editor && prev.type !== 'game' && prev.type !== 'whiteboard'
                     && prev.type !== 'drawio'
                     && prev.type !== 'diff' && prev.type !== 'folder-diff') {
    prev.viewState = editor.saveViewState();
    prev.content = editor.getValue();
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

  // Always hide all special containers first, then show the right one
  gameContainer.classList.add('hidden');
  wbContainer.classList.add('hidden');
  dwContainer?.classList.add('hidden');
  diffContainer?.classList.add('hidden');
  fdiffContainer?.classList.add('hidden');

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
  } else {
    // Show editor, hide special containers
    monacoEl.style.display = '';
    if (editor) {
      editor.setModel(tab.model);
      if (tab.viewState) editor.restoreViewState(tab.viewState);
      editor.focus();
    }
    // Auto-open preview for Mermaid files; update if already open
    if (tab.language === 'mermaid' && !previewOpen) {
      openPreview();
    } else if (previewOpen) {
      if (isPreviewable(tab.language)) {
        updatePreview();
      } else {
        showPreviewPlaceholder();
      }
    }
    // Show/hide Mermaid toolbar based on active language
    updateMermaidToolbar(tab.language === 'mermaid');
  }

  renderTabs();
  updateStatusBar();
  updateTitle();
  updateLanguageStatus();
  updateEncryptToolbarButton();
  updateEncryptionStatusIndicator();
  updateActiveGitRepo();        // Git status follows the active tab's repo
  // LSP — start (or sync) the language server for this tab's language
  try { window.NotePPLsp?.onTabActivated(tab); } catch (e) { console.warn('[lsp]', e); }
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

function renderTabs() {
  tabBar.innerHTML = '';
  tabs.forEach(tab => {
    const el = document.createElement('div');
    el.className = 'tab' + (tab.id === activeTabId ? ' active' : '') + (tab.dirty ? ' dirty' : '');
    el.dataset.id = tab.id;

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
    el.addEventListener('mousedown', (e) => { if (e.button === 1) { e.preventDefault(); closeTab(tab.id); } });
    el.addEventListener('contextmenu', (e) => { e.preventDefault(); showTabContextMenu(e, tab.id); });
    tabBar.appendChild(el);
  });
}

async function closeTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  if (tab.type === 'diff' || tab.type === 'folder-diff') {
    // Compare tab: no model on the tab, no save prompt
    if (tab.type === 'diff') disposeDiffTab(tab);
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
    ['Close', () => closeTab(tabId)],
    ['Close All', () => closeAllTabs()],
    ['Close All But This', () => closeOtherTabs(tabId)],
    null,
    ['Copy Full Path', () => { if (tab.filePath) navigator.clipboard.writeText(tab.filePath); }],
    ['Open Containing Folder', () => { if (tab.filePath) window.electronAPI.shellShowItem(tab.filePath); }],
    null,
  ];
  // Diff entry points — only for saved editor tabs
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
  items.push(['Reveal in File Tree', () => {}]);
  showFloatingMenu(e.clientX, e.clientY, items);
}

function showFloatingMenu(x, y, items) {
  const menu = document.createElement('div');
  menu.style.cssText = `position:fixed;left:${x}px;top:${y}px;z-index:2000;background:var(--ctx-bg);border:1px solid var(--ctx-border);padding:2px 0;min-width:200px;box-shadow:2px 2px 8px rgba(0,0,0,0.25);font-size:12px;max-height:80vh;overflow-y:auto;`;
  items.forEach(item => {
    if (!item) { const sep = document.createElement('div'); sep.className = 'ctx-sep'; menu.appendChild(sep); return; }
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

async function openFile(filePaths) {
  if (!filePaths) {
    const r = await window.electronAPI.openDialog({
      properties: ['openFile', 'multiSelections'],
      // "All Files" is the default so .txt, .json, .xml, .ini and everything
      // else is visible the moment the dialog opens. The named category
      // filters below let the user narrow when they want to.
      filters: [
        { name: 'All Files', extensions: ['*'] },
        { name: 'Text', extensions: ['txt','md','markdown','log','rtf'] },
        { name: 'Config / Data', extensions: ['json','xml','ini','conf','cfg','yaml','yml','toml','env','csv','tsv','properties'] },
        { name: 'Source Code', extensions: ['js','jsx','ts','tsx','py','java','c','cpp','cs','go','rs','rb','php','swift','kt','scala','dart','sh','bash','ps1','bat','sql'] },
        { name: 'Web', extensions: ['html','htm','css','scss','sass','less','xml','json'] },
        { name: 'Whiteboard / Diagrams', extensions: ['mmd','mermaid','whiteboard','excalidraw','json'] },
      ]
    });
    if (r.canceled) return;
    filePaths = r.filePaths;
  }
  for (const fp of filePaths) {
    const existing = tabs.find(t => t.filePath === fp);
    if (existing) { activateTab(existing.id); continue; }
    const res = await window.electronAPI.readFile(fp);
    if (!res.success) { showToast('Error: ' + res.error); continue; }
    // Track in Recent Files (main owns the persisted list + menu)
    try { window.electronAPI.recentFileOpened(fp); } catch {}
    // Watch this file for external changes (git pull, another editor, etc.)
    try { window.electronAPI.watchFile(fp); } catch {}

    // ── Encrypted file? — detect, unlock, decrypt, then open as editor tab.
    const envelope = window.NotePPCrypto.detectEncrypted(res.content);
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
      else createTab(fp, res.content);
    } else if (lower.endsWith('.json')) {
      let isWb = false;
      try {
        const parsed = JSON.parse(res.content);
        isWb = parsed && (parsed.__wb__ === true || parsed.type === 'excalidraw');
      } catch (e) {}
      if (isWb) createWhiteboardTab(fp, res.content);
      else       createTab(fp, res.content);
    } else {
      createTab(fp, res.content);
    }
  }
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
    const r = await window.electronAPI.saveDialog({ defaultPath: tab.name, filters: [{ name: 'All Files', extensions: ['*'] }] });
    if (r.canceled) return false;
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

async function closeAllTabs() { for (const id of [...tabs.map(t => t.id)]) await closeTab(id); }
async function closeOtherTabs(keepId) { for (const id of tabs.filter(t => t.id !== keepId).map(t => t.id)) await closeTab(id); }

// ===== Language Detection =====
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

  // 6. Markdown
  if (/^#{1,6} /m.test(head) || /^```/m.test(head) || /^\*\*\w/.test(head)) return 'markdown';

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
  const selCount = sel && !sel.isEmpty() ? model.getValueInRange(sel).length : 0;
  const selLines = sel && !sel.isEmpty() ? sel.endLineNumber - sel.startLineNumber + 1 : 0;
  statusLnCol.textContent = `Ln : ${pos.lineNumber}    Col : ${pos.column}    Sel : ${selCount} | ${selLines > 1 ? selLines + ' lines' : selCount}`;
  statusLines.textContent = `lines: ${model.getLineCount()}`;
  statusLength.textContent = `length: ${model.getValueLength()}`;
}

function updateTitle() {
  const tab = getActiveTab();
  if (!tab) return;
  if (tab.type === 'game') {
    window.electronAPI.setTitle('🎮 Dev Arcade - Note++');
    return;
  }
  if (tab.type === 'whiteboard') {
    // Use the tab name (e.g. "whiteboard-1.json"), not tab.filePath — the
    // AppData backing file is an implementation detail; users expect the
    // same "name - Note++" pattern as a "new 1" editor tab.
    window.electronAPI.setTitle(`${tab.dirty ? '* ' : ''}${tab.name} - Note++`);
    return;
  }
  window.electronAPI.setTitle(`${tab.dirty ? '* ' : ''}${tab.filePath || tab.name} - Note++`);
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

function toggleDarkMode() {
  isDarkMode = !isDarkMode;
  document.body.classList.toggle('theme-dark', isDarkMode);
  document.body.classList.toggle('theme-light', !isDarkMode);
  monaco.editor.setTheme(isDarkMode ? 'notepp-dark' : 'notepp-light');
  document.getElementById('btn-darkmode').classList.toggle('active', isDarkMode);
  syncMermaidThemeToAppMode(); // keep diagram theme in sync
  sendToWhiteboard({ type: 'wb-theme', dark: isDarkMode }); // keep whiteboard in sync
  sendToDrawio({ type: 'dw-theme', dark: isDarkMode });     // keep draw.io in sync
  saveSetting('ui.darkMode', isDarkMode);          // persist across sessions
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
  document.getElementById('monaco-editor').addEventListener('contextmenu', (e) => {
    e.preventDefault();
    const menu = contextMenu;
    menu.classList.remove('hidden');
    menu.style.left = Math.min(e.clientX, window.innerWidth - 210) + 'px';
    menu.style.top = Math.min(e.clientY, window.innerHeight - 260) + 'px';
  });
  document.addEventListener('click', () => contextMenu.classList.add('hidden'));
  document.querySelectorAll('.ctx-item[data-action]').forEach(item => {
    item.addEventListener('click', () => { handleContextAction(item.dataset.action); contextMenu.classList.add('hidden'); });
  });
}

function handleContextAction(action) {
  const acts = {
    'cut': () => editor.trigger('ctx', 'editor.action.clipboardCutAction', null),
    'copy': () => editor.trigger('ctx', 'editor.action.clipboardCopyAction', null),
    'paste': () => editor.trigger('ctx', 'editor.action.clipboardPasteAction', null),
    'select-all': () => editor.trigger('ctx', 'selectAll', null),
    'format-doc': formatDocument,
    'toggle-comment': toggleComment,
    'find': () => openFindReplace('find'),
    'replace': () => openFindReplace('replace'),
    'google-search': () => { const sel = editor.getSelection(); const text = sel && !sel.isEmpty() ? editor.getModel().getValueInRange(sel) : ''; if (text) window.open(`https://www.google.com/search?q=${encodeURIComponent(text)}`); },
    'b64-encode': base64Encode,
    'b64-decode': base64Decode,
    'json-format': jsonFormat,
  };
  acts[action]?.();
  editor.focus();
}

// ===== Drag & Drop =====
function setupDragDrop() {
  document.addEventListener('dragover', (e) => { e.preventDefault(); document.body.classList.add('drag-over'); });
  document.addEventListener('dragleave', (e) => { if (!e.relatedTarget) document.body.classList.remove('drag-over'); });
  document.addEventListener('drop', (e) => {
    e.preventDefault(); document.body.classList.remove('drag-over');
    const files = Array.from(e.dataTransfer.files).map(f => f.path);
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
      const existing = tabs.find(t => t.filePath === fp);
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

  setupCloudPrefButtons();
  setupAiPrefsPage();
  setupNewDocPrefsPage();
  setupBackupPrefsPage();
  setupEncryptionPrefsPage();
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

  const theme = document.querySelector('input[name="pref-theme"]:checked')?.value || 'light';
  if (theme === 'dark' && !isDarkMode) toggleDarkMode();
  else if (theme === 'light' && isDarkMode) toggleDarkMode();

  // New-document defaults
  newDocDefaults.encoding = document.querySelector('input[name="pref-encoding"]:checked')?.value || 'UTF-8';
  newDocDefaults.eol      = document.querySelector('input[name="pref-eol"]:checked')?.value || 'Windows (CR LF)';
  newDocDefaults.language = document.getElementById('pref-default-lang')?.value || 'plaintext';
  newDocDefaults.template = document.getElementById('pref-new-template')?.value || '';

  // Auto-backup settings — restart timer if changed
  const bkpEnabled  = document.getElementById('pref-backup-enable')?.checked || false;
  const bkpInterval = parseInt(document.getElementById('pref-backup-interval')?.value || '5') * 60 * 1000;
  startAutoBackup(bkpEnabled, bkpInterval);
}

async function openPreferences() {
  document.getElementById('prefs-dialog').classList.remove('hidden');
  await loadCloudPrefs();
  await loadNewDocPrefs();
  await loadBackupPrefs();
  await loadTerminalPrefs();
  refreshEncryptionPrefsPage();
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

  // Double-click on empty tab bar space → new tab
  document.getElementById('tab-bar-container').addEventListener('dblclick', (e) => {
    if (e.target.closest('.tab') || e.target.id === 'tab-new-btn') return;
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
        'paste': () => editor.trigger('tb', 'editor.action.clipboardPasteAction', null),
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
      };
      map[a]?.();
      if (a !== 'games' && a !== 'ai' && a !== 'encrypt-toggle' && a !== 'source-control') editor.focus();
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
    if (!tab || tab.type === 'game' || tab.type === 'whiteboard') return;
    const langs = ['plaintext','javascript','typescript','python','java','c','cpp','csharp','go','rust','ruby','php','html','css','json','xml','markdown','mermaid','sql','shell','powershell','bat','yaml','kotlin','swift','lua','r','dockerfile','scss'];
    const items = [
      ['🖼 New Whiteboard Tab', () => createWhiteboardTab(null, '')],
      ['📊 New Diagram Tab',    () => createDrawioTab(null, '')],
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
    const langRect = statusLang.getBoundingClientRect();
    showFloatingMenu(langRect.left, langRect.bottom, items);
  });

  document.getElementById('file-tree-close').addEventListener('click', () => document.getElementById('file-tree').classList.add('hidden'));
}

// ===== Menu Listeners =====
function setupMenuListeners() {
  const api = window.electronAPI;
  const m = (ch, fn) => api.onMenu(ch, fn);

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
  m('menu-dark-mode', toggleDarkMode);
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

  // Terminal output from run-command (id=0)
  api.onMenu('terminal-output', (id, data) => {
    if (term) term.write(data);
    if (!terminalOpen) openTerminal();
  });
}

// ===== File Tree =====
async function openFolderTree(folderPath) {
  const tree = document.getElementById('file-tree');
  const content = document.getElementById('file-tree-content');
  document.getElementById('file-tree-title').textContent = folderPath.split(/[\\/]/).pop();
  tree.classList.remove('hidden');
  content.innerHTML = '';
  await renderTreeDir(content, folderPath, 0);
}

async function renderTreeDir(container, dirPath, depth) {
  const entries = await window.electronAPI.listDir(dirPath);
  const dirs = entries.filter(e => e.isDir).sort((a, b) => a.name.localeCompare(b.name));
  const files = entries.filter(e => !e.isDir).sort((a, b) => a.name.localeCompare(b.name));

  for (const entry of [...dirs, ...files]) {
    const fullPath = dirPath.replace(/[/\\]$/, '') + '/' + entry.name;
    const row = document.createElement('div');
    row.className = 'tree-item';
    row.style.paddingLeft = (8 + depth * 16) + 'px';
    row.title = fullPath;

    // Build content (icon + name + optional git badge) using DOM nodes so the
    // badge can be coloured + tooltipped independently.
    const label = document.createElement('span');
    label.textContent = (entry.isDir ? '📁 ' : getFileEmoji(entry.name) + ' ') + entry.name;
    row.appendChild(label);

    // Git decoration for files (skip dirs — VS Code shows aggregate badges
    // for folders, but that's expensive to compute per render; defer to v1.1)
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

    if (entry.isDir) {
      let expanded = false, child = null;
      row.addEventListener('click', async () => {
        if (!expanded) {
          child = document.createElement('div');
          row.after(child);
          await renderTreeDir(child, fullPath, depth + 1);
          expanded = true;
        } else { child?.remove(); child = null; expanded = false; }
      });
    } else {
      row.addEventListener('click', () => openFile([fullPath]));
    }
    container.appendChild(row);
  }
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
        dirty: !!tab.dirty,    // preserve unsaved-changes marker across restarts
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
      };
    }
    return {
      id: tab.id,
      name: tab.name,
      filePath: tab.filePath || null,
      // Only inline content for unsaved/small files; large saved files reload from disk
      content: (!tab.filePath || content.length <= MAX_INLINE_SIZE) ? content : null,
      language: tab.language,
      encoding: tab.encoding,
      eol: tab.eol,
      active: tab.id === activeTabId,
      dirty: !!tab.dirty,    // preserve unsaved-changes marker across restarts
    };
  });
  await window.electronAPI.writeSession({ tabs: sessionTabs });
}

async function restoreSession() {
  const res = await window.electronAPI.readSession();
  if (!res.success || !res.data?.tabs?.length) return false;

  const { tabs: saved } = res.data;

  let activeId = null;

  for (const s of saved) {
    let content = s.content ?? '';

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
        // Preserve the unsaved-changes marker across restarts. New unsaved
        // whiteboards with actual content come back showing the red dot, so
        // the user knows to Save As before closing.
        dirty: !!s.dirty,
        language: 'whiteboard',
        encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
        model: null, viewState: null, type: 'whiteboard',
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
        if (window.NotePPCrypto.detectEncrypted(r.content)) {
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
      // Preserve the unsaved-changes marker so unsaved "new N" tabs (and
      // saved tabs the user had edited but not yet saved) come back showing
      // the red dot. Without this, every restart silently "cleans" the dirty
      // flag and the user can't tell their work isn't on disk yet.
      dirty: !!s.dirty,
      language: s.language || 'plaintext',
      encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
      model, viewState: null, type: 'editor',
      encrypted: false, protectedBy: null,
    };
    tabs.push(tab);
    if (s.active) activeId = id;
    // Start watching restored files for external changes
    if (tab.filePath) {
      try { window.electronAPI.watchFile(tab.filePath); } catch {}
    }
  }

  if (tabs.length === 0) return false;

  activateTab(activeId || tabs[tabs.length - 1].id);
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
const GIT_AUTO_FETCH_MS = 3 * 60 * 1000;

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
  if (typeof window.Diff?.diffLines !== 'function') return;

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

  const s = activeGitRepo ? gitRepos.get(activeGitRepo) : null;

  if (!activeGitRepo || !s || !s.success) {
    noRepoMsg.classList.remove('hidden');
    emptyMsg.classList.add('hidden');
    stagedSec.style.display = 'none';
    changesSec.style.display = 'none';
    branchName.textContent = '—';
    branchArrows.textContent = '';
    return;
  }
  noRepoMsg.classList.add('hidden');

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
    if (activeGitRepo) {
      window.electronAPI.git.fetch(activeGitRepo).then(() => refreshGitStatus(activeGitRepo));
    }
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
  await window.NotePPCrypto.setPasswordOnProfile(appEnc.profile, appEnc.rawDek, newPassword);
  const res = await saveEncryptionProfile(appEnc.profile);
  if (!res.success) throw new Error('Failed to save profile: ' + (res.error || 'unknown'));
  return true;
}

// Generate a fresh recovery key, replacing the existing one. Profile must be unlocked.
async function regenerateEncryptionRecoveryKey() {
  if (!isEncUnlocked()) throw new Error('Profile is locked');
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

// ===== Global ESC Handler =====
function setupGlobalEscape() {
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;

    // Priority order: close the topmost visible layer first
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
  }, true); // capture phase so it fires before Monaco consumes the key
}

async function sha256(text) {
  const buf = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return Array.from(new Uint8Array(buf)).map(b => b.toString(16).padStart(2, '0')).join('');
}

// ===== Preview Panel =====
function isPreviewable(lang) {
  return lang === 'html' || lang === 'markdown' || lang === 'mermaid';
}

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
    btn.textContent = previewMaximized ? '⤬' : '⛶';
  }
  editor?.layout();
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

  // Ctrl+wheel inside the preview body → zoom
  document.getElementById('preview-body')?.addEventListener('wheel', (e) => {
    if (!(e.ctrlKey || e.metaKey)) return;
    e.preventDefault();
    previewZoomBy(e.deltaY < 0 ? PREVIEW_ZOOM_STEP : -PREVIEW_ZOOM_STEP);
  }, { passive: false });

  setupMermaidToolbar();

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
      showToast('Preview available for HTML, Markdown and Mermaid (.mmd) files');
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

  // Hide all, then show the right one
  mdEl.classList.add('hidden');
  frame.classList.add('hidden');
  mmdEl.classList.add('hidden');

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
  }
  // Refresh the shared zoom label — it shows different value for Mermaid vs others
  if (typeof updatePreviewZoomLabel === 'function') updatePreviewZoomLabel();
}

function showPreviewPlaceholder() {
  const mdEl  = document.getElementById('preview-md-content');
  const frame = document.getElementById('preview-html-frame');
  const mmdEl = document.getElementById('preview-mermaid-content');
  frame.classList.add('hidden');
  mmdEl.classList.add('hidden');
  updateMermaidToolbar(false);
  mdEl.classList.remove('hidden');
  mdEl.innerHTML = '<div style="text-align:center;padding:40px;opacity:0.5;font-size:13px;">No preview available for this file type.<br>Open an HTML, Markdown or Mermaid (.mmd) file to use preview.</div>';
}

// Detect if content is a raw Mermaid diagram (no markdown code fences)
const MERMAID_START = /^\s*(graph|flowchart|sequenceDiagram|classDiagram|stateDiagram|erDiagram|gantt|pie|journey|gitGraph|mindmap|timeline|xychart|block|packet|architecture|requirementDiagram|C4Context|C4Container|C4Component|C4Dynamic|C4Deployment)\b/i;

function isBareRawMermaid(text) {
  return MERMAID_START.test(text.trim()) && !(/```/.test(text));
}

function renderMarkdownPreview(content, container) {
  if (!window.marked) {
    container.innerHTML = '<div style="color:#c00000;padding:12px;font-size:12px">marked library not loaded. Restart the app.</div>';
    return;
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

  // Parse with one-time options (does NOT mutate global marked state)
  let html;
  try {
    html = marked.parse(content, { renderer, gfm: true, breaks: false, pedantic: false });
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

function runMermaidInContainer(container) {
  const diagrams = container.querySelectorAll('.mermaid');
  if (diagrams.length === 0 || !window.mermaid) return;
  try {
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
    console.warn('Mermaid error:', e);
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

  if (!window.mermaid) {
    diagramEl.innerHTML = '<div style="padding:20px;color:#c00000">Mermaid library not loaded — restart the app.</div>';
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

  // Middle-mouse or Alt+drag → pan
  zoomArea.addEventListener('mousedown', e => {
    if (e.button !== 1 && !(e.button === 0 && e.altKey)) return;
    e.preventDefault();
    mermaidPanning = true;
    mermaidPanStart = { x: e.clientX, y: e.clientY, sl: zoomArea.scrollLeft, st: zoomArea.scrollTop };
    zoomArea.style.cursor = 'grabbing';
  });
  document.addEventListener('mousemove', e => {
    if (!mermaidPanning) return;
    zoomArea.scrollLeft = mermaidPanStart.sl - (e.clientX - mermaidPanStart.x);
    zoomArea.scrollTop  = mermaidPanStart.st - (e.clientY - mermaidPanStart.y);
  });
  document.addEventListener('mouseup', e => {
    if (!mermaidPanning) return;
    mermaidPanning = false;
    zoomArea.style.cursor = '';
  });

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
  el.style.transform       = `scale(${mermaidZoom})`;
  el.style.transformOrigin = 'top center';
  if (label) label.textContent = Math.round(mermaidZoom * 100) + '%';
  // Keep the shared preview-header zoom label in sync when Mermaid pane is active
  if (typeof updatePreviewZoomLabel === 'function') updatePreviewZoomLabel();
}

function mermaidZoomIn()  { mermaidZoom = Math.min(+(mermaidZoom + 0.25).toFixed(2), 4.0); applyMermaidZoom(); }
function mermaidZoomOut() { mermaidZoom = Math.max(+(mermaidZoom - 0.25).toFixed(2), 0.25); applyMermaidZoom(); }
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
