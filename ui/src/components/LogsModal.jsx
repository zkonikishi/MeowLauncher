import React, { useCallback, useEffect, useState } from 'react';
import { api, unwrap } from '../api.js';

export default function LogsModal({ onClose, onToast, onOpenDir }) {
  const [lines, setLines] = useState([]);
  const [loading, setLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = unwrap(await api.log.read(500));
      setLines(Array.isArray(data) ? data : data?.lines || []);
    } catch (err) {
      setLines([`读取失败：${err?.message || err}`]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <div className="modal-head">
          <h2>运行日志</h2>
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="actions" style={{ justifyContent: 'flex-start', marginTop: 0 }}>
            <button type="button" className="btn" onClick={load}>
              刷新
            </button>
            <button type="button" className="btn" onClick={onOpenDir}>
              打开日志目录
            </button>
          </div>
          <pre className="log-box">{loading ? '读取中…' : lines.join('\n') || '（空）'}</pre>
        </div>
      </div>
    </div>
  );
}
