/**
 * Logger — append-only daily log file + in-memory tail used by the UI.
 * The official launcher ships a 9-channel logger module; we keep one file plus
 * the last N lines in memory, which is all the new launcher needs.
 */
import fs from 'node:fs';
import path from 'node:path';

const LEVELS = ['debug', 'info', 'warn', 'error'];

function dayStamp(date = new Date()) {
  const p = (n) => String(n).padStart(2, '0');
  return `${date.getFullYear()}${p(date.getMonth() + 1)}${p(date.getDate())}`;
}

export class Logger {
  constructor(dataDir) {
    this.dir = path.join(dataDir, 'logs');
    fs.mkdirSync(this.dir, { recursive: true });
    this.file = path.join(this.dir, `launcher-${dayStamp()}.log`);
    this.tail = [];
    this.maxTail = 500;
  }

  write(level, message, extra) {
    const normalized = LEVELS.includes(level) ? level : 'info';
    const line = `[${new Date().toISOString()}] [${normalized.toUpperCase()}] ${message}${
      extra === undefined ? '' : ` ${safeJson(extra)}`
    }`;
    this.tail.push(line);
    if (this.tail.length > this.maxTail) this.tail.splice(0, this.tail.length - this.maxTail);
    try {
      fs.appendFileSync(this.file, `${line}\n`, 'utf8');
    } catch {
      /* logging must never crash the launcher */
    }
    return line;
  }

  debug(message, extra) {
    return this.write('debug', message, extra);
  }

  info(message, extra) {
    return this.write('info', message, extra);
  }

  warn(message, extra) {
    return this.write('warn', message, extra);
  }

  error(message, extra) {
    return this.write('error', message, extra);
  }

  /** Last N lines for the in-app log viewer. */
  read(max = 200) {
    return this.tail.slice(-max);
  }

  describe() {
    return { dir: this.dir, file: this.file, lines: this.tail.length };
  }
}

function safeJson(value) {
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}
