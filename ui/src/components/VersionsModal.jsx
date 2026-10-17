import React from 'react';

export default function VersionsModal({ root, versions, selected, onClose, onRescan, onSelect }) {
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <div className="modal-head">
          <h2>版本管理</h2>
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="kv">
            <span className="k">游戏根目录</span>
            <span className="v mono">{root || '（未配置）'}</span>
          </div>
          <div className="actions" style={{ justifyContent: 'flex-start', marginTop: 14, marginBottom: 14 }}>
            <button type="button" className="btn" onClick={onRescan}>
              重新扫描
            </button>
          </div>

          {versions.length === 0 && (
            <p style={{ color: 'var(--muted)' }}>
              没有发现版本。支持两种布局：<code>&lt;root&gt;\versions\&lt;版本名&gt;\version.json</code> 或直接把客户端本体放在
              <code>&lt;root&gt;</code> 下。
            </p>
          )}

          <div className="list">
            {versions.map((v) => (
              <div className="list-row" key={v.name}>
                <div className="grow">
                  <div className="title">{v.version || v.name}</div>
                  <div className="path">{v.dir}</div>
                </div>
                <span className={`chip ${v.verified?.ok ? 'ok' : 'bad'}`}>
                  {v.verified?.ok ? `可启动 · ${v.exe}` : `不可用 · ${(v.verified?.problems || []).join('；') || '未找到可执行文件'}`}
                </span>
                <button type="button" className={`btn ${v.name === selected ? 'primary' : ''}`} onClick={() => onSelect(v.name)}>
                  {v.name === selected ? '当前' : '选择'}
                </button>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
