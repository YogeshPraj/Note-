'use strict';
// =============================================================================
// vault-preload.js — bridge for the isolated vault window.
//
// Narrow by design. This window is the ONLY renderer allowed to see plaintext
// secrets, and it earns that by never loading user content: no file previews,
// no iframes, no model output, strict CSP. The surface below is everything it
// can do — note there is no filesystem access, no shell, and no way to reach
// the main editor renderer except the one explicit "insert at cursor" call.
// =============================================================================

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('vaultAPI', {
  // ── Lifecycle ──────────────────────────────────────────────────────────
  status:          ()               => ipcRenderer.invoke('vault:status'),
  create:          (pw)             => ipcRenderer.invoke('vault:create', pw),
  unlock:          (pw)             => ipcRenderer.invoke('vault:unlock', pw),
  unlockRecovery:  (key)            => ipcRenderer.invoke('vault:unlock-recovery', key),
  lock:            ()               => ipcRenderer.invoke('vault:lock'),
  changePassword:  (oldPw, newPw)   => ipcRenderer.invoke('vault:change-password', oldPw, newPw),
  setAutoLock:     (mins)           => ipcRenderer.invoke('vault:set-autolock', mins),

  // ── Entries (summaries never include secret fields) ────────────────────
  list:            ()               => ipcRenderer.invoke('vault:list'),
  upsert:          (entry)          => ipcRenderer.invoke('vault:upsert', entry),
  remove:          (id)             => ipcRenderer.invoke('vault:remove', id),

  // ── Secret access — one field, one explicit action, at a time ──────────
  reveal:          (id, field)      => ipcRenderer.invoke('vault:reveal', id, field),
  copy:            (id, field)      => ipcRenderer.invoke('vault:copy', id, field),
  insertAtCursor:  (id, field)      => ipcRenderer.invoke('vault:insert', id, field),
  totp:            (id)             => ipcRenderer.invoke('vault:totp', id),
  generate:        (opts)           => ipcRenderer.invoke('vault:generate', opts),

  // ── Connected managers (read-only bridge) ──────────────────────────────
  cliDetect:       ()               => ipcRenderer.invoke('vault:cli-detect'),
  cliList:         (provider, q)    => ipcRenderer.invoke('vault:cli-list', provider, q),
  cliCopy:         (provider, id, f)=> ipcRenderer.invoke('vault:cli-copy', provider, id, f),
  cliInsert:       (provider, id, f)=> ipcRenderer.invoke('vault:cli-insert', provider, id, f),

  // ── Window chrome ──────────────────────────────────────────────────────
  close:           ()               => ipcRenderer.invoke('vault:close-window'),
  onLocked: (cb) => {
    const h = (_e, reason) => cb(reason);
    ipcRenderer.on('vault-locked', h);
    return () => ipcRenderer.removeListener('vault-locked', h);
  },
  onThemeChanged: (cb) => {
    const h = (_e, dark) => cb(dark);
    ipcRenderer.on('vault-theme', h);
    return () => ipcRenderer.removeListener('vault-theme', h);
  },
});
