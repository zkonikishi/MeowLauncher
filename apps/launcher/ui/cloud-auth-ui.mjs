// Optional operator-hosted account UI. Secret sessions/challenges stay native.
export function selectedCloud(config){
 const p=config?.providers?.find(p=>p.id===config.activeProvider);
 return p?.connection?.kind==='cloud-account'?p:null;
}
export function allowedMethods(config){return selectedCloud(config)?.connection.methods||['username-password'];}
const names={'username-password':'用户名密码','email-code':'邮箱验证码','phone-code':'手机验证码','third-party':'第三方账号'};
export function mountAccountExtensions({info,invoke,panel,onAuthenticated}){
 const make=(tag,text)=>{const e=document.createElement(tag);if(text)e.textContent=text;return e;};
 const details=make('details'),summary=make('summary','账号接入配置（服主）');details.append(summary);
 const note=make('p','配置本地/云端账号及已启用方式。只填公开连接信息，不填平台密钥、短信密钥或数据库密码。');details.append(note);
 const editor=make('textarea');editor.id='auth-config-editor';editor.rows=12;editor.maxLength=32768;editor.spellcheck=false;
 editor.value=JSON.stringify(info.authConfig||{schemaVersion:1,activeProvider:'local',providers:[{id:'local',label:'服主网关',connection:{kind:'local-gateway',target:info.effectiveTarget||info.genericProfile?.target||{host:'',port:27002,serverName:'',authorityId:'',realmId:'',saveProfileId:''}}}]},null,2);
 const configStatus=make('p');configStatus.setAttribute('role','status');const save=make('button','保存账号配置并重新登录');save.type='button';
 save.onclick=async()=>{save.disabled=true;try{const config=JSON.parse(editor.value);await invoke('save_auth_config',{config});location.reload();}catch(e){configStatus.textContent=e.message||'配置未能保存。';}finally{save.disabled=false;}};
 details.append(editor,save,configStatus);panel.append(details);
 const cloud=selectedCloud(info.authConfig);if(!cloud)return;
 const methods=allowedMethods(info.authConfig);
 const cloudButton=make('button','其他账号方式 / 注册 / 找回');cloudButton.id='cloud-auth-open';cloudButton.type='button';document.getElementById('login-view').append(cloudButton);
 if(!methods.includes('username-password')){document.getElementById('login-form').hidden=true;document.getElementById('register-account').hidden=true;}
 const modal=make('dialog');modal.id='cloud-auth-dialog';modal.setAttribute('aria-label','云端账号');
 const title=make('h2',cloud.label),close=make('button','关闭');close.type='button';close.id='cloud-auth-close';
 const form=make('form'),fields=make('fieldset'),purpose=make('select'),method=make('select');purpose.id='cloud-auth-purpose';method.id='cloud-auth-method';
 for(const [v,t] of [['login','登录'],['register','注册'],['reset','找回密码']]){const o=make('option',t);o.value=v;purpose.append(o);}
 for(const v of methods){const o=make('option',names[v]);o.value=v;method.append(o);}
 const input=(id,label,type='text')=>{const row=make('label',label),i=make('input');i.id=id;i.type=type;i.autocomplete=type==='password'?'new-password':'off';i.maxLength=type==='password'?72:254;row.append(i);fields.append(row);return i;};
 fields.append(purpose,method);const username=input('cloud-auth-username','游戏用户名'),identifier=input('cloud-auth-identifier','账号 / 邮箱 / 手机号'),password=input('cloud-auth-password','密码','password'),confirm=input('cloud-auth-confirm','确认密码','password'),code=input('cloud-auth-code','验证码');code.inputMode='numeric';code.maxLength=12;
 const send=make('button','获取验证码');send.type='button';send.id='cloud-auth-send-code';const submit=make('button','提交');submit.id='cloud-auth-submit';submit.type='submit';const status=make('p');status.id='cloud-auth-status';status.setAttribute('role','status');fields.append(send,submit);form.append(fields);modal.append(title,close,form,status);document.body.append(modal);
 let busy=false,epoch=0,handle=null,requestId=null,timer=null,completed=false,cooldown=0;
 const clear=()=>{password.value='';confirm.value='';code.value='';};
 const invalidate=()=>{epoch++;clear();clearTimeout(timer);timer=null;const oldRequest=requestId;requestId=null;if(oldRequest)void invoke('cancel_cloud_request',{requestId:oldRequest}).catch(()=>{});const previous=handle;handle=null;if(previous!==null)void invoke('cancel_cloud_login',{handle:previous}).catch(()=>{});};
 function reflect(){invalidate();status.textContent='';const m=method.value,p=purpose.value,otp=m==='email-code'||m==='phone-code';username.closest('label').hidden=p!=='register';password.closest('label').hidden=m==='third-party'||(p==='login'&&otp);confirm.closest('label').hidden=p==='login'||m==='third-party';identifier.closest('label').hidden=m==='third-party';code.closest('label').hidden=!otp;send.hidden=!otp;submit.disabled=m==='third-party'&&p!=='login'||p==='reset'&&!otp;submit.textContent=m==='third-party'?'打开浏览器登录':'提交';}
 purpose.onchange=reflect;method.onchange=reflect;close.onclick=()=>{invalidate();modal.close();};modal.addEventListener('cancel',()=>invalidate());modal.addEventListener('close',()=>{if(!completed)invalidate();else{epoch++;clear();clearTimeout(timer);}completed=false;});
 cloudButton.onclick=()=>{if(busy)return;completed=false;reflect();modal.showModal();};
 const run=async(fn)=>{if(busy)return;busy=true;fields.disabled=true;try{await fn();}catch(e){status.textContent=e.message||'账号操作未完成。';}finally{busy=false;fields.disabled=false;}};
 send.onclick=()=>run(async()=>{if(Date.now()<cooldown){status.textContent='请等待后再获取验证码。';return;}const r=await invoke('request_cloud_code',{method:method.value,identifier:identifier.value,purpose:purpose.value});cooldown=Date.now()+r.retryAfter*1000;status.textContent=`验证码已请求，有效 ${r.expiresIn} 秒。`;});
 async function poll(current,flow){
  if(current!==epoch||!modal.open)return;
  try{const identity=await invoke('poll_cloud_login',{handle:flow.handle});if(current!==epoch||!modal.open)return;if(identity){await invoke('accept_cloud_browser',{handle:flow.handle});if(current!==epoch||!modal.open)return;completed=true;handle=null;modal.close();onAuthenticated(identity);return;}timer=setTimeout(()=>poll(current,flow),flow.interval*1000);}
  catch(e){if(current===epoch){status.textContent=e.message||'浏览器登录未完成。';invalidate();}}
 }
 form.onsubmit=e=>{e.preventDefault();void run(async()=>{
  const m=method.value,p=purpose.value,current=epoch;
  if(m==='third-party'){invalidate();const mine=epoch;const flow=await invoke('begin_cloud_login');if(!modal.open||mine!==epoch){await invoke('cancel_cloud_login',{handle:flow.handle});return;}handle=flow.handle;status.textContent='请在系统浏览器完成授权；可关闭此窗口取消。';timer=setTimeout(()=>poll(mine,flow),flow.interval*1000);return;}
  if(p!=='login'&&password.value!==confirm.value)throw Error('两次密码不一致。');
  const args={method:m,identifier:identifier.value,password:password.value,code:code.value};clear();
  if(p==='login'){requestId=crypto.randomUUID();const identity=await invoke('login_cloud',{...args,requestId});args.password='';if(current!==epoch||!modal.open)return;await invoke('accept_cloud_request',{requestId});if(current!==epoch||!modal.open)return;requestId=null;completed=true;modal.close();onAuthenticated(identity);}
  else if(p==='register'){await invoke('register_cloud',{...args,username:username.value});args.password='';status.textContent='注册已确认，请切换到登录。';}
  else{await invoke('reset_cloud_password',args);args.password='';status.textContent='密码已重设，请重新登录。';}
 });};
 reflect();
}
