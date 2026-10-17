// Bridge to the Electron main process: window.meow is exposed by
// src/preload/preload.cjs. When the UI is opened in a plain browser (design
// preview / headless screenshot QA), a read-only mock keeps the shell renderable.
import mock from './mock.js';

const bridge = typeof window !== 'undefined' ? window.meow : null;

export const hasBridge = Boolean(bridge);

const call = (fn, fallback) => () => (bridge ? fn() : Promise.resolve(fallback));

export const api = {
  info: call(() => bridge.info(), mock.info),
  versionScheme: call(() => bridge.versionScheme(), mock.versionScheme),
  config: {
    get: call(() => bridge.config.get(), mock.config),
    set: (patch) => (bridge ? bridge.config.set(patch) : Promise.resolve({ ok: true, data: { ...mock.config, ...patch } })),
  },
  games: {
    list: call(() => bridge.games.list(), mock.versions),
    launch: (opts) => (bridge ? bridge.games.launch(opts) : Promise.resolve({ ok: false, error: { code: 'preview', message: '预览模式不能启动游戏' } })),
    stop: call(() => bridge.games.stop(), { ok: true, data: { stopped: false } }),
    status: call(() => bridge.games.status(), mock.status),
  },
  dialog: {
    pickDir: call(() => bridge.dialog.pickDir(), { ok: true, data: { canceled: true } }),
    pickExe: call(() => bridge.dialog.pickExe(), { ok: true, data: { canceled: true } }),
  },
  updater: {
    check: call(() => bridge.updater.check(), { ok: true, data: { available: false, current: '26.1.0', reason: 'preview' } }),
  },
  log: {
    read: (max) => (bridge ? bridge.log.read(max) : Promise.resolve({ ok: true, data: mock.logLines })),
  },
  openPath: (target) => (bridge ? bridge.openPath(target) : Promise.resolve({ ok: false, error: { code: 'preview', message: '预览模式' } })),
  openExternal: (url) => (bridge ? bridge.openExternal(url) : Promise.resolve({ ok: false })),
  window: {
    minimize: () => (bridge ? bridge.window.minimize() : undefined),
    toggleMaximize: () => (bridge ? bridge.window.toggleMaximize() : undefined),
    close: () => (bridge ? bridge.window.close() : undefined),
  },
};

/** Unwrap the { ok, data, error } envelope used by every IPC handler. */
export function unwrap(res) {
  if (res && typeof res === 'object' && 'ok' in res) {
    if (res.ok) return res.data;
    const err = new Error(res.error?.message || '调用失败');
    err.code = res.error?.code;
    throw err;
  }
  return res;
}

export { mock };
