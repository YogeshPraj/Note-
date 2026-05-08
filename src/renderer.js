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
    fontLigatures: true,
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
    cursorBlinking: 'smooth',
    cursorSmoothCaretAnimation: 'on',
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
    hover: { enabled: true, delay: 300 },
    contextmenu: false,
    smoothScrolling: true,
    mouseWheelZoom: true,
    multiCursorModifier: 'alt',
    snippetSuggestions: 'inline',
    occurrencesHighlight: 'singleFile',
    selectionHighlight: true,
    renderControlCharacters: false,
    colorDecorators: true,
    inlayHints: { enabled: 'on' },
    lightbulb: { enabled: 'on' },
    stickyScroll: { enabled: true },
    padding: { top: 4, bottom: 4 },
  });

  registerMermaidLanguage();
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
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyG, () => openGameTab());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyA, () => toggleAiPanel());
  editor.addCommand(monaco.KeyCode.F12, () => editor.getAction('editor.action.revealDefinition')?.run());
  editor.addCommand(monaco.KeyMod.Shift | monaco.KeyCode.F12, () => editor.getAction('editor.action.goToReferences')?.run());
  editor.addCommand(monaco.KeyCode.F2, () => editor.getAction('editor.action.rename')?.run());
  editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Backquote, () => toggleTerminal());

  setupMenuListeners();
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
  setupGlobalEscape();
  updateStatusBar();
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
  const language = filePath ? detectLanguage(filePath) : 'plaintext';
  const model = monaco.editor.createModel(content, language);
  const tab = { id, name, filePath, content, dirty: false, language, encoding: 'UTF-8', eol: 'Windows (CR LF)', model, viewState: null, type: 'editor' };
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

function activateTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  const prev = getActiveTab();
  if (prev && editor && prev.type !== 'game') {
    prev.viewState = editor.saveViewState();
    prev.content = editor.getValue();
  }
  activeTabId = id;

  const gameContainer = document.getElementById('game-container');
  const monacoEl     = document.getElementById('monaco-editor');
  const gameFrame    = document.getElementById('game-frame');

  if (tab.type === 'game') {
    // Show game container, hide monaco editor
    monacoEl.style.display = 'none';
    gameContainer.classList.remove('hidden');
    // Load launcher only if not already loaded
    if (!gameFrame.src || !gameFrame.src.endsWith('launcher.html')) {
      const base = window.location.href.replace(/[^/]*$/, '');
      gameFrame.src = base + 'games/launcher.html';
    }
    if (previewOpen) closePreview();
  } else {
    // Show editor, hide game container
    monacoEl.style.display = '';
    gameContainer.classList.add('hidden');
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
}

function renderTabs() {
  tabBar.innerHTML = '';
  tabs.forEach(tab => {
    const el = document.createElement('div');
    el.className = 'tab' + (tab.id === activeTabId ? ' active' : '') + (tab.dirty ? ' dirty' : '');
    el.dataset.id = tab.id;

    const icon = document.createElement('span');
    icon.className = 'tab-icon';
    icon.textContent = tab.type === 'game' ? '🎮' : getFileEmoji(tab.name);

    const name = document.createElement('span');
    name.className = 'tab-name';
    name.title = tab.filePath || tab.name;
    name.textContent = tab.name;

    const close = document.createElement('button');
    close.className = 'tab-close';
    close.textContent = '×';
    close.addEventListener('click', (e) => { e.stopPropagation(); closeTab(tab.id); });

    el.appendChild(icon);
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
  const items = [
    ['Close', () => closeTab(tabId)],
    ['Close All', () => closeAllTabs()],
    ['Close All But This', () => closeOtherTabs(tabId)],
    null,
    ['Copy Full Path', () => { if (tab.filePath) navigator.clipboard.writeText(tab.filePath); }],
    ['Open Containing Folder', () => { if (tab.filePath) window.electronAPI.shellShowItem(tab.filePath); }],
    null,
    ['Reveal in File Tree', () => {}],
  ];
  showFloatingMenu(e.clientX, e.clientY, items);
}

function showFloatingMenu(x, y, items) {
  const menu = document.createElement('div');
  menu.style.cssText = `position:fixed;left:${x}px;top:${y}px;z-index:2000;background:var(--ctx-bg);border:1px solid var(--ctx-border);padding:2px 0;min-width:200px;box-shadow:2px 2px 8px rgba(0,0,0,0.25);font-size:12px;`;
  items.forEach(item => {
    if (!item) { const sep = document.createElement('div'); sep.className = 'ctx-sep'; menu.appendChild(sep); return; }
    const el = document.createElement('div');
    el.className = 'ctx-item';
    el.textContent = item[0];
    el.addEventListener('click', () => { item[1](); document.body.removeChild(menu); });
    menu.appendChild(el);
  });
  document.body.appendChild(menu);
  const hide = () => { if (menu.parentNode) document.body.removeChild(menu); document.removeEventListener('click', hide); };
  setTimeout(() => document.addEventListener('click', hide), 0);
}

// ===== File Operations =====
function newTab() { createTab(); }

async function openFile(filePaths) {
  if (!filePaths) {
    const r = await window.electronAPI.openDialog({
      properties: ['openFile', 'multiSelections'],
      filters: [
        { name: 'All Files', extensions: ['*'] },
        { name: 'Source Code', extensions: ['js','ts','py','java','c','cpp','cs','go','rs','rb','php','swift','kt'] },
        { name: 'Web', extensions: ['html','htm','css','scss','xml','json'] },
        { name: 'Text', extensions: ['txt','md','log','yaml','yml','toml','ini'] },
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
    createTab(fp, res.content);
  }
}

async function saveFile() { const tab = getActiveTab(); if (tab) await saveTabFile(tab); }
async function saveFileAs() { const tab = getActiveTab(); if (tab) await saveTabFile(tab, true); }
async function saveAll() { for (const tab of tabs) { if (tab.dirty) await saveTabFile(tab); } }

async function saveTabFile(tab, forceAs = false) {
  if (!tab.filePath || forceAs) {
    const r = await window.electronAPI.saveDialog({ defaultPath: tab.name, filters: [{ name: 'All Files', extensions: ['*'] }] });
    if (r.canceled) return false;
    tab.filePath = r.filePath;
    tab.name = r.filePath.split(/[\\/]/).pop();
    tab.language = detectLanguage(tab.filePath);
    monaco.editor.setModelLanguage(tab.model, tab.language);
  }
  const content = tab.model.getValue();
  const res = await window.electronAPI.writeFile(tab.filePath, content);
  if (!res.success) { showToast('Error saving: ' + res.error); return false; }
  tab.dirty = false;
  tab.content = content;
  renderTabs();
  updateTitle();
  updateLanguageStatus();
  return true;
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
  const m = { js:'📜',ts:'📘',jsx:'⚛',tsx:'⚛',py:'🐍',java:'☕',c:'©',cpp:'➕',cs:'#',go:'🔵',rs:'🦀',rb:'💎',php:'🐘',html:'🌐',css:'🎨',json:'📋',xml:'📄',md:'📝',sql:'🗄',sh:'🖥',ps1:'💙',bat:'🦇',yaml:'⚙',yml:'⚙',dockerfile:'🐳',svg:'🖼',txt:'📃',log:'📋',mmd:'📊',mermaid:'📊' };
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

  openTerminal();

  const fp = tab.filePath;
  const ext = fp.split('.').pop().toLowerCase();
  const cwd = fp.replace(/[\\/][^\\/]+$/, '');

  const runners = {
    py: `python "${fp}"`, python: `python "${fp}"`,
    js: `node "${fp}"`, mjs: `node "${fp}"`,
    ts: `ts-node "${fp}"`,
    rb: `ruby "${fp}"`,
    php: `php "${fp}"`,
    go: `go run "${fp}"`,
    rs: `cargo run`,
    sh: `bash "${fp}"`,
    ps1: `powershell.exe -File "${fp}"`,
    bat: `"${fp}"`,
    java: `java "${fp}"`,
    r: `Rscript "${fp}"`,
    lua: `lua "${fp}"`,
  };

  const cmd = runners[ext];
  if (!cmd) { showToast(`No runner configured for .${ext}`); return; }

  if (term) {
    term.writeln(`\x1b[33m> Running: ${cmd}\x1b[0m`);
    term.writeln('');
  }

  await window.electronAPI.runCommand(cmd, cwd);
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

    setTimeout(() => { try { fitAddon.fit(); } catch {} }, 50);

    const res = await window.electronAPI.terminalCreate(terminalId);
    if (!res.success) {
      term.writeln('\x1b[31mFailed to start terminal: ' + (res.error || 'unknown error') + '\x1b[0m');
    } else {
      term.writeln('\x1b[32mNote++ Terminal\x1b[0m — PowerShell');
      term.writeln('');
    }

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
    try { fitAddon?.fit(); } catch {}
  });

  document.addEventListener('mouseup', () => {
    dragging = false;
    document.body.style.cursor = '';
    document.body.style.userSelect = '';
  });
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
  setFindStatus(`${matches.length} match${matches.length !== 1 ? 'es' : ''}`);

  const pos = editor.getPosition();
  let idx = dir === 1
    ? matches.findIndex(m => m.range.startLineNumber > pos.lineNumber || (m.range.startLineNumber === pos.lineNumber && m.range.startColumn > pos.column))
    : (() => { for (let i = matches.length - 1; i >= 0; i--) if (matches[i].range.endLineNumber < pos.lineNumber || (matches[i].range.endLineNumber === pos.lineNumber && matches[i].range.endColumn < pos.column)) return i; return -1; })();

  if (idx === -1) idx = dir === 1 ? 0 : matches.length - 1;
  const m = matches[idx];
  editor.setSelection(m.range);
  editor.revealRangeInCenter(m.range);
}

function findAll() {
  const opts = getSearchOpts();
  if (!opts.searchString) return;
  const matches = editor.getModel().findMatches(opts.searchString, true, opts.isRegex, opts.matchCase, opts.wholeWord ? ' \t\n.,!?' : null, true);
  if (!matches.length) { setFindStatus(`"${opts.searchString}" not found`, true); return; }
  searchDecorations = editor.deltaDecorations(searchDecorations, matches.map(m => ({ range: m.range, options: { className: 'find-highlight', inlineClassName: 'find-highlight-inline' } })));
  setFindStatus(`${matches.length} match${matches.length !== 1 ? 'es' : ''} found`);
}

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
}

function toggleDarkMode() {
  isDarkMode = !isDarkMode;
  document.body.classList.toggle('theme-dark', isDarkMode);
  document.body.classList.toggle('theme-light', !isDarkMode);
  monaco.editor.setTheme(isDarkMode ? 'notepp-dark' : 'notepp-light');
  document.getElementById('btn-darkmode').classList.toggle('active', isDarkMode);
  syncMermaidThemeToAppMode(); // keep diagram theme in sync
}

// ===== Zoom =====
let currentFontSize = 13;
function zoomIn() { currentFontSize = Math.min(currentFontSize + 2, 40); editor.updateOptions({ fontSize: currentFontSize }); }
function zoomOut() { currentFontSize = Math.max(currentFontSize - 2, 8); editor.updateOptions({ fontSize: currentFontSize }); }
function zoomReset() { currentFontSize = 13; editor.updateOptions({ fontSize: currentFontSize }); }

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
  document.getElementById('btn-count').addEventListener('click', countMatches);
  document.getElementById('btn-replace').addEventListener('click', doReplace);
  document.getElementById('btn-replace-all').addEventListener('click', doReplaceAll);
  document.getElementById('find-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') e.shiftKey ? doFind(-1) : doFind(1);
    if (e.key === 'Escape') closeFindReplace();
  });
  document.getElementById('replace-input').addEventListener('keydown', e => {
    if (e.key === 'Enter') doReplace();
    if (e.key === 'Escape') closeFindReplace();
  });
  document.querySelectorAll('.fr-tab').forEach(tab => {
    tab.addEventListener('click', () => {
      document.querySelectorAll('.fr-tab').forEach(t => t.classList.remove('active'));
      tab.classList.add('active');
      const mode = tab.dataset.tab;
      document.getElementById('replace-row').style.display = mode === 'replace' ? 'flex' : 'none';
    });
  });
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

  // Open AI settings page when prefs dialog opens from AI settings button
  document.getElementById('prefs-dialog').addEventListener('transitionend', () => {});
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
    hover: { enabled: hover },
    bracketPairColorization: { enabled: bracketColor },
    renderWhitespace: whitespace ? 'all' : 'selection',
  });

  const theme = document.querySelector('input[name="pref-theme"]:checked')?.value || 'light';
  if (theme === 'dark' && !isDarkMode) toggleDarkMode();
  else if (theme === 'light' && isDarkMode) toggleDarkMode();
}

async function openPreferences() {
  document.getElementById('prefs-dialog').classList.remove('hidden');
  await loadCloudPrefs();
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
      };
      map[a]?.();
      if (a !== 'games' && a !== 'ai') editor.focus();
    });
  });

  statusLang.addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab || tab.type === 'game') return;
    const langs = ['plaintext','javascript','typescript','python','java','c','cpp','csharp','go','rust','ruby','php','html','css','json','xml','markdown','mermaid','sql','shell','powershell','bat','yaml','kotlin','swift','lua','r','dockerfile','scss'];
    showFloatingMenu(statusLang.getBoundingClientRect().left, window.innerHeight - 30,
      langs.map(lang => [lang, () => {
        tab.language = lang;
        monaco.editor.setModelLanguage(tab.model, lang);
        updateLanguageStatus();
        updateMermaidToolbar(lang === 'mermaid');
        if (lang === 'mermaid' && !previewOpen) openPreview();
        else if (previewOpen) updatePreview();
        editor.focus();
      }])
    );
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

  m('menu-word-wrap', checked => { isWordWrap = checked; editor.updateOptions({ wordWrap: checked ? 'on' : 'off' }); });
  m('menu-zoom-in', zoomIn);
  m('menu-zoom-out', zoomOut);
  m('menu-zoom-reset', zoomReset);
  m('menu-minimap', checked => editor.updateOptions({ minimap: { enabled: checked } }));
  m('menu-show-whitespace', checked => editor.updateOptions({ renderWhitespace: checked ? 'all' : 'selection' }));
  m('menu-show-indent', checked => editor.updateOptions({ guides: { indentation: checked } }));
  m('menu-dark-mode', toggleDarkMode);
  m('menu-toolbar', show => { document.getElementById('toolbar').style.display = show ? '' : 'none'; });
  m('menu-statusbar', show => { document.getElementById('status-bar').style.display = show ? '' : 'none'; });
  m('menu-tabbar', show => { document.getElementById('tab-bar-container').style.display = show ? '' : 'none'; });

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
    row.textContent = (entry.isDir ? '📁 ' : getFileEmoji(entry.name) + ' ') + entry.name;

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

// ===== Auto-Save Session =====
const MAX_INLINE_SIZE = 512 * 1024; // 512 KB — larger files saved by path only

function scheduleAutoSave() {
  clearTimeout(autoSaveTimer);
  autoSaveTimer = setTimeout(saveSession, AUTO_SAVE_DELAY);
}

async function saveSession() {
  const sessionTabs = tabs.filter(tab => tab.type !== 'game').map(tab => {
    const content = tab.model.getValue();
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

    // Re-read from disk if we only stored the path
    if (s.filePath && s.content === null) {
      const r = await window.electronAPI.readFile(s.filePath);
      if (r.success) content = r.content;
    }

    tabCounter++;
    const id = tabCounter;
    const model = monaco.editor.createModel(content, s.language || 'plaintext');
    const tab = {
      id, name: s.name, filePath: s.filePath || null,
      content, dirty: false, language: s.language || 'plaintext',
      encoding: s.encoding || 'UTF-8', eol: s.eol || 'Windows (CR LF)',
      model, viewState: null,
    };
    tabs.push(tab);
    if (s.active) activeId = id;
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
let aiResponse     = '';          // accumulated response text
let aiModel        = '';          // currently selected model
let aiResizeActive = false;
let aiRefreshing   = false;       // prevents concurrent refreshAiModelList() calls

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
  document.getElementById('btn-ai-settings').addEventListener('click', () => {
    document.getElementById('prefs-dialog').classList.remove('hidden');
    // Switch to AI settings pane
    document.querySelectorAll('.prefs-item').forEach(i => i.classList.remove('active'));
    document.querySelectorAll('.pref-page').forEach(p => p.classList.remove('active'));
    document.querySelector('.prefs-item[data-pref="ai"]').classList.add('active');
    document.getElementById('pref-ai').classList.add('active');
    refreshAiSettingsPage();
  });

  // Model selector in panel header — sync to saved pref
  modelSel.addEventListener('change', () => {
    aiModel = modelSel.value;
    saveSetting('ai.model', aiModel);
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

  // Token streaming: append to response text
  window.electronAPI.onAiToken(token => {
    aiResponse += token;
    const el = document.getElementById('ai-response-text');
    el.classList.remove('hidden');
    document.getElementById('ai-response-placeholder').classList.add('hidden');
    // Show text + blinking cursor
    el.innerHTML = escapeHtml(aiResponse) + '<span class="ai-cursor"></span>';
    el.parentElement.scrollTop = el.parentElement.scrollHeight;
  });

  window.electronAPI.onAiDone(() => {
    aiGenerating = false;
    // Remove cursor, show action bar
    const el = document.getElementById('ai-response-text');
    el.textContent = aiResponse;
    document.getElementById('ai-action-bar').classList.remove('hidden');
    document.getElementById('btn-ai-send').disabled = false;
    document.getElementById('btn-ai-send').textContent = 'Send ↵';
    // Only restore 'online' dot if the model dropdown still has a valid selection.
    // If the dropdown was reset to an error state by a concurrent refresh, don't
    // overwrite the dot with 'online' — that would cause the green-dot + "Ollama not
    // running" inconsistency the user reported.
    const modelSel = document.getElementById('ai-model-select');
    if (modelSel?.value) {
      setAiStatus('online');
    } else {
      // Dropdown lost its model — re-check to sync both indicators correctly
      refreshAiModelList();
    }
  });

  // Load saved model on startup
  loadAiState();
}

async function loadAiState() {
  const settings = await window.electronAPI.readSettings();
  aiModel = settings?.ai?.model || '';
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

    result.models.forEach(m => {
      const opt = document.createElement('option');
      opt.value = m; opt.textContent = m;
      if (m === aiModel || (!aiModel && result.models.length === 1)) opt.selected = true;
      modelSel.appendChild(opt);
    });
    aiModel = modelSel.value;
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
  const model    = modelSel.value;
  if (!model) { showToast('Select a model first'); return; }

  aiGenerating = true;
  aiResponse   = '';
  resetAiResponse(false);

  document.getElementById('btn-ai-send').disabled = true;
  document.getElementById('btn-ai-send').textContent = '…';
  setAiStatus('busy');

  // Build context-aware system prompt
  const settings    = await window.electronAPI.readSettings();
  const userSystem  = settings?.ai?.systemPrompt || '';
  const langName    = tab.language || 'plaintext';
  const fileName    = tab.name || 'untitled';
  const editorModel = editor.getModel();
  const fullContent = editorModel.getValue();

  // Selected text takes priority; otherwise send full file (capped at 6000 chars)
  const selection = editor.getSelection();
  const selText   = selection && !selection.isEmpty()
    ? editorModel.getValueInRange(selection) : '';

  const MAX_CONTENT = 6000;
  const fileContext = fullContent.length <= MAX_CONTENT
    ? fullContent
    : fullContent.slice(0, MAX_CONTENT) + '\n…[file truncated]';

  // Detect "fix" / "correct" / "rewrite" intent — AI should replace the whole selection or file section
  const fixIntent = /\b(fix|correct|repair|rewrite|improve|clean|refactor|error|bug|wrong|broken|syntax)\b/i.test(userPrompt);

  const system = [
    userSystem,
    `You are an expert writing and coding assistant embedded in a text editor called Note++.`,
    `The user is editing a ${langName} file named "${fileName}".`,
    ``,
    `Your task: output ONLY the corrected/generated text — no explanations, no preamble, no "Here is..." intro.`,
    `Rules:`,
    `- Output raw content only, ready to paste directly into the editor`,
    `- Do NOT add markdown code fences (triple backticks) UNLESS the file is .md or .markdown`,
    `- For Markdown files: use correct Markdown + Mermaid syntax; Mermaid goes inside \`\`\`mermaid fences`,
    `- In Mermaid diagrams: use only valid Mermaid syntax — no if/else, no programming constructs`,
    `- Match the indentation and style of the file`,
    `- If asked to fix/correct: output the fully corrected version of the relevant section`,
    ``,
    selText
      ? `The user has SELECTED this text (operate on it):\n\`\`\`\n${selText}\n\`\`\``
      : `Full file content (${fullContent.length} chars):\n\`\`\`\n${fileContext}\n\`\`\``,
    fixIntent && !selText
      ? `\nThe user wants you to FIX something in the file above. Output the corrected version of the relevant section only.`
      : '',
  ].filter(s => s !== undefined).join('\n');

  window.electronAPI.aiGenerate({ model, prompt: userPrompt, system });
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
  refreshAiModelList();
  setTimeout(() => document.getElementById('ai-prompt').focus(), 50);
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

function setupAiTokenListeners() {
  window.electronAPI.onAiToken(token => {
    aiResponse += token;
    const el = document.getElementById('ai-response-text');
    el.classList.remove('hidden');
    document.getElementById('ai-response-placeholder').classList.add('hidden');
    el.innerHTML = escapeHtml(aiResponse) + '<span class="ai-cursor"></span>';
    el.parentElement.scrollTop = el.parentElement.scrollHeight;
  });
  window.electronAPI.onAiDone(() => {
    aiGenerating = false;
    document.getElementById('ai-response-text').textContent = aiResponse;
    document.getElementById('ai-action-bar').classList.remove('hidden');
    document.getElementById('btn-ai-send').disabled = false;
    document.getElementById('btn-ai-send').textContent = 'Send ↵';
    const modelSel = document.getElementById('ai-model-select');
    if (modelSel?.value) {
      setAiStatus('online');
    } else {
      refreshAiModelList();
    }
  });
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

function setupPreview() {
  document.getElementById('btn-preview-close').addEventListener('click', closePreview);
  document.getElementById('btn-preview-refresh').addEventListener('click', () => updatePreview());
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
  document.getElementById('preview-panel').classList.add('hidden');
  document.getElementById('preview-resize-handle').classList.add('hidden');
  previewOpen = false;
  document.getElementById('btn-preview').classList.remove('active');
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

// Guard against Mermaid bomb-SVGs leaking into document.body.
// Mermaid appends temporary render containers (e.g. #dmmd-r1, #mmd-r1) directly
// to document.body. On parse/render errors it may not clean them up, causing the
// huge "Syntax error in text / mermaid version X" bomb icons to appear on screen.
// A MutationObserver fires synchronously for each childList change so we can
// remove stale containers before they are ever painted.
(function installMermaidBodyGuard() {
  const MMD_ID_RE = /^d?mmd-r\d|^mmd-scratch-/;
  const obs = new MutationObserver(mutations => {
    for (const mut of mutations) {
      for (const node of mut.addedNodes) {
        if (node.nodeType === 1 && MMD_ID_RE.test(node.id || '')) {
          // Hide immediately, then remove after current microtask so Mermaid's
          // own cleanup code (removeTempElements) can still run if it wants to.
          node.style.cssText += ';display:none!important;visibility:hidden!important;';
          Promise.resolve().then(() => { if (node.parentNode) node.parentNode.removeChild(node); });
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

  // Create a hidden scratch container and pass it to mermaid.render() so
  // Mermaid never appends anything to document.body (which causes bomb-SVG overflow).
  const scratch = Object.assign(document.createElement('div'), {
    id: `mmd-scratch-${id}`,
    style: 'visibility:hidden;position:absolute;top:-9999px;left:-9999px;',
  });
  document.body.appendChild(scratch);

  try {
    const { svg, bindFunctions } = await mermaid.render(id, text, scratch);
    diagramEl.innerHTML = svg;
    if (typeof bindFunctions === 'function') bindFunctions(diagramEl);
    errorBox.classList.add('hidden');
    mermaidLastContent = text;
    applyMermaidZoom();
  } catch (err) {
    // Keep the last valid SVG; just surface the error
    const raw = err?.message || String(err);
    // Strip any HTML tags Mermaid injects into the error
    errorText.textContent = raw.replace(/<[^>]*>/g, '').slice(0, 400);
    errorBox.classList.remove('hidden');
  } finally {
    // Always remove the scratch element regardless of success or failure.
    scratch.remove();
    // Belt-and-suspenders: sweep any other stray Mermaid render roots in the body.
    document.querySelectorAll('[id^="mmd-r"],[id^="dmmd-r"],[id^="mmd-scratch-"]').forEach(el => {
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
    renderMermaidPreview(editor.getValue());
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

  // Open in mermaid.live
  document.getElementById('btn-mmde-live').addEventListener('click', openInMermaidLive);

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
    if (previewOpen) renderMermaidPreview(editor.getValue());
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

// ── Open in mermaid.live ──────────────────────────────────────────────────────
async function openInMermaidLive() {
  const content = editor?.getValue().trim();
  if (!content) { showToast('Nothing to open'); return; }

  const state = JSON.stringify({
    code: content,
    mermaid: JSON.stringify({ theme: mermaidTheme }),
    updateEditor: false,
    autoSync: true,
    updateDiagram: true,
  });

  try {
    // CompressionStream('deflate-raw') == pako.deflateRaw — both produce RFC 1951 raw deflate
    const encoder  = new TextEncoder();
    const stream   = new CompressionStream('deflate-raw');
    const writer   = stream.writable.getWriter();
    writer.write(encoder.encode(state));
    writer.close();
    const buf    = await new Response(stream.readable).arrayBuffer();
    const bytes  = new Uint8Array(buf);
    let   binary = '';
    bytes.forEach(b => { binary += String.fromCharCode(b); });
    const base64 = btoa(binary);
    window.electronAPI.openUrl(`https://mermaid.live/edit#pako:${base64}`);
    showToast('Opening in mermaid.live…');
  } catch {
    // Fallback if CompressionStream unavailable
    window.electronAPI.openUrl('https://mermaid.live');
    showToast('Opened mermaid.live (encoding unavailable)');
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
