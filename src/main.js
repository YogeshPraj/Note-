const { app, BrowserWindow, Menu, dialog, ipcMain, shell, protocol } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

// ── draw.io custom protocol (registered BEFORE app.ready) ───────────────────
// drawio:// serves files from the on-demand download bundle in userData. The
// privileged registration enables fetch/XHR/secure context for the iframe.
protocol.registerSchemesAsPrivileged([
  { scheme: 'drawio', privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true } },
]);

// node-pty for true PTY terminal (proper resize, colours, interactive programs)
let pty;
try { pty = require('node-pty'); } catch { pty = null; }

let mainWindow;
const terminalProcesses = new Map();

// ── Dev-mode: clear stale V8 code cache on every startup ─────────────────
// When source files change, Electron's V8 cache can serve stale compiled JS.
// In dev mode (npx electron .) we always wipe it so edits take effect immediately.
if (!app.isPackaged) {
  // Clear stale Chromium caches (Code Cache, disk Cache) so every run sees fresh files
  for (const dir of ['Code Cache', 'Cache', 'DawnCache', 'GPUCache']) {
    try { fs.rmSync(path.join(app.getPath('userData'), dir), { recursive: true, force: true }); }
    catch (_) { /* ignore */ }
  }
}

// ── Single-instance lock ──────────────────────────────────────────────────
// If another instance launches, focus the existing window and open the file.
// Files queued for the renderer to open. Filled by:
//   - process.argv on first launch (double-click / "Open with")
//   - second-instance argv on subsequent launches (Windows file-association)
//   - macOS 'open-file' event
// Drained when the renderer sends 'renderer-ready' — this avoids a race where
// IPC sends fire before the renderer has registered its 'open-files' listener.
const pendingOpenFiles = [];
let rendererReady = false;
function queueOpenFiles(files) {
  if (!files || !files.length) return;
  for (const f of files) if (f && !pendingOpenFiles.includes(f)) pendingOpenFiles.push(f);
  flushPendingOpens();
}
function flushPendingOpens() {
  if (!rendererReady || !mainWindow || !pendingOpenFiles.length) return;
  const files = pendingOpenFiles.splice(0);
  try { mainWindow.webContents.send('open-files', files); } catch {}
}

const gotTheLock = app.requestSingleInstanceLock();
if (!gotTheLock) {
  app.quit();
} else {
  app.on('second-instance', (_event, argv) => {
    if (mainWindow) {
      if (mainWindow.isMinimized()) mainWindow.restore();
      mainWindow.focus();
    }
    const file = fileFromArgv(argv);
    if (file) queueOpenFiles([file]);
  });
}

// macOS Finder open-with
app.on('open-file', (e, filePath) => {
  e.preventDefault();
  queueOpenFiles([filePath]);
});

// Extract the first real file path from a argv array
function fileFromArgv(argv) {
  // argv[0] = exe, skip flags starting with '-'
  for (let i = 1; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('-') && a !== '.' && fs.existsSync(a)) return a;
  }
  return null;
}

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 850,
    minWidth: 400,
    minHeight: 300,
    title: 'Note++',
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.js'),
      webviewTag: true,           // needed for <webview> (mermaid live preview)
    },
    backgroundColor: '#1e1e1e',
    show: false
  });

  mainWindow.loadFile(path.join(__dirname, 'index.html'));
  mainWindow.once('ready-to-show', () => {
    mainWindow.show();
    // Queue any file passed on the command line (double-click / "Open with").
    // It'll be delivered when the renderer sends 'renderer-ready' below.
    const file = fileFromArgv(process.argv);
    if (file) queueOpenFiles([file]);
    // F12 → toggle DevTools (dev mode only)
    if (!app.isPackaged) {
      mainWindow.webContents.on('before-input-event', (_e, input) => {
        if (input.key === 'F12' && input.type === 'keyDown') {
          if (mainWindow.webContents.isDevToolsOpened()) {
            mainWindow.webContents.closeDevTools();
          } else {
            mainWindow.webContents.openDevTools({ mode: 'bottom' });
          }
        }
      });
    }
  });

  mainWindow.on('close', (e) => {
    e.preventDefault();
    mainWindow.webContents.send('app-before-close');
  });

  buildMenu();
}

// ── External file-change watcher ─────────────────────────────────────────
// Each open editor tab subscribes via `watch-file` so we can notify the
// renderer when the file is modified outside Note++ (git checkout, another
// editor, etc.). fs.watch + lightweight debouncing — no chokidar dep.
const fileWatchers = new Map();      // filePath → { watcher, mtimeMs, debounceTimer }

ipcMain.handle('watch-file', (e, filePath) => {
  if (!filePath || fileWatchers.has(filePath)) return { success: true };
  try {
    const mtimeMs = fs.statSync(filePath).mtimeMs;
    const entry = { mtimeMs, debounceTimer: null, watcher: null };
    // fs.watch on the FILE itself (not its directory) — simpler & works for
    // most editors. For atomic-write editors (e.g. vim default), watching
    // the dir is more reliable but adds complexity; revisit if reports come.
    entry.watcher = fs.watch(filePath, { persistent: false }, () => {
      clearTimeout(entry.debounceTimer);
      entry.debounceTimer = setTimeout(() => {
        try {
          if (!fs.existsSync(filePath)) {
            if (mainWindow) mainWindow.webContents.send('file-changed-externally', { filePath, deleted: true });
            return;
          }
          const m = fs.statSync(filePath).mtimeMs;
          if (m === entry.mtimeMs) return; // no real change
          entry.mtimeMs = m;
          if (mainWindow) mainWindow.webContents.send('file-changed-externally', { filePath, deleted: false });
        } catch {}
      }, 250);  // debounce burst events from "atomic write" editors
    });
    fileWatchers.set(filePath, entry);
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('unwatch-file', (e, filePath) => {
  const entry = fileWatchers.get(filePath);
  if (!entry) return { success: true };
  try { entry.watcher?.close(); } catch {}
  clearTimeout(entry.debounceTimer);
  fileWatchers.delete(filePath);
  return { success: true };
});

// When the renderer saves a file we update the cached mtime so the watcher
// doesn't immediately fire "changed externally" for our own write.
ipcMain.handle('file-saved-by-app', (e, filePath) => {
  const entry = fileWatchers.get(filePath);
  if (entry) {
    try { entry.mtimeMs = fs.statSync(filePath).mtimeMs; } catch {}
  }
  return { success: true };
});

// ── Recent files persistence ─────────────────────────────────────────────
// Stored in settings.json under `recentFiles` (most-recent first). Cap 15.
const RECENT_FILES_MAX = 15;
function getRecentFiles() {
  const s = readSettings();
  const arr = Array.isArray(s.recentFiles) ? s.recentFiles : [];
  // Drop any that no longer exist on disk so the menu doesn't show ghosts
  return arr.filter(p => { try { return fs.existsSync(p); } catch { return false; } });
}
function addRecentFile(filePath) {
  if (!filePath) return;
  const s = readSettings();
  const cur = Array.isArray(s.recentFiles) ? s.recentFiles : [];
  // Dedupe case-insensitively on Windows
  const norm = process.platform === 'win32' ? filePath.toLowerCase() : filePath;
  const filtered = cur.filter(p => (process.platform === 'win32' ? p.toLowerCase() : p) !== norm);
  filtered.unshift(filePath);
  s.recentFiles = filtered.slice(0, RECENT_FILES_MAX);
  writeSettings(s);
  buildMenu(); // rebuild so the submenu reflects the new entry
}
function clearRecentFiles() {
  const s = readSettings();
  s.recentFiles = [];
  writeSettings(s);
  buildMenu();
}
function buildRecentFilesSubmenu() {
  const list = getRecentFiles();
  if (!list.length) return [{ label: 'No recent files', enabled: false }];
  const truncate = (p, n) => p.length > n ? '…' + p.slice(p.length - n + 1) : p;
  const items = list.map((p, i) => ({
    label: (i < 9 ? `&${i + 1}  ` : '    ') + truncate(p, 60),
    click: () => queueOpenFiles([p]),
  }));
  items.push({ type: 'separator' });
  items.push({ label: 'Clear Recent Files', click: () => clearRecentFiles() });
  return items;
}

function buildMenu() {
  const template = [
    {
      label: 'File',
      submenu: [
        { label: 'New', accelerator: 'CmdOrCtrl+N', click: () => send('menu-new') },
        { type: 'separator' },
        { label: 'Open...', accelerator: 'CmdOrCtrl+O', click: () => handleOpen() },
        { label: 'Open Folder...', accelerator: 'CmdOrCtrl+Shift+O', click: () => handleOpenFolder() },
        { label: 'Open Recent', submenu: buildRecentFilesSubmenu() },
        { type: 'separator' },
        { label: 'Compare', submenu: [
          { label: 'Compare Files…',         click: () => send('menu-compare-files') },
          { label: 'Compare with Saved…',    click: () => send('menu-compare-with-saved') },
          { type: 'separator' },
          { label: 'Compare Folders…',       click: () => send('menu-compare-folders') },
        ]},
        { type: 'separator' },
        { label: 'Reload from Disk', accelerator: 'CmdOrCtrl+Shift+R', click: () => send('menu-reload') },
        { type: 'separator' },
        { label: 'Close', accelerator: 'CmdOrCtrl+W', click: () => send('menu-close') },
        { label: 'Close All', click: () => send('menu-close-all') },
        { label: 'Close All BUT Current', click: () => send('menu-close-others') },
        { type: 'separator' },
        { label: 'Save', accelerator: 'CmdOrCtrl+S', click: () => send('menu-save') },
        { label: 'Save As...', accelerator: 'CmdOrCtrl+Alt+S', click: () => send('menu-save-as') },
        { label: 'Save All', accelerator: 'CmdOrCtrl+Shift+S', click: () => send('menu-save-all') },
        { type: 'separator' },
        { label: 'Print...', accelerator: 'CmdOrCtrl+P', click: () => send('menu-print') },
        { type: 'separator' },
        { label: 'Exit', accelerator: 'Alt+F4', click: () => app.exit(0) }
      ]
    },
    {
      label: 'Edit',
      submenu: [
        { label: 'Undo', accelerator: 'CmdOrCtrl+Z', click: () => send('menu-undo') },
        { label: 'Redo', accelerator: 'CmdOrCtrl+Y', click: () => send('menu-redo') },
        { type: 'separator' },
        { label: 'Cut', accelerator: 'CmdOrCtrl+X', role: 'cut' },
        { label: 'Copy', accelerator: 'CmdOrCtrl+C', role: 'copy' },
        { label: 'Paste', accelerator: 'CmdOrCtrl+V', role: 'paste' },
        { label: 'Select All', accelerator: 'CmdOrCtrl+A', role: 'selectAll' },
        { type: 'separator' },
        { label: 'Duplicate Line', accelerator: 'CmdOrCtrl+D', click: () => send('menu-duplicate-line') },
        { label: 'Delete Line', accelerator: 'CmdOrCtrl+Shift+K', click: () => send('menu-delete-line') },
        { label: 'Move Line Up', accelerator: 'Alt+Up', click: () => send('menu-move-line-up') },
        { label: 'Move Line Down', accelerator: 'Alt+Down', click: () => send('menu-move-line-down') },
        { type: 'separator' },
        { label: 'Convert Case to', submenu: [
          { label: 'UPPERCASE', accelerator: 'CmdOrCtrl+Shift+U', click: () => send('menu-uppercase') },
          { label: 'lowercase', accelerator: 'CmdOrCtrl+U', click: () => send('menu-lowercase') },
          { label: 'Title Case', click: () => send('menu-titlecase') },
        ]},
        { label: 'Line Operations', submenu: [
          { label: 'Sort Lines Ascending', click: () => send('menu-sort-asc') },
          { label: 'Sort Lines Descending', click: () => send('menu-sort-desc') },
          { label: 'Remove Duplicate Lines', click: () => send('menu-remove-dup-lines') },
          { label: 'Remove Empty Lines', click: () => send('menu-remove-empty-lines') },
          { label: 'Join Lines', click: () => send('menu-join-lines') },
        ]},
        { type: 'separator' },
        { label: 'Base64 Encode', click: () => send('menu-b64-encode') },
        { label: 'Base64 Decode', click: () => send('menu-b64-decode') },
        { type: 'separator' },
        { label: 'Set Read-Only', click: () => send('menu-readonly') },
        { label: 'Clear Read-Only Flag', click: () => send('menu-clear-readonly') },
      ]
    },
    {
      label: 'Code',
      submenu: [
        { label: 'Format Document (Auto-detect)', accelerator: 'CmdOrCtrl+Shift+F', click: () => send('menu-format-doc') },
        { label: 'Format Selection', click: () => send('menu-format-sel') },
        { type: 'separator' },
        { label: 'Toggle Line Comment', accelerator: 'CmdOrCtrl+/', click: () => send('menu-toggle-comment') },
        { label: 'Toggle Block Comment', accelerator: 'Shift+Alt+A', click: () => send('menu-block-comment') },
        { type: 'separator' },
        { label: 'Indent', accelerator: 'Tab', click: () => send('menu-indent-increase') },
        { label: 'Outdent', accelerator: 'Shift+Tab', click: () => send('menu-indent-decrease') },
        { type: 'separator' },
        { label: 'Fold All', accelerator: 'CmdOrCtrl+K CmdOrCtrl+0', click: () => send('menu-fold-all') },
        { label: 'Unfold All', accelerator: 'CmdOrCtrl+K CmdOrCtrl+J', click: () => send('menu-unfold-all') },
        { label: 'Toggle Fold', accelerator: 'CmdOrCtrl+Shift+[', click: () => send('menu-toggle-fold') },
        { type: 'separator' },
        { label: 'Go to Definition', accelerator: 'F12', click: () => send('menu-goto-definition') },
        { label: 'Go to Symbol...', accelerator: 'CmdOrCtrl+Shift+O', click: () => send('menu-goto-symbol') },
        { label: 'Go to References', accelerator: 'Shift+F12', click: () => send('menu-goto-refs') },
        { label: 'Rename Symbol', accelerator: 'F2', click: () => send('menu-rename-symbol') },
        { type: 'separator' },
        { label: 'Trigger Suggest', accelerator: 'CmdOrCtrl+Space', click: () => send('menu-trigger-suggest') },
        { label: 'Trigger Parameter Hints', accelerator: 'CmdOrCtrl+Shift+Space', click: () => send('menu-trigger-hints') },
        { type: 'separator' },
        { label: 'Pretty Print JSON', click: () => send('menu-json-format') },
        { label: 'Pretty Print XML', click: () => send('menu-xml-format') },
        { label: 'Minify JSON', click: () => send('menu-json-minify') },
        { type: 'separator' },
        { label: 'Diagram (draw.io)', submenu: [
          { label: 'New Diagram', click: () => send('menu-new-drawio') },
          { label: 'From Template', submenu: [
            { label: 'Flowchart',              click: () => send('menu-new-drawio-template', 'flowchart') },
            { label: 'Sequence Diagram',       click: () => send('menu-new-drawio-template', 'sequence') },
            { label: 'Class Diagram (UML)',    click: () => send('menu-new-drawio-template', 'classDiagram') },
            { label: 'Entity Relationship',    click: () => send('menu-new-drawio-template', 'erDiagram') },
          ]},
          { type: 'separator' },
          { label: 'Check for updates',  click: () => send('menu-drawio-check-updates') },
        ]},
      ]
    },
    {
      label: 'Search',
      submenu: [
        { label: 'Find...', accelerator: 'CmdOrCtrl+F', click: () => send('menu-find') },
        { label: 'Find Next', accelerator: 'F3', click: () => send('menu-find-next') },
        { label: 'Find Previous', accelerator: 'Shift+F3', click: () => send('menu-find-prev') },
        { label: 'Find All in Current Document', click: () => send('menu-find-all') },
        { type: 'separator' },
        { label: 'Replace...', accelerator: 'CmdOrCtrl+H', click: () => send('menu-replace') },
        { type: 'separator' },
        { label: 'Quick Open (Go to File)', accelerator: 'CmdOrCtrl+P', click: () => send('menu-quick-open') },
        { label: 'Command Palette', accelerator: 'CmdOrCtrl+Shift+P', click: () => send('menu-cmd-palette') },
        { type: 'separator' },
        { label: 'Go to Line...', accelerator: 'CmdOrCtrl+G', click: () => send('menu-goto-line') },
        { label: 'Go to Matching Brace', accelerator: 'CmdOrCtrl+B', click: () => send('menu-goto-brace') },
        { type: 'separator' },
        { label: 'Toggle Bookmark', accelerator: 'CmdOrCtrl+F2', click: () => send('menu-toggle-bookmark') },
        { label: 'Next Bookmark', accelerator: 'F2', click: () => send('menu-next-bookmark') },
        { label: 'Previous Bookmark', accelerator: 'Shift+F2', click: () => send('menu-prev-bookmark') },
        { label: 'Regex Tester...', click: () => send('menu-regex-tester') },
      ]
    },
    {
      label: 'View',
      submenu: [
        { label: 'Always on Top', type: 'checkbox', click: (i) => mainWindow.setAlwaysOnTop(i.checked) },
        { label: 'Full Screen', accelerator: 'F11', type: 'checkbox', click: (i) => mainWindow.setFullScreen(i.checked) },
        { type: 'separator' },
        { label: 'Minimap', type: 'checkbox', checked: true, click: (i) => send('menu-minimap', i.checked) },
        { label: 'Word Wrap', accelerator: 'Alt+Z', type: 'checkbox', click: (i) => send('menu-word-wrap', i.checked) },
        { label: 'Breadcrumbs', type: 'checkbox', checked: true, click: (i) => send('menu-breadcrumbs', i.checked) },
        { type: 'separator' },
        { label: 'Show Whitespace', type: 'checkbox', click: (i) => send('menu-show-whitespace', i.checked) },
        { label: 'Show Indent Guides', type: 'checkbox', checked: true, click: (i) => send('menu-show-indent', i.checked) },
        { type: 'separator' },
        { label: 'Zoom In', accelerator: 'CmdOrCtrl+=', click: () => send('menu-zoom-in') },
        { label: 'Zoom Out', accelerator: 'CmdOrCtrl+-', click: () => send('menu-zoom-out') },
        { label: 'Reset Zoom', accelerator: 'CmdOrCtrl+0', click: () => send('menu-zoom-reset') },
        { type: 'separator' },
        { label: 'Toggle Terminal', accelerator: 'CmdOrCtrl+`', click: () => send('menu-toggle-terminal') },
        { label: 'Toggle Sidebar', accelerator: 'CmdOrCtrl+B', click: () => send('menu-toggle-sidebar') },
        { type: 'separator' },
        { label: 'Toggle Toolbar', type: 'checkbox', checked: true, click: (i) => send('menu-toolbar', i.checked) },
        { label: 'Toggle Status Bar', type: 'checkbox', checked: true, click: (i) => send('menu-statusbar', i.checked) },
        { label: 'Toggle Tab Bar', type: 'checkbox', checked: true, click: (i) => send('menu-tabbar', i.checked) },
      ]
    },
    {
      label: 'Encoding',
      submenu: [
        { label: 'UTF-8', type: 'radio', checked: true, click: () => send('menu-encoding', 'UTF-8') },
        { label: 'UTF-8 BOM', type: 'radio', click: () => send('menu-encoding', 'UTF-8 BOM') },
        { label: 'ANSI', type: 'radio', click: () => send('menu-encoding', 'ANSI') },
        { label: 'UTF-16 LE', type: 'radio', click: () => send('menu-encoding', 'UTF-16 LE') },
        { label: 'UTF-16 BE', type: 'radio', click: () => send('menu-encoding', 'UTF-16 BE') },
      ]
    },
    {
      label: 'Language',
      submenu: [
        { label: 'Plain Text', click: () => send('menu-lang', 'plaintext') },
        { type: 'separator' },
        { label: 'Batch', click: () => send('menu-lang', 'bat') },
        { label: 'C', click: () => send('menu-lang', 'c') },
        { label: 'C++', click: () => send('menu-lang', 'cpp') },
        { label: 'C#', click: () => send('menu-lang', 'csharp') },
        { label: 'CSS', click: () => send('menu-lang', 'css') },
        { label: 'Dockerfile', click: () => send('menu-lang', 'dockerfile') },
        { label: 'Go', click: () => send('menu-lang', 'go') },
        { label: 'HTML', click: () => send('menu-lang', 'html') },
        { label: 'Java', click: () => send('menu-lang', 'java') },
        { label: 'JavaScript', click: () => send('menu-lang', 'javascript') },
        { label: 'JSON', click: () => send('menu-lang', 'json') },
        { label: 'Kotlin', click: () => send('menu-lang', 'kotlin') },
        { label: 'Lua', click: () => send('menu-lang', 'lua') },
        { label: 'Markdown', click: () => send('menu-lang', 'markdown') },
        { label: 'Mermaid', click: () => send('menu-lang', 'mermaid') },
        { label: 'PHP', click: () => send('menu-lang', 'php') },
        { label: 'PowerShell', click: () => send('menu-lang', 'powershell') },
        { label: 'Python', click: () => send('menu-lang', 'python') },
        { label: 'R', click: () => send('menu-lang', 'r') },
        { label: 'Ruby', click: () => send('menu-lang', 'ruby') },
        { label: 'Rust', click: () => send('menu-lang', 'rust') },
        { label: 'SCSS', click: () => send('menu-lang', 'scss') },
        { label: 'Shell Script', click: () => send('menu-lang', 'shell') },
        { label: 'SQL', click: () => send('menu-lang', 'sql') },
        { label: 'Swift', click: () => send('menu-lang', 'swift') },
        { label: 'TOML', click: () => send('menu-lang', 'ini') },
        { label: 'TypeScript', click: () => send('menu-lang', 'typescript') },
        { label: 'XML', click: () => send('menu-lang', 'xml') },
        { label: 'YAML', click: () => send('menu-lang', 'yaml') },
      ]
    },
    {
      label: 'Terminal',
      submenu: [
        { label: 'New Terminal', accelerator: 'CmdOrCtrl+Shift+`', click: () => send('menu-new-terminal') },
        { label: 'Toggle Terminal', accelerator: 'CmdOrCtrl+`', click: () => send('menu-toggle-terminal') },
        { label: 'Kill Terminal', click: () => send('menu-kill-terminal') },
        { label: 'Clear Terminal', click: () => send('menu-clear-terminal') },
        { type: 'separator' },
        { label: 'Run File', accelerator: 'F5', click: () => send('menu-run-file') },
        { label: 'Run Selection', accelerator: 'Shift+F5', click: () => send('menu-run-selection') },
        { type: 'separator' },
        { label: 'Open Containing Folder', accelerator: 'CmdOrCtrl+Alt+Shift+R', click: () => send('menu-open-explorer') },
        { label: 'Copy File Path', click: () => send('menu-copy-path') },
      ]
    },
    {
      label: 'Settings',
      submenu: [
        { label: 'Preferences...', accelerator: 'CmdOrCtrl+,', click: () => send('menu-preferences') },
        { type: 'separator' },
        { label: 'Toggle Dark Mode', accelerator: 'CmdOrCtrl+Alt+D', click: () => send('menu-dark-mode') },
        { type: 'separator' },
        { label: 'Font Size +', accelerator: 'CmdOrCtrl+=', click: () => send('menu-zoom-in') },
        { label: 'Font Size -', accelerator: 'CmdOrCtrl+-', click: () => send('menu-zoom-out') },
      ]
    },
    {
      label: 'Tools',
      submenu: [
        { label: 'Regex Tester', click: () => send('menu-regex-tester') },
        { type: 'separator' },
        { label: 'MD5', submenu: [
          { label: 'Generate from Selection', click: () => send('menu-md5-selection') },
        ]},
        { label: 'SHA-256', submenu: [
          { label: 'Generate from Selection', click: () => send('menu-sha256-selection') },
        ]},
        { type: 'separator' },
        { label: 'Base64 Encode', click: () => send('menu-b64-encode') },
        { label: 'Base64 Decode', click: () => send('menu-b64-decode') },
        { type: 'separator' },
        { label: 'Pretty Print JSON', click: () => send('menu-json-format') },
        { label: 'Pretty Print XML', click: () => send('menu-xml-format') },
        { label: 'Minify JSON', click: () => send('menu-json-minify') },
        { type: 'separator' },
        { label: 'Diagram (draw.io)', submenu: [
          { label: 'New Diagram', click: () => send('menu-new-drawio') },
          { label: 'From Template', submenu: [
            { label: 'Flowchart',              click: () => send('menu-new-drawio-template', 'flowchart') },
            { label: 'Sequence Diagram',       click: () => send('menu-new-drawio-template', 'sequence') },
            { label: 'Class Diagram (UML)',    click: () => send('menu-new-drawio-template', 'classDiagram') },
            { label: 'Entity Relationship',    click: () => send('menu-new-drawio-template', 'erDiagram') },
          ]},
          { type: 'separator' },
          { label: 'Check for updates',  click: () => send('menu-drawio-check-updates') },
        ]},
      ]
    },
    {
      label: 'Window',
      submenu: [
        { label: 'Previous Document', accelerator: 'CmdOrCtrl+PageUp', click: () => send('menu-prev-tab') },
        { label: 'Next Document', accelerator: 'CmdOrCtrl+PageDown', click: () => send('menu-next-tab') },
        { type: 'separator' },
        { label: 'Tab 1', accelerator: 'CmdOrCtrl+1', click: () => send('menu-tab', 0) },
        { label: 'Tab 2', accelerator: 'CmdOrCtrl+2', click: () => send('menu-tab', 1) },
        { label: 'Tab 3', accelerator: 'CmdOrCtrl+3', click: () => send('menu-tab', 2) },
        { label: 'Tab 4', accelerator: 'CmdOrCtrl+4', click: () => send('menu-tab', 3) },
        { label: 'Tab 5', accelerator: 'CmdOrCtrl+5', click: () => send('menu-tab', 4) },
      ]
    },
    {
      label: '?',
      submenu: [
        { label: 'About Note++', click: () => send('menu-about') },
        { type: 'separator' },
        { label: 'Keyboard Shortcuts Reference', click: () => send('menu-shortcuts-ref') },
        { type: 'separator' },
        {
          label: 'Check for Updates Automatically',
          type: 'checkbox',
          checked: isAutoUpdateEnabled(),
          click: (item) => {
            const s = readSettings();
            s.autoUpdate = !!item.checked;
            writeSettings(s);
            scheduleAutoUpdateCheck();
            send('auto-update-pref-changed', !!item.checked);
          },
        },
        { label: 'Check for Updates Now', click: async () => {
            if (!app.isPackaged) {
              dialog.showMessageBox(mainWindow, { type: 'info', message: 'Auto-update only runs in the installed build.' });
              return;
            }
            initAutoUpdater();
            try { await autoUpdater?.checkForUpdates(); } catch (e) { console.error(e); }
          },
        },
        { type: 'separator' },
        { label: 'Developer Tools', accelerator: 'F12', click: () => mainWindow.webContents.toggleDevTools() },
      ]
    }
  ];

  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

function send(channel, ...args) {
  if (mainWindow) mainWindow.webContents.send(channel, ...args);
}

// IPC handlers
// Renderer signals it's done wiring listeners — flush any files queued for it
ipcMain.handle('renderer-ready', () => {
  rendererReady = true;
  flushPendingOpens();
  return { success: true };
});

// Renderer notifies main when it has successfully opened a file from disk so
// we can add it to the Recent Files list and refresh the menu.
ipcMain.handle('recent-file-opened',  (e, filePath) => { addRecentFile(filePath); return { success: true }; });
ipcMain.handle('recent-files-get',    () => getRecentFiles());
ipcMain.handle('recent-files-clear',  () => { clearRecentFiles(); return { success: true }; });

ipcMain.handle('dialog-open', async (e, opts) => dialog.showOpenDialog(mainWindow, opts));
ipcMain.handle('dialog-save', async (e, opts) => dialog.showSaveDialog(mainWindow, opts));
ipcMain.handle('dialog-message', async (e, opts) => dialog.showMessageBox(mainWindow, opts));

ipcMain.handle('read-file', async (e, filePath) => {
  try { return { success: true, content: fs.readFileSync(filePath, 'utf-8') }; }
  catch (err) { return { success: false, error: err.message }; }
});

ipcMain.handle('write-file', async (e, filePath, content) => {
  try {
    const dir = path.dirname(filePath);
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    fs.writeFileSync(filePath, content, 'utf-8');
    return { success: true };
  }
  catch (err) { return { success: false, error: err.message }; }
});

// ---- Backup ----
ipcMain.handle('backup-files', async (e, files, opts) => {
  try {
    const backupRoot = (opts.backupPath && opts.backupPath.trim())
      ? opts.backupPath.trim()
      : path.join(app.getPath('userData'), 'backups');

    const ts = new Date().toISOString().slice(0, 19).replace(/:/g, '-'); // 2025-05-11T14-30-00
    const keepN = Math.max(1, opts.versionsToKeep || 5);
    let count = 0;

    for (const file of files) {
      if (!file.filePath || !file.content) continue;
      const base = path.basename(file.filePath);
      const ext  = path.extname(base);
      const stem = path.basename(base, ext);
      const dir  = path.join(backupRoot, base);
      if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });

      fs.writeFileSync(path.join(dir, `${stem}_${ts}${ext}`), file.content, 'utf-8');
      count++;

      // Prune old versions — keep only the N most recent
      const versions = fs.readdirSync(dir)
        .filter(f => f.startsWith(stem + '_'))
        .sort()
        .reverse();
      versions.slice(keepN).forEach(v => {
        try { fs.unlinkSync(path.join(dir, v)); } catch {}
      });
    }

    return { success: true, count, backupRoot };
  } catch (err) { return { success: false, error: err.message }; }
});

ipcMain.handle('get-backup-root', (e, customPath) => {
  return (customPath && customPath.trim())
    ? customPath.trim()
    : path.join(app.getPath('userData'), 'backups');
});

ipcMain.handle('write-file-binary', async (e, filePath, base64content) => {
  try {
    const buf = Buffer.from(base64content, 'base64');
    fs.writeFileSync(filePath, buf);
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('list-dir', async (e, dirPath) => {
  try {
    return fs.readdirSync(dirPath, { withFileTypes: true })
      .map(e => ({ name: e.name, isDir: e.isDirectory() }));
  } catch { return []; }
});

ipcMain.handle('set-title', (e, title) => mainWindow.setTitle(title));
ipcMain.handle('close-window', () => app.exit(0));
ipcMain.handle('shell-open', (e, p) => shell.openPath(p));
ipcMain.handle('shell-show-item', (e, p) => shell.showItemInFolder(p));
ipcMain.handle('get-user-data-path', () => app.getPath('userData'));

// ---- Settings (persisted app preferences) ----
function settingsPath() { return path.join(app.getPath('userData'), 'settings.json'); }

function readSettings() {
  try {
    if (fs.existsSync(settingsPath())) return JSON.parse(fs.readFileSync(settingsPath(), 'utf-8'));
  } catch {}
  return {};
}

function writeSettings(data) {
  try { fs.writeFileSync(settingsPath(), JSON.stringify(data, null, 2), 'utf-8'); } catch {}
}

ipcMain.handle('read-settings', () => readSettings());
ipcMain.handle('write-settings', (e, data) => { writeSettings(data); return { success: true }; });

// ---- Cloud storage detection ----
function detectCloudPaths() {
  const home = process.env.USERPROFILE || process.env.HOME || '';
  const results = {};

  const candidates = {
    onedrive: [
      process.env.OneDrive,
      process.env.OneDriveConsumer,
      path.join(home, 'OneDrive'),
    ],
    googledrive: [
      path.join(home, 'Google Drive'),
      path.join(home, 'My Drive'),
      path.join(home, 'GoogleDrive'),
      // Google Drive for Desktop mounts here by default on Windows
      'G:\\My Drive',
      'H:\\My Drive',
    ],
    dropbox: [
      process.env.DROPBOX,
      path.join(home, 'Dropbox'),
    ],
  };

  // Also check Google Drive registry key (Drive for Desktop)
  try {
    const { execSync } = require('child_process');
    const reg = execSync('reg query "HKCU\\Software\\Google\\DriveFS" /v PerAccountPreferences 2>nul', { encoding: 'utf-8' });
    const match = reg.match(/rootPrefs.*?mount_point_path.*?"([^"]+)"/s);
    if (match) candidates.googledrive.unshift(match[1]);
  } catch {}

  for (const [key, paths] of Object.entries(candidates)) {
    for (const p2 of paths) {
      if (p2 && fs.existsSync(p2)) { results[key] = p2; break; }
    }
  }
  return results;
}

ipcMain.handle('detect-cloud-paths', () => detectCloudPaths());

ipcMain.handle('validate-path', (e, p2) => ({ exists: p2 ? fs.existsSync(p2) : false }));

ipcMain.handle('open-folder-picker', async () => {
  const r = await dialog.showOpenDialog(mainWindow, { properties: ['openDirectory'] });
  return r.canceled ? null : r.filePaths[0];
});

// ---- Session (AppData + cloud) ----
function writeSessionFile(dir, data) {
  if (!dir) return;
  try {
    const target = path.join(dir, 'Note++');
    if (!fs.existsSync(target)) fs.mkdirSync(target, { recursive: true });
    fs.writeFileSync(path.join(target, 'session.json'), JSON.stringify(data), 'utf-8');
  } catch {}
}

ipcMain.handle('write-session', async (e, data) => {
  try {
    // 1. Always write to AppData
    const localDir = path.join(app.getPath('userData'), 'autosave');
    if (!fs.existsSync(localDir)) fs.mkdirSync(localDir, { recursive: true });
    fs.writeFileSync(path.join(localDir, 'session.json'), JSON.stringify(data), 'utf-8');

    // 2. Write to each enabled cloud folder
    const settings = readSettings();
    const cloud = settings.cloud || {};
    for (const [, cfg] of Object.entries(cloud)) {
      if (cfg.enabled && cfg.path) writeSessionFile(cfg.path, data);
    }

    return { success: true };
  } catch (err) { return { success: false, error: err.message }; }
});

ipcMain.handle('read-session', async () => {
  try {
    const file = path.join(app.getPath('userData'), 'autosave', 'session.json');
    if (!fs.existsSync(file)) return { success: false };
    return { success: true, data: JSON.parse(fs.readFileSync(file, 'utf-8')) };
  } catch { return { success: false }; }
});

// ── Terminal IPC (node-pty for true PTY; fallback to child_process) ──────────
function getShell(settings) {
  const saved = settings?.terminal?.shell;
  if (saved && saved.trim()) return saved.trim();
  return process.platform === 'win32' ? 'powershell.exe' : (process.env.SHELL || 'bash');
}

ipcMain.handle('terminal-create', async (e, id, opts) => {
  const settings = readSettings();
  const sh   = getShell(settings);
  const cols  = opts?.cols  || 80;
  const rows  = opts?.rows  || 24;
  const cwd   = process.env.USERPROFILE || process.env.HOME || '.';

  try {
    if (pty) {
      // ── True PTY via node-pty ──────────────────────────────────────────────
      const isWin = process.platform === 'win32';
      const ptyArgs = isWin ? ['-NoLogo'] : [];
      const proc = pty.spawn(sh, ptyArgs, {
        name: 'xterm-256color',
        cols, rows, cwd,
        env: process.env,
      });

      terminalProcesses.set(id, proc);
      proc.onData(d   => send('terminal-output', id, d));
      proc.onExit(({ exitCode }) => { send('terminal-exit', id, exitCode); terminalProcesses.delete(id); });
      return { success: true, pid: proc.pid, pty: true };
    } else {
      // ── Fallback: plain child_process ─────────────────────────────────────
      const isWin = process.platform === 'win32';
      const args  = isWin ? ['-NoLogo'] : [];
      const proc  = spawn(sh, args, {
        env: { ...process.env, TERM: 'xterm-256color' },
        cwd,
        stdio: ['pipe', 'pipe', 'pipe'],
        windowsHide: true,
      });
      terminalProcesses.set(id, proc);
      proc.stdout.on('data', d => send('terminal-output', id, d.toString()));
      proc.stderr.on('data', d => send('terminal-output', id, d.toString()));
      proc.on('exit', code => { send('terminal-exit', id, code); terminalProcesses.delete(id); });
      proc.on('error', err => send('terminal-output', id, `\r\nError: ${err.message}\r\n`));
      return { success: true, pid: proc.pid, pty: false };
    }
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('terminal-input', (e, id, data) => {
  const proc = terminalProcesses.get(id);
  if (!proc) return;
  if (pty && proc.write) {
    proc.write(data);           // node-pty
  } else if (proc.stdin && !proc.stdin.destroyed) {
    proc.stdin.write(data);     // child_process fallback
  }
});

ipcMain.handle('terminal-kill', (e, id) => {
  const proc = terminalProcesses.get(id);
  if (!proc) return;
  if (pty && proc.kill) proc.kill();
  else try { proc.kill(); } catch {}
  terminalProcesses.delete(id);
});

ipcMain.handle('terminal-resize', (e, id, cols, rows) => {
  const proc = terminalProcesses.get(id);
  if (proc && pty && proc.resize) {
    try { proc.resize(Math.max(1, cols), Math.max(1, rows)); } catch {}
  }
});

// ── Project context (AGENTS.md + .notepp/memory.md) ──────────────────────
// Walk up from a file path looking for context files the AI should always see.
// AGENTS.md is the cross-vendor standard adopted by Cursor / Codex / Copilot /
// Cline / Codex / Jules / Gemini etc. (60k+ repos). .notepp/memory.md is our
// own per-project instructions file (analogous to CLAUDE.md or .cursorrules).
ipcMain.handle('project-context-find', (e, startPath) => {
  if (!startPath) return { agentsMd: null, memoryMd: null };
  let dir = startPath;
  try {
    const stat = fs.statSync(dir);
    if (stat.isFile()) dir = path.dirname(dir);
  } catch { return { agentsMd: null, memoryMd: null }; }
  const result = { agentsMd: null, agentsMdPath: null, memoryMd: null, memoryMdPath: null };
  while (true) {
    if (!result.agentsMd) {
      const p = path.join(dir, 'AGENTS.md');
      if (fs.existsSync(p)) {
        try { result.agentsMd = fs.readFileSync(p, 'utf-8'); result.agentsMdPath = p; } catch {}
      }
    }
    if (!result.memoryMd) {
      const p = path.join(dir, '.notepp', 'memory.md');
      if (fs.existsSync(p)) {
        try { result.memoryMd = fs.readFileSync(p, 'utf-8'); result.memoryMdPath = p; } catch {}
      }
    }
    // Stop at .git boundary or filesystem root — no point walking past the repo
    if (fs.existsSync(path.join(dir, '.git'))) break;
    const parent = path.dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return result;
});

// Ensure .notepp/memory.md exists for the given workspace, then return its path.
// Used by the "Edit project memory" button.
ipcMain.handle('project-memory-ensure', (e, repoOrFolder) => {
  if (!repoOrFolder) return { success: false, error: 'no folder' };
  try {
    const dir = path.join(repoOrFolder, '.notepp');
    if (!fs.existsSync(dir)) fs.mkdirSync(dir, { recursive: true });
    const file = path.join(dir, 'memory.md');
    if (!fs.existsSync(file)) {
      fs.writeFileSync(file,
        '# Note++ Project Memory\n\n' +
        '> The AI Assistant always reads this file when this project is open.\n' +
        '> Use it for project-specific instructions, conventions, terminology,\n' +
        '> known gotchas, or anything you want the AI to remember.\n\n' +
        '## Conventions\n\n- (e.g. "Use 2-space indents", "Prefer arrow functions", "All API calls go through src/api/")\n\n' +
        '## Glossary\n\n- (e.g. "OBO = On-Behalf-Of OAuth flow")\n', 'utf-8');
    }
    return { success: true, path: file };
  } catch (err) { return { success: false, error: err.message }; }
});

// ── Folder compare via dir-compare ───────────────────────────────────────
// Returns a flat list of entries with status:
//   added    — only on right
//   removed  — only on left
//   modified — in both, content differs
//   equal    — in both, identical
// Comparison: name + size + content-hash (dir-compare's `compareContent`)
ipcMain.handle('compare-folders', async (e, { left, right }) => {
  if (!left || !right) return { success: false, error: 'Both paths required' };
  try {
    const dirCompare = require('dir-compare');
    const SKIP = [
      'node_modules', '.git', '.svn', '.hg', 'dist', 'build', 'out',
      '.next', '.cache', '.vscode', '.idea', '__pycache__',
    ];
    const options = {
      compareContent: true,
      compareSize: true,
      compareDate: false,
      excludeFilter: SKIP.map(s => '/**/' + s).join(','),
      skipSymlinks: true,
    };
    const res = await dirCompare.compare(left, right, options);
    // Map dir-compare's `diffSet` to our flat schema
    const entries = (res.diffSet || []).map(d => ({
      // Path relative to the compared root (uses forward slashes regardless of platform)
      relPath: (d.relativePath || '').replace(/^[\\/]/, '') + (d.relativePath && !d.relativePath.endsWith('/') && !d.relativePath.endsWith('\\') ? '/' : ''),
      name:    d.name1 || d.name2 || '',
      isDir:   d.type1 === 'directory' || d.type2 === 'directory',
      leftPath:  d.path1 && d.name1 ? path.join(d.path1, d.name1) : null,
      rightPath: d.path2 && d.name2 ? path.join(d.path2, d.name2) : null,
      // dir-compare's `state`: 'equal' | 'left' | 'right' | 'distinct'
      status: d.state === 'equal'  ? 'equal'
            : d.state === 'left'   ? 'removed'   // only on left → removed from right
            : d.state === 'right'  ? 'added'     // only on right → added vs left
            : 'modified',
      sizeLeft:  d.size1,
      sizeRight: d.size2,
    }));
    return {
      success: true,
      entries,
      summary: {
        equalCount:    res.equal,
        distinctCount: res.distinct,
        leftOnlyCount: res.left,
        rightOnlyCount: res.right,
        totalDirs:     res.totalDirs,
        totalFiles:    res.totalFiles,
      },
    };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// ── Find in Files — recursive search across a folder ────────────────────
// Filter is a comma-separated glob list ("*.js,*.md"). "*.*" means any.
// Returns { success, files: [{ path, hits: [{ line, col, content }] }], totalHits }
ipcMain.handle('find-in-files', async (e, { root, pattern, filter, matchCase, isRegex, wholeWord }) => {
  if (!root) return { success: false, error: 'No directory' };
  if (!pattern) return { success: false, error: 'No pattern' };
  try {
    const globs = (filter || '*.*').split(',').map(s => s.trim()).filter(Boolean);
    const wantAny = globs.includes('*.*') || globs.includes('*');
    // Build a simple regex per glob: *.js → \.js$, *.md → \.md$
    const globRegexes = wantAny ? null : globs.map(g => {
      const escaped = g.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
      return new RegExp('^' + escaped + '$', 'i');
    });
    const matchesGlob = (name) => wantAny || globRegexes.some(r => r.test(name));

    // Build the search regex
    let re;
    try {
      const flags = matchCase ? 'g' : 'gi';
      if (isRegex) {
        re = new RegExp(pattern, flags);
      } else {
        let escaped = pattern.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
        if (wholeWord) escaped = '\\b' + escaped + '\\b';
        re = new RegExp(escaped, flags);
      }
    } catch (err) {
      return { success: false, error: 'Bad regex: ' + err.message };
    }

    const results = [];
    let totalHits = 0;
    const MAX_FILES = 5000;
    const MAX_TOTAL_HITS = 5000;
    let filesScanned = 0;

    // Folders we never recurse into — saves time + avoids choking on huge dirs
    const SKIP_DIRS = new Set([
      'node_modules', '.git', '.svn', '.hg', 'dist', 'build', 'out',
      '.next', '.cache', '.vscode', '.idea', '__pycache__',
    ]);

    function walk(dir) {
      if (filesScanned >= MAX_FILES || totalHits >= MAX_TOTAL_HITS) return;
      let entries;
      try { entries = fs.readdirSync(dir, { withFileTypes: true }); }
      catch { return; }
      for (const ent of entries) {
        if (filesScanned >= MAX_FILES || totalHits >= MAX_TOTAL_HITS) return;
        const full = path.join(dir, ent.name);
        if (ent.isDirectory()) {
          if (SKIP_DIRS.has(ent.name)) continue;
          walk(full);
        } else if (ent.isFile() && matchesGlob(ent.name)) {
          filesScanned++;
          // Skip very large files (>2 MB) — likely binary or generated
          let stat;
          try { stat = fs.statSync(full); } catch { continue; }
          if (stat.size > 2 * 1024 * 1024) continue;
          let text;
          try { text = fs.readFileSync(full, 'utf-8'); } catch { continue; }
          // Quick binary check — bail if a NUL byte in the first 4 KB
          if (text.slice(0, 4096).indexOf('\0') >= 0) continue;
          const lines = text.split(/\r?\n/);
          const hits = [];
          for (let i = 0; i < lines.length; i++) {
            re.lastIndex = 0;
            let m;
            while ((m = re.exec(lines[i])) !== null) {
              hits.push({ line: i + 1, col: m.index + 1, content: lines[i] });
              totalHits++;
              if (totalHits >= MAX_TOTAL_HITS) break;
              if (m.index === re.lastIndex) re.lastIndex++; // avoid zero-length loop
            }
            if (totalHits >= MAX_TOTAL_HITS) break;
          }
          if (hits.length) results.push({ path: full, hits });
        }
      }
    }

    walk(root);
    return {
      success: true,
      files: results,
      totalHits,
      filesScanned,
      capped: filesScanned >= MAX_FILES || totalHits >= MAX_TOTAL_HITS,
    };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// ── Git integration (CLI wrapper) ─────────────────────────────────────────
// All handlers delegate to src/git-service.js. See features/GIT.md for the design.
const gitService = require('./git-service');

ipcMain.handle('git-find-repo',     (e, p)             => gitService.findRepoRoot(p));
ipcMain.handle('git-status',        (e, root)          => gitService.status(root));
ipcMain.handle('git-stage',         (e, root, paths)   => gitService.stage(root, paths));
ipcMain.handle('git-unstage',       (e, root, paths)   => gitService.unstage(root, paths));
ipcMain.handle('git-discard',       (e, root, paths)   => gitService.discard(root, paths));
ipcMain.handle('git-clean',         (e, root, paths)   => gitService.cleanUntracked(root, paths));
ipcMain.handle('git-commit',        (e, root, msg)     => gitService.commit(root, msg));
ipcMain.handle('git-commit-amend',  (e, root, msg)     => gitService.commitAmend(root, msg));
ipcMain.handle('git-fetch',         (e, root)          => gitService.fetch(root));
ipcMain.handle('git-pull',          (e, root)          => gitService.pull(root));
ipcMain.handle('git-push',          (e, root)          => gitService.push(root));
ipcMain.handle('git-push-upstream', (e, root, r2, b)   => gitService.pushSetUpstream(root, r2, b));
ipcMain.handle('git-sync',          (e, root)          => gitService.sync(root));
ipcMain.handle('git-branch-list',   (e, root)          => gitService.branchList(root));
ipcMain.handle('git-branch-switch', (e, root, name)    => gitService.branchSwitch(root, name));
ipcMain.handle('git-branch-create', (e, root, n, f)    => gitService.branchCreate(root, n, f));
ipcMain.handle('git-installed',     ()                 => gitService.isGitInstalled());
ipcMain.handle('git-show-head',     (e, root, rel)     => gitService.showHead(root, rel));

// ── draw.io service (on-demand download, persistent bundle) ────────────────
const drawioService = require('./drawio-service');

ipcMain.handle('drawio:status',    () => drawioService.getStatus());
ipcMain.handle('drawio:download',  async () => {
  try { return await drawioService.download(); }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('drawio:uninstall', () => drawioService.uninstall());

// ── LSP service ──────────────────────────────────────────────────────────
const lspService = require('./lsp-service');

ipcMain.handle('lsp-ensure-started', async (e, { langId, workspaceRoot }) =>
  lspService.ensureStarted(langId, workspaceRoot, e.sender));

ipcMain.handle('lsp-send', async (e, { langId, method, params }) => {
  try {
    const result = await lspService.sendRequest(langId, method, params);
    return { success: true, result };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('lsp-notify', (e, { langId, method, params }) => {
  lspService.sendNotification(langId, method, params);
  return { success: true };
});

ipcMain.handle('lsp-stop', (e, { langId }) => lspService.stopServer(langId));

ipcMain.handle('lsp-language-for', (e, monacoId) => lspService.lookupByMonacoId(monacoId));
ipcMain.handle('lsp-language-config', (e, langId) => lspService.getLanguageConfig(langId));

app.on('before-quit', () => { lspService.stopAllServers(); });

// ── App version (single source of truth: package.json) ───────────────────
ipcMain.handle('get-app-version', () => app.getVersion());

// ── LSP install — spawn `npm install -g <pkg>` and stream output to renderer ─
// Used by the click-to-install status pill when a server is missing.
ipcMain.handle('lsp-install', async (e, { langId }) => {
  const cfg = lspService.getLanguageConfig(langId);
  if (!cfg || !cfg.install) return { success: false, error: 'No install recipe for ' + langId };

  // cfg.install.cmd is e.g. "npm install -g pyright" — split into argv.
  // We invoke through cmd.exe on Windows so the npm.cmd shim is resolved.
  const parts = cfg.install.cmd.split(/\s+/);
  const isWin = process.platform === 'win32';
  const bin   = isWin ? 'cmd.exe' : parts[0];
  const args  = isWin ? ['/c', ...parts] : parts.slice(1);

  return new Promise((resolve) => {
    let proc;
    try {
      proc = spawn(bin, args, { windowsHide: true, env: process.env });
    } catch (err) {
      return resolve({ success: false, error: err.message });
    }
    let out = '', err = '';
    proc.stdout?.on('data', d => { const s = d.toString(); out += s; send('lsp-install-output', { langId, chunk: s }); });
    proc.stderr?.on('data', d => { const s = d.toString(); err += s; send('lsp-install-output', { langId, chunk: s }); });
    proc.on('error', e2 => resolve({ success: false, error: e2.message }));
    proc.on('exit', code => {
      send('lsp-install-done', { langId, exitCode: code });
      resolve({ success: code === 0, exitCode: code, stdout: out, stderr: err });
    });
  });
});

// ── Auto-updater (electron-updater + GitHub provider) ────────────────────
// We only run the updater on packaged builds. The renderer toggles the
// preference through `set-auto-update`; we re-check on every change so
// the menu/setting takes effect without a restart.
let autoUpdater = null;
let updateDownloaded = false;
let pendingUpdateVersion = null;     // version string we're about to install
let updaterWindow = null;            // small window shown during install
let updateInstallStarted = false;    // guards against re-entry
let updateCheckTimer = null;

function initAutoUpdater() {
  if (!app.isPackaged) return; // never runs in `npm start`
  if (autoUpdater) return;
  try {
    ({ autoUpdater } = require('electron-updater'));
  } catch (err) {
    console.error('[auto-update] electron-updater not available:', err.message);
    return;
  }
  autoUpdater.autoDownload = true;
  // We handle install-on-quit manually so we can show the updater window
  // before quitAndInstall fires (autoInstallOnAppQuit would skip the UI).
  autoUpdater.autoInstallOnAppQuit = false;
  autoUpdater.on('error', (e) => console.error('[auto-update] error:', e?.message || e));
  autoUpdater.on('checking-for-update', () => send('auto-update-status', { state: 'checking' }));
  autoUpdater.on('update-available', (info) => send('auto-update-status', { state: 'available', version: info?.version }));
  autoUpdater.on('update-not-available', () => send('auto-update-status', { state: 'none' }));
  autoUpdater.on('download-progress', (p) => send('auto-update-status', { state: 'downloading', percent: p?.percent }));
  autoUpdater.on('update-downloaded', (info) => {
    updateDownloaded = true;
    pendingUpdateVersion = info?.version || null;
    send('auto-update-status', { state: 'downloaded', version: info?.version });
  });
}

// Small frameless window that visualises the install. Opened from the
// main-window 'close' handler when an update is pending. On its own, it
// just shows progress → "Upgraded to vX.Y.Z" → calls quitAndInstall via
// IPC, which tears down the process and relaunches the new build.
function openUpdaterWindow() {
  if (updaterWindow) { updaterWindow.focus(); return; }
  updaterWindow = new BrowserWindow({
    width: 460,
    height: 240,
    resizable: false,
    minimizable: true,           // user can minimize to keep working in other apps
    maximizable: false,
    fullscreenable: false,
    frame: false,                // custom in-window controls (see updater.html)
    alwaysOnTop: true,
    center: true,
    skipTaskbar: false,
    title: 'Updating Note++',
    backgroundColor: '#1e1e2e',
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'updater-preload.js'),
    },
  });
  updaterWindow.setMenuBarVisibility(false);
  updaterWindow.loadFile(path.join(__dirname, 'updater.html'));
  updaterWindow.on('closed', () => {
    updaterWindow = null;
    // If the updater window closed WITHOUT the install having been kicked
    // off (e.g. user Alt+F4'd it), restore the main window so they don't
    // get stuck with a tray-less, hidden app. They can try closing again
    // later to retry, or install the update on next launch automatically.
    if (!updateInstallStarted && mainWindow && !mainWindow.isDestroyed()) {
      try { mainWindow.show(); } catch {}
    }
  });
}

ipcMain.handle('updater:get-pending-version', () => pendingUpdateVersion || app.getVersion());

ipcMain.handle('updater:minimize', () => {
  if (updaterWindow && !updaterWindow.isDestroyed()) updaterWindow.minimize();
});

ipcMain.handle('updater:install-and-quit', () => {
  if (updateInstallStarted) return { success: true, alreadyStarted: true };
  updateInstallStarted = true;
  // isSilent: true   → run the NSIS installer with no UI
  // isForceRunAfter: true → relaunch Note++ once install completes
  try {
    if (autoUpdater) autoUpdater.quitAndInstall(true, true);
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// User clicked the "Click to update" pill in the renderer toolbar. We:
//   1. Hide the main window so it feels closed
//   2. Open the updater progress window (the same one the old close-hook
//      used to invoke) — it shows progress → success → triggers
//      quitAndInstall via the IPC above
ipcMain.handle('updater:start-install-flow', () => {
  if (!updateDownloaded) return { success: false, error: 'no-update-pending' };
  if (updateInstallStarted || updaterWindow) return { success: true, alreadyStarted: true };
  try { if (mainWindow) mainWindow.hide(); } catch {}
  openUpdaterWindow();
  return { success: true };
});

function isAutoUpdateEnabled() {
  const s = readSettings();
  // Default ON when never set
  return s.autoUpdate !== false;
}

function scheduleAutoUpdateCheck() {
  if (!app.isPackaged) return;
  initAutoUpdater();
  if (!autoUpdater) return;
  if (updateCheckTimer) { clearInterval(updateCheckTimer); updateCheckTimer = null; }
  if (!isAutoUpdateEnabled()) return;
  // Kick a check shortly after launch, then every 6 h
  setTimeout(() => { try { autoUpdater.checkForUpdates(); } catch {} }, 8000);
  updateCheckTimer = setInterval(() => {
    try { autoUpdater.checkForUpdates(); } catch {}
  }, 6 * 60 * 60 * 1000);
}

ipcMain.handle('set-auto-update', (e, enabled) => {
  const s = readSettings();
  s.autoUpdate = !!enabled;
  writeSettings(s);
  scheduleAutoUpdateCheck();
  return { success: true };
});

ipcMain.handle('get-auto-update', () => isAutoUpdateEnabled());

ipcMain.handle('check-for-updates-now', async () => {
  if (!app.isPackaged) return { success: false, error: 'not-packaged' };
  initAutoUpdater();
  if (!autoUpdater) return { success: false, error: 'updater-unavailable' };
  try {
    const r = await autoUpdater.checkForUpdates();
    return { success: true, version: r?.updateInfo?.version || null };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// No close-time install hook — closing the app is now just closing the app.
// Installing an update is an explicit user gesture: click the "Click to
// update" pill in the renderer toolbar, which calls
// `updater:start-install-flow` (defined above).

app.whenReady().then(() => { scheduleAutoUpdateCheck(); });

// ── AI Assistant (Ollama) ─────────────────────────────────────────────────
const http = require('http');
let currentAiReq = null; // active streaming request (for abort)

function ollamaRequest(path2, method, body, onChunk) {
  return new Promise((resolve, reject) => {
    const bodyStr = body ? JSON.stringify(body) : '';
    const opts = {
      hostname: '127.0.0.1', port: 11434,
      path: path2, method,
      headers: { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(bodyStr) }
    };
    const req = http.request(opts, (res) => {
      let buf = '';
      res.on('data', chunk => {
        buf += chunk.toString();
        const lines = buf.split('\n');
        buf = lines.pop(); // keep incomplete line
        for (const line of lines) {
          if (!line.trim()) continue;
          try { onChunk && onChunk(JSON.parse(line)); } catch {}
        }
      });
      res.on('end', () => {
        if (buf.trim()) { try { onChunk && onChunk(JSON.parse(buf)); } catch {} }
        resolve();
      });
      res.on('error', reject);
    });
    req.on('error', reject);
    if (bodyStr) req.write(bodyStr);
    req.end();
    currentAiReq = req;
  });
}

// Check if Ollama is running + list installed models
ipcMain.handle('ai-check', async () => {
  try {
    const data = await new Promise((resolve, reject) => {
      const req = http.get('http://127.0.0.1:11434/api/tags', (res) => {
        let raw = '';
        res.on('data', d => raw += d);
        res.on('end', () => { try { resolve(JSON.parse(raw)); } catch { resolve({}); } });
      });
      req.on('error', reject);
      req.setTimeout(2000, () => { req.destroy(); reject(new Error('timeout')); });
    });
    const models = (data.models || []).map(m => m.name);
    return { running: true, models };
  } catch {
    return { running: false, models: [] };
  }
});

// Stream generation (legacy single-shot endpoint) — tokens via 'ai-token'
ipcMain.handle('ai-generate', async (event, { model, prompt, system }) => {
  try {
    if (currentAiReq) { try { currentAiReq.destroy(); } catch {} currentAiReq = null; }
    await ollamaRequest('/api/generate', 'POST',
      { model, prompt, system, stream: true },
      (d) => {
        if (d.response) event.sender.send('ai-token', d.response);
        if (d.done)     event.sender.send('ai-done');
      }
    );
    return { success: true };
  } catch (err) {
    event.sender.send('ai-done');
    return { success: false, error: err.message };
  }
});

// Multi-turn chat using Ollama's /api/chat endpoint. `messages` is an array
// of { role: 'system' | 'user' | 'assistant', content: string }. Streams
// reply tokens via the same 'ai-token' / 'ai-done' channel as ai-generate
// so the renderer can use a single set of listeners.
ipcMain.handle('ai-chat', async (event, { model, messages }) => {
  try {
    if (currentAiReq) { try { currentAiReq.destroy(); } catch {} currentAiReq = null; }
    await ollamaRequest('/api/chat', 'POST',
      { model, messages, stream: true },
      (d) => {
        // /api/chat shape: { message: { role, content }, done }
        if (d.message?.content) event.sender.send('ai-token', d.message.content);
        if (d.done)             event.sender.send('ai-done');
      }
    );
    return { success: true };
  } catch (err) {
    event.sender.send('ai-done');
    return { success: false, error: err.message };
  }
});

// Abort current generation
ipcMain.handle('ai-abort', () => {
  if (currentAiReq) { try { currentAiReq.destroy(); } catch {} currentAiReq = null; }
  return { success: true };
});

// Detect whether the `ollama` binary is on PATH (or in the standard Windows
// install location). Returns { installed: bool, version: string | null,
// path: string | null }.
ipcMain.handle('ai-detect-installed', async () => {
  // Try common install locations on top of $PATH so detection works even
  // when the installer didn't reach the current shell's PATH yet.
  const candidates = [
    'ollama', // PATH lookup (any platform)
    process.platform === 'win32' && process.env.LOCALAPPDATA
      ? path.join(process.env.LOCALAPPDATA, 'Programs', 'Ollama', 'ollama.exe')
      : null,
    process.platform === 'darwin' ? '/usr/local/bin/ollama' : null,
    process.platform === 'darwin' ? '/opt/homebrew/bin/ollama' : null,
    process.platform === 'linux' ? '/usr/local/bin/ollama' : null,
    process.platform === 'linux' ? '/usr/bin/ollama' : null,
  ].filter(Boolean);

  for (const cand of candidates) {
    const found = await new Promise((resolve) => {
      try {
        const proc = spawn(cand, ['--version'], { windowsHide: true });
        let out = '';
        proc.stdout.on('data', d => out += d.toString());
        proc.on('error', () => resolve(null));
        proc.on('close', (code) => {
          if (code === 0) resolve({ path: cand, version: out.trim() });
          else resolve(null);
        });
      } catch { resolve(null); }
    });
    if (found) return { installed: true, version: found.version, path: found.path };
  }
  return { installed: false, version: null, path: null };
});

// Spawn `ollama serve` detached so it lives independent of Note++. Returns
// { success, pid } or { success: false, error }. Does NOT wait for the
// HTTP server to come up — caller polls `ai-check` until it's ready.
let ollamaServerProc = null;
ipcMain.handle('ai-start-server', async (e, ollamaPath) => {
  if (ollamaServerProc && !ollamaServerProc.killed) {
    return { success: true, pid: ollamaServerProc.pid, alreadyRunning: true };
  }
  try {
    const bin = ollamaPath || 'ollama';
    const proc = spawn(bin, ['serve'], {
      detached: true,
      stdio: 'ignore',
      windowsHide: true,
      env: process.env,
    });
    proc.unref(); // let it outlive Note++
    ollamaServerProc = proc;
    return { success: true, pid: proc.pid };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Pull (download) a model — progress events sent via 'ai-pull-progress'
ipcMain.handle('ai-pull', async (event, model) => {
  try {
    await ollamaRequest('/api/pull', 'POST', { name: model, stream: true }, (d) => {
      event.sender.send('ai-pull-progress', {
        model,
        status:    d.status || '',
        total:     d.total     || 0,
        completed: d.completed || 0,
        done: d.status === 'success'
      });
    });
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Open URL in default browser
ipcMain.handle('open-url', (e, url) => { shell.openExternal(url); });

ipcMain.handle('run-command', async (e, cmd, cwd) => {
  return new Promise((resolve) => {
    const isWin = process.platform === 'win32';
    const proc = spawn(isWin ? 'cmd.exe' : 'bash',
      isWin ? ['/c', cmd] : ['-c', cmd],
      { cwd: cwd || '.', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true }
    );
    let out = '', err = '';
    proc.stdout.on('data', d => { out += d; send('terminal-output', 0, d.toString()); });
    proc.stderr.on('data', d => { err += d; send('terminal-output', 0, '\x1b[31m' + d.toString() + '\x1b[0m'); });
    proc.on('exit', code => resolve({ exitCode: code, stdout: out, stderr: err }));
  });
});

async function handleOpen() {
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openFile', 'multiSelections'],
    filters: [
      { name: 'All Files', extensions: ['*'] },
      { name: 'Text Files', extensions: ['txt', 'log', 'md'] },
      { name: 'Source Files', extensions: ['js', 'ts', 'py', 'java', 'c', 'cpp', 'cs', 'go', 'rs', 'rb', 'php', 'swift', 'kt'] },
      { name: 'Web Files', extensions: ['html', 'htm', 'css', 'scss', 'xml', 'json'] },
    ]
  });
  if (!result.canceled) send('open-files', result.filePaths);
}

async function handleOpenFolder() {
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openDirectory']
  });
  if (!result.canceled) send('open-folder', result.filePaths[0]);
}

app.whenReady().then(() => {
  if (gotTheLock) createWindow();
  // Register the drawio:// custom protocol now that app is ready. Safe to
  // call even if the bundle isn't downloaded yet — protocol just returns
  // ENOENT for any request until the user triggers the download.
  try { drawioService.registerProtocol(); } catch (e) { console.error('[drawio] protocol register failed:', e); }
});
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
app.on('quit', () => { terminalProcesses.forEach(p => p.kill()); });
