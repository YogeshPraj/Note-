const { app, BrowserWindow, Menu, Tray, dialog, ipcMain, nativeImage, nativeTheme, shell, protocol, Notification } = require('electron');
const path = require('path');
const fs = require('fs');
const { spawn } = require('child_process');

// ── Tray / startup mode flags ──────────────────────────────────────────────
// `--hidden` is passed by the Windows / macOS / Linux autostart entry when
// the user has opted into "Launch Note++ on system startup". In that mode
// we boot tray-only: no BrowserWindow, no renderer, no Monaco. The window
// is created on first user interaction (tray click, file-open, second
// instance), which keeps idle memory near the Electron base (~40 MB)
// instead of a fully-loaded ~250 MB renderer.
const HIDDEN_BOOT = process.argv.includes('--hidden');

// Tray-only boot doesn't render anything, so the entire GPU process is
// dead weight (~30-50 MB RSS + ~50 ms spawn time). Disable hardware
// acceleration when we know we won't draw. This MUST be called before
// app is ready, hence the early placement.
if (HIDDEN_BOOT) {
  try { app.disableHardwareAcceleration(); } catch {}
}

let tray = null;
// app.isQuitting is the convention used by Electron tray apps: when set,
// the `close` handler stops intercepting + lets the app actually quit.
app.isQuitting = false;

// Settings.startupMode: 'off' | 'on'. When 'on', we register the OS login
// item + start hidden + close-to-tray. Kept as a small string so future
// modes ('on-tray-only', 'on-visible', etc.) slot in without a migration.
function _isStayInTrayEnabled() {
  try { return readSettings().startupMode === 'on'; }
  catch { return false; }
}

// Bring up the main window: if it's already alive (just hidden) show it
// instantly; otherwise create it cold. Either way, focus + restore from
// minimized. Used by tray click, second-instance, and the open-file IPC.
function showOrCreateMainWindow() {
  if (mainWindow && !mainWindow.isDestroyed()) {
    if (mainWindow.isMinimized()) mainWindow.restore();
    if (!mainWindow.isVisible()) mainWindow.show();
    try { mainWindow.focus(); } catch {}
    return;
  }
  createWindow();
}

// Pick the highest-quality tray icon available on this platform.
function _trayIconPath() {
  // assets/icon.ico is multi-resolution on Windows; PNG works for mac/linux.
  const ico = path.join(__dirname, 'assets', 'icon.ico');
  const png = path.join(__dirname, 'assets', 'icon.png');
  if (process.platform === 'win32' && fs.existsSync(ico)) return ico;
  if (fs.existsSync(png)) return png;
  return ico; // fall back to whatever's there
}

function createTray() {
  if (tray && !tray.isDestroyed()) return tray;
  let icon;
  try {
    icon = nativeImage.createFromPath(_trayIconPath());
    // macOS prefers a small template image; resize down for the menu bar.
    if (process.platform === 'darwin') {
      icon = icon.resize({ width: 16, height: 16 });
      icon.setTemplateImage(true);
    }
  } catch (e) {
    console.error('[tray] icon load failed:', e?.message || e);
    return null;
  }
  try {
    tray = new Tray(icon);
  } catch (e) {
    console.error('[tray] failed to create:', e?.message || e);
    return null;
  }
  tray.setToolTip('Note++');
  const menu = Menu.buildFromTemplate([
    { label: 'Open Note++',  click: () => showOrCreateMainWindow() },
    { label: 'New File',     click: () => { showOrCreateMainWindow(); setTimeout(() => send('menu-new'), 50); } },
    { label: 'Open File…',   click: () => { showOrCreateMainWindow(); setTimeout(() => handleOpen(), 50); } },
    { type: 'separator' },
    { label: 'Quit Note++',  click: () => { app.isQuitting = true; app.exit(0); } },
  ]);
  tray.setContextMenu(menu);
  // Single-click on Windows / Linux opens the window; macOS uses the menu only.
  tray.on('click', () => {
    if (process.platform !== 'darwin') showOrCreateMainWindow();
  });
  tray.on('double-click', () => showOrCreateMainWindow());
  return tray;
}

function destroyTray() {
  if (tray && !tray.isDestroyed()) {
    try { tray.destroy(); } catch {}
  }
  tray = null;
}

// ── draw.io custom protocol (registered BEFORE app.ready) ───────────────────
// drawio:// serves files from the on-demand download bundle in userData. The
// privileged registration enables fetch/XHR/secure context for the iframe.
protocol.registerSchemesAsPrivileged([
  { scheme: 'drawio', privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true } },
]);

// ── Electron tuning flags (must be set BEFORE app.whenReady) ───────────────
// 1. disable-renderer-backgrounding — Chromium throttles JS timers in hidden
//    windows by default. Note++ has long-running watchers (autosave, git
//    polling, LSP) that misbehave when throttled; the visibility guards in
//    the renderer already prevent waste so the throttling buys us nothing.
// 2. disable-background-timer-throttling — same intent, different vector
//    (specifically setTimeout/setInterval inside background pages).
// These also help on macOS where Chromium aggressively throttles minimized
// windows that don't have focus.
app.commandLine.appendSwitch('disable-renderer-backgrounding');
app.commandLine.appendSwitch('disable-background-timer-throttling');

// node-pty for true PTY terminal (proper resize, colours, interactive
// programs). Lazy-required on the first terminal-create IPC so the
// native binding (~200 KB load + N-API binding bootstrap) doesn't
// block the cold app-start path. Falls back to plain child_process
// if the require ever fails (no prebuilt for this arch, etc.).
let pty;
let _ptyTried = false;
function getPty() {
  if (_ptyTried) return pty;
  _ptyTried = true;
  try { pty = require('node-pty'); }
  catch { pty = null; }
  return pty;
}

let mainWindow;
const terminalProcesses = new Map();

// ── Dev-mode: clear stale V8 code cache on every startup ─────────────────
// When source files change, Electron's V8 cache can serve stale compiled JS.
// In dev mode this is now opt-in (`NOTEPP_CLEAR_DEV_CACHE=1`) because deleting
// multiple cache folders on every launch noticeably slows startup.
if (!app.isPackaged && process.env.NOTEPP_CLEAR_DEV_CACHE === '1') {
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
    // If we booted in --hidden tray mode, mainWindow doesn't exist yet
    // — creating it now is what makes "double-click a file in Explorer"
    // pop the editor open quickly.
    showOrCreateMainWindow();
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

// Theme keys → BrowserWindow.backgroundColor. These ONLY have to match the
// CSS `background` of each theme well enough that the brief native-window
// fill before the renderer paints doesn't look jarringly different from
// the eventual first paint. Kept in main so we don't have to load the
// renderer's THEMES table here.
const THEME_BG = {
  light:          '#f0f0f0',
  dark:           '#1e1e1e',
  flower:         '#fff0f5',
  dracula:        '#282a36',
  tokyonight:     '#1a1b26',
  nord:           '#2e3440',
  monokai:        '#272822',
  solarizedLight: '#fdf6e3',
};

// Resolve the user's saved theme preference (which may be 'system') to a
// concrete backgroundColor so the BrowserWindow opens with the right
// fill colour from the very first frame. Eliminates the "flash of white
// before the editor goes dark" on cold start.
function _initialBackgroundColor(settings) {
  let pref = settings?.ui?.theme;
  // First-launch fallback: if no setting yet but the OS is dark, follow it.
  if (!pref) pref = nativeTheme.shouldUseDarkColors ? 'dark' : 'light';
  if (pref === 'system') pref = nativeTheme.shouldUseDarkColors ? 'dark' : 'light';
  return THEME_BG[pref] || THEME_BG.light;
}

function createWindow() {
  const initSettings    = readSettings();
  const themedTitlebar  = initSettings?.ui?.themedTitlebar === true; // default off (native)
  const initBgColor     = _initialBackgroundColor(initSettings);

  mainWindow = new BrowserWindow({
    width: 1280,
    height: 850,
    minWidth: 400,
    minHeight: 300,
    title: 'Note++',
    // titleBarStyle is a constructor-only option, so we read the setting here.
    // Themed mode: hidden title bar + Win32 overlay controls + HTML menu bar.
    // Native mode: OS default title bar + native menu bar (no HTML chrome needed).
    titleBarStyle: themedTitlebar ? 'hidden' : 'default',
    ...(themedTitlebar ? {
      titleBarOverlay: {
        // Match the resolved theme so even the title-bar overlay doesn't
        // flash light on cold start. Updated later via the
        // set-title-bar-overlay IPC when the user changes themes.
        color:       initBgColor,
        symbolColor: /^#([0-9a-f]{6})$/i.test(initBgColor) && parseInt(initBgColor.slice(1), 16) < 0x808080
          ? '#dddddd' : '#333333',
        height: 30,
      },
    } : {}),
    autoHideMenuBar: themedTitlebar,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'preload.js'),
      webviewTag: true,           // needed for <webview> (mermaid live preview)
      // bypassHeatCheck — use V8's compiled-code cache from the very first
      // launch, instead of waiting for a script to be "hot" before caching.
      // Trades a one-time disk write for substantially faster warm starts.
      v8CacheOptions: 'bypassHeatCheck',
      // backgroundThrottling: false on the renderer too, paired with the
      // app.commandLine switches above. Belt-and-braces.
      backgroundThrottling: false,
      // Chromium's platform spellcheck pulls in dictionaries on the first
      // <textarea> / contenteditable focus. Monaco owns its own text
      // surface and never uses platform spellcheck, so disable it: saves
      // ~20-50 ms of dictionary bootstrap on cold launch and a few MB of
      // memory.
      spellcheck: false,
    },
    // Set the window-fill colour to match the user's saved theme so the
    // brief pre-renderer paint doesn't flash light before the page goes
    // dark. The renderer's inline-script theme application still runs in
    // parallel; this just makes the pre-paint background harmonious.
    backgroundColor: initBgColor,
    show: false
  });

  // Route any window.open() call (Monaco's Ctrl+Click "open link" action
  // fires this for http/https URLs in the editor) through the OS default
  // browser instead of letting Electron open a child BrowserWindow.
  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    try {
      if (/^(https?:|mailto:)/i.test(url)) shell.openExternal(url);
    } catch {}
    return { action: 'deny' };
  });
  // Belt-and-braces: if anything tries to navigate the main window itself
  // (e.g. a stray <a target="_self"> in HTML preview that escaped the
  // sandboxed iframe), block the navigation and open externally instead.
  mainWindow.webContents.on('will-navigate', (event, url) => {
    if (url === mainWindow.webContents.getURL()) return; // self-reload, ignore
    if (/^(https?:|mailto:)/i.test(url)) {
      event.preventDefault();
      try { shell.openExternal(url); } catch {}
    }
  });

  mainWindow.loadFile(path.join(__dirname, 'index.html'));

  // Fallback: force-show after 4 s in case ready-to-show is delayed by a
  // renderer hiccup (e.g. slow machine, large session restore). Prevents the
  // window from being permanently invisible.
  const showFallback = setTimeout(() => { if (!mainWindow.isVisible()) mainWindow.show(); }, 4000);

  mainWindow.once('ready-to-show', () => {
    clearTimeout(showFallback);
    mainWindow.show();
    // Queue any file passed on the command line (double-click / "Open with").
    // It'll be delivered when the renderer sends 'renderer-ready' below.
    const file = fileFromArgv(process.argv);
    if (file) queueOpenFiles([file]);
    // Hard-block common Chromium DevTools shortcuts for end users.
    // Keep F12 free for editor actions (Go to Definition), so we only block
    // Ctrl/Cmd+Shift+I/J/C and Ctrl/Cmd+Alt+I.
    mainWindow.webContents.on('before-input-event', (event, input) => {
      if (input.type !== 'keyDown') return;
      const key = String(input.key || '').toUpperCase();
      const ctrlOrCmd = !!input.control || !!input.meta;
      const isShiftCombo = ctrlOrCmd && !!input.shift && ['I', 'J', 'C'].includes(key);
      const isAltCombo = ctrlOrCmd && !!input.alt && key === 'I';
      if (isShiftCombo || isAltCombo) {
        event.preventDefault();
      }
      // ── Close-tab (Ctrl/Cmd+W) — single authoritative handler ───────────
      // Own Ctrl+W entirely from the main process so a single physical press
      // closes exactly ONE tab, for every tab type (editor, compare/quick-diff,
      // folder-diff, …). Two independent paths used to fire for one press:
      //   1. the native menu accelerator 'CmdOrCtrl+W' (File ▸ Close), and
      //   2. Monaco's own editor.addCommand(Ctrl+W) in the renderer.
      // For a normal editor tab Monaco consumes the key (so the accelerator is
      // suppressed) and only one close happens. But when a compare/diff editor
      // is focused, focus juggling during the close made Monaco re-dispatch the
      // still-pending keystroke, cascading closeTab() across the tab strip —
      // silently closing saved tabs and popping a lone Save prompt. OS key
      // auto-repeat (holding the combo a moment) produced the same cascade.
      // Calling preventDefault() here blocks BOTH the page keydown (so Monaco's
      // binding never fires / can't re-dispatch) AND the menu accelerator, then
      // we emit exactly one 'menu-close', ignoring auto-repeat. Deterministic,
      // in the main process, immune to renderer focus/re-dispatch races.
      if (ctrlOrCmd && !input.alt && !input.shift && key === 'W') {
        event.preventDefault();
        if (!input.isAutoRepeat) {
          try { mainWindow.webContents.send('menu-close'); } catch {}
        }
      }
    });
  });

  mainWindow.on('close', (e) => {
    // The renderer's `app-before-close` handler runs saveSession() then
    // calls `close-window` IPC. When tray mode is on AND the user hasn't
    // explicitly chosen Quit, that path hides the window instead of
    // exiting. Keep preventDefault here so the renderer always gets the
    // save-pass chance regardless of tray state.
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
      label: '&File',
      submenu: [
        { label: '&New', accelerator: 'CmdOrCtrl+N', click: () => send('menu-new') },
        { type: 'separator' },
        { label: '&Open...', accelerator: 'CmdOrCtrl+O', click: () => handleOpen() },
        { label: 'Open &Folder...', accelerator: 'CmdOrCtrl+Shift+O', click: () => handleOpenFolder() },
        { label: 'Open &Recent', submenu: buildRecentFilesSubmenu() },
        { type: 'separator' },
        { label: '&Compare', submenu: [
          { label: 'Compare &Files…',         click: () => send('menu-compare-files') },
          { label: 'Compare &with Saved…',    click: () => send('menu-compare-with-saved') },
          { label: 'Compare with &Clipboard', click: () => send('menu-compare-clipboard') },
          { type: 'separator' },
          { label: 'Compare F&olders…',       click: () => send('menu-compare-folders') },
        ]},
        { type: 'separator' },
        { label: 'Reload from &Disk', accelerator: 'CmdOrCtrl+Shift+R', click: () => send('menu-reload') },
        { type: 'separator' },
        // No accelerator here on purpose: Ctrl/Cmd+W is owned solely by the
        // main-process before-input-event handler (see mainWindow ready-to-show),
        // which emits exactly one 'menu-close' per physical press. Registering an
        // accelerator here too would fire 'menu-close' a SECOND time for one press,
        // closing multiple tabs. The click handler keeps File ▸ Close working.
        { label: 'C&lose', click: () => send('menu-close') },
        { label: 'Close &All', click: () => send('menu-close-all') },
        { label: 'Close All &But Current', click: () => send('menu-close-others') },
        { type: 'separator' },
        { label: '&Save', accelerator: 'CmdOrCtrl+S', click: () => send('menu-save') },
        { label: 'Save &As...', accelerator: 'CmdOrCtrl+Alt+S', click: () => send('menu-save-as') },
        { label: 'Save A&ll', accelerator: 'CmdOrCtrl+Shift+S', click: () => send('menu-save-all') },
        { type: 'separator' },
        { label: '&Print...', accelerator: 'CmdOrCtrl+P', click: () => send('menu-print') },
        { type: 'separator' },
        { label: 'E&xit', accelerator: 'Alt+F4', click: () => app.exit(0) }
      ]
    },
    {
      label: '&Edit',
      submenu: [
        { label: '&Undo', accelerator: 'CmdOrCtrl+Z', click: () => send('menu-undo') },
        { label: '&Redo', accelerator: 'CmdOrCtrl+Y', click: () => send('menu-redo') },
        { type: 'separator' },
        { label: 'Cu&t', accelerator: 'CmdOrCtrl+X', role: 'cut' },
        { label: '&Copy', accelerator: 'CmdOrCtrl+C', role: 'copy' },
        { label: '&Paste', accelerator: 'CmdOrCtrl+V', role: 'paste' },
        { label: 'Select &All', accelerator: 'CmdOrCtrl+A', role: 'selectAll' },
        { type: 'separator' },
        { label: '&Duplicate Line', accelerator: 'CmdOrCtrl+D', click: () => send('menu-duplicate-line') },
        { label: 'De&lete Line', accelerator: 'CmdOrCtrl+Shift+K', click: () => send('menu-delete-line') },
        { label: '&Move Line Up', accelerator: 'Alt+Up', click: () => send('menu-move-line-up') },
        { label: 'Move Line D&own', accelerator: 'Alt+Down', click: () => send('menu-move-line-down') },
        { type: 'separator' },
        { label: 'Con&vert Case to', submenu: [
          { label: '&UPPERCASE', accelerator: 'CmdOrCtrl+Shift+U', click: () => send('menu-uppercase') },
          { label: '&lowercase', accelerator: 'CmdOrCtrl+U', click: () => send('menu-lowercase') },
          { label: '&Title Case', click: () => send('menu-titlecase') },
        ]},
        { label: 'Li&ne Operations', submenu: [
          { label: 'Sort Lines &Ascending', click: () => send('menu-sort-asc') },
          { label: 'Sort Lines &Descending', click: () => send('menu-sort-desc') },
          { label: 'Remove &Duplicate Lines', click: () => send('menu-remove-dup-lines') },
          { label: 'Remove &Empty Lines', click: () => send('menu-remove-empty-lines') },
          { label: '&Join Lines', click: () => send('menu-join-lines') },
        ]},
        { type: 'separator' },
        { label: 'Base64 &Encode', click: () => send('menu-b64-encode') },
        { label: 'Base64 &Decode', click: () => send('menu-b64-decode') },
        { type: 'separator' },
        { label: '&Set Read-Only', click: () => send('menu-readonly') },
        { label: 'Clear Read-Only &Flag', click: () => send('menu-clear-readonly') },
      ]
    },
    {
      label: '&Code',
      submenu: [
        { label: '&Format Document (Auto-detect)', accelerator: 'CmdOrCtrl+Shift+F', click: () => send('menu-format-doc') },
        { label: 'Format &Selection', click: () => send('menu-format-sel') },
        { type: 'separator' },
        { label: 'Toggle &Line Comment', accelerator: 'CmdOrCtrl+/', click: () => send('menu-toggle-comment') },
        { label: 'Toggle &Block Comment', accelerator: 'Shift+Alt+A', click: () => send('menu-block-comment') },
        { type: 'separator' },
        { label: '&Indent', accelerator: 'Tab', click: () => send('menu-indent-increase') },
        { label: '&Outdent', accelerator: 'Shift+Tab', click: () => send('menu-indent-decrease') },
        { type: 'separator' },
        { label: 'Fold &All', accelerator: 'CmdOrCtrl+K CmdOrCtrl+0', click: () => send('menu-fold-all') },
        { label: '&Unfold All', accelerator: 'CmdOrCtrl+K CmdOrCtrl+J', click: () => send('menu-unfold-all') },
        { label: '&Toggle Fold', accelerator: 'CmdOrCtrl+Shift+[', click: () => send('menu-toggle-fold') },
        { type: 'separator' },
        { label: 'Go to &Definition', accelerator: 'F12', click: () => send('menu-goto-definition') },
        { label: 'Go to S&ymbol...', accelerator: 'CmdOrCtrl+Shift+O', click: () => send('menu-goto-symbol') },
        { label: 'Go to &References', accelerator: 'Shift+F12', click: () => send('menu-goto-refs') },
        { label: 'Re&name Symbol', accelerator: 'F2', click: () => send('menu-rename-symbol') },
        { type: 'separator' },
        { label: 'Trigger Su&ggest', accelerator: 'CmdOrCtrl+Space', click: () => send('menu-trigger-suggest') },
        { label: 'Trigger &Parameter Hints', accelerator: 'CmdOrCtrl+Shift+Space', click: () => send('menu-trigger-hints') },
        { type: 'separator' },
        { label: 'Pretty Print &JSON', click: () => send('menu-json-format') },
        { label: 'Pretty Print &XML', click: () => send('menu-xml-format') },
        { label: '&Minify JSON', click: () => send('menu-json-minify') },
        { type: 'separator' },
        { label: '&Diagram (draw.io)', submenu: [
          { label: '&New Diagram', click: () => send('menu-new-drawio') },
          { label: '&From Template', submenu: [
            { label: '&Flowchart',              click: () => send('menu-new-drawio-template', 'flowchart') },
            { label: '&Sequence Diagram',       click: () => send('menu-new-drawio-template', 'sequence') },
            { label: '&Class Diagram (UML)',    click: () => send('menu-new-drawio-template', 'classDiagram') },
            { label: '&Entity Relationship',    click: () => send('menu-new-drawio-template', 'erDiagram') },
          ]},
          { type: 'separator' },
          { label: '&Check for updates',  click: () => send('menu-drawio-check-updates') },
        ]},
      ]
    },
    {
      label: '&Search',
      submenu: [
        { label: '&Find...', accelerator: 'CmdOrCtrl+F', click: () => send('menu-find') },
        { label: 'Find &Next', accelerator: 'F3', click: () => send('menu-find-next') },
        { label: 'Find &Previous', accelerator: 'Shift+F3', click: () => send('menu-find-prev') },
        { label: 'Find &All in Current Document', click: () => send('menu-find-all') },
        { type: 'separator' },
        { label: '&Replace...', accelerator: 'CmdOrCtrl+H', click: () => send('menu-replace') },
        { type: 'separator' },
        { label: '&Quick Open (Go to File)', accelerator: 'CmdOrCtrl+P', click: () => send('menu-quick-open') },
        { label: '&Command Palette', accelerator: 'CmdOrCtrl+Shift+P', click: () => send('menu-cmd-palette') },
        { type: 'separator' },
        { label: 'Go to &Line...', accelerator: 'CmdOrCtrl+G', click: () => send('menu-goto-line') },
        { label: 'Go to &Matching Brace', accelerator: 'CmdOrCtrl+B', click: () => send('menu-goto-brace') },
        { type: 'separator' },
        { label: '&Toggle Bookmark', accelerator: 'CmdOrCtrl+F2', click: () => send('menu-toggle-bookmark') },
        { label: 'Ne&xt Bookmark', accelerator: 'F2', click: () => send('menu-next-bookmark') },
        { label: 'P&revious Bookmark', accelerator: 'Shift+F2', click: () => send('menu-prev-bookmark') },
        { label: 'Re&gex Tester...', click: () => send('menu-regex-tester') },
      ]
    },
    {
      label: '&View',
      submenu: [
        { label: '&Always on Top', type: 'checkbox', click: (i) => mainWindow.setAlwaysOnTop(i.checked) },
        { label: '&Zen Mode', accelerator: 'F11', click: () => send('menu-zen-mode') },
        { type: 'separator' },
        { label: '&Minimap', type: 'checkbox', checked: true, click: (i) => send('menu-minimap', i.checked) },
        { label: '&Word Wrap', accelerator: 'Alt+Z', type: 'checkbox', click: (i) => send('menu-word-wrap', i.checked) },
        { label: '&Breadcrumbs', type: 'checkbox', checked: true, click: (i) => send('menu-breadcrumbs', i.checked) },
        { type: 'separator' },
        { label: 'S&how Whitespace', type: 'checkbox', click: (i) => send('menu-show-whitespace', i.checked) },
        { label: 'Show &Indent Guides', type: 'checkbox', checked: true, click: (i) => send('menu-show-indent', i.checked) },
        { type: 'separator' },
        { label: '&Zoom In', accelerator: 'CmdOrCtrl+=', click: () => send('menu-zoom-in') },
        { label: 'Zoom &Out', accelerator: 'CmdOrCtrl+-', click: () => send('menu-zoom-out') },
        { label: '&Reset Zoom', accelerator: 'CmdOrCtrl+0', click: () => send('menu-zoom-reset') },
        { type: 'separator' },
        { label: 'Toggle &Terminal', accelerator: 'CmdOrCtrl+`', click: () => send('menu-toggle-terminal') },
        { label: 'Toggle &Sidebar', accelerator: 'CmdOrCtrl+B', click: () => send('menu-toggle-sidebar') },
        { type: 'separator' },
        { label: 'Toggle Tool&bar', type: 'checkbox', checked: true, click: (i) => send('menu-toolbar', i.checked) },
        { label: 'Toggle St&atus Bar', type: 'checkbox', checked: true, click: (i) => send('menu-statusbar', i.checked) },
        { label: 'Toggle Ta&b Bar', type: 'checkbox', checked: true, click: (i) => send('menu-tabbar', i.checked) },
      ]
    },
    {
      label: 'E&ncoding',
      submenu: [
        { label: '&UTF-8', type: 'radio', checked: true, click: () => send('menu-encoding', 'UTF-8') },
        { label: 'UTF-8 &BOM', type: 'radio', click: () => send('menu-encoding', 'UTF-8 BOM') },
        { label: '&ANSI', type: 'radio', click: () => send('menu-encoding', 'ANSI') },
        { label: 'UTF-16 &LE', type: 'radio', click: () => send('menu-encoding', 'UTF-16 LE') },
        { label: 'UTF-16 B&E', type: 'radio', click: () => send('menu-encoding', 'UTF-16 BE') },
      ]
    },
    {
      label: '&Language',
      submenu: [
        { label: '&Plain Text', click: () => send('menu-lang', 'plaintext') },
        { type: 'separator' },
        { label: '&Batch', click: () => send('menu-lang', 'bat') },
        { label: '&C', click: () => send('menu-lang', 'c') },
        { label: 'C&++', click: () => send('menu-lang', 'cpp') },
        { label: 'C&#', click: () => send('menu-lang', 'csharp') },
        { label: 'CS&S', click: () => send('menu-lang', 'css') },
        { label: '&Dockerfile', click: () => send('menu-lang', 'dockerfile') },
        { label: '&Go', click: () => send('menu-lang', 'go') },
        { label: '&HTML', click: () => send('menu-lang', 'html') },
        { label: 'J&ava', click: () => send('menu-lang', 'java') },
        { label: '&JavaScript', click: () => send('menu-lang', 'javascript') },
        { label: 'JS&ON', click: () => send('menu-lang', 'json') },
        { label: '&Kotlin', click: () => send('menu-lang', 'kotlin') },
        { label: '&Lua', click: () => send('menu-lang', 'lua') },
        { label: '&Markdown', click: () => send('menu-lang', 'markdown') },
        { label: 'Mer&maid', click: () => send('menu-lang', 'mermaid') },
        { label: '&PHP', click: () => send('menu-lang', 'php') },
        { label: 'Po&werShell', click: () => send('menu-lang', 'powershell') },
        { label: 'Py&thon', click: () => send('menu-lang', 'python') },
        { label: '&R', click: () => send('menu-lang', 'r') },
        { label: 'R&uby', click: () => send('menu-lang', 'ruby') },
        { label: 'R&ust', click: () => send('menu-lang', 'rust') },
        { label: '&SCSS', click: () => send('menu-lang', 'scss') },
        { label: '&Shell Script', click: () => send('menu-lang', 'shell') },
        { label: 'S&QL', click: () => send('menu-lang', 'sql') },
        { label: 'S&wift', click: () => send('menu-lang', 'swift') },
        { label: '&TOML', click: () => send('menu-lang', 'ini') },
        { label: 'T&ypeScript', click: () => send('menu-lang', 'typescript') },
        { label: '&XML', click: () => send('menu-lang', 'xml') },
        { label: 'Y&AML', click: () => send('menu-lang', 'yaml') },
      ]
    },
    {
      label: '&Terminal',
      submenu: [
        { label: '&New Terminal', accelerator: 'CmdOrCtrl+Shift+`', click: () => send('menu-new-terminal') },
        { label: '&Toggle Terminal', accelerator: 'CmdOrCtrl+`', click: () => send('menu-toggle-terminal') },
        { label: '&Kill Terminal', click: () => send('menu-kill-terminal') },
        { label: '&Clear Terminal', click: () => send('menu-clear-terminal') },
        { type: 'separator' },
        { label: '&Run File', accelerator: 'F5', click: () => send('menu-run-file') },
        { label: 'Run &Selection', accelerator: 'Shift+F5', click: () => send('menu-run-selection') },
        { type: 'separator' },
        { label: '&Open Containing Folder', accelerator: 'CmdOrCtrl+Alt+Shift+R', click: () => send('menu-open-explorer') },
        { label: 'Copy &File Path', click: () => send('menu-copy-path') },
      ]
    },
    {
      label: 'Sett&ings',
      submenu: [
        { label: '&Preferences...', accelerator: 'CmdOrCtrl+,', click: () => send('menu-preferences') },
        { type: 'separator' },
        { label: 'Toggle &Dark Mode', accelerator: 'CmdOrCtrl+Alt+D', click: () => send('menu-dark-mode') },
        { type: 'separator' },
        { label: 'Font &Size +', accelerator: 'CmdOrCtrl+=', click: () => send('menu-zoom-in') },
        { label: 'Font Si&ze -', accelerator: 'CmdOrCtrl+-', click: () => send('menu-zoom-out') },
      ]
    },
    {
      label: 'T&ools',
      submenu: [
        { label: '&Regex Tester', click: () => send('menu-regex-tester') },
        { type: 'separator' },
        { label: '&MD5', submenu: [
          { label: '&Generate from Selection', click: () => send('menu-md5-selection') },
        ]},
        { label: '&SHA-256', submenu: [
          { label: '&Generate from Selection', click: () => send('menu-sha256-selection') },
        ]},
        { type: 'separator' },
        { label: 'Base64 &Encode', click: () => send('menu-b64-encode') },
        { label: 'Base64 &Decode', click: () => send('menu-b64-decode') },
        { type: 'separator' },
        { label: '&JWT Decoder…', click: () => send('menu-jwt-decoder') },
        { type: 'separator' },
        { label: 'Pretty Print &JSON', click: () => send('menu-json-format') },
        { label: 'Pretty Print &XML', click: () => send('menu-xml-format') },
        { label: '&Minify JSON', click: () => send('menu-json-minify') },
        { type: 'separator' },
        { label: '&Diagram (draw.io)', submenu: [
          { label: '&New Diagram', click: () => send('menu-new-drawio') },
          { label: '&From Template', submenu: [
            { label: '&Flowchart',              click: () => send('menu-new-drawio-template', 'flowchart') },
            { label: '&Sequence Diagram',       click: () => send('menu-new-drawio-template', 'sequence') },
            { label: '&Class Diagram (UML)',    click: () => send('menu-new-drawio-template', 'classDiagram') },
            { label: '&Entity Relationship',    click: () => send('menu-new-drawio-template', 'erDiagram') },
          ]},
          { type: 'separator' },
          { label: '&Check for updates',  click: () => send('menu-drawio-check-updates') },
        ]},
      ]
    },
    {
      label: '&Macros',
      submenu: [
        { label: 'Start / Stop &Recording', accelerator: 'CmdOrCtrl+Shift+R', click: () => send('menu-macro-record') },
        { type: 'separator' },
        { label: '&Run Last Macro', accelerator: 'F9', click: () => send('menu-macro-run') },
        { label: 'Run &N Times…', accelerator: 'CmdOrCtrl+F9', click: () => send('menu-macro-run-n') },
        { type: 'separator' },
        { label: '&Manage Macros…', click: () => send('menu-macro-manage') },
      ]
    },
    {
      label: '&Window',
      submenu: [
        { label: '&Previous Document', accelerator: 'CmdOrCtrl+PageUp', click: () => send('menu-prev-tab') },
        { label: '&Next Document', accelerator: 'CmdOrCtrl+PageDown', click: () => send('menu-next-tab') },
        { type: 'separator' },
        { label: 'Tab &1', accelerator: 'CmdOrCtrl+1', click: () => send('menu-tab', 0) },
        { label: 'Tab &2', accelerator: 'CmdOrCtrl+2', click: () => send('menu-tab', 1) },
        { label: 'Tab &3', accelerator: 'CmdOrCtrl+3', click: () => send('menu-tab', 2) },
        { label: 'Tab &4', accelerator: 'CmdOrCtrl+4', click: () => send('menu-tab', 3) },
        { label: 'Tab &5', accelerator: 'CmdOrCtrl+5', click: () => send('menu-tab', 4) },
      ]
    },
    {
      label: '?',
      submenu: [
        { label: '&About Note++', click: () => send('menu-about') },
        { type: 'separator' },
        { label: 'Show &Feature Tour', click: () => send('menu-feature-tour') },
        { label: 'Show &Boot Performance', click: () => send('menu-boot-perf') },
        { type: 'separator' },
        { label: '&Keyboard Shortcuts Reference', click: () => send('menu-shortcuts-ref') },
        { type: 'separator' },
        {
          label: 'Check for Updates &Automatically',
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
        { label: 'Check for Updates &Now', click: async () => {
            if (!app.isPackaged) {
              dialog.showMessageBox(mainWindow, { type: 'info', message: 'Auto-update only runs in the installed build.' });
              return;
            }
            initAutoUpdater();
            try { await autoUpdater?.checkForUpdates(); } catch (e) { console.error(e); }
          },
        },
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
  try {
    const size = fs.statSync(filePath).size;
    return { success: true, content: fs.readFileSync(filePath, 'utf-8'), size };
  }
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

// close-window is the "renderer says it's OK to close now" signal (it
// already finished saveSession). When tray-stay mode is on, we just hide
// the window so subsequent opens are instant. Quit happens only via
// tray-menu "Quit" or another explicit exit gesture (both flip
// app.isQuitting first).
ipcMain.handle('close-window', () => {
  if (!app.isQuitting && _isStayInTrayEnabled() && mainWindow && !mainWindow.isDestroyed()) {
    try { mainWindow.hide(); } catch {}
    return;
  }
  app.exit(0);
});

// ── Startup / tray IPC ─────────────────────────────────────────────────────
ipcMain.handle('startup-mode:get', () => {
  const s = readSettings();
  // Cross-check with the OS — if the user removed the autostart entry
  // through Windows Task Manager / macOS Login Items, reflect reality.
  let osConfigured = false;
  try {
    const li = app.getLoginItemSettings();
    osConfigured = !!li.openAtLogin;
  } catch {}
  return {
    enabled: s.startupMode === 'on',
    osConfigured,
  };
});

ipcMain.handle('startup-mode:set', (e, enabled) => {
  const s = readSettings();
  s.startupMode = enabled ? 'on' : 'off';
  writeSettings(s);
  try {
    if (enabled) {
      // openAsHidden gives macOS the "start hidden" affordance; on
      // Windows + Linux electron-builder ships an autostart entry that
      // includes our `args` array — we add --hidden so the boot path
      // routes into createTray() + skips createWindow().
      app.setLoginItemSettings({
        openAtLogin: true,
        openAsHidden: true,
        args: ['--hidden'],
      });
      // Create the tray now so the user sees the icon immediately.
      createTray();
    } else {
      app.setLoginItemSettings({ openAtLogin: false });
      // Don't tear down the tray right away — the user may have just
      // unchecked the option but still have a window open. Leaving the
      // tray in place is harmless; it goes away at next quit. Removing
      // it here would mean the user loses the "stay in tray" affordance
      // even though their window is still open.
    }
  } catch (err) {
    return { success: false, error: err?.message || String(err) };
  }
  return { success: true };
});
ipcMain.handle('relaunch-app', () => { app.relaunch(); app.exit(0); });

// Theme — sync the Win32 window-controls overlay colour with the active theme.
ipcMain.handle('set-title-bar-overlay', (_, color, symbolColor) => {
  if (mainWindow && process.platform === 'win32') {
    try { mainWindow.setTitleBarOverlay({ color, symbolColor, height: 30 }); }
    catch (_e) { /* older Electron / unsupported OS — ignore */ }
  }
});

// Window state helpers for the custom menu bar
ipcMain.handle('set-always-on-top',       (_, flag) => { if (mainWindow) mainWindow.setAlwaysOnTop(flag); });
ipcMain.handle('set-full-screen',         (_, flag) => { if (mainWindow) mainWindow.setFullScreen(flag); });
ipcMain.handle('set-menu-bar-visibility', (_, visible) => {
  if (mainWindow) {
    mainWindow.setMenuBarVisibility(visible);
    mainWindow.setAutoHideMenuBar(!visible);
  }
});
ipcMain.handle('shell-open', (e, p) => shell.openPath(p));
ipcMain.handle('shell-show-item', (e, p) => shell.showItemInFolder(p));
ipcMain.handle('get-user-data-path', () => app.getPath('userData'));

// ---- Settings (persisted app preferences) ----
function settingsPath() { return path.join(app.getPath('userData'), 'settings.json'); }

// In-memory cache so the dozen+ ipcMain handlers that read settings on
// every invocation don't all hit disk + reparse JSON. Invalidated on
// every writeSettings; bypassable with `readSettings(true)` for the rare
// case (none today) where we want to defeat the cache.
let _settingsCache = null;
function readSettings(forceFresh = false) {
  if (!forceFresh && _settingsCache) return _settingsCache;
  let parsed = {};
  try {
    if (fs.existsSync(settingsPath())) {
      parsed = JSON.parse(fs.readFileSync(settingsPath(), 'utf-8')) || {};
    }
  } catch {}
  _settingsCache = parsed;
  return parsed;
}

function writeSettings(data) {
  try { fs.writeFileSync(settingsPath(), JSON.stringify(data, null, 2), 'utf-8'); } catch {}
  // Refresh the cache from the just-written payload (cheaper + can't
  // race a concurrent IPC) rather than re-reading from disk.
  _settingsCache = data || {};
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

// ── Binary → Markdown conversion (PDF / DOCX) ───────────────────────────────
// Loaded lazily on first use so the cold start path doesn't pay for it.
let _mdConverter = null;
function getMdConverter() {
  if (!_mdConverter) _mdConverter = require('./markdown-converter');
  return _mdConverter;
}
ipcMain.handle('convert-to-markdown:can-convert', (e, srcPath) => {
  try { return getMdConverter().canConvert(srcPath); }
  catch { return false; }
});
ipcMain.handle('convert-to-markdown:supported-exts', () => {
  try { return getMdConverter().SUPPORTED_EXTENSIONS; }
  catch { return []; }
});
// ── Markdown preview export (HTML / PDF / DOCX) ─────────────────────────────
// Renderer passes a fully serialised HTML document (DOCTYPE + <html> + body
// with inlined styles). For PDF we spin up an off-screen BrowserWindow,
// load the HTML as a data URL, call `printToPDF`, then close it.
// For DOCX we route through the html-to-docx library (Node-side).
// Both return the bytes as base64 so the existing `writeFileBinary` IPC
// can persist them.
ipcMain.handle('preview-export:to-pdf', async (e, html) => {
  let win = null;
  try {
    win = new BrowserWindow({
      show: false,
      width: 1100,
      height: 1500,
      webPreferences: {
        nodeIntegration: false,
        contextIsolation: true,
        sandbox: true,
        // Standalone — no preload / no electronAPI surface.
      },
    });
    // Use a data URL so we don't need a temp file. Encode as base64 to
    // sidestep the URL-length limits some Chromium versions had on
    // percent-encoded data URLs.
    const dataUrl = 'data:text/html;base64,' + Buffer.from(html, 'utf-8').toString('base64');
    await win.loadURL(dataUrl);
    // Give web fonts + images a couple hundred ms to settle before
    // printToPDF snapshots the page.
    await new Promise(r => setTimeout(r, 250));
    const pdfBuf = await win.webContents.printToPDF({
      printBackground: true,
      pageSize: 'A4',
      margins: { top: 0.5, bottom: 0.5, left: 0.5, right: 0.5 },
    });
    return { success: true, base64: pdfBuf.toString('base64') };
  } catch (err) {
    return { success: false, error: err?.message || String(err) };
  } finally {
    try { win?.close(); } catch {}
  }
});

ipcMain.handle('preview-export:to-docx', async (e, html) => {
  try {
    // html-to-docx is a CommonJS default-export wrapper. Lazy-require so
    // it stays out of the cold-start path.
    const htmlToDocx = require('html-to-docx');
    const buf = await htmlToDocx(html, null, {
      orientation: 'portrait',
      pageNumber: false,
      table: { row: { cantSplit: true } },
      footer: false,
    });
    // html-to-docx returns either a Node Buffer or a Blob depending on
    // platform — coerce both into a Node Buffer.
    let nodeBuf;
    if (Buffer.isBuffer(buf)) {
      nodeBuf = buf;
    } else if (buf && typeof buf.arrayBuffer === 'function') {
      nodeBuf = Buffer.from(await buf.arrayBuffer());
    } else {
      nodeBuf = Buffer.from(buf);
    }
    return { success: true, base64: nodeBuf.toString('base64') };
  } catch (err) {
    return { success: false, error: err?.message || String(err) };
  }
});

// ── Spell-check IPC ────────────────────────────────────────────────────────
// Lazy-required so the dictionary (~555 KB) isn't loaded for sessions
// that never enable spell-check.
let _spellService = null;
function getSpellService() {
  if (!_spellService) _spellService = require('./spell-service');
  return _spellService;
}
ipcMain.handle('spell:check', async (e, words) => {
  try { return { success: true, results: await getSpellService().checkWords(words) }; }
  catch (err) { return { success: false, error: err?.message || String(err) }; }
});
ipcMain.handle('spell:suggest', async (e, word, max) => {
  try { return { success: true, suggestions: await getSpellService().suggest(word, max) }; }
  catch (err) { return { success: false, error: err?.message || String(err) }; }
});
ipcMain.handle('spell:add-word', async (e, word) => {
  try { await getSpellService().ensureLoaded(); return { success: getSpellService().addWord(word) }; }
  catch (err) { return { success: false, error: err?.message || String(err) }; }
});

ipcMain.handle('convert-to-markdown:start', async (e, srcPath, jobId) => {
  try {
    const conv = getMdConverter();
    const onProgress = (percent, stage) => {
      try { e.sender.send('convert-to-markdown:progress', { jobId, percent, stage }); }
      catch { /* sender gone */ }
    };
    const markdown = await conv.convert(srcPath, onProgress);
    return { success: true, markdown };
  } catch (err) {
    return { success: false, error: err?.message || String(err) };
  }
});

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
    const ptyMod = getPty();
    if (ptyMod) {
      // ── True PTY via node-pty ──────────────────────────────────────────────
      const isWin = process.platform === 'win32';
      const ptyArgs = isWin ? ['-NoLogo'] : [];
      const proc = ptyMod.spawn(sh, ptyArgs, {
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
// All handlers delegate to src/git-service.js. Loaded lazily on first use.
let gitService = null;
function getGitService() {
  if (!gitService) gitService = require('./git-service');
  return gitService;
}

ipcMain.handle('git-find-repo',     (e, p)             => getGitService().findRepoRoot(p));
ipcMain.handle('git-status',        (e, root)          => getGitService().status(root));
ipcMain.handle('git-stage',         (e, root, paths)   => getGitService().stage(root, paths));
ipcMain.handle('git-unstage',       (e, root, paths)   => getGitService().unstage(root, paths));
ipcMain.handle('git-discard',       (e, root, paths)   => getGitService().discard(root, paths));
ipcMain.handle('git-clean',         (e, root, paths)   => getGitService().cleanUntracked(root, paths));
ipcMain.handle('git-commit',        (e, root, msg)     => getGitService().commit(root, msg));
ipcMain.handle('git-commit-amend',  (e, root, msg)     => getGitService().commitAmend(root, msg));
ipcMain.handle('git-fetch',         (e, root)          => getGitService().fetch(root));
ipcMain.handle('git-pull',          (e, root)          => getGitService().pull(root));
ipcMain.handle('git-push',          (e, root)          => getGitService().push(root));
ipcMain.handle('git-push-upstream', (e, root, r2, b)   => getGitService().pushSetUpstream(root, r2, b));
ipcMain.handle('git-sync',          (e, root)          => getGitService().sync(root));
ipcMain.handle('git-branch-list',   (e, root)          => getGitService().branchList(root));
ipcMain.handle('git-branch-switch', (e, root, name)    => getGitService().branchSwitch(root, name));
ipcMain.handle('git-branch-create', (e, root, n, f)    => getGitService().branchCreate(root, n, f));
ipcMain.handle('git-installed',     ()                 => getGitService().isGitInstalled());
ipcMain.handle('git-show-head',     (e, root, rel)     => getGitService().showHead(root, rel));

// ── draw.io service (on-demand download, persistent bundle) ────────────────
let drawioService = null;
function getDrawioService() {
  if (!drawioService) drawioService = require('./drawio-service');
  return drawioService;
}


// ── Azure Icons library (maskati.github.io/azure-icons) ──────────────────
// Fetch-and-cache: on first "Icons" panel open, main-process pulls the
// static HTML page, parses its table into a JSON manifest, and stores it
// under userData. Individual SVGs are downloaded on demand and cached
// permanently under `icon-cache/svgs/`. Zero network cost on subsequent
// uses once fetched. A "Refresh" button in the panel forces a re-fetch.
const ICON_LIB_HOST = 'https://maskati.github.io';
const ICON_LIB_INDEX = ICON_LIB_HOST + '/azure-icons/';
const _iconCacheRoot = () => path.join(app.getPath('userData'), 'icon-cache');
const _iconManifestFile = () => path.join(_iconCacheRoot(), 'manifest.json');
const _iconSvgDir = () => path.join(_iconCacheRoot(), 'svgs');

function _httpsGet(url) {
  // Thin promise wrapper around Node's https.get. Follows one redirect.
  const https = require('https');
  return new Promise((resolve, reject) => {
    const req = https.get(url, { headers: { 'User-Agent': 'notepp' } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        const next = res.headers.location.startsWith('http')
          ? res.headers.location
          : new URL(res.headers.location, url).toString();
        res.resume(); // drain
        return _httpsGet(next).then(resolve, reject);
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      const chunks = [];
      res.on('data', c => chunks.push(c));
      res.on('end', () => resolve(Buffer.concat(chunks).toString('utf-8')));
      res.on('error', reject);
    });
    req.on('error', reject);
    req.setTimeout(20000, () => { req.destroy(new Error('timeout')); });
  });
}

// Parse maskati's HTML table. Each <tr> contains:
//   <td><img src="svg/Namespace/File.svg"></td>
//   <td>Display name</td>
//   <td>type namespace</td>
//   <td>space-separated keywords</td>
//   <td>optional description</td>
// We ignore markup we don't need and pull just the fields for search+drag.
function _parseIconManifest(html) {
  const rows = [];
  const trRegex = /<tr[^>]*>([\s\S]*?)<\/tr>/g;
  const tdRegex = /<td[^>]*>([\s\S]*?)<\/td>/g;
  const imgRegex = /<img[^>]+src=["']([^"']+)["']/i;
  const stripTags = s => s.replace(/<[^>]+>/g, '').replace(/\s+/g, ' ').trim();
  const decode = s => s.replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'");
  let m;
  while ((m = trRegex.exec(html))) {
    const tds = [];
    let tm;
    tdRegex.lastIndex = 0;
    while ((tm = tdRegex.exec(m[1]))) tds.push(tm[1]);
    if (tds.length < 4) continue;
    const img = imgRegex.exec(tds[0]);
    if (!img) continue;
    rows.push({
      svgPath:     decode(img[1].trim()),
      name:        decode(stripTags(tds[1])),
      type:        decode(stripTags(tds[2])),
      keywords:    decode(stripTags(tds[3])),
      description: decode(stripTags(tds[4] || '')),
    });
  }
  return rows;
}

ipcMain.handle('icons:fetch-manifest', async (_e, force) => {
  try {
    fs.mkdirSync(_iconCacheRoot(), { recursive: true });
    if (!force && fs.existsSync(_iconManifestFile())) {
      const cached = JSON.parse(fs.readFileSync(_iconManifestFile(), 'utf-8'));
      return { success: true, cached: true, count: cached.length, icons: cached };
    }
    const html = await _httpsGet(ICON_LIB_INDEX);
    const icons = _parseIconManifest(html);
    if (!icons.length) return { success: false, error: 'no icons parsed from index page' };
    fs.writeFileSync(_iconManifestFile(), JSON.stringify(icons));
    return { success: true, cached: false, count: icons.length, icons };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// Get an SVG by its relative manifest path. Downloads + caches on first
// request. Returns the raw SVG string.
ipcMain.handle('icons:get-svg', async (_e, relPath) => {
  try {
    if (typeof relPath !== 'string' || relPath.includes('..')) {
      return { success: false, error: 'invalid path' };
    }
    const cacheFile = path.join(_iconSvgDir(), relPath);
    if (fs.existsSync(cacheFile)) {
      return { success: true, cached: true, svg: fs.readFileSync(cacheFile, 'utf-8') };
    }
    const svg = await _httpsGet(ICON_LIB_INDEX + relPath);
    fs.mkdirSync(path.dirname(cacheFile), { recursive: true });
    fs.writeFileSync(cacheFile, svg);
    return { success: true, cached: false, svg };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('icons:clear-cache', () => {
  try {
    if (fs.existsSync(_iconCacheRoot())) fs.rmSync(_iconCacheRoot(), { recursive: true, force: true });
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// ── Tasks & Schedule ──────────────────────────────────────────────────────
// Main process owns the merged task list: manual tasks from the store plus
// derived tasks scanned out of the workspace, with overlay state (sticky
// geometry / snooze) folded in. The renderer asks for the merged view; it
// never assembles it itself, so the scheduler and the UI can never disagree
// about what's due.
const taskService = require('./task-service');

const _tasks = {
  workspaceRoot: null,
  storeFile: null,
  store: taskService.emptyStore(),
  derived: [],
  merged: [],
  scanStats: null,
  scheduler: null,
};

function _tasksStoreFile() {
  return taskService.storePathFor(_tasks.workspaceRoot, app.getPath('userData'));
}

// Merge = manual tasks + derived tasks, with per-derived overlay applied.
// Derived tasks whose overlay marks them done are filtered to `done` without
// touching the file — lets someone tick a TODO off without a source edit.
function _rebuildMergedTasks() {
  const overlay = _tasks.store.overlay || {};
  const manual = (_tasks.store.tasks || []).map(taskService.normalizeTask);
  const derived = (_tasks.derived || []).map(d => {
    const ov = overlay[d.id] || {};
    return taskService.normalizeTask({
      ...d,
      status: ov.status || d.status,
      sticky: ov.sticky || null,
      snoozedUntil: ov.snoozedUntil || null,
      remindMinsBefore: ov.remindMinsBefore ?? null,
      notes: ov.notes || '',
      completedAt: ov.completedAt || null,
    });
  });
  _tasks.merged = [...manual, ...derived];
  if (_tasks.scheduler) _tasks.scheduler.setTasks(_tasks.merged);
  _updateTaskBadge();
  return _tasks.merged;
}

// Tray tooltip + taskbar overlay reflect how much is overdue / due today, so
// the count is visible even when the window is hidden in tray mode.
function _updateTaskBadge() {
  const now = Date.now();
  const endOfDay = new Date(); endOfDay.setHours(23, 59, 59, 999);
  let overdue = 0, today = 0;
  for (const t of _tasks.merged) {
    if (t.status === 'done' || !t.due) continue;
    const due = new Date(t.due).getTime();
    if (isNaN(due)) continue;
    if (due < now) overdue++;
    else if (due <= endOfDay.getTime()) today++;
  }
  const total = overdue + today;
  try {
    if (tray && !tray.isDestroyed()) {
      tray.setToolTip(total
        ? `Note++ — ${overdue} overdue, ${today} due today`
        : 'Note++');
    }
  } catch {}
  try {
    if (mainWindow && !mainWindow.isDestroyed()) {
      if (process.platform === 'darwin') {
        app.setBadgeCount(overdue);
      } else if (process.platform === 'win32') {
        // Windows taskbar overlay — a small numeric badge on the app icon.
        if (overdue > 0) {
          const label = overdue > 9 ? '9+' : String(overdue);
          const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32">
            <circle cx="16" cy="16" r="15" fill="#e5534b"/>
            <text x="16" y="22" font-family="Segoe UI,sans-serif" font-size="${overdue > 9 ? 14 : 18}"
                  fill="#fff" text-anchor="middle" font-weight="600">${label}</text></svg>`;
          const img = nativeImage.createFromDataURL(
            'data:image/svg+xml;base64,' + Buffer.from(svg).toString('base64'));
          mainWindow.setOverlayIcon(img, `${overdue} overdue tasks`);
        } else {
          mainWindow.setOverlayIcon(null, '');
        }
      }
    }
  } catch {}
  return { overdue, today, total };
}

function _sendToRenderer(channel, ...args) {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(channel, ...args);
  }
}

function _initTaskScheduler() {
  if (_tasks.scheduler) return _tasks.scheduler;
  _tasks.scheduler = new taskService.ReminderScheduler((task, meta) => {
    // Native OS notification — fires regardless of window visibility, which
    // is the whole point of running in the tray.
    try {
      if (Notification.isSupported()) {
        const n = new Notification({
          title: meta.overdue ? '⏰ Overdue task' : '⏰ Task due',
          body: task.title + (task.source?.filePath
            ? `\n${path.basename(task.source.filePath)}:${task.source.line}` : ''),
          urgency: task.priority >= 3 ? 'critical' : 'normal',
          timeoutType: 'default',
        });
        n.on('click', () => {
          showOrCreateMainWindow();
          _sendToRenderer('task-notification-click', task.id);
        });
        n.show();
      }
    } catch (err) { console.error('[tasks] notification failed:', err); }
    _sendToRenderer('task-reminder-fired', { task, meta });
  });
  _tasks.scheduler.start();
  return _tasks.scheduler;
}

// Point the task system at a workspace (or null for the personal store).
// Reloads the store and rescans. Called by the renderer whenever the folder
// tree root changes.
ipcMain.handle('tasks:set-workspace', async (_e, root) => {
  try {
    const normalized = root && typeof root === 'string' ? root : null;
    const rootChanged = normalized !== _tasks.workspaceRoot;
    _tasks.workspaceRoot = normalized;

    // The store is global (see storePathFor) — load it once and keep it.
    // Changing the scan root must never swap the user's task list.
    if (!_tasks.storeFile) {
      _tasks.storeFile = _tasksStoreFile();
      _tasks.store = taskService.loadStore(_tasks.storeFile);
    }
    // Only the DERIVED half is workspace-dependent, so that's all we clear.
    if (rootChanged) {
      _tasks.derived = [];
      _tasks.scanStats = null;
    }
    _initTaskScheduler();
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged, workspaceRoot: _tasks.workspaceRoot };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('tasks:list', () => {
  _initTaskScheduler();
  return {
    success: true,
    tasks: _tasks.merged,
    workspaceRoot: _tasks.workspaceRoot,
    scanStats: _tasks.scanStats,
    badge: _updateTaskBadge(),
  };
});

// Full workspace scan for TODO/FIXME comments + markdown checkboxes.
ipcMain.handle('tasks:scan', async () => {
  if (!_tasks.workspaceRoot) {
    _tasks.derived = [];
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged, scanStats: null, noWorkspace: true };
  }
  try {
    const { tasks, stats } = await taskService.scanWorkspace(_tasks.workspaceRoot);
    // Dedupe by id — the same text in the same file twice is one task.
    const seen = new Set();
    _tasks.derived = tasks.filter(t => {
      if (seen.has(t.id)) return false;
      seen.add(t.id); return true;
    });
    _tasks.scanStats = stats;
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged, scanStats: stats };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// Create / update a manual task.
ipcMain.handle('tasks:save', (_e, task) => {
  try {
    const t = taskService.normalizeTask(task || {});
    if (!t.title) return { success: false, error: 'Task needs a title' };
    const list = _tasks.store.tasks || (_tasks.store.tasks = []);
    const idx = list.findIndex(x => x.id === t.id);
    if (idx >= 0) list[idx] = t; else list.push(t);
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    return { success: true, task: t, tasks: _tasks.merged };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('tasks:delete', (_e, id) => {
  try {
    _tasks.store.tasks = (_tasks.store.tasks || []).filter(t => t.id !== id);
    if (_tasks.store.overlay) delete _tasks.store.overlay[id];
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// Toggle done. For derived tasks this writes back to the source file so the
// change lives in git — a checked markdown box becomes `[x]`, a completed
// code TODO becomes `DONE`. Falls back to overlay if the rewrite fails
// (file moved, read-only, etc.) so the UI still reflects the user's intent.
ipcMain.handle('tasks:toggle-done', (_e, id, done) => {
  try {
    const task = _tasks.merged.find(t => t.id === id);
    if (!task) return { success: false, error: 'not found' };
    const kind = task.source?.kind;

    if (kind === 'manual') {
      const t = (_tasks.store.tasks || []).find(x => x.id === id);
      if (t) {
        t.status = done ? 'done' : 'open';
        t.completedAt = done ? Date.now() : null;
      }
    } else {
      let rewrote = false;
      try {
        if (kind === 'markdown') {
          rewrote = taskService.toggleMarkdownCheckbox(task.source.filePath, task.source.line, done);
        } else if (kind === 'code' && done) {
          rewrote = taskService.markCodeTagDone(task.source.filePath, task.source.line);
        }
      } catch (err) {
        console.warn('[tasks] source rewrite failed, falling back to overlay:', err.message);
      }
      // Overlay always records intent — belt and braces if the rewrite missed.
      _tasks.store.overlay = _tasks.store.overlay || {};
      _tasks.store.overlay[id] = {
        ..._tasks.store.overlay[id],
        status: done ? 'done' : 'open',
        completedAt: done ? Date.now() : null,
      };
      if (rewrote) _sendToRenderer('task-source-changed', task.source.filePath);
    }
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// Change a due date. For derived tasks this rewrites the `due:` token in the
// source line, which is what makes calendar drag-to-reschedule work.
ipcMain.handle('tasks:set-due', (_e, id, dueIso) => {
  try {
    const task = _tasks.merged.find(t => t.id === id);
    if (!task) return { success: false, error: 'not found' };
    if (task.source?.kind === 'manual') {
      const t = (_tasks.store.tasks || []).find(x => x.id === id);
      if (t) t.due = dueIso || null;
    } else {
      try {
        const ok = taskService.setSourceDue(task.source.filePath, task.source.line, dueIso);
        if (ok) _sendToRenderer('task-source-changed', task.source.filePath);
      } catch (err) {
        return { success: false, error: 'Could not update the source file: ' + err.message };
      }
    }
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    return { success: true, tasks: _tasks.merged };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('tasks:snooze', (_e, id, minutes) => {
  try {
    const until = _tasks.scheduler?.snooze(id, minutes);
    // Persist so a restart doesn't resurrect the reminder immediately.
    _tasks.store.overlay = _tasks.store.overlay || {};
    const manual = (_tasks.store.tasks || []).find(x => x.id === id);
    if (manual) manual.snoozedUntil = until;
    else _tasks.store.overlay[id] = { ..._tasks.store.overlay[id], snoozedUntil: until };
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    return { success: true, until, tasks: _tasks.merged };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('tasks:badge', () => _updateTaskBadge());

// ── Sticky note windows ───────────────────────────────────────────────────
// A sticky note is not a separate entity — it's a task rendered as a small
// frameless always-on-top window. One data model, two presentations. Same
// aux-window pattern as the updater window.
const _stickyWindows = new Map();   // taskId → BrowserWindow

const STICKY_COLORS = {
  yellow: { bg: '#fef3a8', fg: '#4a3f00', accent: '#e8d24a' },
  green:  { bg: '#c8f0c4', fg: '#14401a', accent: '#6fc96f' },
  blue:   { bg: '#c3e4fb', fg: '#0b3350', accent: '#5aaeea' },
  pink:   { bg: '#fbd0e2', fg: '#4d0f2c', accent: '#ea8ab5' },
  purple: { bg: '#e0d4fb', fg: '#2e1a53', accent: '#a98ce8' },
  grey:   { bg: '#e2e5e9', fg: '#25292e', accent: '#a8b0b8' },
};

function _persistStickyState(taskId, patch) {
  const manual = (_tasks.store.tasks || []).find(t => t.id === taskId);
  if (manual) {
    manual.sticky = { ...(manual.sticky || {}), ...patch };
  } else {
    _tasks.store.overlay = _tasks.store.overlay || {};
    const prev = _tasks.store.overlay[taskId] || {};
    _tasks.store.overlay[taskId] = { ...prev, sticky: { ...(prev.sticky || {}), ...patch } };
  }
  taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
  _rebuildMergedTasks();
}

function _openStickyWindow(task) {
  const existing = _stickyWindows.get(task.id);
  if (existing && !existing.isDestroyed()) { existing.focus(); return existing; }

  const s = task.sticky || {};
  const color = STICKY_COLORS[s.color] ? s.color : 'yellow';
  const win = new BrowserWindow({
    width:  Math.max(200, s.w || 280),
    height: Math.max(160, s.h || 220),
    x: Number.isFinite(s.x) ? s.x : undefined,
    y: Number.isFinite(s.y) ? s.y : undefined,
    frame: false,
    transparent: false,
    resizable: true,
    minimizable: false,
    maximizable: false,
    fullscreenable: false,
    alwaysOnTop: true,
    skipTaskbar: true,          // stickies shouldn't clutter the taskbar
    title: task.title || 'Sticky',
    backgroundColor: STICKY_COLORS[color].bg,
    webPreferences: {
      nodeIntegration: false,
      contextIsolation: true,
      preload: path.join(__dirname, 'sticky-preload.js'),
      additionalArguments: [`--sticky-task-id=${task.id}`],
    },
  });
  win.setMenuBarVisibility(false);
  win.loadFile(path.join(__dirname, 'sticky.html'));

  // Persist geometry as the user moves/resizes. Debounced so a drag doesn't
  // hammer the store with a write per pixel.
  let geomTimer = null;
  const saveGeom = () => {
    clearTimeout(geomTimer);
    geomTimer = setTimeout(() => {
      if (win.isDestroyed()) return;
      const [x, y] = win.getPosition();
      const [w, h] = win.getSize();
      _persistStickyState(task.id, { x, y, w, h, open: true });
    }, 400);
  };
  win.on('move', saveGeom);
  win.on('resize', saveGeom);
  win.on('closed', () => {
    clearTimeout(geomTimer);
    _stickyWindows.delete(task.id);
  });

  _stickyWindows.set(task.id, win);
  return win;
}

ipcMain.handle('sticky:open', (_e, taskId) => {
  try {
    const task = _tasks.merged.find(t => t.id === taskId);
    if (!task) return { success: false, error: 'task not found' };
    _persistStickyState(taskId, { open: true, color: task.sticky?.color || 'yellow' });
    const fresh = _tasks.merged.find(t => t.id === taskId) || task;
    _openStickyWindow(fresh);
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

ipcMain.handle('sticky:close', (_e, taskId) => {
  const win = _stickyWindows.get(taskId);
  if (win && !win.isDestroyed()) win.close();
  _persistStickyState(taskId, { open: false });
  return { success: true };
});

// The sticky window asks who it is on load.
ipcMain.handle('sticky:get-task', (_e, taskId) => {
  const task = _tasks.merged.find(t => t.id === taskId);
  if (!task) return { success: false, error: 'not found' };
  const color = STICKY_COLORS[task.sticky?.color] ? task.sticky.color : 'yellow';
  return { success: true, task, palette: STICKY_COLORS[color], colorName: color, colors: STICKY_COLORS };
});

ipcMain.handle('sticky:set-color', (_e, taskId, colorName) => {
  if (!STICKY_COLORS[colorName]) return { success: false, error: 'bad colour' };
  _persistStickyState(taskId, { color: colorName });
  const win = _stickyWindows.get(taskId);
  if (win && !win.isDestroyed()) {
    win.webContents.send('sticky-color-changed', colorName, STICKY_COLORS[colorName]);
  }
  return { success: true, palette: STICKY_COLORS[colorName] };
});

// Sticky edits write straight through to the task store.
ipcMain.handle('sticky:update-task', (_e, taskId, patch) => {
  try {
    const manual = (_tasks.store.tasks || []).find(t => t.id === taskId);
    if (manual) {
      if (typeof patch.title === 'string') manual.title = patch.title;
      if (typeof patch.notes === 'string') manual.notes = patch.notes;
      if (patch.status) { manual.status = patch.status; manual.completedAt = patch.status === 'done' ? Date.now() : null; }
    } else {
      _tasks.store.overlay = _tasks.store.overlay || {};
      _tasks.store.overlay[taskId] = { ..._tasks.store.overlay[taskId], ...patch };
    }
    taskService.saveStore(_tasks.storeFile || _tasksStoreFile(), _tasks.store);
    _rebuildMergedTasks();
    _sendToRenderer('tasks-changed');
    return { success: true };
  } catch (err) {
    return { success: false, error: err.message || String(err) };
  }
});

// Re-open every sticky the user had open last session.
function restoreOpenStickies() {
  try {
    for (const t of _tasks.merged) {
      if (t.sticky && t.sticky.open) _openStickyWindow(t);
    }
  } catch (err) { console.warn('[sticky] restore failed:', err.message); }
}
ipcMain.handle('sticky:restore-all', () => { restoreOpenStickies(); return { success: true }; });

ipcMain.handle('drawio:status',    () => getDrawioService().getStatus());
ipcMain.handle('drawio:download',  async () => {
  try { return await getDrawioService().download(); }
  catch (err) { return { success: false, error: err.message }; }
});
ipcMain.handle('drawio:uninstall', () => getDrawioService().uninstall());

// ── LSP service ──────────────────────────────────────────────────────────
let lspService = null;
function getLspService() {
  if (!lspService) lspService = require('./lsp-service');
  return lspService;
}

ipcMain.handle('lsp-ensure-started', async (e, { langId, workspaceRoot }) =>
  getLspService().ensureStarted(langId, workspaceRoot, e.sender));

ipcMain.handle('lsp-send', async (e, { langId, method, params }) => {
  try {
    const result = await getLspService().sendRequest(langId, method, params);
    return { success: true, result };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('lsp-notify', (e, { langId, method, params }) => {
  getLspService().sendNotification(langId, method, params);
  return { success: true };
});

ipcMain.handle('lsp-stop', (e, { langId }) => getLspService().stopServer(langId));

ipcMain.handle('lsp-language-for', (e, monacoId) => getLspService().lookupByMonacoId(monacoId));
ipcMain.handle('lsp-language-config', (e, langId) => getLspService().getLanguageConfig(langId));

app.on('before-quit', () => { if (lspService) lspService.stopAllServers(); });

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
  // Windows: bypass the OS publisher-name verification. Our installer is
  // signed with a self-signed cert whose root isn't in Windows' trust
  // store, so Get-AuthenticodeSignature returns `UnknownError` after the
  // download — that makes electron-updater suppress `update-downloaded`
  // and leaves the toolbar pill stuck at "Downloading 100%". The integrity
  // of the downloaded file is still guaranteed by the sha512 in latest.yml
  // (served over HTTPS from the GitHub release), so dropping the CN check
  // is safe. Revisit when we ship with a CA-issued code-signing cert.
  if (process.platform === 'win32') {
    try {
      autoUpdater.verifyUpdateCodeSignature = () => Promise.resolve(null);
    } catch (e) { console.error('[auto-update] failed to override signature verifier:', e?.message || e); }
  }
  autoUpdater.on('error', (e) => {
    console.error('[auto-update] error:', e?.message || e);
    // Surface the error to the renderer so the pill leaves the
    // "Downloading…" state instead of looking frozen.
    send('auto-update-status', { state: 'error', error: e?.message || String(e) });
  });
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

app.whenReady().then(() => {
  // Defer updater bootstrap so first-launch UI work wins the startup race.
  setTimeout(() => { scheduleAutoUpdateCheck(); }, 15000);
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
  if (!gotTheLock) return;

  const stayInTray = _isStayInTrayEnabled();

  // ── Boot strategy ────────────────────────────────────────────────────────
  // HIDDEN_BOOT (--hidden flag from OS autostart) OR explicit tray mode
  // means "stay resident, no window unless asked". In that case we skip
  // createWindow() entirely — the renderer + Monaco aren't loaded until
  // the user first interacts, which keeps idle memory at the Electron
  // base (~40 MB) instead of a fully-loaded renderer (~250 MB).
  if (HIDDEN_BOOT) {
    // Tray-only boot. If the tray API isn't available (rare: headless
    // Linux without a system-tray, missing icon asset, etc.) fall back
    // to a normal window so the user isn't stranded with no UI.
    const t = createTray();
    if (!t) createWindow();
  } else {
    createWindow();
    if (stayInTray) createTray();
  }

  // Defer drawio protocol wiring — it requires `drawio-service.js` which
  // pulls in `extract-zip` (and through it, several KB of zlib/yauzl
  // setup). The renderer doesn't need the drawio:// scheme until the
  // user actually opens a .drawio tab, so push this off the critical
  // path. setImmediate runs after current I/O completes, giving the
  // renderer load a clean main-process thread.
  setImmediate(() => {
    try { getDrawioService().registerProtocol(); }
    catch (e) { console.error('[drawio] protocol register failed:', e); }
  });
});
app.on('window-all-closed', () => {
  // When we have a tray icon, "all windows closed" is not a quit signal —
  // it's just the user hiding the editor while keeping Note++ resident.
  if (tray && !tray.isDestroyed()) return;
  if (process.platform !== 'darwin') app.quit();
});
app.on('activate', () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
app.on('quit', () => { terminalProcesses.forEach(p => p.kill()); });
