// =============================================================================
// Note++ — Lazy-loaders for UMD scripts (Mermaid, xterm, jsdiff)
// =============================================================================
// Extracted from renderer.js as part of the renderer split. Loaded as a
// classic <script> tag before renderer.js in index.html, so the four
// helpers below are attached to the global scope automatically (top-level
// `function foo()` in a classic script is window.foo).
//
// All three libraries are big (mermaid ~3 MB, xterm ~270 KB, jsdiff ~50 KB)
// and only matter to a subset of sessions. We defer them with a tiny
// dynamic-script-injection helper + an AMD-trap guard.
//
// The AMD-trap: by the time these helpers run, Monaco's vs/loader.js has
// installed a global define(). Each library here is UMD — if `define` is
// present at load time the script registers as an anonymous AMD module
// (Monaco's loader complains, the global never sets, the library breaks).
// _loadUmdScript() nulls out window.define for the duration of each load
// so the UMD scripts hit their "set window.X" branch instead.
// =============================================================================

// Shared script-injection primitive with the AMD trap-guard.
function _loadUmdScript(src) {
  return new Promise((resolve, reject) => {
    const savedDefine = window.define;
    try { window.define = undefined; } catch {}
    const script = document.createElement('script');
    script.src = src;
    script.async = true;
    const restore = () => { if (savedDefine) window.define = savedDefine; };
    script.onload  = () => { restore(); resolve(); };
    script.onerror = (e) => { restore(); reject(e || new Error('script load failed: ' + src)); };
    document.head.appendChild(script);
  });
}

// xterm + xterm-addon-fit (~270 KB combined) — first terminal open.
// Also injects xterm.css on demand so launches that never open the
// terminal don't pay the stylesheet-parse cost.
function ensureXterm() {
  if (window.Terminal && window.FitAddon) return Promise.resolve();
  if (ensureXterm._loading) return ensureXterm._loading;
  ensureXterm._loading = (async () => {
    if (!document.getElementById('xterm-css')) {
      const link = document.createElement('link');
      link.id = 'xterm-css';
      link.rel = 'stylesheet';
      link.href = '../node_modules/xterm/css/xterm.css';
      document.head.appendChild(link);
    }
    await _loadUmdScript('../node_modules/xterm/lib/xterm.js');
    await _loadUmdScript('../node_modules/xterm-addon-fit/lib/xterm-addon-fit.js');
  })();
  return ensureXterm._loading;
}

// crypto.js (~372 lines) — wraps Web Crypto primitives for the encrypted-
// pad feature. Only invoked on encrypt/decrypt actions, restore of an
// encrypted file, or the encryption preferences page — none of which
// happen on cold start. Lazy-load it so non-encryption sessions don't
// pay the parse cost. The file's an internal classic script (not UMD)
// that hangs `window.NotePPCrypto` once it executes.
function ensureCrypto() {
  if (window.NotePPCrypto) return Promise.resolve(window.NotePPCrypto);
  if (ensureCrypto._loading) return ensureCrypto._loading;
  ensureCrypto._loading = (async () => {
    await _loadUmdScript('./crypto.js');
    if (!window.NotePPCrypto) throw new Error('crypto.js did not register NotePPCrypto');
    return window.NotePPCrypto;
  })();
  return ensureCrypto._loading;
}

// json5 (~30 KB) — only needed when the JSON preview hits a parse error
// and we want to try a forgiving re-parse (trailing commas, single
// quotes, comments, unquoted keys, etc.) to offer an auto-fix.
function ensureJson5() {
  if (window.JSON5) return Promise.resolve(window.JSON5);
  if (ensureJson5._loading) return ensureJson5._loading;
  ensureJson5._loading = (async () => {
    await _loadUmdScript('../node_modules/json5/dist/index.min.js');
    if (!window.JSON5) throw new Error('JSON5 did not register on window');
    return window.JSON5;
  })();
  return ensureJson5._loading;
}

// marked (~43 KB UMD) — first Markdown preview render. Loaded eagerly
// before this lazy approach; deferring it saves the parse+execute cost
// on every launch that doesn't open a Markdown preview.
function ensureMarked() {
  if (window.marked) return Promise.resolve(window.marked);
  if (ensureMarked._loading) return ensureMarked._loading;
  ensureMarked._loading = (async () => {
    await _loadUmdScript('../node_modules/marked/lib/marked.umd.js');
    if (!window.marked) throw new Error('marked did not register on window');
    return window.marked;
  })();
  return ensureMarked._loading;
}

// jsdiff (~50 KB) — first inline git diff render.
function ensureDiff() {
  if (window.Diff) return Promise.resolve();
  if (ensureDiff._loading) return ensureDiff._loading;
  ensureDiff._loading = _loadUmdScript('../node_modules/diff/dist/diff.min.js');
  return ensureDiff._loading;
}

// JSONEditor (~2 MB) — first JSON file preview.
function ensureJsonEditor() {
  if (window.JSONEditor) return Promise.resolve(window.JSONEditor);
  if (ensureJsonEditor._loading) return ensureJsonEditor._loading;
  ensureJsonEditor._loading = (async () => {
    if (!document.getElementById('jsoneditor-css')) {
      const link = document.createElement('link');
      link.id = 'jsoneditor-css';
      link.rel = 'stylesheet';
      link.href = '../node_modules/jsoneditor/dist/jsoneditor.min.css';
      document.head.appendChild(link);
    }
    await _loadUmdScript('../node_modules/jsoneditor/dist/jsoneditor.min.js');
    if (!window.JSONEditor) throw new Error('JSONEditor did not register on window');
    return window.JSONEditor;
  })();
  return ensureJsonEditor._loading;
}

// Mermaid (~3 MB) — first preview render that needs a diagram.
// Initializes mermaid eagerly on first load with the current theme so
// callers can call mermaid.render() immediately after ensureMermaid().
function ensureMermaid() {
  if (window.mermaid) return Promise.resolve(window.mermaid);
  if (ensureMermaid._loading) return ensureMermaid._loading;
  ensureMermaid._loading = (async () => {
    await _loadUmdScript('../node_modules/mermaid/dist/mermaid.min.js');
    if (!window.mermaid) throw new Error('mermaid did not register on window');
    try {
      // `isDarkMode` is a renderer.js global — defined before this helper
      // runs (we're only called from interactive code paths).
      mermaid.initialize({
        startOnLoad: false,
        theme: (typeof isDarkMode !== 'undefined' && isDarkMode) ? 'dark' : 'default',
        securityLevel: 'loose',
        fontFamily: "'Segoe UI', sans-serif",
      });
    } catch (e) { console.warn('Mermaid initialize failed:', e); }
    return window.mermaid;
  })();
  return ensureMermaid._loading;
}
