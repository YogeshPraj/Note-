'use strict';
// =============================================================================
// sticky-preload.js — narrow bridge for sticky-note windows.
// Deliberately tiny: a sticky can read its own task and write back title /
// notes / done-state / colour. It cannot touch the filesystem or any other
// task. Same isolation posture as updater-preload.js.
// =============================================================================

const { contextBridge, ipcRenderer } = require('electron');

// The window's task id is injected via additionalArguments at creation time
// so the page knows who it is without a round-trip.
const taskIdArg = process.argv.find(a => a.startsWith('--sticky-task-id='));
const TASK_ID = taskIdArg ? taskIdArg.split('=')[1] : null;

contextBridge.exposeInMainWorld('stickyAPI', {
  taskId: TASK_ID,
  getTask:    ()        => ipcRenderer.invoke('sticky:get-task', TASK_ID),
  update:     (patch)   => ipcRenderer.invoke('sticky:update-task', TASK_ID, patch),
  setColor:   (color)   => ipcRenderer.invoke('sticky:set-color', TASK_ID, color),
  close:      ()        => ipcRenderer.invoke('sticky:close', TASK_ID),
  onColorChanged: (cb) => {
    const h = (_e, name, palette) => cb(name, palette);
    ipcRenderer.on('sticky-color-changed', h);
    return () => ipcRenderer.removeListener('sticky-color-changed', h);
  },
});
