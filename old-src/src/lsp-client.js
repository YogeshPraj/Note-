// =============================================================================
// Note++ — LSP client (renderer)
// =============================================================================
// Bridges Monaco editor providers ↔ LSP messages over IPC.
//
// Public API (exposed on `window.NotePPLsp`):
//   NotePPLsp.attachEditor(monacoEditorInstance)
//   NotePPLsp.onTabActivated(tab)
//   NotePPLsp.onTabContentChange(tab)  // call from your existing onChange
//   NotePPLsp.onTabClosed(tab)
//
// State per language: registered Monaco providers, open docs, version counter.
// State per tab: { uri, version, lastSyncedContent } stored on tab._lsp.
// =============================================================================

(function () {
  'use strict';

  const PROVIDERS_REGISTERED = new Set();   // langId set
  const READY_LANGUAGES = new Set();
  const STATUS_BY_LANG = new Map();         // langId → 'starting' | 'ready' | 'missing' | 'crashed'
  const DOC_VERSIONS = new Map();           // uri → integer

  // Editor instance is wired by the renderer once Monaco is up
  let _editor = null;

  function attachEditor(ed) {
    _editor = ed;
    wireGlobalListeners();
  }

  // ── Path → URI ─────────────────────────────────────────────────────────
  function pathToUri(p) {
    if (!p) return 'untitled:Untitled-1';
    let s = p.replace(/\\/g, '/');
    if (!s.startsWith('/')) s = '/' + s;
    return 'file://' + encodeURI(s).replace(/#/g, '%23').replace(/\?/g, '%3F');
  }

  // ── Tab → language ─────────────────────────────────────────────────────
  // Returns the LSP language id (registry key) for a tab, or null
  async function langIdForTab(tab) {
    if (!tab || tab.type !== 'editor' || !tab.language) return null;
    return await window.electronAPI.lsp.languageFor(tab.language);
  }

  // ── Lifecycle hooks called by renderer.js ──────────────────────────────
  async function onTabActivated(tab) {
    if (!tab || tab.type !== 'editor' || !tab.filePath) return;
    const langId = await langIdForTab(tab);
    if (!langId) return;

    // Make sure the server is up
    if (!READY_LANGUAGES.has(langId)) {
      const ws = workspaceRootForTab(tab);
      const r = await window.electronAPI.lsp.ensureStarted(langId, ws);
      if (!r.ready) return;   // status-bar listener will reflect missing/crashed
      READY_LANGUAGES.add(langId);
    }

    // Register Monaco providers once per language
    if (!PROVIDERS_REGISTERED.has(langId)) {
      registerProviders(langId);
      PROVIDERS_REGISTERED.add(langId);
    }

    // didOpen / didChange this tab's document if not already synced
    syncDocument(tab, langId);
  }

  function onTabContentChange(tab) {
    if (!tab || tab.type !== 'editor' || !tab._lsp) return;
    // Debounce: a real LSP-client batches incrementally, but we just send
    // a full-text didChange on a short timer.
    clearTimeout(tab._lsp.changeTimer);
    tab._lsp.changeTimer = setTimeout(() => sendDidChange(tab), 250);
  }

  async function onTabClosed(tab) {
    if (!tab?._lsp) return;
    const langId = tab._lsp.langId;
    if (!langId) return;
    await window.electronAPI.lsp.notify(langId, 'textDocument/didClose', {
      textDocument: { uri: tab._lsp.uri },
    });
    tab._lsp = null;
  }

  function workspaceRootForTab(tab) {
    // Reuse the renderer's git repo detection if available, else use the file's folder
    if (typeof window.activeGitRepo === 'string' && window.activeGitRepo) return window.activeGitRepo;
    if (!tab.filePath) return null;
    return tab.filePath.replace(/[\\/][^\\/]+$/, '');
  }

  // ── Document sync ──────────────────────────────────────────────────────
  function syncDocument(tab, langId) {
    const content = tab.model.getValue();
    const uri = pathToUri(tab.filePath);
    if (!tab._lsp) {
      tab._lsp = { langId, uri, version: 1, lastContent: content, changeTimer: null };
      DOC_VERSIONS.set(uri, 1);
      window.electronAPI.lsp.notify(langId, 'textDocument/didOpen', {
        textDocument: {
          uri,
          languageId: tab.language,
          version: 1,
          text: content,
        },
      });
    } else if (tab._lsp.lastContent !== content) {
      sendDidChange(tab);
    }
  }

  function sendDidChange(tab) {
    if (!tab?._lsp) return;
    const content = tab.model.getValue();
    if (content === tab._lsp.lastContent) return;
    tab._lsp.version++;
    tab._lsp.lastContent = content;
    window.electronAPI.lsp.notify(tab._lsp.langId, 'textDocument/didChange', {
      textDocument: { uri: tab._lsp.uri, version: tab._lsp.version },
      contentChanges: [{ text: content }],   // full-sync
    });
  }

  // ── LSP → Monaco translators ───────────────────────────────────────────
  // LSP ranges are 0-indexed (line, character); Monaco is 1-indexed
  function lspRangeToMonaco(r) {
    return new monaco.Range(
      (r.start.line   ?? 0) + 1,
      (r.start.character ?? 0) + 1,
      (r.end.line     ?? 0) + 1,
      (r.end.character   ?? 0) + 1,
    );
  }
  // LSP DiagnosticSeverity: 1=Error 2=Warning 3=Info 4=Hint
  function lspSeverityToMonaco(sev) {
    switch (sev) {
      case 1: return monaco.MarkerSeverity.Error;
      case 2: return monaco.MarkerSeverity.Warning;
      case 3: return monaco.MarkerSeverity.Info;
      case 4: return monaco.MarkerSeverity.Hint;
      default: return monaco.MarkerSeverity.Info;
    }
  }
  // LSP CompletionItemKind → Monaco
  function lspKindToMonaco(k) {
    const M = monaco.languages.CompletionItemKind;
    return [
      M.Text, M.Method, M.Function, M.Constructor, M.Field, M.Variable, M.Class, M.Interface,
      M.Module, M.Property, M.Unit, M.Value, M.Enum, M.Keyword, M.Snippet, M.Color, M.File,
      M.Reference, M.Folder, M.EnumMember, M.Constant, M.Struct, M.Event, M.Operator, M.TypeParameter,
    ][(k || 1) - 1] || M.Text;
  }
  function lspInsertTextRuleToMonaco(it) {
    // 1 = PlainText, 2 = Snippet
    const R = monaco.languages.CompletionItemInsertTextRule;
    return it === 2 ? R.InsertAsSnippet : R.None;
  }

  // ── Diagnostics: server → editor markers ───────────────────────────────
  function applyDiagnostics(uri, diagnostics) {
    // Find the model with this URI
    const model = monaco.editor.getModels().find(m => pathToUri(m.uri?.fsPath || '') === uri);
    if (!model) return;
    const markers = (diagnostics || []).map(d => ({
      severity: lspSeverityToMonaco(d.severity),
      message:  d.message || '',
      source:   d.source  || '',
      code:     d.code != null ? String(d.code) : undefined,
      ...(() => {
        const r = lspRangeToMonaco(d.range);
        return {
          startLineNumber: r.startLineNumber,
          startColumn:     r.startColumn,
          endLineNumber:   r.endLineNumber,
          endColumn:       r.endColumn,
        };
      })(),
    }));
    monaco.editor.setModelMarkers(model, 'lsp', markers);
  }

  // ── Provider registrations (per language, once) ────────────────────────
  function registerProviders(langId) {
    const cfg = LSP_LANGUAGES_MIRROR[langId];
    if (!cfg) return;
    for (const mid of cfg.monacoIds) {
      // Completion
      monaco.languages.registerCompletionItemProvider(mid, {
        triggerCharacters: ['.', ' ', ':', '(', '['],
        async provideCompletionItems(model, position) {
          const tab = findTabByModel(model);
          if (!tab?._lsp) return { suggestions: [] };
          // Make sure the latest content is synced
          await flushSync(tab);
          try {
            const r = await window.electronAPI.lsp.send(langId, 'textDocument/completion', {
              textDocument: { uri: tab._lsp.uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
              context: { triggerKind: 1 },
            });
            if (!r.success) return { suggestions: [] };
            const items = Array.isArray(r.result) ? r.result : (r.result?.items || []);
            const word = model.getWordUntilPosition(position);
            const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn);
            return {
              suggestions: items.map(it => ({
                label: typeof it.label === 'string' ? it.label : (it.label?.label || ''),
                kind: lspKindToMonaco(it.kind),
                insertText: it.insertText || (typeof it.label === 'string' ? it.label : ''),
                insertTextRules: lspInsertTextRuleToMonaco(it.insertTextFormat),
                detail: it.detail || '',
                documentation: typeof it.documentation === 'string'
                  ? it.documentation
                  : (it.documentation?.value ? { value: it.documentation.value, isTrusted: false } : undefined),
                sortText: it.sortText || it.label,
                filterText: it.filterText || (typeof it.label === 'string' ? it.label : ''),
                range,
              })),
            };
          } catch { return { suggestions: [] }; }
        },
      });

      // Hover
      monaco.languages.registerHoverProvider(mid, {
        async provideHover(model, position) {
          const tab = findTabByModel(model);
          if (!tab?._lsp) return null;
          await flushSync(tab);
          try {
            const r = await window.electronAPI.lsp.send(langId, 'textDocument/hover', {
              textDocument: { uri: tab._lsp.uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            });
            if (!r.success || !r.result) return null;
            const h = r.result;
            const contents = [];
            if (Array.isArray(h.contents)) {
              for (const c of h.contents) contents.push(toMarked(c));
            } else if (h.contents) {
              contents.push(toMarked(h.contents));
            }
            return {
              contents: contents.filter(Boolean),
              range: h.range ? lspRangeToMonaco(h.range) : undefined,
            };
          } catch { return null; }
        },
      });

      // Definition (cheap win — Monaco's Ctrl+Click handles the navigation)
      monaco.languages.registerDefinitionProvider(mid, {
        async provideDefinition(model, position) {
          const tab = findTabByModel(model);
          if (!tab?._lsp) return null;
          await flushSync(tab);
          try {
            const r = await window.electronAPI.lsp.send(langId, 'textDocument/definition', {
              textDocument: { uri: tab._lsp.uri },
              position: { line: position.lineNumber - 1, character: position.column - 1 },
            });
            if (!r.success || !r.result) return null;
            const locs = Array.isArray(r.result) ? r.result : [r.result];
            return locs.map(l => ({
              uri: monaco.Uri.parse(l.uri || l.targetUri || ''),
              range: lspRangeToMonaco(l.range || l.targetSelectionRange || l.targetRange),
            }));
          } catch { return null; }
        },
      });
    }
  }

  function toMarked(c) {
    if (typeof c === 'string') return { value: c };
    if (c && c.kind === 'markdown') return { value: c.value || '' };
    if (c && c.value) return { value: c.value };
    return null;
  }

  // Make sure we've sent the latest didChange before asking the server a question
  async function flushSync(tab) {
    if (!tab?._lsp) return;
    if (tab._lsp.changeTimer) {
      clearTimeout(tab._lsp.changeTimer);
      tab._lsp.changeTimer = null;
      sendDidChange(tab);
    }
  }

  function findTabByModel(model) {
    if (!window.tabs) return null;
    return window.tabs.find(t => t.model === model) || null;
  }

  // ── Push-handler: status updates + server notifications ────────────────
  function wireGlobalListeners() {
    window.electronAPI.lsp.onStatus(({ langId, state, error }) => {
      STATUS_BY_LANG.set(langId, state);
      updateStatusBar(langId, state, error);
      if (state === 'ready')   READY_LANGUAGES.add(langId);
      if (state === 'crashed' || state === 'missing') READY_LANGUAGES.delete(langId);
    });
    window.electronAPI.lsp.onNotification(({ langId, method, params }) => {
      if (method === 'textDocument/publishDiagnostics') {
        applyDiagnostics(params.uri, params.diagnostics || []);
      }
      // (showMessage / logMessage / etc. left as no-ops for v1)
    });
  }

  // Tracks which langId the pill currently represents so the click handler
  // knows what to install.
  let CURRENT_PILL_LANG = null;
  let INSTALLING_LANGS  = new Set();

  function updateStatusBar(langId, state, error) {
    const el = document.getElementById('status-lsp');
    if (!el) return;
    const cfg = LSP_LANGUAGES_MIRROR[langId];
    const name = cfg?.install?.label || langId;
    let text = '';
    let title = '';
    let clickable = false;
    switch (state) {
      case 'starting':   text = `🧠 LSP: ${name} starting…`; break;
      case 'ready':      text = `🧠 LSP: ${name}`;            title = 'Language server running'; break;
      case 'missing':    text = `⬇ Install ${name}`;          title = `Click to install — runs: npm install -g ${name}`; clickable = true; break;
      case 'installing': text = `⏳ Installing ${name}…`;      title = 'npm install in progress'; break;
      case 'crashed':    text = `✗ LSP: ${name} crashed`;     title = error || 'restarting…'; break;
      default:           text = '';
    }
    el.textContent = text;
    el.title = title;
    el.classList.toggle('hidden', !text);
    el.dataset.state = state;
    el.classList.toggle('clickable', !!clickable);
    el.style.cursor = clickable ? 'pointer' : '';
    CURRENT_PILL_LANG = state ? langId : null;

    // One-shot toast for missing servers — guide user toward clicking the pill
    if (state === 'missing' && !MISSING_TOASTED.has(langId)) {
      MISSING_TOASTED.add(langId);
      if (typeof window.showToast === 'function') {
        window.showToast(`Click "Install ${name}" in the status bar to set it up.`);
      }
    }
  }
  const MISSING_TOASTED = new Set();

  // Wire one global click handler for the pill (installs the missing server).
  function wirePillClickHandler() {
    const el = document.getElementById('status-lsp');
    if (!el || el._installWired) return;
    el._installWired = true;
    el.addEventListener('click', async () => {
      if (el.dataset.state !== 'missing') return;
      const langId = CURRENT_PILL_LANG;
      if (!langId || INSTALLING_LANGS.has(langId)) return;
      INSTALLING_LANGS.add(langId);
      const cfg = LSP_LANGUAGES_MIRROR[langId];
      const name = cfg?.install?.label || langId;
      updateStatusBar(langId, 'installing');
      if (typeof window.showToast === 'function') window.showToast(`Installing ${name}…`);
      try {
        const res = await window.electronAPI.lsp.install(langId);
        INSTALLING_LANGS.delete(langId);
        if (res?.success) {
          if (typeof window.showToast === 'function') window.showToast(`${name} installed — retry will start automatically.`);
          // Re-attempt server start; on success the status pill updates.
          try { await window.electronAPI.lsp.ensureStarted(langId, null); } catch {}
        } else {
          updateStatusBar(langId, 'missing', res?.error || 'install failed');
          if (typeof window.showToast === 'function') {
            window.showToast(`Install failed (exit ${res?.exitCode ?? '?'}). See terminal for details.`);
          }
        }
      } catch (err) {
        INSTALLING_LANGS.delete(langId);
        updateStatusBar(langId, 'missing', err.message);
      }
    });
  }
  // Defer until DOM is ready
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', wirePillClickHandler, { once: true });
  } else {
    wirePillClickHandler();
  }

  // Mirror of the main-side registry — keeps providers from needing async lookups
  // every keystroke. Kept tiny; full source of truth still lives in lsp-service.js.
  const LSP_LANGUAGES_MIRROR = {
    python: {
      monacoIds: ['python'],
      install: { label: 'pyright' },
    },
  };

  // ── Export ─────────────────────────────────────────────────────────────
  window.NotePPLsp = {
    attachEditor,
    onTabActivated,
    onTabContentChange,
    onTabClosed,
    statusOf: (langId) => STATUS_BY_LANG.get(langId) || 'idle',
  };
})();
