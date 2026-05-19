const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('electronAPI', {
  openDialog:    (opts) => ipcRenderer.invoke('dialog-open', opts),
  saveDialog:    (opts) => ipcRenderer.invoke('dialog-save', opts),
  messageDialog: (opts) => ipcRenderer.invoke('dialog-message', opts),
  readFile:      (p)    => ipcRenderer.invoke('read-file', p),
  writeFile:       (p, c)    => ipcRenderer.invoke('write-file', p, c),
  writeFileBinary: (p, b64)  => ipcRenderer.invoke('write-file-binary', p, b64),
  listDir:       (p)    => ipcRenderer.invoke('list-dir', p),
  setTitle:      (t)    => ipcRenderer.invoke('set-title', t),
  closeWindow:   ()     => ipcRenderer.invoke('close-window'),
  shellOpen:     (p)    => ipcRenderer.invoke('shell-open', p),
  shellShowItem: (p)    => ipcRenderer.invoke('shell-show-item', p),

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
