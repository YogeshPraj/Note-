# Note++ — LSP (Language Server Protocol) Integration Spec

> Status: design locked, v1 implementation in progress.
> v1 ships **diagnostics + hover + completion for Python** (via [pyright]).
> Other languages are config-only additions to a registry table.

## Goals

- IntelliSense for languages **beyond** Monaco's built-in JS/TS support
- Real-time **diagnostics** (the most visible win — squiggly underlines + Problems-style markers)
- **Hover** documentation
- **Completion** with snippets

Out of scope for v1: go-to-definition, find-references, rename, signature-help, code-actions, formatting. They land in v1.1 once the plumbing is proven.

## Architecture decision — custom, NOT `monaco-languageclient`

`monaco-languageclient` is the official Microsoft bridge but historically has trouble with **AMD-loaded Monaco** (which is what Note++ uses via `vs/loader.js`). It's also a heavy dep. Instead we build a small custom client:

```
┌─ Renderer ────────────────────────────────────────────────────┐
│  Monaco editor                                                │
│  ↓ registers per-language providers:                          │
│  - registerCompletionItemProvider                             │
│  - registerHoverProvider                                      │
│  - editor.setModelMarkers (diagnostics)                       │
│                                                               │
│  Each provider serialises the request to LSP shape and        │
│  calls electronAPI.lsp.send(langId, method, params)           │
└───────────────────────────┬───────────────────────────────────┘
                            │ IPC
┌───────────────────────────▼───────────────────────────────────┐
│  Main process: src/lsp-service.js                             │
│  - spawn(cmd, args) per language → child process              │
│  - LSP stdio JSON-RPC framing (`Content-Length:` header)      │
│  - Track requests by id, resolve when reply arrives           │
│  - Forward server-pushed notifications (publishDiagnostics)   │
│    to renderer via `lsp-notification` event                   │
└───────────────────────────────────────────────────────────────┘
```

**Trade-offs accepted:**
- We translate LSP types ↔ Monaco types manually (small table per feature).
- Some LSP features have no Monaco equivalent (workspace-symbol, etc.) — fine, we don't need them.
- One server per language at a time (no per-workspace multi-server multiplexing).

## Language registry

A single config table in main controls every supported language:

```js
const LSP_LANGUAGES = {
  python: {
    monacoIds: ['python'],
    extensions: ['.py', '.pyi'],
    cmd: ['pyright-langserver', '--stdio'],
    fallbackCmds: [
      ['npx', '-y', 'pyright-langserver', '--stdio'],
    ],
    install: { npm: 'pyright', note: 'Run `npm i -g pyright` once.' },
  },
  // (Future) go: { cmd: ['gopls'] ... }
  // (Future) rust: { cmd: ['rust-analyzer'] ... }
};
```

Adding a new language is one entry. The plumbing is shared.

## Per-language lifecycle

1. **Tab activation** of a matching file → renderer sends `lsp-ensure-started` with langId
2. **Main**: looks up registry → spawns server → completes LSP `initialize` handshake → on success, sends `lsp-ready { langId }` to renderer
3. **Renderer**: on `lsp-ready` → registers Monaco providers for that langId (idempotent)
4. **Document sync**: each file's open/change/close is forwarded to the server via standard LSP `textDocument/did*` notifications
5. **Diagnostics**: server pushes `textDocument/publishDiagnostics` → main forwards → renderer applies `setModelMarkers`
6. **Completion / hover**: Monaco provider → IPC `lsp.send` → main forwards as LSP request → reply → Monaco displays
7. **Tab close** of last file using a language → server is gracefully `shutdown`+`exit`-ed after a short idle timeout (e.g. 30 s)

## Failure / missing-server UX

- If the binary isn't on PATH and `fallbackCmds` also fails → show a one-time toast:
  *"Python IntelliSense needs pyright. Run `npm install -g pyright` (or open a terminal: `npm i -g pyright`)."*
- Status-bar pill shows `LSP: pyright (running)` / `(starting)` / `(missing)` / `(crashed)`
- Crashes auto-restart up to 3 times within 30 s, then give up until next activation

## v1 implementation phases

1. **Spec + IPC + main-side service** with framing + request tracking + auto-restart
2. **Renderer LSP client module** + Monaco provider wiring (diagnostics first, then hover, then completion)
3. **Python via pyright** as the first registered language
4. **Status-bar indicator** + missing-server toast

## Files touched

| File | What |
|---|---|
| `src/lsp-service.js` | New — main-process server manager |
| `src/lsp-client.js` | New — renderer LSP client + Monaco provider wiring (loaded as a `<script>` like crypto.js) |
| `src/main.js` | Wire LSP IPC handlers |
| `src/preload.js` | `electronAPI.lsp.*` bridge |
| `src/index.html` | Status-bar `#status-lsp` element + `<script src="lsp-client.js">` |
| `src/renderer.js` | Call `lspOnTabActivated()` from `activateTab` + `lspOnTabClosed()` from `closeTab` |
| `src/style.css` | Status-bar styling for `#status-lsp` |

## Out of scope (v2+)

- Auto-install language servers via npm/pip from inside the app
- Per-workspace `pyright`/etc. configs (read `pyproject.toml`)
- Go-to-definition / find-references / rename / signature-help / formatting / code-actions
- File operations: server-side rename, workspace symbol search
- Multiple servers per language (e.g. pyright + ruff together)
- Bundling pyright into the installer
