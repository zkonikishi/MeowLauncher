/**
 * Config — persisted launcher settings (JSON, atomic write, schema-versioned).
 * Split out of the official launcher's 43 KB config module; ours only keeps what
 * MeowLauncher 26.X.X actually needs.
 */
import fs from 'node:fs';
import path from 'node:path';

export const CONFIG_SCHEMA = 1;
export const CHANNELS = ['dev', 'snapshot', 'beta', 'stable'];

export const DEFAULT_CONFIG = {
  schema: CONFIG_SCHEMA,
  channel: 'beta',
  language: 'zh-CN',
  game: {
    // Game root (the folder that holds versions/<ver>/). Empty = not configured yet.
    root: '',
    // Executable relative to the selected version folder (or absolute).
    exe: 'game.exe',
    // Argument template; tokens: {root} {version} {account} {channel}
    args: '--host-ip=127.0.0.1 --gf115-local-login=1 --cmd_option=99',
    // Last played version directory name.
    lastVersion: '',
  },
  window: { width: 1280, height: 780, maximized: false },
  updater: { manifest: '', autoCheck: true },
  launcher: { closeToTray: false, startOnBoot: false },
};

function isObject(v) {
  return v !== null && typeof v === 'object' && !Array.isArray(v);
}

export function deepMerge(base, patch) {
  const out = Array.isArray(base) ? base.slice() : { ...base };
  if (!isObject(patch)) return out;
  for (const [key, value] of Object.entries(patch)) {
    out[key] = isObject(value) && isObject(out[key]) ? deepMerge(out[key], value) : value;
  }
  return out;
}

function getPath(obj, dotted) {
  return dotted.split('.').reduce((acc, part) => (acc == null ? undefined : acc[part]), obj);
}

function setPath(obj, dotted, value) {
  const parts = dotted.split('.');
  let cur = obj;
  for (const part of parts.slice(0, -1)) {
    if (!isObject(cur[part])) cur[part] = {};
    cur = cur[part];
  }
  cur[parts[parts.length - 1]] = value;
  return obj;
}

export class Config {
  constructor(dataDir, logger) {
    this.dataDir = dataDir;
    this.file = path.join(dataDir, 'config.json');
    this.logger = logger;
    this.data = structuredClone(DEFAULT_CONFIG);
    this.loadedAt = null;
    this.dirty = false;
  }

  load() {
    let raw = null;
    try {
      raw = JSON.parse(fs.readFileSync(this.file, 'utf8'));
    } catch {
      raw = null;
    }
    this.data = raw ? deepMerge(DEFAULT_CONFIG, raw) : structuredClone(DEFAULT_CONFIG);
    this.data.schema = CONFIG_SCHEMA;
    if (!CHANNELS.includes(this.data.channel)) this.data.channel = 'beta';
    this.loadedAt = new Date().toISOString();
    this.dirty = false;
    return this.data;
  }

  save() {
    fs.mkdirSync(this.dataDir, { recursive: true });
    const tmp = `${this.file}.tmp`;
    fs.writeFileSync(tmp, `${JSON.stringify(this.data, null, 2)}\n`, 'utf8');
    fs.renameSync(tmp, this.file);
    this.dirty = false;
    if (this.logger) this.logger.info('config saved', { file: this.file });
    return this.file;
  }

  all() {
    return structuredClone(this.data);
  }

  get(dotted) {
    return getPath(this.data, dotted);
  }

  set(dotted, value) {
    setPath(this.data, dotted, value);
    this.dirty = true;
    return this.data;
  }

  /** Apply a partial patch object (e.g. {game:{root:"..."}}) and persist it. */
  patch(patch) {
    this.data = deepMerge(this.data, patch);
    this.dirty = true;
    this.save();
    return this.all();
  }
}
