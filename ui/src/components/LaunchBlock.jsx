import React from 'react';

function fmtBytes(bytes) {
  if (!bytes) return '';
  const gb = bytes / 1024 ** 3;
  if (gb >= 1) return `${gb.toFixed(1)} GB`;
  return `${Math.max(1, Math.round(bytes / 1024 ** 2))} MB`;
}

export default function LaunchBlock({ version, totalVersions, root, running, busy, progress, onLaunch, onStop, onOpenVersions, onOpenSettings }) {
  const label = running ? '游戏中' : busy ? '启动中…' : version ? '开始游戏' : '选择目录';
  const meta = version
    ? `${version.exe || 'game.exe'} · ${version.verified?.ok ? '校验通过' : '未校验'} · 共 ${totalVersions} 个版本`
    : root
      ? `未发现可用版本 · ${root}`
      : '前往设置选择游戏目录';

  return (
    <div className="launch-block">
      <div className="version-card">
        <div style={{ minWidth: 0 }}>
          <div className="v-name">{version ? version.version || version.name : '未配置游戏目录'}</div>
          <div className="v-meta" title={version?.dir || root}>
            {fmtBytes(version?.sizeBytes) ? `${fmtBytes(version.sizeBytes)} · ` : ''}
            {meta}
          </div>
        </div>
        <button type="button" className="link" onClick={onOpenVersions}>
          切换版本
        </button>
      </div>

      <button type="button" className={`launch-btn ${busy ? 'busy' : ''}`} disabled={busy} onClick={onLaunch}>
        {label}
      </button>

      <div className="launch-state">
        {running ? (
          <span>
            运行中{gameVersionText(version)} ·{' '}
            <button type="button" className="link" style={{ padding: 0 }} onClick={onStop}>
              结束游戏
            </button>
          </span>
        ) : (
          <span title={version?.dir || ''}>{progress.text || (version ? `准备就绪 · ${version.dir}` : '尚未选择游戏目录')}</span>
        )}
      </div>

      <div className="progress">
        <i style={{ width: `${Math.max(0, Math.min(100, progress.percent || 0))}%` }} />
      </div>

      {!version && (
        <button type="button" className="btn ghost" onClick={onOpenSettings}>
          打开设置
        </button>
      )}
    </div>
  );
}

function gameVersionText(version) {
  return version ? ` · ${version.version || version.name}` : '';
}
