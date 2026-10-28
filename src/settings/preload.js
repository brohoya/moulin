'use strict';
const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('moulin', {
  get: () => ipcRenderer.invoke('settings:get'),
  set: (patch) => ipcRenderer.invoke('settings:set', patch),
  capture: (on) => ipcRenderer.send('settings:capture', on),
  signIn: () => ipcRenderer.invoke('account:sign-in'),
  signOut: () => ipcRenderer.invoke('account:sign-out'),
  openAccessibility: () => ipcRenderer.send('app:accessibility'),
  onStatus: (fn) => ipcRenderer.on('status', (_event, snapshot) => fn(snapshot)),
});
