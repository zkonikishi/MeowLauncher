import React,{useRef,useState} from 'react';
import {api,unwrap} from '../api.js';
import OfficialBackground from './OfficialBackground.jsx';
import BetaBrand from './BetaBrand.jsx';
import bg from '../assets/official/seasonal/login-1600x910.jpg';
import '../official.css';
import '../max.css';

export default function BetaLoginPage({info,onLogin}){
 const [mode,setMode]=useState('login'),[username,setUsername]=useState(''),[password,setPassword]=useState(''),[confirm,setConfirm]=useState(''),[message,setMessage]=useState(''),[busy,setBusy]=useState(false);
 const lock=useRef(false);
 async function submit(e){
  e.preventDefault();if(lock.current)return;lock.current=true;setBusy(true);setMessage('');
  const input={username,password};setPassword('');setConfirm('');
  try{
   if(mode==='register'){
    if(input.password!==confirm)throw Error('两次密码不一致。');
    unwrap(await api.account.register(input));setMode('login');setMessage('注册已确认，请使用新账号登录。');
   }else{const account=unwrap(await api.account.login(input));if(!account?.nativeAccountId)throw Error('未取得有效的账号身份。');onLogin({method:'generic',account});}
  }catch(error){setMessage(error.message||'账号操作未完成。');}finally{input.password='';lock.current=false;setBusy(false);}
 }
 function change(next){setMode(next);setPassword('');setConfirm('');setMessage('');}
 return <div className="ol-shell max-official-login" style={{'--ol-bg':`url(${bg})`}}>
  <OfficialBackground login/><BetaBrand/>
  <div className="ol-controls"><button type="button" aria-label="最小化" onClick={()=>api.window.minimize()}>—</button><button type="button" aria-label="关闭" onClick={()=>api.window.close()}>×</button></div>
  <aside className="ol-sidebar"><div className="ol-sidebar-scroll"><form className="ol-content-box max-account-form" onSubmit={submit}>
   <div className="max-login-heading"><span>MEOWLAUNCHER BETA</span><h1>{mode==='register'?'注册账号':'账号登录'}</h1></div>
   <label className="max-form-row">用户名<input id="beta-account-username" value={username} disabled={busy} onChange={e=>setUsername(e.target.value)} autoComplete="username" maxLength={32} pattern="[A-Za-z0-9_-]{1,32}" required/></label>
   <label className="max-form-row">密码<input id="beta-account-password" type="password" value={password} disabled={busy} onChange={e=>setPassword(e.target.value)} autoComplete={mode==='register'?'new-password':'current-password'} required/></label>
   {mode==='register'&&<label className="max-form-row">确认密码<input id="beta-account-confirm" type="password" value={confirm} disabled={busy} onChange={e=>setConfirm(e.target.value)} autoComplete="new-password" required/></label>}
   <p className="max-flow-note">连接服主提供的账号服务。未配置接入服务时不会登录；账号登录成功不代表已进入游戏。</p>
   <p id="beta-account-message" className="max-login-error" role="alert">{message}</p>
   <button id="beta-account-submit" className="ol-submit" disabled={busy}>{busy?'处理中…':mode==='register'?'注册账号':'登录'}</button>
   <div className="max-login-links"><button className="ol-link" type="button" disabled={busy} onClick={()=>change(mode==='login'?'register':'login')}>{mode==='login'?'注册账号':'返回登录'}</button><button type="button" className="ol-link" disabled={busy} onClick={()=>setMessage('当前通用接入尚未配置找回密码服务，请联系服主。')}>忘记密码</button></div>
   <small>作者 zkonikishi · {info?.version}</small>
  </form></div></aside>
 </div>;
}
