// MeowLauncher 26.X.X — preload bridge (CommonJS, contextIsolation on).
// Everything the renderer may do is listed here; no node integration leaks.
const { contextBridge, ipcRenderer } = require('electron');

const invoke = (channel, payload) => ipcRenderer.invoke(channel, payload);

const api = Object.freeze({
  runtime: Object.freeze({
    electron: process.versions.electron,
    chrome: process.versions.chrome,
    node: process.versions.node,
  }),
  info: () => invoke('ml:info'),
  versionScheme: () => invoke('ml:version:scheme'),
  config: Object.freeze({
    get: () => invoke('ml:config:get'),
    set: (patch) => invoke('ml:config:set', patch),
  }),
  games: Object.freeze({
    list: () => invoke('ml:versions:list'),
    launch: (opts) => invoke('ml:game:launch', opts),
    stop: () => invoke('ml:game:stop'),
    status: () => invoke('ml:game:status'),
  }),
  dialog: Object.freeze({
    pickExe: () => invoke('ml:dialog:pick-exe'),
    pickDir: () => invoke('ml:dialog:pick-dir'),
  }),
  updater: Object.freeze({
    check: () => invoke('ml:updater:check'),
  }),
  log: Object.freeze({
    read: (max) => invoke('ml:log:read', max),
    write: (level, message, extra) => invoke('ml:log:append', { level, message, extra }),
  }),
  openPath: (target) => invoke('ml:open:path', target),
  openExternal: (url) => invoke('ml:open:external', url),
  window: Object.freeze({
    minimize: () => invoke('ml:window:minimize'),
    toggleMaximize: () => invoke('ml:window:toggle-maximize'),
    close: () => invoke('ml:window:close'),
  }),
  quit: () => invoke('ml:app:quit'),
});

contextBridge.exposeInMainWorld('meow', api);
