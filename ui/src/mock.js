// Demo data used when the UI runs in a plain browser (no Electron bridge).
// Shapes mirror the real IPC payloads exactly (see src/main/modules/ipc.mjs).
export const mock = {
  info: {
    app: 'MeowLauncher',
    line: '26.1',
    version: '26.1.0',
    channel: 'beta',
    displayName: 'MeowLauncher 26.1.0 Beta',
    snapshot: '26w42a',
    codename: 'Dolphin',
    builtAt: '2026-10-17T05:35:00.000Z',
    electron: '31.7.5',
    chrome: '126.0.6478.234',
    node: '20.18.0',
    platform: 'win32',
    arch: 'x64',
    exeDir: 'D:\\Games\\MeowLauncher-26\\resources',
    dataDir: 'C:\\Users\\meow\\AppData\\Roaming\\MeowLauncher26',
    log: { dir: 'C:\\Users\\meow\\AppData\\Roaming\\MeowLauncher26\\logs', file: '…\\launcher-20261017.log', lines: 3 },
  },
  versionScheme: {
    current: '26.1.0',
    line: '26.1',
    channel: 'beta',
    snapshot: '26w42a',
    nextPatch: '26.1.1',
    channels: ['dev', 'snapshot', 'beta', 'stable'],
  },
  config: {
    schema: 1,
    channel: 'beta',
    language: 'zh-CN',
    game: {
      root: 'D:\\Game\\Client',
      exe: 'game.exe',
      args: '--host-ip=127.0.0.1 --gf115-local-login=1 --cmd_option=99',
      lastVersion: '1.0.0.34',
    },
    window: { width: 1280, height: 780, maximized: false },
    updater: { manifest: '', autoCheck: true },
    launcher: { closeToTray: false, startOnBoot: false },
  },
  versions: {
    root: 'D:\\Game\\Client',
    versions: [
      {
        name: '1.0.0.34',
        dir: '<official-launcher>',
        version: '1.0.0.34',
        exe: 'game.exe',
        args: '',
        source: 'single-dir',
        verified: { ok: true, problems: [] },
      },
    ],
  },
  status: { running: false, pid: null, version: '', startedAt: null, uptimeMs: 0 },
  logLines: [
    '[2026-10-17T05:35:00.000Z] [INFO] starting {"version":"MeowLauncher 26.1.0 Beta (beta)","dataDir":"C:\\\\Users\\\\meow\\\\AppData\\\\Roaming\\\\MeowLauncher26"}',
    '[2026-10-17T05:35:00.120Z] [INFO] ipc registered {"channels":21}',
    '[2026-10-17T05:35:00.180Z] [INFO] main window created {"renderer":"ui/index.html"}',
  ],
};

export default mock;
