import React from 'react';

function Row({ k, v, mono }) {
  return (
    <div className="kv">
      <span className="k">{k}</span>
      <span className={`v ${mono ? 'mono' : ''}`}>{v || '—'}</span>
    </div>
  );
}

export default function AboutModal({ info, onClose }) {
  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal narrow">
        <div className="modal-head">
          <h2>关于 MeowLauncher</h2>
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <p style={{ color: 'var(--muted)', marginTop: 0 }}>
            MeowLauncher 26.X.X（Beta）。外壳为 React + Vite 重写，界面结构参考官方启动器解包结果；不包含官方 JS 代码，
            不执行官方客户端逻辑。Electron 运行时复用官方安装包中的 31.7.5 二进制。
          </p>
          <Row k="版本" v={info?.version} />
          <Row k="版本线" v={info?.line} />
          <Row k="快照" v={info?.snapshot} />
          <Row k="更新频道" v={info?.channel} />
          <Row k="代号" v={info?.codename} />
          <Row k="构建时间" v={info?.builtAt} />
          <Row k="Electron / Chrome / Node" v={[info?.electron, info?.chrome, info?.node].filter(Boolean).join(' / ')} />
          <Row k="平台" v={info ? `${info.platform} ${info.arch}` : ''} />
          <Row k="安装目录" v={info?.exeDir} mono />
          <Row k="数据目录" v={info?.dataDir} mono />
          <Row k="日志文件" v={info?.log?.file} mono />
          <div className="actions">
            <button type="button" className="btn primary" onClick={onClose}>
              关闭
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
