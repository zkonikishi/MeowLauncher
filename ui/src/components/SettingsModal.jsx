import React, { useState } from 'react';
import { api, unwrap } from '../api.js';

export default function SettingsModal({ config, info, onClose, onSave, onToast }) {
  const [form, setForm] = useState({
    root: config?.game?.root ?? '',
    exe: config?.game?.exe ?? 'game.exe',
    args: config?.game?.args ?? '',
    channel: config?.channel ?? 'beta',
    manifest: config?.updater?.manifest ?? '',
  });
  const [saving, setSaving] = useState(false);

  const set = (key) => (e) => setForm((f) => ({ ...f, [key]: e.target.value }));

  const pick = async (kind) => {
    try {
      const data = unwrap(await api.dialog[kind]());
      if (data?.canceled) return;
      if (kind === 'pickDir' && data?.dir) setForm((f) => ({ ...f, root: data.dir }));
      if (kind === 'pickExe' && data?.file) setForm((f) => ({ ...f, exe: data.file }));
    } catch (err) {
      onToast(`选择失败：${err?.message || err}`, 'err');
    }
  };

  const save = async () => {
    setSaving(true);
    try {
      await onSave(form);
      onClose();
    } catch (err) {
      onToast(`保存失败：${err?.message || err}`, 'err');
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="overlay" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal">
        <div className="modal-head">
          <h2>设置</h2>
          <button type="button" className="x" onClick={onClose}>
            ✕
          </button>
        </div>
        <div className="modal-body">
          <div className="field">
            <label>游戏根目录（包含 versions 或客户端本体）</label>
            <div className="input-row">
              <input type="text" value={form.root} onChange={set('root')} placeholder="例如 C:\Games\Client" />
              <button type="button" className="btn" onClick={() => pick('pickDir')}>
                选择…
              </button>
            </div>
          </div>

          <div className="field">
            <label>可执行文件（相对版本目录或绝对路径）</label>
            <div className="input-row">
              <input type="text" value={form.exe} onChange={set('exe')} placeholder="game.exe" />
              <button type="button" className="btn" onClick={() => pick('pickExe')}>
                选择…
              </button>
            </div>
          </div>

          <div className="field">
            <label>启动参数模板，可用 {'{root} {version} {account} {channel}'}</label>
            <textarea rows={3} value={form.args} onChange={set('args')} placeholder="--host-ip=127.0.0.1 ..." />
          </div>

          <div className="field">
            <label>更新频道</label>
            <select value={form.channel} onChange={set('channel')}>
              <option value="dev">dev</option>
              <option value="snapshot">snapshot</option>
              <option value="beta">beta</option>
              <option value="stable">stable</option>
            </select>
          </div>

          <div className="field">
            <label>更新清单地址（本地路径或 https URL；26.1.0 暂只做检查）</label>
            <input type="text" value={form.manifest} onChange={set('manifest')} placeholder="https://…/channel-beta.json" />
          </div>

          <div className="kv">
            <span className="k">启动器版本</span>
            <span className="v mono">{info?.version || '26.1.0'}</span>
          </div>
          <div className="kv">
            <span className="k">数据目录</span>
            <span className="v mono">{info?.dataDir || '—'}</span>
          </div>

          <div className="actions">
            <button type="button" className="btn ghost" onClick={onClose}>
              取消
            </button>
            <button type="button" className="btn primary" disabled={saving} onClick={save}>
              {saving ? '保存中…' : '保存'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
