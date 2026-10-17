import React from 'react';
import wrench from '../assets/official/icon-toolbox-wrench-NIwi5Nqa.svg';

export default function BottomBar({ version, channel, onSettings, onVersions, onLogs, onAbout, onRepair, onOpenFolder }) {
  return (
    <footer className="bottombar">
      <button type="button" className="bar-btn" onClick={onSettings}>
        <img src={wrench} alt="" />
        设置
      </button>
      <button type="button" className="bar-btn" onClick={onVersions}>
        版本管理
      </button>
      <button type="button" className="bar-btn" onClick={onRepair}>
        修复 / 重新扫描
      </button>
      <button type="button" className="bar-btn" onClick={onOpenFolder}>
        打开数据目录
      </button>
      <button type="button" className="bar-btn" onClick={onLogs}>
        日志
      </button>
      <button type="button" className="bar-btn" onClick={onAbout}>
        关于
      </button>
      <span className="spacer" />
      <span className="build">
        {version || '26.1.0'} · {channel || 'beta'} · 本地存档，不联网
      </span>
    </footer>
  );
}
