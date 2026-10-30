// Preload for the updater window — keeps contextIsolation strict.
// Exposes only the two IPC calls the updater UI needs.
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('updaterApi', {
  getPendingVersion: () => ipcRenderer.invoke('updater:get-pending-version'),
  installAndQuit:    () => ipcRenderer.invoke('updater:install-and-quit'),
  minimize:          () => ipcRenderer.invoke('updater:minimize'),
});
