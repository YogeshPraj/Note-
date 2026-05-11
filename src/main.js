const { app, BrowserWindow, Menu, dialog, ipcMain, shell } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

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
    if (file && mainWindow) mainWindow.webContents.send('open-files', [file]);
  });
}

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
    // Open file passed via command line (e.g. double-click or "Open with")
    const file = fileFromArgv(process.argv);
    if (file) mainWindow.webContents.send('open-files', [file]);
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

function buildMenu() {
  const template = [
    {
      label: 'File',
      submenu: [
        { label: 'New', accelerator: 'CmdOrCtrl+N', click: () => send('menu-new') },
        { type: 'separator' },
        { label: 'Open...', accelerator: 'CmdOrCtrl+O', click: () => handleOpen() },
        { label: 'Open Folder...', accelerator: 'CmdOrCtrl+Shift+O', click: () => handleOpenFolder() },
        { label: 'Open Recent', submenu: [{ label: 'No recent files', enabled: false }] },
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

// Terminal IPC
ipcMain.handle('terminal-create', async (e, id) => {
  const isWin = process.platform === 'win32';
  const sh = isWin ? 'powershell.exe' : (process.env.SHELL || 'bash');
  const args = isWin ? ['-NoLogo'] : [];

  try {
    const proc = spawn(sh, args, {
      env: { ...process.env, TERM: 'xterm-256color' },
      cwd: process.env.USERPROFILE || process.env.HOME || '.',
      stdio: ['pipe', 'pipe', 'pipe'],
      windowsHide: true,
    });

    terminalProcesses.set(id, proc);

    proc.stdout.on('data', (d) => send('terminal-output', id, d.toString()));
    proc.stderr.on('data', (d) => send('terminal-output', id, d.toString()));
    proc.on('exit', (code) => {
      send('terminal-exit', id, code);
      terminalProcesses.delete(id);
    });
    proc.on('error', (err) => send('terminal-output', id, `\r\nError: ${err.message}\r\n`));

    return { success: true, pid: proc.pid };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('terminal-input', (e, id, data) => {
  const proc = terminalProcesses.get(id);
  if (proc && !proc.stdin.destroyed) proc.stdin.write(data);
});

ipcMain.handle('terminal-kill', (e, id) => {
  const proc = terminalProcesses.get(id);
  if (proc) { proc.kill(); terminalProcesses.delete(id); }
});

ipcMain.handle('terminal-resize', (e, id, cols, rows) => {
  // node-pty would be needed for true PTY resize; skip silently without it
});

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

// Stream generation — tokens sent via 'ai-token' event
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

// Abort current generation
ipcMain.handle('ai-abort', () => {
  if (currentAiReq) { try { currentAiReq.destroy(); } catch {} currentAiReq = null; }
  return { success: true };
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

app.whenReady().then(() => { if (gotTheLock) createWindow(); });
app.on('window-all-closed', () => { if (process.platform !== 'darwin') app.quit(); });
app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
app.on('quit', () => { terminalProcesses.forEach(p => p.kill()); });
