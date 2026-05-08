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
  terminalCreate: (id)         => ipcRenderer.invoke('terminal-create', id),
  terminalInput:  (id, data)   => ipcRenderer.invoke('terminal-input', id, data),
  terminalKill:   (id)         => ipcRenderer.invoke('terminal-kill', id),
  terminalResize: (id, c, r)   => ipcRenderer.invoke('terminal-resize', id, c, r),
  runCommand:     (cmd, cwd)   => ipcRenderer.invoke('run-command', cmd, cwd),

  getUserDataPath:   () => ipcRenderer.invoke('get-user-data-path'),
  writeSession:      (data) => ipcRenderer.invoke('write-session', data),
  readSession:       () => ipcRenderer.invoke('read-session'),
  readSettings:      () => ipcRenderer.invoke('read-settings'),
  writeSettings:     (data) => ipcRenderer.invoke('write-settings', data),
  detectCloudPaths:  () => ipcRenderer.invoke('detect-cloud-paths'),
  validatePath:      (p) => ipcRenderer.invoke('validate-path', p),
  openFolderPicker:  () => ipcRenderer.invoke('open-folder-picker'),

  // Menu events (main → renderer)
  onMenu: (channel, cb) => ipcRenderer.on(channel, (e, ...args) => cb(...args)),
  removeMenuListener: (ch) => ipcRenderer.removeAllListeners(ch),

  // AI Assistant (Ollama)
  aiCheck:      ()               => ipcRenderer.invoke('ai-check'),
  aiGenerate:   (opts)           => ipcRenderer.invoke('ai-generate', opts),
  aiAbort:      ()               => ipcRenderer.invoke('ai-abort'),
  aiPull:       (model)          => ipcRenderer.invoke('ai-pull', model),
  onAiToken:    (cb)             => ipcRenderer.on('ai-token',        (e, t)    => cb(t)),
  onAiDone:     (cb)             => ipcRenderer.on('ai-done',         ()        => cb()),
  onAiProgress: (cb)             => ipcRenderer.on('ai-pull-progress',(e, d)    => cb(d)),
  removeAiListeners: ()          => {
    ipcRenderer.removeAllListeners('ai-token');
    ipcRenderer.removeAllListeners('ai-done');
    ipcRenderer.removeAllListeners('ai-pull-progress');
  },
  openUrl: (url) => ipcRenderer.invoke('open-url', url),
});
