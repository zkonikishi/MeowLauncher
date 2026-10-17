import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import TitleBar from './components/TitleBar.jsx';
import NewsPanel from './components/NewsPanel.jsx';
import LaunchBlock from './components/LaunchBlock.jsx';
import BottomBar from './components/BottomBar.jsx';
import SettingsModal from './components/SettingsModal.jsx';
import VersionsModal from './components/VersionsModal.jsx';
import LogsModal from './components/LogsModal.jsx';
import AboutModal from './components/AboutModal.jsx';
import Toast from './components/Toast.jsx';
import { api, unwrap, hasBridge } from './api.js';
import bgImage from './assets/official/bg-default-Bg1qvOgr.jpg';
import launchBg from './assets/official/button-bottom-right-bg-C_bPzHtD.png';

// Server cards (115 line).
export const SERVERS = [
  { id: '115us', name: '115US', state: 'closed' },
  { id: '115cn', name: '115CN', state: 'closed' },
  { id: '115kr', name: '115KR', state: 'closed' },
];

const NEWS = [
  { tag: '公告', title: 'MeowLauncher 26.1.0 Beta：React + Vite 外壳与版本管理就绪', date: '10-17', fresh: true },
  { tag: '公告', title: '界面结构参考官方启动器解包结果，应用层完全自研', date: '10-17', fresh: true },
  { tag: '活动', title: '26w42a 快照通道开放，可在设置中切换更新频道', date: '10-16' },
  { tag: '公告', title: '版本命名参考《我的世界》：26.1.0 正式版 / 26w42a 快照', date: '10-16' },
  { tag: '提示', title: '本启动器不加载、不执行任何官方客户端代码', date: '10-15' },
];

function toMessage(err) {
  return (err && err.message) || String(err || '未知错误');
}

const STYLE = {
  '--bg-image': `url(${bgImage})`,
  '--launch-bg': `url(${launchBg})`,
};

export default function App() {
  const [info, setInfo] = useState(null);
  const [config, setConfig] = useState(null);
  const [versions, setVersions] = useState({ root: '', list: [] });
  const [game, setGame] = useState({ running: false, pid: null, version: '' });
  const [progress, setProgress] = useState({ percent: 0, text: '' });
  const [modal, setModal] = useState(null);
  const [toasts, setToasts] = useState([]);
  const [busy, setBusy] = useState(false);
  const [serverId, setServerId] = useState('115cn');
  const [serverMenu, setServerMenu] = useState(false);
  const toastId = useRef(0);

  const pushToast = useCallback((text, kind = 'info') => {
    const id = ++toastId.current;
    setToasts((list) => [...list, { id, text, kind }]);
    window.setTimeout(() => setToasts((list) => list.filter((t) => t.id !== id)), 3800);
  }, []);

  const listVersions = useCallback(async () => {
    const data = unwrap(await api.games.list());
    setVersions({ root: data?.root || '', list: data?.versions || [] });
    return data;
  }, []);

  const refreshStatus = useCallback(async () => {
    const status = unwrap(await api.games.status());
    setGame({
      running: Boolean(status?.running),
      pid: status?.pid ?? null,
      version: status?.version || '',
    });
    return status;
  }, []);

  const refreshInfoConfig = useCallback(async () => {
    const [i, c] = await Promise.all([api.info(), api.config.get()]);
    setInfo(unwrap(i));
    setConfig(unwrap(c));
  }, []);

  useEffect(() => {
    let alive = true;
    (async () => {
      try {
        await refreshInfoConfig();
        if (!alive) return;
        await Promise.all([listVersions(), refreshStatus()]);
      } catch (err) {
        if (alive) pushToast(`初始化失败：${toMessage(err)}`, 'err');
      }
    })();
    return () => {
      alive = false;
    };
  }, [refreshInfoConfig, listVersions, refreshStatus, pushToast]);

  // game:status polling (IPC events are a later milestone; polling keeps the shell honest)
  useEffect(() => {
    const timer = window.setInterval(() => refreshStatus().catch(() => {}), 2500);
    return () => window.clearInterval(timer);
  }, [refreshStatus]);

  useEffect(() => {
    if (!hasBridge) pushToast('预览模式：未检测到启动器桥接，数据为演示数据', 'info');
  }, [pushToast]);

  const selectedVersion = useMemo(() => {
    if (!versions.list.length) return null;
    const wanted = config?.game?.lastVersion;
    return versions.list.find((v) => v.name === wanted) || versions.list[0];
  }, [versions, config]);

  const currentServer = useMemo(() => SERVERS.find((s) => s.id === serverId) || SERVERS[0], [serverId]);

  const applyConfig = useCallback(
    async (patch, { silent = false } = {}) => {
      const data = unwrap(await api.config.set(patch));
      setConfig(data);
      if (!silent) pushToast('设置已保存', 'ok');
      return data;
    },
    [pushToast],
  );

  const handleLaunch = useCallback(async () => {
    if (busy) return;
    if (!selectedVersion) {
      setModal('settings');
      pushToast('请先在设置里选择游戏目录', 'err');
      return;
    }
    setBusy(true);
    setProgress({ percent: 35, text: '正在启动游戏…' });
    try {
      unwrap(await api.games.launch({ version: selectedVersion.name, account: 'local' }));
      await applyConfig({ game: { lastVersion: selectedVersion.name } }, { silent: true });
      await refreshStatus();
      setProgress({ percent: 100, text: '游戏已启动' });
      pushToast(`已启动 ${selectedVersion.version}`, 'ok');
    } catch (err) {
      setProgress({ percent: 0, text: '' });
      pushToast(`启动失败：${toMessage(err)}`, 'err');
    } finally {
      setBusy(false);
    }
  }, [busy, selectedVersion, applyConfig, refreshStatus, pushToast]);

  const handleStop = useCallback(async () => {
    try {
      unwrap(await api.games.stop());
      await refreshStatus();
      pushToast('已结束游戏进程', 'ok');
    } catch (err) {
      pushToast(`结束进程失败：${toMessage(err)}`, 'err');
    }
  }, [refreshStatus, pushToast]);

  const handleRescan = useCallback(async () => {
    try {
      const data = await listVersions();
      pushToast(`已重新扫描：${data?.versions?.length ?? 0} 个版本`, 'ok');
    } catch (err) {
      pushToast(`扫描失败：${toMessage(err)}`, 'err');
    }
  }, [listVersions, pushToast]);

  const openDir = useCallback(
    async (target) => {
      try {
        unwrap(await api.openPath(target));
      } catch (err) {
        pushToast(`无法打开目录：${toMessage(err)}`, 'err');
      }
    },
    [pushToast],
  );

  const saveSettings = useCallback(
    async (form) => {
      const data = await applyConfig({
        channel: form.channel,
        game: {
          root: form.root,
          exe: form.exe || 'game.exe',
          args: form.args,
          lastVersion: config?.game?.lastVersion || '',
        },
        updater: { manifest: form.manifest, autoCheck: config?.updater?.autoCheck ?? true },
      });
      await listVersions();
      return data;
    },
    [applyConfig, listVersions, config],
  );

  return (
    <div className="shell" style={STYLE} onClick={() => serverMenu && setServerMenu(false)}>
      <div className="shell-bg" />

      <TitleBar
        info={info}
        servers={SERVERS}
        currentServer={currentServer}
        menuOpen={serverMenu}
        onToggleMenu={(e) => {
          e.stopPropagation();
          setServerMenu((v) => !v);
        }}
        onSelectServer={(id) => {
          setServerId(id);
          setServerMenu(false);
        }}
        onAbout={() => setModal('about')}
      />

      <div className="stage">
        <NewsPanel news={NEWS} onOpenLogs={() => setModal('logs')} />
        <LaunchBlock
          version={selectedVersion}
          totalVersions={versions.list.length}
          root={versions.root}
          running={game.running}
          busy={busy}
          progress={progress}
          onLaunch={handleLaunch}
          onStop={handleStop}
          onOpenVersions={() => setModal('versions')}
          onOpenSettings={() => setModal('settings')}
        />
      </div>

      <BottomBar
        version={info?.version}
        channel={info?.channel}
        onSettings={() => setModal('settings')}
        onVersions={() => setModal('versions')}
        onLogs={() => setModal('logs')}
        onAbout={() => setModal('about')}
        onRepair={handleRescan}
        onOpenFolder={() => openDir('data')}
      />

      {modal === 'settings' && (
        <SettingsModal config={config} info={info} onClose={() => setModal(null)} onSave={saveSettings} onToast={pushToast} />
      )}
      {modal === 'versions' && (
        <VersionsModal
          root={versions.root}
          versions={versions.list}
          selected={config?.game?.lastVersion}
          onClose={() => setModal(null)}
          onRescan={handleRescan}
          onSelect={(name) => applyConfig({ game: { lastVersion: name } }, { silent: true }).catch(() => {})}
        />
      )}
      {modal === 'logs' && <LogsModal onClose={() => setModal(null)} onToast={pushToast} onOpenDir={() => openDir('logs')} />}
      {modal === 'about' && <AboutModal info={info} onClose={() => setModal(null)} />}

      <Toast toasts={toasts} />
    </div>
  );
}
