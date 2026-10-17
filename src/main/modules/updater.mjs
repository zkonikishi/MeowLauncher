/**
 * Updater — channel manifest based self-update (skeleton, safe by default).
 *
 * Manifest (ours, plain JSON — replaces Tencent's encrypted apollo_reslist.flist):
 * {
 *   "schema": 1,
 *   "channel": "beta",
 *   "latest": {
 *     "version": "26.1.1",
 *     "url": "https://example.invalid/meowlauncher/26.1.1/app.asar",
 *     "sha256": "<hex>",
 *     "size": 123456,
 *     "minLauncher": "26.1.0",
 *     "notes": "..."
 *   }
 * }
 *
 * `check()` only reports; installing is an explicit future step (we never replace
 * a running app.asar in place — the installer/updater handoff is documented in
 * docs/03-26.X.X-架构设计.md).
 */
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { compareVersions, parseVersion, isRelease } from './version.mjs';

function isHttp(value) {
  return /^https?:\/\//i.test(String(value || ''));
}

export class Updater {
  constructor({ config, logger, appRoot, exeDir }) {
    this.config = config;
    this.logger = logger;
    this.appRoot = appRoot;
    this.exeDir = exeDir;
  }

  currentVersion() {
    try {
      const file = path.join(this.exeDir, 'version.json');
      return JSON.parse(fs.readFileSync(file, 'utf8')).version || '0.0.0';
    } catch {
      return '0.0.0';
    }
  }

  async readManifest(source) {
    if (isHttp(source)) {
      const res = await fetch(source, { redirect: 'follow' });
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`);
      return await res.json();
    }
    const text = fs.readFileSync(source, 'utf8');
    return JSON.parse(text);
  }

  async check() {
    const source = this.config.get('updater.manifest') || '';
    if (!source) {
      return { ok: true, data: { configured: false, current: this.currentVersion(), reason: 'updater.manifest 未配置' } };
    }
    try {
      const manifest = await this.readManifest(source);
      const latest = manifest && manifest.latest ? manifest.latest : null;
      if (!latest || !isRelease(latest.version)) {
        throw new Error('manifest.latest.version 不是合法发行版本号');
      }
      const current = this.currentVersion();
      let hasUpdate = false;
      try {
        hasUpdate = compareVersions(latest.version, current) > 0;
      } catch {
        hasUpdate = latest.version !== current;
      }
      this.logger.info('updater check', { current, latest: latest.version, hasUpdate });
      return {
        ok: true,
        data: {
          configured: true,
          source,
          current,
          latest: latest.version,
          hasUpdate,
          channel: manifest.channel || this.config.get('channel'),
          url: latest.url || '',
          sha256: latest.sha256 || '',
          size: latest.size || 0,
          notes: latest.notes || '',
          parsed: parseVersion(latest.version),
        },
      };
    } catch (err) {
      this.logger.error('updater check failed', { error: String((err && err.message) || err) });
      return { ok: false, error: { code: 'updater_check_failed', message: String((err && err.message) || err) } };
    }
  }

  /** Download + verify a payload into <dataDir>/updates. Not applied automatically. */
  async fetchPayload({ url, sha256, targetName }) {
    if (!isHttp(url)) return { ok: false, error: { code: 'updater_bad_url', message: `只接受 http(s) 地址: ${url}` } };
    const dir = path.join(this.exeDir, '..', 'MeowLauncher-data', 'updates');
    fs.mkdirSync(dir, { recursive: true });
    const target = path.join(dir, targetName || path.basename(new URL(url).pathname) || 'payload.bin');
    try {
      const res = await fetch(url, { redirect: 'follow' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const buffer = Buffer.from(await res.arrayBuffer());
      if (sha256 && crypto.createHash('sha256').update(buffer).digest('hex').toLowerCase() !== String(sha256).toLowerCase()) {
        throw new Error('sha256 校验失败');
      }
      fs.writeFileSync(target, buffer);
      return { ok: true, data: { file: target, size: buffer.length } };
    } catch (err) {
      return { ok: false, error: { code: 'updater_fetch_failed', message: String((err && err.message) || err) } };
    }
  }
}
