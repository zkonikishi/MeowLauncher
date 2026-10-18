// Select the independent ordinary entry before mounting any platform forms.
let ordinary=false;
try {
 const info=await window.__TAURI__?.core?.invoke('bootstrap');
 ordinary=info?.edition==='sponsor'&&info?.accountMode==='simple';
} catch {}
await import(ordinary?'./generic-ui.mjs':'./app.js');
