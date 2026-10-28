'use strict';
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('moulin', {
  onState: (fn) => ipcRenderer.on('overlay:state', (_event, state) => fn(state)),
  onCommand: (fn) => ipcRenderer.on('rec', (_event, command) => fn(command)),
  started: () => ipcRenderer.send('rec:started'),
  audio: (bytes, mimeType, durationMs) => ipcRenderer.send('rec:audio', bytes, mimeType, durationMs),
  error: (message) => ipcRenderer.send('rec:error', message),
  clicked: () => ipcRenderer.send('overlay:clicked'),
});
