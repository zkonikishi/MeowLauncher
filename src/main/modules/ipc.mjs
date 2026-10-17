/**
 * IPC — one place for every renderer <-> main channel of MeowLauncher 26.X.X.
 * Every handler returns either { ok: true, data } or { ok: false, error }.
 * Channel list is intentionally small and typed; the official launcher has 237
 * channels, we start with 20 and grow only when the UI needs it.
 */
import { dialog, shell } from 'electron';
import fs from 'node:fs';
import path from 'node:path';

import { CHANNELS } from './config.mjs';
import { nextPatch, snapshotFor } from './version.mjs';

const CONFIG_WHITELIST = ['channel', 'language', 'game', 'window', 'updater', 'launcher'];
const FORBIDDEN_KEYS = new Set(['__proto__', 'constructor', 'prototype']);

const ok = (data) => ({ ok: true, data });
const fail = (code, message, extra) => ({ ok: false, error: { code, message, ...(extra || {}) } });

function sanitizePatch(input) {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('config patch 必须是对象');
  }
  const patch = {};
  for (const [key, value] of Object.entries(input)) {
    if (!CONFIG_WHITELIST.includes(key)) continue;
    if (FORBIDDEN_KEYS.has(key)) throw new Error(`非法字段: ${key}`);
    patch[key] = value;
  }
  if (patch.channel !== undefined && !CHANNELS.includes(patch.channel)) {
    throw new Error(`非法频道: ${patch.channel}`);
  }
  if (patch.game && patch.game.root !== undefined) {
    if (typeof patch.game.root !== 'string') throw new Error('game.root 必须是字符串');
    if (patch.game.root && !fs.existsSync(patch.game.root)) throw new Error(`游戏目录不存在: ${patch.game.root}`);
  }
  return patch;
}

export function registerIpc({ ipcMain, app, config, logger, game, updater, windows, version, exeDir, dataDir }) {
  ipcMain.handle('ml:info', () =>
    ok({
      app: 'MeowLauncher',
      line: version.line,
      version: version.version,
      channel: version.channel,
      displayName: version.displayName,
      snapshot: version.snapshot,
      codename: version.codename,
      builtAt: version.builtAt,
      electron: process.versions.electron,
      chrome: process.versions.chrome,
      node: process.versions.node,
      platform: process.platform,
      arch: process.arch,
      exeDir,
      dataDir: logger.dir ? path.dirname(logger.dir) : '',
      log: logger.describe(),
    })
  );

  ipcMain.handle('ml:version:scheme', () =>
    ok({
      current: version.version,
      line: version.line,
      channel: version.channel,
      snapshot: version.snapshot || snapshotFor(),
      nextPatch: (() => {
        try {
          return nextPatch(version.version);
        } catch {
          return '';
        }
      })(),
      channels: CHANNELS,
    })
  );

  ipcMain.handle('ml:config:get', () => ok(config.all()));

  ipcMain.handle('ml:config:set', (_event, patch) => {
    try {
      const clean = sanitizePatch(patch);
      const data = config.patch(clean);
      logger.info('config patched', { keys: Object.keys(clean) });
      return ok(data);
    } catch (err) {
      return fail('config_invalid', String((err && err.message) || err));
    }
  });

  ipcMain.handle('ml:versions:list', () => {
    try {
      const versions = game.versions().map((entry) => ({
        name: entry.name,
        dir: entry.dir,
        version: entry.version,
        exe: game.executableFor(entry),
        args: entry.args,
        source: entry.source,
        verified: game.verify(entry),
      }));
      return ok({ root: game.root(), versions });
    } catch (err) {
      return fail('versions_failed', String((err && err.message) || err));
    }
  });

  ipcMain.handle('ml:game:launch', (_event, opts) => {
    const result = game.launch(opts || {});
    if (!result.ok) logger.warn('game launch rejected', result.error);
    return result;
  });

  ipcMain.handle('ml:game:stop', () => game.stop());
  ipcMain.handle('ml:game:status', () => ok(game.status()));

  ipcMain.handle('ml:dialog:pick-exe', async () => {
    const result = await dialog.showOpenDialog(windows.window, {
      title: '选择游戏可执行文件',
      properties: ['openFile'],
      filters: [
        { name: '可执行文件', extensions: ['exe'] },
        { name: '所有文件', extensions: ['*'] },
      ],
    });
    return result.canceled ? ok({ canceled: true }) : ok({ canceled: false, file: result.filePaths[0] });
  });

  ipcMain.handle('ml:dialog:pick-dir', async () => {
    const result = await dialog.showOpenDialog(windows.window, {
      title: '选择游戏根目录（包含 versions 或客户端本体）',
      properties: ['openDirectory'],
    });
    return result.canceled ? ok({ canceled: true }) : ok({ canceled: false, dir: result.filePaths[0] });
  });

  ipcMain.handle('ml:updater:check', () => updater.check());

  ipcMain.handle('ml:log:read', (_event, max) => ok(logger.read(Number(max) || 200)));

  ipcMain.handle('ml:log:append', (_event, payload) => {
    const level = payload && payload.level ? String(payload.level) : 'info';
    const message = payload && payload.message ? String(payload.message) : '';
    return ok({ line: logger.write(level, message, payload && payload.extra) });
  });

  ipcMain.handle('ml:open:external', async (_event, url) => {
    if (!/^https?:\/\//i.test(String(url || ''))) return fail('open_bad_url', '只允许 http(s) 链接');
    await shell.openExternal(url);
    return ok({ opened: true });
  });

  ipcMain.handle('ml:open:path', async (_event, target) => {
    const allowed = new Set(['data', 'logs', 'exe', 'config']);
    if (!allowed.has(String(target || ''))) return fail('open_bad_target', '非法目录目标');
    const map = {
      data: dataDir,
      logs: logger.dir,
      exe: exeDir,
      config: dataDir,
    };
    const result = await shell.openPath(map[String(target)]);
    if (result) return fail('open_failed', result);
    return ok({ opened: true, path: map[String(target)] });
  });

  ipcMain.handle('ml:window:minimize', () => {
    windows.minimize();
    return ok(true);
  });

  ipcMain.handle('ml:window:toggle-maximize', () => ok(windows.toggleMaximize()));

  ipcMain.handle('ml:window:close', () => {
    windows.close();
    return ok(true);
  });

  ipcMain.handle('ml:app:quit', () => {
    logger.info('quit requested from renderer');
    app.quit();
    return ok(true);
  });

  logger.info('ipc registered', { channels: ipcMain.eventNames().filter((n) => String(n).startsWith('ml:')).length });
}
