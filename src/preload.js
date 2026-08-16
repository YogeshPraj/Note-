const { contextBridge, ipcRenderer, webUtils } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  // Electron 32 removed File.path on the renderer side; resolve via webUtils.
  getPathForFile: (file) => webUtils.getPathForFile(file),
  openDialog:    (opts) => ipcRenderer.invoke('dialog-open', opts),
  saveDialog:    (opts) => ipcRenderer.invoke('dialog-save', opts),
  messageDialog: (opts) => ipcRenderer.invoke('dialog-message', opts),
  readFile:      (p)    => ipcRenderer.invoke('read-file', p),
  writeFile:       (p, c)    => ipcRenderer.invoke('write-file', p, c),
  writeFileBinary: (p, b64)  => ipcRenderer.invoke('write-file-binary', p, b64),
  listDir:       (p)    => ipcRenderer.invoke('list-dir', p),
  setTitle:           (t)           => ipcRenderer.invoke('set-title', t),
  setTitleBarOverlay: (color, sym)  => ipcRenderer.invoke('set-title-bar-overlay', color, sym),
  setAlwaysOnTop:       (flag)        => ipcRenderer.invoke('set-always-on-top', flag),
  setFullScreen:        (flag)        => ipcRenderer.invoke('set-full-screen', flag),
  setMenuBarVisibility: (flag)        => ipcRenderer.invoke('set-menu-bar-visibility', flag),
  closeWindow:   ()     => ipcRenderer.invoke('close-window'),
  relaunchApp:   ()     => ipcRenderer.invoke('relaunch-app'),
  shellOpen:     (p)    => ipcRenderer.invoke('shell-open', p),
  shellShowItem: (p)    => ipcRenderer.invoke('shell-show-item', p),

  // Startup / tray mode — "Launch Note++ on system startup" checkbox.
  startupMode: {
    get: () => ipcRenderer.invoke('startup-mode:get'),
    set: (enabled) => ipcRenderer.invoke('startup-mode:set', enabled),
  },

  // Spell-check (nspell + dictionary-en, lives in main process)
  spell: {
    check:   (words)    => ipcRenderer.invoke('spell:check', words),
    suggest: (word, max) => ipcRenderer.invoke('spell:suggest', word, max),
    addWord: (word)     => ipcRenderer.invoke('spell:add-word', word),
  },

  // Azure Icons library — fetch-and-cache from maskati.github.io/azure-icons.
  iconLib: {
    fetchManifest: (force)   => ipcRenderer.invoke('icons:fetch-manifest', !!force),
    getSvg:        (relPath) => ipcRenderer.invoke('icons:get-svg', relPath),
    clearCache:    ()        => ipcRenderer.invoke('icons:clear-cache'),
  },

  // Tasks & Schedule — manual tasks in .notepp/tasks.json plus tasks derived
  // from TODO/FIXME comments and markdown checkboxes in the workspace.
  tasks: {
    setWorkspace: (root)        => ipcRenderer.invoke('tasks:set-workspace', root),
    list:         ()            => ipcRenderer.invoke('tasks:list'),
    scan:         ()            => ipcRenderer.invoke('tasks:scan'),
    save:         (task)        => ipcRenderer.invoke('tasks:save', task),
    remove:       (id)          => ipcRenderer.invoke('tasks:delete', id),
    toggleDone:   (id, done)    => ipcRenderer.invoke('tasks:toggle-done', id, done),
    setDue:       (id, dueIso)  => ipcRenderer.invoke('tasks:set-due', id, dueIso),
    snooze:       (id, mins)    => ipcRenderer.invoke('tasks:snooze', id, mins),
    badge:        ()            => ipcRenderer.invoke('tasks:badge'),
    // Main → renderer events
    onReminder:   (cb) => { const h = (_e, p) => cb(p); ipcRenderer.on('task-reminder-fired', h); return () => ipcRenderer.removeListener('task-reminder-fired', h); },
    onNotifClick: (cb) => { const h = (_e, id) => cb(id); ipcRenderer.on('task-notification-click', h); return () => ipcRenderer.removeListener('task-notification-click', h); },
    onSourceChanged: (cb) => { const h = (_e, fp) => cb(fp); ipcRenderer.on('task-source-changed', h); return () => ipcRenderer.removeListener('task-source-changed', h); },
    onChanged:    (cb) => { const h = () => cb(); ipcRenderer.on('tasks-changed', h); return () => ipcRenderer.removeListener('tasks-changed', h); },
  },

  // Password manager — drives the official Bitwarden CLI that main downloads
  // on demand. Note++ owns no vault: listing returns metadata only, and a
  // secret crosses this bridge only for a single explicit Reveal. Copy and
  // Insert are serviced entirely in main so the value never lands here.
  vault: {
    status:        ()            => ipcRenderer.invoke('vault:status'),
    install:       ()            => ipcRenderer.invoke('vault:install'),
    uninstall:     ()            => ipcRenderer.invoke('vault:uninstall'),
    // Unlock paths, best first: adopt an existing BW_SESSION, accept a token
    // the user made in the terminal, or (last resort) take the password here.
    adoptSession:  ()            => ipcRenderer.invoke('vault:adopt-session'),
    useToken:      (tok)         => ipcRenderer.invoke('vault:use-token', tok),
    unlock:        (pw)          => ipcRenderer.invoke('vault:unlock', pw),
    login:         (email, pw)   => ipcRenderer.invoke('vault:login', email, pw),
    lock:          ()            => ipcRenderer.invoke('vault:lock'),
    sync:          ()            => ipcRenderer.invoke('vault:sync'),
    setAutoLock:   (mins)        => ipcRenderer.invoke('vault:set-autolock', mins),
    list:          (query)       => ipcRenderer.invoke('vault:list', query),
    reveal:        (id, field)   => ipcRenderer.invoke('vault:reveal', id, field),
    copy:          (id, field)   => ipcRenderer.invoke('vault:copy', id, field),
    insert:        (id, field)   => ipcRenderer.invoke('vault:insert', id, field),
    // Main refuses to serve a secret unless the Passwords tab is in front, so
    // a background injection can't quietly drain the vault.
    setTabActive:  (active)      => ipcRenderer.send('vault:tab-active', !!active),
    onInstallProgress: (cb) => { const h = (_e, p) => cb(p); ipcRenderer.on('vault-install-progress', h); return () => ipcRenderer.removeListener('vault-install-progress', h); },
    onLocked:      (cb) => { const h = (_e, r) => cb(r); ipcRenderer.on('vault-locked', h); return () => ipcRenderer.removeListener('vault-locked', h); },
    onInsertText:  (cb) => { const h = (_e, t) => cb(t); ipcRenderer.on('vault-insert-text', h); return () => ipcRenderer.removeListener('vault-insert-text', h); },
  },

  // Sticky notes — a task rendered as a floating always-on-top window.
  sticky: {
    open:       (id)          => ipcRenderer.invoke('sticky:open', id),
    close:      (id)          => ipcRenderer.invoke('sticky:close', id),
    setColor:   (id, color)   => ipcRenderer.invoke('sticky:set-color', id, color),
    restoreAll: ()            => ipcRenderer.invoke('sticky:restore-all'),
  },

  // Markdown preview export (HTML / PDF / DOCX). HTML is just `writeFile`;
  // PDF and DOCX route through main-process converters that return base64.

  previewExport: {
    toPdf:  (html) => ipcRenderer.invoke('preview-export:to-pdf', html),
    toDocx: (html) => ipcRenderer.invoke('preview-export:to-docx', html),
  },

  // Binary-to-Markdown conversion (PDF / DOCX)
  convert: {
    canConvert:    (path)             => ipcRenderer.invoke('convert-to-markdown:can-convert', path),
    supportedExts: ()                 => ipcRenderer.invoke('convert-to-markdown:supported-exts'),
    start:         (path, jobId)      => ipcRenderer.invoke('convert-to-markdown:start', path, jobId),
    onProgress:    (cb)               => {
      const handler = (_e, payload) => cb(payload);
      ipcRenderer.on('convert-to-markdown:progress', handler);
      return () => ipcRenderer.removeListener('convert-to-markdown:progress', handler);
    },
  },

  // Terminal
  terminalCreate: (id, opts)   => ipcRenderer.invoke('terminal-create', id, opts),
  terminalInput:  (id, data)   => ipcRenderer.invoke('terminal-input', id, data),
  terminalKill:   (id)         => ipcRenderer.invoke('terminal-kill', id),
  terminalResize: (id, c, r)   => ipcRenderer.invoke('terminal-resize', id, c, r),
  runCommand:     (cmd, cwd)   => ipcRenderer.invoke('run-command', cmd, cwd),

  getUserDataPath:   () => ipcRenderer.invoke('get-user-data-path'),
  backupFiles:       (files, opts) => ipcRenderer.invoke('backup-files', files, opts),
  getBackupRoot:     (customPath)  => ipcRenderer.invoke('get-backup-root', customPath),
  writeSession:      (data) => ipcRenderer.invoke('write-session', data),
  readSession:       () => ipcRenderer.invoke('read-session'),
  readSettings:      () => ipcRenderer.invoke('read-settings'),
  writeSettings:     (data) => ipcRenderer.invoke('write-settings', data),
  detectCloudPaths:  () => ipcRenderer.invoke('detect-cloud-paths'),
  validatePath:      (p) => ipcRenderer.invoke('validate-path', p),
  openFolderPicker:  () => ipcRenderer.invoke('open-folder-picker'),
  findInFiles:       (opts) => ipcRenderer.invoke('find-in-files', opts),
  compareFolders:    (opts) => ipcRenderer.invoke('compare-folders', opts),

  // Menu events (main → renderer)
  onMenu: (channel, cb) => ipcRenderer.on(channel, (e, ...args) => cb(...args)),
  removeMenuListener: (ch) => ipcRenderer.removeAllListeners(ch),

  // AI Assistant (Ollama)
  aiCheck:           ()              => ipcRenderer.invoke('ai-check'),
  aiDetectInstalled: ()              => ipcRenderer.invoke('ai-detect-installed'),
  aiStartServer:     (path)          => ipcRenderer.invoke('ai-start-server', path),
  aiGenerate:        (opts)          => ipcRenderer.invoke('ai-generate', opts),
  aiChat:            (opts)          => ipcRenderer.invoke('ai-chat', opts),
  aiAbort:           ()              => ipcRenderer.invoke('ai-abort'),
  aiPull:            (model)         => ipcRenderer.invoke('ai-pull', model),
  onAiToken:    (cb)             => ipcRenderer.on('ai-token',        (e, t)    => cb(t)),
  onAiDone:     (cb)             => ipcRenderer.on('ai-done',         ()        => cb()),
  onAiProgress: (cb)             => ipcRenderer.on('ai-pull-progress',(e, d)    => cb(d)),
  removeAiListeners: ()          => {
    ipcRenderer.removeAllListeners('ai-token');
    ipcRenderer.removeAllListeners('ai-done');
    ipcRenderer.removeAllListeners('ai-pull-progress');
  },
  openUrl: (url) => ipcRenderer.invoke('open-url', url),

  // Renderer signals it has finished wiring listeners — main flushes any
  // file-open args queued from double-click / "Open with"
  rendererReady: () => ipcRenderer.invoke('renderer-ready'),

  // Recent Files — main owns the persisted list + menu
  recentFileOpened: (filePath) => ipcRenderer.invoke('recent-file-opened', filePath),
  recentFilesGet:   ()         => ipcRenderer.invoke('recent-files-get'),
  recentFilesClear: ()         => ipcRenderer.invoke('recent-files-clear'),

  // External file-change watcher
  watchFile:        (filePath) => ipcRenderer.invoke('watch-file', filePath),
  unwatchFile:      (filePath) => ipcRenderer.invoke('unwatch-file', filePath),
  fileSavedByApp:   (filePath) => ipcRenderer.invoke('file-saved-by-app', filePath),
  onFileChangedExternally: (cb) =>
    ipcRenderer.on('file-changed-externally', (e, data) => cb(data)),
  removeFileChangedListener: () =>
    ipcRenderer.removeAllListeners('file-changed-externally'),

  // Project context — AGENTS.md + .notepp/memory.md (auto-injected into AI prompts)
  projectContext: {
    find:         (path)            => ipcRenderer.invoke('project-context-find', path),
    ensureMemory: (folder)          => ipcRenderer.invoke('project-memory-ensure', folder),
  },

  // Git integration — see features/GIT.md
  git: {
    findRepo:     (path)            => ipcRenderer.invoke('git-find-repo', path),
    status:       (root)            => ipcRenderer.invoke('git-status', root),
    stage:        (root, paths)     => ipcRenderer.invoke('git-stage', root, paths),
    unstage:      (root, paths)     => ipcRenderer.invoke('git-unstage', root, paths),
    discard:      (root, paths)     => ipcRenderer.invoke('git-discard', root, paths),
    clean:        (root, paths)     => ipcRenderer.invoke('git-clean', root, paths),
    commit:       (root, msg)       => ipcRenderer.invoke('git-commit', root, msg),
    commitAmend:  (root, msg)       => ipcRenderer.invoke('git-commit-amend', root, msg),
    fetch:        (root)            => ipcRenderer.invoke('git-fetch', root),
    pull:         (root)            => ipcRenderer.invoke('git-pull', root),
    push:         (root)            => ipcRenderer.invoke('git-push', root),
    pushUpstream: (root, r, b)      => ipcRenderer.invoke('git-push-upstream', root, r, b),
    sync:         (root)            => ipcRenderer.invoke('git-sync', root),
    branchList:   (root)            => ipcRenderer.invoke('git-branch-list', root),
    branchSwitch: (root, name)      => ipcRenderer.invoke('git-branch-switch', root, name),
    branchCreate: (root, n, f)      => ipcRenderer.invoke('git-branch-create', root, n, f),
    installed:    ()                => ipcRenderer.invoke('git-installed'),
    showHead:     (root, relPath)   => ipcRenderer.invoke('git-show-head', root, relPath),
  },

  // LSP — see features/LSP.md
  lsp: {
    ensureStarted: (langId, workspaceRoot) =>
      ipcRenderer.invoke('lsp-ensure-started', { langId, workspaceRoot }),
    send:    (langId, method, params) => ipcRenderer.invoke('lsp-send',   { langId, method, params }),
    notify:  (langId, method, params) => ipcRenderer.invoke('lsp-notify', { langId, method, params }),
    stop:    (langId)               => ipcRenderer.invoke('lsp-stop',   { langId }),
    languageFor: (monacoId)         => ipcRenderer.invoke('lsp-language-for', monacoId),
    languageConfig: (langId)        => ipcRenderer.invoke('lsp-language-config', langId),
    install:        (langId)        => ipcRenderer.invoke('lsp-install', { langId }),
    onInstallOutput: (cb)           => ipcRenderer.on('lsp-install-output', (e, p) => cb(p)),
    onInstallDone:   (cb)           => ipcRenderer.on('lsp-install-done',   (e, p) => cb(p)),
    onStatus:       (cb)            => ipcRenderer.on('lsp-status',       (e, p) => cb(p)),
    onNotification: (cb)            => ipcRenderer.on('lsp-notification', (e, p) => cb(p)),
    removeListeners: () => {
      ipcRenderer.removeAllListeners('lsp-status');
      ipcRenderer.removeAllListeners('lsp-notification');
      ipcRenderer.removeAllListeners('lsp-install-output');
      ipcRenderer.removeAllListeners('lsp-install-done');
    },
  },

  // draw.io — on-demand download + persistent bundle in userData
  drawio: {
    status:     ()  => ipcRenderer.invoke('drawio:status'),
    download:   ()  => ipcRenderer.invoke('drawio:download'),
    uninstall:  ()  => ipcRenderer.invoke('drawio:uninstall'),
    onProgress: (cb) => ipcRenderer.on('drawio:progress', (e, p) => cb(p)),
    removeProgressListener: () => ipcRenderer.removeAllListeners('drawio:progress'),
  },

  // App version (from package.json — single source of truth)
  getAppVersion: () => ipcRenderer.invoke('get-app-version'),

  // Auto-update (electron-updater)
  autoUpdate: {
    get:           ()        => ipcRenderer.invoke('get-auto-update'),
    set:           (enabled) => ipcRenderer.invoke('set-auto-update', enabled),
    checkNow:      ()        => ipcRenderer.invoke('check-for-updates-now'),
    installNow:    ()        => ipcRenderer.invoke('updater:start-install-flow'),
    onStatus:      (cb)      => ipcRenderer.on('auto-update-status',       (e, p) => cb(p)),
    onPrefChanged: (cb)      => ipcRenderer.on('auto-update-pref-changed', (e, v) => cb(v)),
    removeListeners: () => {
      ipcRenderer.removeAllListeners('auto-update-status');
      ipcRenderer.removeAllListeners('auto-update-pref-changed');
    },
  },
});
