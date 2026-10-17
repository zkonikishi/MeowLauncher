import React from 'react';

export default function Toast({ toasts }) {
  if (!toasts.length) return null;
  return (
    <div className="toast-wrap">
      {toasts.map((t) => (
        <div key={t.id} className={`toast ${t.kind || 'info'}`}>
          {t.text}
        </div>
      ))}
    </div>
  );
}
