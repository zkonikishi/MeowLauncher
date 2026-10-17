import React, { useState } from 'react';

const TABS = ['公告', '活动', '全部'];

export default function NewsPanel({ news, onOpenLogs }) {
  const [tab, setTab] = useState('公告');
  const rows = tab === '全部' ? news : news.filter((n) => n.tag === tab);

  return (
    <section className="news-panel">
      <div className="news-head">
        <div className="news-tabs">
          {TABS.map((t) => (
            <button key={t} type="button" className={`news-tab ${tab === t ? 'active' : ''}`} onClick={() => setTab(t)}>
              {t}
            </button>
          ))}
        </div>
      </div>
      <ul className="news-list">
        {rows.map((n, i) => (
          <li key={i} className="news-item">
            <span className="tag">{n.tag}</span>
            <span className="title">{n.title}</span>
            {n.fresh && <span className="new" />}
            <span style={{ marginLeft: 'auto', fontSize: 11, color: 'var(--dim)' }}>{n.date}</span>
          </li>
        ))}
      </ul>
      <div className="news-foot">
        <span>本地公告 · 不联网获取</span>
        <button type="button" className="btn ghost" style={{ padding: '2px 10px', fontSize: 11.5 }} onClick={onOpenLogs}>
          查看日志
        </button>
      </div>
    </section>
  );
}
