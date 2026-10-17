/**
 * WindowManager — main window lifecycle for MeowLauncher 26.X.X.
 * Frameless with our own title bar (the renderer draws it), single main window,
 * bounds persisted through Config.
 *
 * Renderer resolution order:
 *   1. MEOW_DEV_SERVER env / --dev-server=<url>  -> loadURL (Vite dev server)
 *   2. <appRoot>/ui/index.html                   -> React + Vite build (primary)
 *   3. <appRoot>/renderer/index.html             -> legacy plain-HTML shell (fallback)
 */
import { BrowserWindow, nativeTheme } from 'electron';
import fs from 'node:fs';
import path from 'node:path';

export class WindowManager {
  constructor({ appRoot, exeDir, config, logger, version }) {
    this.appRoot = appRoot;
    this.exeDir = exeDir;
    this.config = config;
    this.logger = logger;
    this.version = version;
    this.win = null;
  }

  get window() {
    return this.win;
  }

  resolveRenderer() {
    const argv = process.argv.slice(1);
    const devArg = argv.find((a) => a.startsWith('--dev-server='));
    const devServer = process.env.MEOW_DEV_SERVER || (devArg ? devArg.slice('--dev-server='.length) : '');
    if (devServer) return { kind: 'url', value: devServer };

    const reactUi = path.join(this.appRoot, 'ui', 'index.html');
    if (fs.existsSync(reactUi)) return { kind: 'file', value: reactUi };

    const legacy = path.join(this.appRoot, 'renderer', 'index.html');
    if (fs.existsSync(legacy)) return { kind: 'file', value: legacy };

    throw new Error('no renderer found (ui/index.html or renderer/index.html)');
  }

  async create() {
    if (this.win && !this.win.isDestroyed()) return this.win;
    nativeTheme.themeSource = 'dark';
    const bounds = this.config.get('window') || {};
    this.win = new BrowserWindow({
      width: bounds.width || 1280,
      height: bounds.height || 780,
      minWidth: 1024,
      minHeight: 640,
      show: false,
      frame: false,
      backgroundColor: '#0a0c11',
      title: `MeowLauncher ${this.version.version}`,
      webPreferences: {
        preload: path.join(this.appRoot, 'preload', 'preload.cjs'),
        contextIsolation: true,
        nodeIntegration: false,
        sandbox: false,
        spellcheck: false,
      },
    });

    this.win.on('ready-to-show', () => {
      if (bounds.maximized) this.win.maximize();
      this.win.show();
    });
    this.win.on('resize', () => this.persistBounds());
    this.win.on('maximize', () => {
      this.config.set('window.maximized', true);
      this.persistBounds();
    });
    this.win.on('unmaximize', () => {
      this.config.set('window.maximized', false);
      this.persistBounds();
    });
    this.win.on('closed', () => {
      this.win = null;
    });

    const target = this.resolveRenderer();
    if (target.kind === 'url') {
      await this.win.loadURL(target.value);
    } else {
      await this.win.loadFile(target.value);
    }
    this.logger.info('main window created', { renderer: target.kind === 'url' ? target.value : path.relative(this.appRoot, target.value) });
    return this.win;
  }

  persistBounds() {
    if (!this.win || this.win.isDestroyed()) return;
    const maximized = this.win.isMaximized();
    const bounds = maximized ? this.win.getNormalBounds() : this.win.getBounds();
    this.config.set('window.width', bounds.width);
    this.config.set('window.height', bounds.height);
    this.config.set('window.maximized', maximized);
    this.config.save();
  }

  focus() {
    if (!this.win || this.win.isDestroyed()) return;
    if (this.win.isMinimized()) this.win.restore();
    this.win.focus();
  }

  close() {
    if (this.win && !this.win.isDestroyed()) this.win.close();
  }

  minimize() {
    if (this.win && !this.win.isDestroyed()) this.win.minimize();
  }

  toggleMaximize() {
    if (!this.win || this.win.isDestroyed()) return false;
    if (this.win.isMaximized()) this.win.unmaximize();
    else this.win.maximize();
    return this.win.isMaximized();
  }
}
