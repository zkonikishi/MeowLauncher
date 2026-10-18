import {errorMessage} from './model.mjs';
import {validateRegistrationIdentity} from './platform-identity.mjs';
export function mountAccountForms(invoke,{canOpen,onRegistered,onReset,platformIdentity=false}){
 const $=id=>document.getElementById(id),dialog=$('account-dialog');
 if(platformIdentity){dialog.classList.add('platform-registration');const fields=$('account-fields');for(const [id,title,placeholder,limit] of [['account-login-id','登录 ID','4–32 位英文字母或数字',32],['account-nickname','昵称（可选）','留空时使用登录 ID',64]]){const label=document.createElement('label');label.textContent=title;const input=document.createElement('input');input.id=id;input.maxLength=limit;input.placeholder=placeholder;input.autocomplete='off';label.append(input);fields.insertBefore(label,$('account-email').closest('label'));}const notice=document.createElement('p');notice.className='identity-registration-notice';notice.textContent='登录 ID 与 UID 是唯一身份标识，注册后不能自行修改。昵称可在账户中心修改。';fields.prepend(notice);}
 let purpose='register',working=false,epoch=0,timer=0,nextSend=0;
 function clear(){epoch++;for(const id of ['account-email','account-code','account-password','account-confirm','account-login-id','account-nickname'])if($(id))$(id).value='';}
 function cooldown(){const left=Math.max(0,Math.ceil((nextSend-Date.now())/1000));$('account-send').disabled=working||left>0;$('account-send').textContent=left?`${left} 秒后重发`:'获取验证码';}
 function lock(value){working=value;$('account-fields').disabled=value;$('account-close').disabled=value;cooldown();}
 function open(mode){if(!canOpen()||working)return;purpose=mode;clear();$('account-title').textContent=mode==='register'?'注册账号':'重设密码';$('account-submit').textContent=mode==='register'?'注册':'确认重设密码';$('account-state').textContent=mode==='register'?'完成邮箱验证后分配 UID，注册成功后请登录。':'请验证账号绑定的邮箱，再设置新密码。';dialog.showModal();cooldown();clearInterval(timer);timer=setInterval(cooldown,1000);}
 $('register-account').onclick=()=>open('register');$('forgot-password').onclick=()=>{if(platformIdentity){void invoke('open_account_page',{page:'forgot'}).catch(e=>{$('account-state').textContent=errorMessage(e);});}else open('reset');};
 $('account-close').onclick=()=>{if(!working)dialog.close();};
 dialog.addEventListener('cancel',e=>{if(working)e.preventDefault();});
 dialog.addEventListener('close',()=>{clear();clearInterval(timer);});
 function email(){const value=$('account-email').value.trim();if(!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value))throw{code:'invalid_email',message:'请输入有效邮箱。'};return value;}
 $('account-send').onclick=async()=>{
  if(working||Date.now()<nextSend)return;let target;try{target=email();}catch(e){$('account-state').textContent=errorMessage(e);return;}
  const current=epoch;lock(true);$('account-state').textContent='正在请求验证码…';
  try{const result=await invoke('request_code',{email:target,purpose});nextSend=Date.now()+Math.max(1,Math.min(3600,Number(result.retryAfter)||60))*1000;if(current===epoch)$('account-state').textContent='验证码请求已受理，请查收邮箱及垃圾邮件。';}
  catch(e){if(e?.retry_after)nextSend=Date.now()+Math.min(3600,Number(e.retry_after))*1000;if(current===epoch)$('account-state').textContent=errorMessage(e);}
  finally{lock(false);}
 };
 $('account-form').onsubmit=async e=>{
  e.preventDefault();if(working)return;
  let target;const password=$('account-password').value,confirm=$('account-confirm').value,code=$('account-code').value.trim();
  const loginId=platformIdentity?$('account-login-id').value:'',nickname=platformIdentity?$('account-nickname').value:'';
  if(platformIdentity){const invalid=validateRegistrationIdentity(loginId,nickname);if(invalid){$('account-state').textContent=invalid;return;}}
  try{target=email();if(!/^\d{6}$/.test(code))throw{code:'invalid_code',message:'请输入 6 位数字验证码。'};if(password.length<6||password.length>128)throw{code:'invalid_password',message:'密码需 6–128 个字符。'};if(password!==confirm)throw{code:'password_mismatch',message:'两次输入的密码不一致。'};}
  catch(error){$('account-state').textContent=errorMessage(error);return;}
  const current=epoch;lock(true);$('account-password').value='';$('account-confirm').value='';$('account-state').textContent='正在提交，请稍候…';
  try{if(purpose==='register'){const result=await invoke(platformIdentity?'platform_register':'complete_registration',{email:target,code,password,...(platformIdentity?{loginId,nickname:nickname||null}:{})});if(!/^[0-9]{8}$/.test(result.uid)||result.uid==='00000000')throw{code:'invalid_response',message:'注册结果未确认，请先找回 UID，勿重复注册。'};if(current===epoch){dialog.close();onRegistered(result.uid,result.provisioningPending,loginId);}}
   else{await invoke('complete_password_reset',{email:target,code,newPassword:password});if(current===epoch){dialog.close();await onReset();}}}
  catch(error){if(current===epoch)$('account-state').textContent=errorMessage(error);}
  finally{lock(false);}
 };
}
