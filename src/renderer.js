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
  setupGlobalEscape();
  updateStatusBar();
});

// ===== Tab Management =====
function createTab(filePath = null, content = '') {
  tabCounter++;
  const id = tabCounter;
  const name = filePath ? filePath.split(/[\\/]/).pop() : `new ${tabCounter}`;
  const language = filePath ? detectLanguage(filePath) : 'plaintext';
  const model = monaco.editor.createModel(content, language);
  const tab = { id, name, filePath, content, dirty: false, language, encoding: 'UTF-8', eol: 'Windows (CR LF)', model, viewState: null };
  tabs.push(tab);
  activateTab(id);
  renderTabs();
  return tab;
}

function activateTab(id) {
  const tab = tabs.find(t => t.id === id);
  if (!tab) return;
  const prev = getActiveTab();
  if (prev && editor) {
    prev.viewState = editor.saveViewState();
    prev.content = editor.getValue();
  }
  activeTabId = id;
  if (editor) {
    editor.setModel(tab.model);
    if (tab.viewState) editor.restoreViewState(tab.viewState);
    editor.focus();
  }
  renderTabs();
  updateStatusBar();
  updateTitle();
  updateLanguageStatus();
  // Update preview for the new active tab
  if (previewOpen) {
    if (isPreviewable(tab.language)) {
      updatePreview();
    } else {
      // Show placeholder for non-previewable files
      showPreviewPlaceholder();
    }
  }
}

function renderTabs() {
  tabBar.innerHTML = '';
  tabs.forEach(tab => {
    const el = document.createElement('div');
    el.className = 'tab' + (tab.id === activeTabId ? ' active' : '') + (tab.dirty ? ' dirty' : '');
    el.dataset.id = tab.id;

    const icon = document.createElement('span');
    icon.className = 'tab-icon';
    icon.textContent = getFileEmoji(tab.name);

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
  const m = { js:'📜',ts:'📘',jsx:'⚛',tsx:'⚛',py:'🐍',java:'☕',c:'©',cpp:'➕',cs:'#',go:'🔵',rs:'🦀',rb:'💎',php:'🐘',html:'🌐',css:'🎨',json:'📋',xml:'📄',md:'📝',sql:'🗄',sh:'🖥',ps1:'💙',bat:'🦇',yaml:'⚙',yml:'⚙',dockerfile:'🐳',svg:'🖼',txt:'📃',log:'📋' };
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

  // 5. YAML (must come before general key:value checks)
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
  window.electronAPI.setTitle(`${tab.dirty ? '* ' : ''}${tab.filePath || tab.name} - Note++`);
}

function updateLanguageStatus() {
  const tab = getActiveTab();
  if (!tab) return;
  const names = { plaintext:'Normal Text',javascript:'JavaScript',typescript:'TypeScript',python:'Python',java:'Java',c:'C',cpp:'C++',csharp:'C#',go:'Go',rust:'Rust',ruby:'Ruby',php:'PHP',html:'HTML',css:'CSS',json:'JSON',xml:'XML',markdown:'Markdown',sql:'SQL',shell:'Shell Script',powershell:'PowerShell',bat:'Batch',yaml:'YAML',lua:'Lua',kotlin:'Kotlin',swift:'Swift',r:'R',dockerfile:'Dockerfile',scss:'SCSS' };
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
      };
      map[a]?.();
      editor.focus();
    });
  });

  statusLang.addEventListener('click', () => {
    const tab = getActiveTab();
    if (!tab) return;
    const langs = ['plaintext','javascript','typescript','python','java','c','cpp','csharp','go','rust','ruby','php','html','css','json','xml','markdown','sql','shell','powershell','bat','yaml','kotlin','swift','lua','r','dockerfile','scss'];
    showFloatingMenu(statusLang.getBoundingClientRect().left, window.innerHeight - 30,
      langs.map(lang => [lang, () => {
        tab.language = lang;
        monaco.editor.setModelLanguage(tab.model, lang);
        updateLanguageStatus();
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
  const sessionTabs = tabs.map(tab => {
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
  await window.electronAPI.writeSession({ tabs: sessionTabs, tabCounter });
}

async function restoreSession() {
  const res = await window.electronAPI.readSession();
  if (!res.success || !res.data?.tabs?.length) return false;

  const { tabs: saved, tabCounter: savedCounter } = res.data;
  if (savedCounter) tabCounter = savedCounter;

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
  return lang === 'html' || lang === 'markdown';
}

function setupPreview() {
  document.getElementById('btn-preview-close').addEventListener('click', closePreview);
  document.getElementById('btn-preview-refresh').addEventListener('click', () => updatePreview());

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
      showToast('Preview is only available for HTML and Markdown files');
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
  const mdEl  = document.getElementById('preview-md-content');
  const frame = document.getElementById('preview-html-frame');

  if (tab.language === 'markdown') {
    mdEl.classList.remove('hidden');
    frame.classList.add('hidden');
    renderMarkdownPreview(content, mdEl);
  } else if (tab.language === 'html') {
    mdEl.classList.add('hidden');
    frame.classList.remove('hidden');
    renderHtmlPreview(content, frame, tab);
  }
}

function showPreviewPlaceholder() {
  const mdEl  = document.getElementById('preview-md-content');
  const frame = document.getElementById('preview-html-frame');
  frame.classList.add('hidden');
  mdEl.classList.remove('hidden');
  mdEl.innerHTML = '<div style="text-align:center;padding:40px;opacity:0.5;font-size:13px;">No preview available for this file type.<br>Open an HTML or Markdown file to use preview.</div>';
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
