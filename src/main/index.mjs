/**
 * MeowLauncher 26.X.X (Beta) — main process entry.
 *
 * This is OUR launcher: the app.asar this file lives in is built from `Beta/src`
 * by `tools/build.ps1`. It runs on the Electron 31.7.5 runtime that ships with the
 * official launcher installer, but no
 * Tencent launcher code is loaded or executed.
 *
 * Machine-readable modes (used by tools/smoke-test.ps1):
 *   --self-test   : run in-process checks, print one JSON object to stdout, exit 0/1
 *   --dump-info   : print version/app info JSON and exit
 * GUI mode (no flags): create the main window.
 */
import { app, ipcMain } from 'electron';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import { Config } from './modules/config.mjs';
import { Logger } from './modules/logger.mjs';
import { readVersionInfo, versionLine } from './modules/version.mjs';
import { WindowManager } from './modules/window.mjs';
import { GameManager } from './modules/game.mjs';
import { Updater } from './modules/updater.mjs';
import { registerIpc } from './modules/ipc.mjs';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(__dirname, '..'); // src/ root inside app.asar
const EXE_DIR = path.dirname(process.execPath); // install directory of the launcher

const argv = process.argv.slice(1);
const hasFlag = (name) => argv.includes(name) || argv.some((a) => a.startsWith(`${name}=`));
const flagValue = (name) => {
  const hit = argv.find((a) => a.startsWith(`${name}=`));
  return hit ? hit.slice(name.length + 1) : null;
};

const VERSION = readVersionInfo({ appRoot: APP_ROOT, exeDir: EXE_DIR });
const PORTABLE = fs.existsSync(path.join(EXE_DIR, 'portable.marker'));
const DATA_DIR = PORTABLE
  ? path.join(EXE_DIR, 'MeowLauncher-data')
  : path.join(process.env.APPDATA || path.join(os.homedir(), 'AppData', 'Roaming'), 'MeowLauncher26');

fs.mkdirSync(DATA_DIR, { recursive: true });
const logger = new Logger(DATA_DIR);
const config = new Config(DATA_DIR, logger);
config.load();

const game = new GameManager({ config, logger, dataDir: DATA_DIR });
const updater = new Updater({ config, logger, appRoot: APP_ROOT, exeDir: EXE_DIR });
const windows = new WindowManager({ appRoot: APP_ROOT, exeDir: EXE_DIR, config, logger, version: VERSION });

function emitJson(obj) {
  // stdout is captured by tools/smoke-test.ps1; keep it to a single JSON document.
  process.stdout.write(`${JSON.stringify(obj, null, 2)}\n`);
}

async function runSelfTest() {
  await app.whenReady();
  const checks = [];
  const check = (name, fn) => {
    try {
      const detail = fn();
      checks.push({ name, ok: true, detail: detail === undefined ? null : detail });
    } catch (err) {
      checks.push({ name, ok: false, error: String((err && err.message) || err) });
    }
  };

  check('version.json', () => ({ version: VERSION.version, channel: VERSION.channel, line: VERSION.line }));
  check('ui/index.html', () => fs.existsSync(path.join(APP_ROOT, 'ui', 'index.html')));
  check('preload/preload.cjs', () => fs.existsSync(path.join(APP_ROOT, 'preload', 'preload.cjs')));
  check('config.save()', () => {
    config.save();
    return config.file;
  });
  check('config.reload()', () => {
    config.load();
    return config.get('channel');
  });
  check('versions.scan()', () => game.versions().length);
  check('dataDir writable', () => {
    const probe = path.join(DATA_DIR, '.write-probe');
    fs.writeFileSync(probe, 'ok');
    fs.rmSync(probe, { force: true });
    return DATA_DIR;
  });
  check('electron runtime', () => ({
    electron: process.versions.electron,
    chrome: process.versions.chrome,
    node: process.versions.node,
  }));

  const ok = checks.every((c) => c.ok);
  logger.info('self-test finished', { ok });
  emitJson({
    ok,
    app: 'MeowLauncher',
    version: VERSION.version,
    channel: VERSION.channel,
    line: VERSION.line,
    displayName: VERSION.displayName,
    electron: process.versions.electron,
    chrome: process.versions.chrome,
    node: process.versions.node,
    platform: process.platform,
    arch: process.arch,
    portable: PORTABLE,
    dataDir: DATA_DIR,
    exeDir: EXE_DIR,
    checks,
  });
  app.exit(ok ? 0 : 1);
}

function runDumpInfo() {
  emitJson({
    ok: true,
    app: 'MeowLauncher',
    version: VERSION.version,
    channel: VERSION.channel,
    line: VERSION.line,
    displayName: VERSION.displayName,
    snapshot: VERSION.snapshot,
    electron: process.versions.electron,
    portable: PORTABLE,
    dataDir: DATA_DIR,
    exeDir: EXE_DIR,
    appRoot: APP_ROOT,
  });
  app.exit(0);
}

/**
 * GUI smoke test: create the real window, load the React build, exercise the IPC
 * bridge from inside the renderer, capture a screenshot and write one JSON report.
 * Used by tools/smoke-test.ps1; exit code 0 means "the shell really renders".
 */
async function runSmokeTest() {
  await app.whenReady();
  const outPath = flagValue('--smoke-out') || path.join(EXE_DIR, 'smoke-test.json');
  const shotPath = outPath.replace(/\.json$/i, '.png');
  const checks = [];
  const check = (name, fn) => {
    try {
      const detail = fn();
      checks.push({ name, ok: true, detail: detail === undefined ? null : detail });
    } catch (err) {
      checks.push({ name, ok: false, error: String((err && err.message) || err) });
    }
  };
  const timed = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  try {
    registerIpc({ ipcMain, app, config, logger, game, updater, windows, version: VERSION, exeDir: EXE_DIR, dataDir: DATA_DIR });
    const win = await windows.create();

    check('window created', () => ({ id: win.id, visible: win.isVisible(), size: win.getSize() }));
    check('renderer resolved to ui/index.html', () => windows.resolveRenderer());
    check('did-finish-load (no loading)', () => {
      if (win.webContents.isLoading()) throw new Error('renderer still loading');
      return { url: String(win.webContents.getURL()) };
    });

    await timed(1500); // let React commit effects, first IPC round-trips and fonts settle

    const dom = await win.webContents.executeJavaScript(`(async () => {
      const q = (s) => document.querySelector(s);
      const pick = (s) => (q(s) ? String(q(s).textContent || '').trim() : null);
      const call = async (fn) => {
        try {
          return await fn();
        } catch (err) {
          return { ok: false, error: { message: String((err && err.message) || err) } };
        }
      };
      const [info, cfg, versions, status, logs] = await Promise.all([
        call(() => window.meow.info()),
        call(() => window.meow.config.get()),
        call(() => window.meow.games.list()),
        call(() => window.meow.games.status()),
        call(() => window.meow.log.read(10)),
      ]);
      return {
        bridge: Boolean(window.meow),
        bridgeKeys: window.meow ? Object.keys(window.meow).sort() : [],
        rootChildren: document.querySelectorAll('#root > *').length,
        topbar: Boolean(q('.topbar')),
        newsPanel: Boolean(q('.news-panel')),
        launchButton: pick('.launch-btn'),
        versionName: pick('.v-name'),
        bottomBar: Boolean(q('.bottombar')),
        winButtons: document.querySelectorAll('.win-btn').length,
        title: (q('.brand-title') ? pick('.brand-title') + ' ' + pick('.brand-sub') : null) || document.title,
        infoOk: Boolean(info && info.ok),
        infoVersion: info && info.data ? info.data.version : null,
        infoElectron: info && info.data ? info.data.electron : null,
        configOk: Boolean(cfg && cfg.ok),
        configChannel: cfg && cfg.data ? cfg.data.channel : null,
        versionsOk: Boolean(versions && versions.ok),
        versionsCount: versions && versions.data ? versions.data.versions.length : null,
        versionsRoot: versions && versions.data ? versions.data.root : null,
        statusOk: Boolean(status && status.ok),
        statusRunning: status && status.data ? status.data.running : null,
        logsOk: Boolean(logs && logs.ok),
        logLines: logs && logs.data ? (Array.isArray(logs.data) ? logs.data.length : 1) : null,
      };
    })()`);

    check('DOM rendered', () => ({
      rootChildren: dom.rootChildren,
      topbar: dom.topbar,
      newsPanel: dom.newsPanel,
      bottomBar: dom.bottomBar,
      launchButton: dom.launchButton,
      versionName: dom.versionName,
      title: dom.title,
    }));
    check('bridge exposed (window.meow)', () => ({ bridge: dom.bridge, keys: dom.bridgeKeys }));
    check('IPC ml:info', () => ({ ok: dom.infoOk, version: dom.infoVersion, electron: dom.infoElectron }));
    check('IPC ml:config:get', () => ({ ok: dom.configOk, channel: dom.configChannel }));
    check('IPC ml:versions:list', () => ({ ok: dom.versionsOk, count: dom.versionsCount, root: dom.versionsRoot }));
    check('IPC ml:game:status', () => ({ ok: dom.statusOk, running: dom.statusRunning }));
    check('IPC ml:log:read', () => ({ ok: dom.logsOk, lines: dom.logLines }));

    const image = await win.webContents.capturePage();
    const png = image.toPNG();
    fs.mkdirSync(path.dirname(shotPath), { recursive: true });
    fs.writeFileSync(shotPath, png);
    check('screenshot captured', () => ({ file: shotPath, bytes: png.length, size: image.getSize() }));

    // The window is frameless; the React title bar must have drawn its own controls.
    check('window controls present', () => {
      if (!dom.winButtons) throw new Error('no .win-btn in DOM');
      return { winButtons: dom.winButtons };
    });

    const ok = checks.every((c) => c.ok);
    const report = {
      ok,
      app: 'MeowLauncher',
      version: VERSION.version,
      channel: VERSION.channel,
      line: VERSION.line,
      displayName: VERSION.displayName,
      snapshot: VERSION.snapshot,
      electron: process.versions.electron,
      chrome: process.versions.chrome,
      node: process.versions.node,
      platform: process.platform,
      arch: process.arch,
      portable: PORTABLE,
      exeDir: EXE_DIR,
      dataDir: DATA_DIR,
      appRoot: APP_ROOT,
      renderer: windows.resolveRenderer(),
      screenshot: shotPath,
      checks,
    };
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    logger.info('smoke test finished', { ok, out: outPath, checks: checks.length });
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    app.exit(ok ? 0 : 1);
  } catch (err) {
    const report = {
      ok: false,
      app: 'MeowLauncher',
      version: VERSION.version,
      error: String((err && err.stack) || err),
      checks,
      exeDir: EXE_DIR,
      dataDir: DATA_DIR,
    };
    try {
      fs.mkdirSync(path.dirname(outPath), { recursive: true });
      fs.writeFileSync(outPath, `${JSON.stringify(report, null, 2)}\n`, 'utf8');
    } catch {
      /* ignore */
    }
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    app.exit(1);
  }
}

async function bootstrap() {
  app.setAppUserModelId('com.meowlauncher.app');
  app.on('second-instance', () => windows.focus());
  if (!app.requestSingleInstanceLock()) {
    logger.warn('another instance is already running');
    app.quit();
    return;
  }
  await app.whenReady();
  logger.info('starting', { version: versionLine(VERSION), dataDir: DATA_DIR });
  registerIpc({ ipcMain, app, config, logger, game, updater, windows, version: VERSION, exeDir: EXE_DIR, dataDir: DATA_DIR });
  await windows.create();
  app.on('window-all-closed', () => {
    logger.info('all windows closed');
    if (process.platform !== 'darwin') app.quit();
  });
  app.on('before-quit', () => {
    game.stop({ silent: true });
  });
}

if (hasFlag('--smoke-test')) {
  runSmokeTest().catch((err) => {
    emitJson({ ok: false, app: 'MeowLauncher', error: String((err && err.stack) || err) });
    app.exit(1);
  });
} else if (hasFlag('--self-test')) {
  runSelfTest().catch((err) => {
    emitJson({ ok: false, app: 'MeowLauncher', error: String((err && err.stack) || err) });
    app.exit(1);
  });
} else if (hasFlag('--dump-info')) {
  if (app.isReady()) runDumpInfo();
  else app.whenReady().then(runDumpInfo);
} else {
  bootstrap().catch((err) => {
    logger.error('bootstrap failed', { error: String((err && err.stack) || err) });
    emitJson({ ok: false, error: String((err && err.message) || err) });
    app.exit(1);
  });
}

// Exported for future tooling (e.g. unit tests running the file as a module).
export { APP_ROOT, EXE_DIR, DATA_DIR, VERSION, flagValue };
