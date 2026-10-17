/**
 * Version — Minecraft-inspired version scheme for MeowLauncher 26.X.X.
 *
 * Release   : YY.<minor>.<patch>      e.g. 26.1.0  (year 2026, release line 1)
 * Pre/RC    : 26.1.0-pre1 / 26.1.0-rc1
 * Snapshot  : 26w42a                  (year + ISO week + letter, Minecraft-like)
 *
 * The authoritative file at build time is Beta/version.json; it is stamped into
 * the package (dist/<version>/version.json) and copied into src/package.json.
 */
import fs from 'node:fs';
import path from 'node:path';

const RELEASE_RE = /^(\d{2})\.(\d{1,2})\.(\d{1,2})(?:-(pre|rc)(\d{1,2}))?$/;
const SNAPSHOT_RE = /^(\d{2})w(\d{2})([a-z])$/;

export function isSnapshot(value) {
  return SNAPSHOT_RE.test(String(value || ''));
}

export function isRelease(value) {
  return RELEASE_RE.test(String(value || ''));
}

export function parseVersion(value) {
  const raw = String(value || '').trim();
  const snap = SNAPSHOT_RE.exec(raw);
  if (snap) {
    return { kind: 'snapshot', year: Number(snap[1]), week: Number(snap[2]), letter: snap[3], raw };
  }
  const rel = RELEASE_RE.exec(raw);
  if (rel) {
    return {
      kind: 'release',
      year: Number(rel[1]),
      minor: Number(rel[2]),
      patch: Number(rel[3]),
      pre: rel[4] || null,
      preNumber: rel[5] ? Number(rel[5]) : 0,
      raw,
    };
  }
  throw new Error(`invalid version: ${raw}`);
}

/** Compare two release versions. Snapshots rank below any release of the same year. */
export function compareVersions(a, b) {
  const pa = parseVersion(a);
  const pb = parseVersion(b);
  const rank = (p) => (p.kind === 'snapshot' ? 0 : 1);
  const fields = (p) =>
    p.kind === 'snapshot' ? [p.year, p.week, p.letter.charCodeAt(0)] : [p.year, p.minor, p.patch, p.pre ? (p.pre === 'rc' ? 2 : 1) : 3, p.preNumber];
  if (rank(pa) !== rank(pb)) return rank(pa) - rank(pb);
  const fa = fields(pa);
  const fb = fields(pb);
  for (let i = 0; i < Math.max(fa.length, fb.length); i += 1) {
    const d = (fa[i] || 0) - (fb[i] || 0);
    if (d !== 0) return d;
  }
  return 0;
}

export function nextPatch(version) {
  const p = parseVersion(version);
  if (p.kind !== 'release') throw new Error('nextPatch expects a release version');
  return `${String(p.year).padStart(2, '0')}.${p.minor}.${p.patch + 1}`;
}

export function snapshotFor(date = new Date()) {
  const year = String(date.getFullYear()).slice(-2);
  const start = new Date(date.getFullYear(), 0, 1);
  const week = Math.ceil(((date - start) / 86400000 + start.getDay() + 1) / 7);
  return `${year}w${String(week).padStart(2, '0')}a`;
}

export function versionLine(versionInfo) {
  return `${versionInfo.displayName || `MeowLauncher ${versionInfo.version}`} (${versionInfo.channel || 'beta'})`;
}

/**
 * Resolve version info: dist/<version>/version.json (next to the exe) wins,
 * then src/version.json, then package.json. Never throws.
 */
export function readVersionInfo({ appRoot, exeDir }) {
  const candidates = [path.join(exeDir, 'version.json'), path.join(appRoot, 'version.json')];
  for (const candidate of candidates) {
    try {
      const info = JSON.parse(fs.readFileSync(candidate, 'utf8'));
      if (info && info.version) {
        return {
          schema: info.schema || 1,
          line: info.line || '26.1',
          version: info.version,
          channel: info.channel || 'beta',
          displayName: info.displayName || `MeowLauncher ${info.version}`,
          snapshot: info.snapshot || snapshotFor(),
          codename: info.codename || '',
          builtAt: info.builtAt || '',
          electron: info.electron || process.versions.electron,
          notes: info.notes || '',
        };
      }
    } catch {
      /* try next candidate */
    }
  }
  let fallback = '26.1.0';
  try {
    fallback = JSON.parse(fs.readFileSync(path.join(appRoot, 'package.json'), 'utf8')).version || fallback;
  } catch {
    /* keep fallback */
  }
  return {
    schema: 1,
    line: fallback.split('.').slice(0, 2).join('.'),
    version: fallback,
    channel: 'beta',
    displayName: `MeowLauncher ${fallback}`,
    snapshot: snapshotFor(),
    codename: '',
    builtAt: '',
    electron: process.versions.electron,
    notes: '',
  };
}
