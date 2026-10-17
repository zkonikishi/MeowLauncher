/**
 * GameManager — version discovery and process lifecycle.
 *
 * A "version" is a folder holding one playable client build. We support two layouts:
 *
 *   <game.root>/versions/<versionName>/version.json     (preferred, multi-version)
 *   <game.root>/                                        (single install directory)
 *
 * version.json manifest schema (ours):
 * {
 *   "schema": 1,
 *   "version": "115cn-56.0.21.0",       // display name of the build
 *   "exe": "game.exe",                    // executable, relative to the version dir
 *   "args": "--host-ip=127.0.0.1 ...",   // optional per-version argument template
 *   "files": [{"path": "game.exe", "size": 278706808, "sha256": "..."}]
 * }
 *
 * This is the hook where the official launcher's OneUp/flist machinery is
 * replaced by our own plain-JSON manifests (see docs/03-26.X.X-架构设计.md).
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { compareVersions } from './version.mjs';

const DEFAULT_ACCOUNT = 'local';

function tokenizeArgs(template) {
  const out = [];
  const re = /"([^"]*)"|'([^']*)'|(\S+)/g;
  let m;
  while ((m = re.exec(String(template || ''))) !== null) {
    out.push(m[1] ?? m[2] ?? m[3]);
  }
  return out;
}

export class GameManager {
  constructor({ config, logger, dataDir }) {
    this.config = config;
    this.logger = logger;
    this.dataDir = dataDir;
    this.child = null;
    this.startedAt = null;
    this.runningVersion = '';
  }

  root() {
    return this.config.get('game.root') || '';
  }

  /** Returns [{ name, dir, version, exe, args, manifest, source }] sorted newest-first. */
  versions() {
    const root = this.root();
    if (!root || !fs.existsSync(root)) return [];
    const versionsDir = path.join(root, 'versions');
    const entries = [];

    if (fs.existsSync(versionsDir) && fs.statSync(versionsDir).isDirectory()) {
      for (const dirent of fs.readdirSync(versionsDir, { withFileTypes: true })) {
        if (!dirent.isDirectory()) continue;
        const dir = path.join(versionsDir, dirent.name);
        const manifest = this.readManifest(dir);
        entries.push({
          name: dirent.name,
          dir,
          version: (manifest && manifest.version) || dirent.name,
          exe: (manifest && manifest.exe) || this.config.get('game.exe') || 'game.exe',
          args: (manifest && manifest.args) || this.config.get('game.args') || '',
          manifest,
          source: 'versions-dir',
        });
      }
    } else {
      const manifest = this.readManifest(root);
      entries.push({
        name: path.basename(root) || 'local',
        dir: root,
        version: (manifest && manifest.version) || path.basename(root) || 'local',
        exe: (manifest && manifest.exe) || this.config.get('game.exe') || 'game.exe',
        args: (manifest && manifest.args) || this.config.get('game.args') || '',
        manifest,
        source: 'single-dir',
      });
    }

    const rank = (value) => {
      try {
        return compareVersions(value, '0.0.0');
      } catch {
        return 0;
      }
    };
    return entries.sort((a, b) => {
      try {
        return compareVersions(b.version, a.version);
      } catch {
        return rank(a.version) - rank(b.version);
      }
    });
  }

  readManifest(dir) {
    const file = path.join(dir, 'version.json');
    try {
      const manifest = JSON.parse(fs.readFileSync(file, 'utf8'));
      return manifest && typeof manifest === 'object' ? manifest : null;
    } catch {
      return null;
    }
  }

  executableFor(entry) {
    if (!entry) return null;
    const exe = path.isAbsolute(entry.exe) ? entry.exe : path.join(entry.dir, entry.exe);
    return exe;
  }

  buildArgs(entry, { account = DEFAULT_ACCOUNT } = {}) {
    const template = String((entry && entry.args) || '');
    const tokens = {
      root: entry ? entry.dir : '',
      version: entry ? entry.version : '',
      account,
      channel: this.config.get('channel') || 'beta',
    };
    return tokenizeArgs(template).map((arg) => arg.replace(/\{(\w+)\}/g, (_, key) => tokens[key] ?? ''));
  }

  /** Validates the version without starting anything. */
  verify(entry) {
    const problems = [];
    if (!entry) problems.push('未选择版本');
    else {
      const exe = this.executableFor(entry);
      if (!exe || !fs.existsSync(exe)) problems.push(`找不到可执行文件: ${exe}`);
      const manifest = entry.manifest;
      if (manifest && Array.isArray(manifest.files)) {
        for (const file of manifest.files) {
          const full = path.join(entry.dir, file.path);
          if (!fs.existsSync(full)) {
            problems.push(`缺少文件: ${file.path}`);
            continue;
          }
          if (typeof file.size === 'number' && fs.statSync(full).size !== file.size) {
            problems.push(`文件大小不符: ${file.path}`);
          }
        }
      }
    }
    return { ok: problems.length === 0, problems };
  }

  status() {
    return {
      running: Boolean(this.child && this.child.exitCode === null && !this.child.killed),
      pid: this.child ? this.child.pid : null,
      startedAt: this.startedAt,
      version: this.runningVersion,
      uptimeMs: this.startedAt ? Date.now() - this.startedAt : 0,
    };
  }

  launch({ version, account } = {}) {
    if (this.status().running) {
      return { ok: false, error: { code: 'game_already_running', message: '游戏已经在运行。', pid: this.child.pid } };
    }
    const all = this.versions();
    const entry = version ? all.find((v) => v.name === version || v.version === version) : all[0];
    const check = this.verify(entry);
    if (!check.ok) {
      return { ok: false, error: { code: 'game_invalid', message: check.problems.join('；'), problems: check.problems } };
    }
    const exe = this.executableFor(entry);
    const args = this.buildArgs(entry, { account });
    this.logger.info('game launch', { exe, args, version: entry.version });
    try {
      const child = spawn(exe, args, {
        cwd: entry.dir,
        detached: false,
        windowsHide: false,
        stdio: 'ignore',
      });
      this.child = child;
      this.startedAt = Date.now();
      this.runningVersion = entry.version;
      this.config.set('game.lastVersion', entry.name);
      this.config.save();
      child.on('exit', (code, signal) => {
        this.logger.info('game exited', { code, signal, version: this.runningVersion });
        if (this.child === child) {
          this.child = null;
          this.startedAt = null;
          this.runningVersion = '';
        }
      });
      child.on('error', (err) => {
        this.logger.error('game spawn failed', { error: String(err && err.message) });
        if (this.child === child) {
          this.child = null;
          this.startedAt = null;
          this.runningVersion = '';
        }
      });
      return { ok: true, data: { pid: child.pid, exe, args, version: entry.version } };
    } catch (err) {
      this.logger.error('game launch threw', { error: String((err && err.message) || err) });
      return { ok: false, error: { code: 'game_launch_failed', message: String((err && err.message) || err) } };
    }
  }

  stop({ silent = false } = {}) {
    const status = this.status();
    if (!status.running) return { ok: true, data: { stopped: false } };
    const pid = this.child.pid;
    this.logger.info('game stop', { pid });
    try {
      if (process.platform === 'win32') {
        spawn('taskkill', ['/pid', String(pid), '/t', '/f'], { windowsHide: true, stdio: 'ignore' });
      } else {
        process.kill(-pid, 'SIGTERM');
      }
    } catch (err) {
      if (!silent) return { ok: false, error: { code: 'game_stop_failed', message: String((err && err.message) || err) } };
    }
    return { ok: true, data: { stopped: true, pid } };
  }
}
