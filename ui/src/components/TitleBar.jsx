import React from 'react';
import { api } from '../api.js';
import logo from '../assets/official/app-logo-login-mqasHnSv.png';

export default function TitleBar({ info, servers, currentServer, menuOpen, onToggleMenu, onSelectServer, onAbout }) {
  return (
    <header className="topbar">
      <div className="brand">
        <img className="brand-logo" src={logo} alt="" />
        <div className="brand-text">
          <span className="brand-title">MeowLauncher</span>
          <span className="brand-sub">26.X.X</span>
        </div>
      </div>

      <span className="topbar-pill">Beta {info?.version || '26.1.0'}</span>

      <div className="topbar-spacer" />

      <div className="server-select">
        <button type="button" onClick={onToggleMenu}>
          <span className={`server-dot ${currentServer.state === 'open' ? '' : 'closed'}`} />
          <span>区服 · {currentServer.name}</span>
          <span style={{ opacity: 0.5 }}>▾</span>
        </button>
        {menuOpen && (
          <div className="server-menu" onClick={(e) => e.stopPropagation()}>
            {servers.map((s) => (
              <button
                key={s.id}
                type="button"
                className={`server-item ${s.id === currentServer.id ? 'active' : ''}`}
                onClick={() => onSelectServer(s.id)}
              >
                <span>{s.name}</span>
                <span className="state">{s.state === 'open' ? '流畅' : '维护'}</span>
              </button>
            ))}
          </div>
        )}
      </div>

      <button type="button" className="bar-btn" style={{ background: 'transparent', border: 0, color: 'var(--muted)', cursor: 'pointer', fontSize: 12.5 }} onClick={onAbout}>
        关于
      </button>

      <div className="win-controls">
        <button type="button" className="win-btn" title="最小化" onClick={() => api.window.minimize()}>
          —
        </button>
        <button type="button" className="win-btn" title="最大化" onClick={() => api.window.toggleMaximize()}>
          ▢
        </button>
        <button type="button" className="win-btn close" title="关闭" onClick={() => api.window.close()}>
          ✕
        </button>
      </div>
    </header>
  );
}
