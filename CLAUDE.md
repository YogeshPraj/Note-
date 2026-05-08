# Note++ — Project Context

> Claude reads this file at the start of every session. Keep it up to date.

## What This Is

**Note++** — a developer-focused desktop text editor built as an Electron + Monaco clone of Notepad++.  
Located at `D:\NewNotepad`. GitHub: https://github.com/YogeshPraj/Note-

**Target user:** software developers (not generic notepad users).  
**Key differentiators over plain Notepad++:** IntelliSense, integrated terminal, live HTML/Markdown preview with Mermaid diagrams, cloud session sync, Monaco editor engine.

---

## Tech Stack

| Layer | Tech |
|---|---|
| Runtime | Electron 28 |
| Editor | Monaco Editor 0.45.0 |
| Markdown | marked v18.0.3 |
| Diagrams | Mermaid v11.14.0 |
| Terminal | xterm 5.3 + xterm-addon-fit |
| Security | `contextIsolation: true`, `nodeIntegration: false` |
| Node bridge | `src/preload.js` → `window.electronAPI` |
| Platform | Windows (primary), Node.js |

---

## Project Structure

```
D:\NewNotepad\
├── package.json          ← "main": "src/main.js"
├── package-lock.json
├── launch.bat            ← double-click to run (npx electron .)
├── .gitignore
├── CLAUDE.md             ← this file
├── node_modules/
└── src/
    ├── main.js           ← Electron main process (509 lines)
    ├── preload.js        ← IPC bridge (window.electronAPI)
    ├── index.html        ← renderer entry point
    ├── renderer.js       ← all UI logic (~2050 lines)
    ├── style.css         ← all styles
    ├── monaco-worker.js  ← Monaco web worker helper
    └── assets/           ← 21 SVG toolbar icons
```

---

## Architecture Notes

### IPC Pattern
All Node/filesystem access goes through `preload.js`. The renderer calls `window.electronAPI.readFile(path)`, etc. Never use `require()` directly in renderer code.

### Monaco AMD Loader — Critical Loading Order
Monaco's `vs/loader.js` installs a global `define()` function. **marked.umd.js MUST load before `vs/loader.js`**, otherwise marked registers as an AMD module instead of setting `window.marked`, breaking the Markdown preview.

Current script load order in `index.html`:
1. `../node_modules/marked/lib/marked.umd.js`  ← MUST be first
2. `../node_modules/monaco-editor/min/vs/loader.js`
3. xterm, xterm-addon-fit
4. mermaid
5. `renderer.js`

### Monaco Worker URLs
Since `index.html` lives in `src/`, all `node_modules` paths use `../node_modules/`. The Monaco worker inline script in `index.html` uses `${base}../node_modules/monaco-editor/min/`.

In `renderer.js`, the require config uses `'../node_modules/monaco-editor/min/vs'`.

### Preview Panel
- Toggled with `Ctrl+Shift+V` or the 👁 toolbar button
- Only activates for `html` and `markdown` language modes
- Split-pane: editor left, preview right (resizable via drag handle)
- 400ms debounce on keystroke before re-render
- **Markdown preview**: `marked.parse()` with custom `renderer.code` override for Mermaid fences; result injected into `#preview-md-content`
- **Mermaid**: bare Mermaid files (no code fences, starts with `graph`/`flowchart`/etc.) are detected by `isBareRawMermaid()` and rendered directly
- **HTML preview**: rendered in sandboxed `<iframe srcdoc="...">` with mermaid script injected into `<head>`
- **Mermaid Live Editor** (`.mmd` / `.mermaid`): dedicated split-pane editor — Monaco left, live diagram right. Auto-opens preview. Toolbar: templates, SVG/PNG export, zoom controls (Ctrl+scroll too). Error box shows parse failures while preserving last valid diagram. Uses `mermaid.render()` API for direct SVG injection.

### Mermaid (v11) API
```javascript
// For Markdown/HTML preview (run on all .mermaid divs):
mermaid.initialize({ startOnLoad: false, theme: 'default'|'dark', securityLevel: 'loose' });
mermaid.run({ nodes: Array.from(container.querySelectorAll('.mermaid')) });

// For Mermaid Live Editor (direct SVG render):
const { svg, bindFunctions } = await mermaid.render(uniqueId, diagramText);
container.innerHTML = svg;
if (bindFunctions) bindFunctions(container);
```

---

## Features Implemented

- [x] Multi-tab editor (Monaco)
- [x] Syntax highlighting for 50+ languages
- [x] IntelliSense / auto-complete
- [x] Integrated terminal (xterm + PowerShell)
- [x] Find/Replace panel (regex support)
- [x] Command palette (`Ctrl+Shift+P`)
- [x] Quick Open (`Ctrl+P`)
- [x] File tree sidebar
- [x] Live preview for HTML and Markdown (`Ctrl+Shift+V`)
- [x] Mermaid Live Editor for `.mmd` / `.mermaid` files — auto-opens split pane, templates, SVG/PNG export, zoom
- [x] Mermaid diagram rendering in preview
- [x] Dark/light mode toggle
- [x] Cloud session sync (Google Drive, OneDrive, Dropbox)
- [x] Auto-save session
- [x] Code formatting (JSON, XML, language-aware)
- [x] Base64 encode/decode
- [x] Regex tester
- [x] Word wrap, minimap, breadcrumbs toggles
- [x] Preferences dialog
- [x] Status bar (line/col, length, encoding, EOL, language)
- [x] Context menu
- [x] Keyboard shortcuts throughout
- [x] Zoom in/out
- [x] Bookmark navigation
- [x] Run file (F5)

---

## Key renderer.js Functions

| Function | Purpose |
|---|---|
| `setupPreview()` | Wires close/refresh buttons and col-resize drag |
| `togglePreview()` | Opens if previewable lang, closes if open |
| `updatePreview()` | Routes to `renderMarkdownPreview` or `renderHtmlPreview` |
| `renderMarkdownPreview(content, container)` | Parses MD with marked, runs Mermaid |
| `renderBareMarkdown(content, container)` | Wraps raw Mermaid content in `.mermaid` div |
| `runMermaidInContainer(container)` | Calls `mermaid.run()` on all `.mermaid` nodes |
| `renderHtmlPreview(content, frame, tab)` | Injects mermaid script, sets `iframe.srcdoc` |
| `isBareRawMermaid(text)` | Returns true if entire file is raw Mermaid syntax |
| `schedulePreviewUpdate()` | 400ms debounced call to `updatePreview()` |
| `activateTab(id)` | Switches active tab, updates editor model + preview |
| `createTab(opts)` | Creates new tab object + DOM tab element |

---

## Session History

### Session 1 (origin: 69d8d00e)
- Scaffolded entire Note++ application from scratch
- Electron main process, preload bridge, Monaco editor setup
- Multi-tab system, file open/save, syntax highlighting
- Terminal integration (xterm + PowerShell)
- Find/Replace panel, command palette, quick open
- File tree sidebar, status bar, toolbar
- Dark mode, preferences dialog, cloud sync

### Session 2 (continued from Session 1)
- Added HTML + Markdown live preview panel (`Ctrl+Shift+V`)
- Resizable split-pane layout (CSS flex)
- Mermaid diagram support in Markdown preview (code fences)
- **Bug fix**: Preview was empty — root cause was Monaco AMD loader intercepting marked's UMD registration. Fixed by moving `marked.umd.js` script tag before `vs/loader.js`.
- Added `isBareRawMermaid()` detection for raw Mermaid files (no code fences)
- Moved all source files to `src/` subdirectory
- Updated all `node_modules/` paths to `../node_modules/` in `src/`
- Initialized git repo, pushed initial commit to https://github.com/YogeshPraj/Note-

---

## How to Run

```bash
cd D:\NewNotepad
npm start          # or double-click launch.bat
```

## How to Build

```bash
npm run build      # electron-builder (needs electron-builder in devDeps)
```

---

## Known Issues / Future Work

- [ ] `node-pty` not installed — terminal resize (`terminal-resize` IPC) is silently ignored; text wrapping in terminal is approximate
- [ ] `electron-builder` not in `devDependencies` yet — `npm run build` will fail until added
- [ ] No diff view / git integration in editor yet
- [ ] No LSP server connections (only Monaco's built-in JS/TS IntelliSense)
- [ ] `pref-new-doc` and `pref-backup` preference pages in Preferences dialog are stubs (UI exists, no logic)
