import { defaults, validateCredentials, settingsFromInputs, errorMessage } from './model.mjs';
import {validatePlatformCredentials} from './platform-identity.mjs';
const $=id=>document.getElementById(id);
let settings=defaults(), busy=false, authenticated=false;
// 0.6.0 第 7 项「记住密码」：密码不回传界面，界面只用掩码表示“已保存”，
// 点登录时由主程序用本机 DPAPI 密文换票（README §13.1 第 7 项）。
const SAVED_PASSWORD_MASK='••••••••••';
let savedPassword=false;
const native=window.__TAURI__?.core;
const invoke=(command,args)=>{const started=performance.now();const result=native?native.invoke(command,args):Promise.reject({code:'desktop_required',message:'请使用 MeowLauncher 桌面程序，浏览器预览不提供登录或启动功能。'});return result.finally(()=>{if(command==='platform_login'||command==='refresh_native_target')performance.measure('launcher:'+command,{start:started,end:performance.now()});});};
let bootInfo,bootError;
try { bootInfo=await invoke('bootstrap'); settings=bootInfo.settings; } catch(e) { bootError=e; }
const edition='modern'; // Accept older settings, render the unified launcher.
settings.theme='modern';
const platformIdentity=bootInfo?.accountMode==='platform-login-id-v1';
let platformGameReady=false, platformSessionEpoch=0, platformTargetPendingEpoch=0;
if(platformIdentity?!/^[A-Za-z0-9]{4,32}$/.test(settings.identifier):(!/^[0-9]{8}$/.test(settings.identifier)||settings.identifier==='00000000'))settings.identifier='';
const legacyOnline=bootInfo?.accountMode==='legacy-online';
document.documentElement.dataset.edition=edition;
const {markup}=await import('./modern.mjs');
$('edition').innerHTML=markup;
if(platformIdentity){
 document.documentElement.dataset.accountMode='platform';document.querySelector('.town-feature small').textContent='115US';$('select-server').textContent='115US';for(const choice of document.querySelectorAll('#server-dialog input[name=server]')){choice.checked=choice.value==='115us';choice.disabled=choice.value!=='115us';}const author=document.createElement('small');author.className='launcher-author';author.textContent='by zkonikishi';document.querySelector('.modern-stage').append(author);
 $('directory').placeholder='选择包含 DFO.exe 的 115US 客户端目录';for(const id of ['manual-client-download','check-client-update','download-client','pause-client-download'])$(id).hidden=true;$('client-download-state').textContent='请选择已有的 115US 客户端目录。';
 const input=$('identifier'),wrapper=document.createElement('div');wrapper.className='platform-identifier';input.before(wrapper);wrapper.append(input);input.maxLength=254;input.placeholder='登录 ID / UID / 邮箱';input.setAttribute('aria-label','登录 ID、UID 或邮箱');
 const method=document.createElement('select');method.id='platform-login-method';method.setAttribute('aria-label','登录方式');for(const [value,label] of [['auto','自动'],['id','ID'],['uid','UID'],['email','邮箱']]){const option=document.createElement('option');option.value=value;option.textContent=label;method.append(option);}wrapper.append(method);
 $('remember-password').closest('label').hidden=true;$('recover-uid-open').hidden=true;document.querySelector('.vault-settings').hidden=true;
}
if(platformIdentity){const {mountGameWindow}=await import('./game-window.mjs');await mountGameWindow(invoke);}
const isOrdinaryEdition=bootInfo?.edition==='sponsor';
$('edition-usage-notice').hidden=!isOrdinaryEdition;$('edition-usage-details').hidden=!isOrdinaryEdition;
if(legacyOnline){$('identifier').placeholder='请输入 8 位 UID';$('identifier').setAttribute('aria-label','8 位 UID');}
if(bootInfo?.publicConfig){
 const config=bootInfo.publicConfig;
 const label=document.createElement('p');label.textContent=config.brand+' · '+new URL(config.accountBaseUrl).host;
 label.title=label.textContent;label.className='account-destination';document.querySelector('#login-view').prepend(label);
 $('directory').readOnly=true;$('choose').disabled=true;
 const realm=document.querySelector('.region');if(realm)realm.textContent=config.realmName;
}

const {mountMusic}=await import('./music.mjs');
const music=mountMusic();
window.__TAURI__?.event?.listen("game-exited",()=>music.resumeAfterGame());
if(edition==='modern'){
  const {mountScene}=await import('./scene.mjs');
  mountScene(document.querySelector('.modern-stage'));
  const {mountContent}=await import('./content.mjs');mountContent(invoke,bootInfo?.publicConfig?.realmId==='hk'?'s4a21':(bootInfo?.publicConfig?.realmId||'s4a21'));
}
function status(message,kind='neutral'){$('status').textContent=message;$('status-dot').dataset.kind=kind;}
function setBusy(value){busy=value;$('login-fields').disabled=value;for(const id of ['choose','save','inspect','probe','check-client-update','download-client','recover-uid-open'])$(id).disabled=value||(id==='choose'&&!!bootInfo?.publicConfig);$('launch').querySelector('span').textContent=value?'处理中…':edition==='retro'?'开始游戏':'登录';}
async function run(message,fn){if(busy)return;setBusy(true);status(message,'busy');try{await fn();}catch(e){status(errorMessage(e),'error');}finally{setBusy(false);}}
function credentialState(){
  const state=$('credential-state');if(state)state.textContent=savedPassword?`已保存密码（UID ${$('identifier').value.trim()||'—'}）`:'未保存密码';
  const hint=$('credential-hint');if(hint)hint.textContent=savedPassword?'已保存密码：账号已填充，直接点“登录”即可。':'';
}
function armSavedPassword(){const field=$('password');if(savedPassword&&field&&!field.value)field.value=SAVED_PASSWORD_MASK;}
async function loadSavedCredential(){
  if(platformIdentity){savedPassword=false;return;}
  try{
    const saved=await invoke('load_credential');
    savedPassword=!!saved;
    if(saved){
      if(saved.identifier)$('identifier').value=saved.identifier;
      $('remember').checked=true;if($('remember-password'))$('remember-password').checked=true;
      armSavedPassword();
    }
  }catch{savedPassword=false;}
  credentialState();
}
async function clearSavedCredential(){
  await invoke('clear_credential');
  savedPassword=false;const field=$('password');if(field&&field.value===SAVED_PASSWORD_MASK)field.value='';
  if($('remember-password'))$('remember-password').checked=false;
  credentialState();
}
async function persistCredential(identifier,password){
  if(!$('remember-password')?.checked){await clearSavedCredential();return false;}
  try{await invoke('save_credential',{identifier,password});savedPassword=true;credentialState();return true;}
  catch(e){savedPassword=false;credentialState();throw e;}
}
let locationReady=false,locationTimer;
function locationFeedback(ready){
  locationReady=ready;clearTimeout(locationTimer);
  const card=$('home-directory');if(!card)return;
  card.hidden=false;card.dataset.state=ready?'ready':'pending';const start=$('show-login');if(start){const label=$('start-label')||start;label.textContent=ready?'开始游戏':'安装游戏';}updatePlatformStart();
  card.querySelector('strong').textContent=ready?'客户端已就绪':'定位游戏客户端';
  scheduleLocationHide();
}
function scheduleLocationHide(){
  clearTimeout(locationTimer);
  if(locationReady&&edition==='modern'&&!$('home-view').hidden&&!$('settings-dialog').open){
    locationTimer=setTimeout(()=>{$('home-directory').hidden=true;},1800);
  }
}
function showPage(page){if(page==='settings')$('settings-dialog').showModal();else $('settings-dialog').close();scheduleLocationHide();}
function screen(name){if(edition!=='modern'||(name==='home'&&!authenticated))return;$('account-menu-toggle').hidden=!authenticated;$('account-menu').hidden=true;$('account-menu-toggle').setAttribute('aria-expanded','false');document.querySelector('.modern-stage').dataset.screen=name;$('login-view').hidden=name!=='login';$('home-view').hidden=name!=='home';scheduleLocationHide();}
function snapshot(){return {...settingsFromInputs($('directory').value,settings.theme,$('remember').checked||!!$('remember-password')?.checked,$('identifier').value),...(platformIdentity?{identifier:$('remember').checked&&/^[A-Za-z0-9]{4,32}$/.test($('identifier').value)?$('identifier').value:''}:{}),closeToTray:$('close-behavior').value==='tray',restoreAfterGame:$('restore-after-game').checked};}
function updatePlatformStart(){if(!platformIdentity)return;const ready=authenticated&&platformGameReady&&bootInfo.nativeTargetReady===true;$('show-login').disabled=!ready;($('start-label')||$('show-login')).textContent=ready?'开始游戏':authenticated&&platformGameReady?'正在连接…':'账号准备中';$('show-login').title=ready?'开始游戏':'游戏账号或服务器连接尚未准备好';}
async function refreshPlatformTarget(epoch){if(platformTargetPendingEpoch===epoch)return;platformTargetPendingEpoch=epoch;try{const ready=await invoke('refresh_native_target');if(epoch!==platformSessionEpoch||!authenticated)return;bootInfo.nativeTargetReady=ready===true;updatePlatformStart();status(ready?'登录成功，请点击开始游戏。':'连接尚未就绪，可打开服务器选择后确认重试。',ready?'success':'error');}catch(e){if(epoch!==platformSessionEpoch||!authenticated)return;bootInfo.nativeTargetReady=false;updatePlatformStart();status(errorMessage(e)+' 可打开服务器选择后确认重试。','error');}finally{if(platformTargetPendingEpoch===epoch)platformTargetPendingEpoch=0;}}
function summary(){$('directory-summary').textContent=settings.clientDirectory?settings.clientDirectory:'尚未选择客户端目录';$('directory-summary').title=settings.clientDirectory;}
async function persist(){const old=settings.clientDirectory;settings=await invoke('save_settings',{settings:snapshot()});summary();if(old!==settings.clientDirectory)locationFeedback(false);}
async function inspectDirectory(directory){
  locationFeedback(false);
  try{const report=await invoke('inspect_client',{directory});locationFeedback(report.ready&&directory===settings.clientDirectory);return report;}
  catch(e){locationFeedback(false);throw e;}
}
function renderReport(report){
  $('check-items').replaceChildren();
  for(const item of report.items){const row=document.createElement('div');row.className='check-row';const icon=document.createElement('span');icon.className=`check-icon${item.ok?'':' bad'}`;icon.textContent=item.ok?'✓':'×';const name=document.createElement('strong');name.textContent=item.name;const detail=document.createElement('small');detail.textContent=item.detail;row.append(icon,name,detail);$('check-items').append(row);}
  $('check-summary').textContent=report.ready?'文件条件已满足':'需要处理';$('check-note').textContent=report.note+((report.warnings||[]).length?' 提示：'+report.warnings.join('；'):'');
  status(report.ready?'客户端文件检查通过；进入选角仍需游戏内验证。':'客户端检查未通过，请处理列表中的问题。',report.ready?'success':'error');
}
function settingsTab(name){for(const panel of document.querySelectorAll('[data-settings-panel]'))panel.hidden=panel.dataset.settingsPanel!==name;for(const button of document.querySelectorAll('[data-settings-tab]')){const active=button.dataset.settingsTab===name;button.classList.toggle('selected',active);if(active)button.setAttribute('aria-current','page');else button.removeAttribute('aria-current');}}
for(const button of document.querySelectorAll('[data-settings-tab]'))button.onclick=()=>settingsTab(button.dataset.settingsTab);
$('save-preferences').onclick=()=>run('正在保存设置…',async()=>{await persist();status('设置已保存。','success');});
$('account-menu-toggle').onclick=()=>{const opened=$('account-menu').hidden;$('account-menu').hidden=!opened;$('account-menu-toggle').setAttribute('aria-expanded',String(opened));};
$('account-menu-settings').onclick=()=>{$('account-menu').hidden=true;$('account-menu-toggle').setAttribute('aria-expanded','false');showPage('settings');};
document.addEventListener('pointerdown',e=>{if(!e.target.closest('#account-menu')&&!e.target.closest('#account-menu-toggle')){$('account-menu').hidden=true;$('account-menu-toggle').setAttribute('aria-expanded','false');}});
document.addEventListener('keydown',e=>{if(e.key==='Escape'){$('account-menu').hidden=true;$('account-menu-toggle').setAttribute('aria-expanded','false');}});
function closeToolbox(){$('toolbox-panel').hidden=true;$('toolbox-toggle').setAttribute('aria-expanded','false');}
$('toolbox-toggle').onclick=()=>{const show=$('toolbox-panel').hidden;$('toolbox-panel').hidden=!show;$('toolbox-toggle').setAttribute('aria-expanded',String(show));};
$('toolbox-client').onclick=()=>{closeToolbox();showPage('settings');settingsTab('storage');};
$('toolbox-update').onclick=()=>{closeToolbox();showPage('settings');settingsTab('about');void checkUpdate();};
$('toolbox-diagnostics').onclick=()=>{closeToolbox();$('diagnostics').click();};
$('toolbox-website').onclick=()=>{closeToolbox();$('website').click();};
document.addEventListener('pointerdown',e=>{if(!e.target.closest('#toolbox-panel')&&!e.target.closest('#toolbox-toggle'))closeToolbox();});
document.addEventListener('keydown',e=>{if(e.key==='Escape')closeToolbox();});
$('client-shortcut').addEventListener('click',()=>showPage('settings'));
$('close-settings').addEventListener('click',()=>showPage('home'));
$('settings-dialog').addEventListener('close',scheduleLocationHide);
$('directory').addEventListener('input',()=>locationFeedback(false));

$('show-login')?.addEventListener('click',()=>{
 if(!authenticated){screen('login');return;}
 if(!locationReady||!$('directory').value.trim()){showPage('settings');status('请选择客户端目录，可定位已有客户端，或下载 / 更新游戏。','neutral');return;}
 run('正在检查并启动游戏…',async()=>{try{await persist();const response=await invoke('start_authenticated',{directory:settings.clientDirectory});music.stopForGame();status(response.message,'success');}catch(e){if(['login_required','token_expired','auth_failed'].includes(e.code)){authenticated=false;screen('login');armSavedPassword();}throw e;}});
});
const {mountAccountForms}=await import('./account-forms.mjs');
mountAccountForms(invoke,{platformIdentity,canOpen:()=>!busy,onRegistered(uid,pending,id){$('identifier').value=platformIdentity?id:uid;if(platformIdentity)$('platform-login-method').value='id';savedPassword=false;$('password').value='';credentialState();status(`注册成功，UID ${uid}。${pending?'游戏账号正在开通，请稍后登录。':'请使用新账号登录。'}`,'success');},async onReset(){await clearSavedCredential();status('密码已重设，请使用新密码登录。','success');}});
if(platformIdentity)$('forgot-password').onclick=()=>run('正在打开账户中心…',()=>invoke('open_account_page',{page:'forgot'}));
$('home-directory')?.addEventListener('click',()=>showPage('settings'));
$('select-server')?.addEventListener('click',()=>{if(!busy)$('server-dialog').showModal();});
$('server-close').onclick=()=>$('server-dialog').close('cancel');
$('server-dialog').addEventListener('close',()=>{if(platformIdentity&&authenticated&&platformGameReady&&!bootInfo.nativeTargetReady&&$('server-dialog').returnValue==='confirm'){status('正在重新连接游戏服务…','busy');void refreshPlatformTarget(platformSessionEpoch);}});

$('retro-inspect')?.addEventListener('click',()=>{if(!busy){showPage('settings');$('inspect').click();}});
for(const [id,action] of [['minimize','minimize'],['close-window','close']])$(id).addEventListener('click',async()=>{try{await invoke('window_action',{action});}catch(e){status(errorMessage(e),'error');}});
$('identifier').addEventListener('input',()=>{if(savedPassword){savedPassword=false;if($('password').value===SAVED_PASSWORD_MASK)$('password').value='';credentialState();}});
$('toggle-password').addEventListener('click',()=>{const field=$('password');if(savedPassword&&field.value===SAVED_PASSWORD_MASK&&field.type==='password'){status('已保存的密码不会在界面中显示；直接点“登录”即可，重新输入则覆盖。','neutral');return;}const show=field.type==='password';field.type=show?'text':'password';$('toggle-password').textContent=show?'隐藏':'显示';$('toggle-password').setAttribute('aria-label',show?'隐藏密码':'显示密码');});
$('remember')?.addEventListener('change',()=>{if(!$('remember').checked)run('正在清除已保存密码…',async()=>{await clearSavedCredential();status('已取消记住账号，本机保存的密码已清除。','success');});});
$('remember-password')?.addEventListener('change',()=>{if($('remember-password').checked){$('remember').checked=true;credentialState();}else run('正在清除已保存密码…',async()=>{await clearSavedCredential();status('已取消记住密码，本机保存的密码已清除。','success');});});
$('clear-credential')?.addEventListener('click',()=>run('正在清除已保存密码…',async()=>{await clearSavedCredential();status('已清除本机保存的密码。','success');}));
$('choose').addEventListener('click',()=>run('请选择客户端目录…',async()=>{const directory=await invoke('choose_directory');if(directory){$('directory').value=directory;await persist();renderReport(await inspectDirectory(directory));}else status('已取消选择，当前目录未变更。');}));
$('save').addEventListener('click',()=>run('正在保存并检查…',async()=>{await persist();renderReport(await inspectDirectory(settings.clientDirectory));}));
$('inspect').addEventListener('click',()=>run('正在检查客户端文件…',async()=>renderReport(await inspectDirectory($('directory').value.trim()))));
$('probe').addEventListener('click',()=>run('正在安全连接账号服务，最多等待 12 秒…',async()=>{try{const response=await invoke('check_service');$('service-status').textContent='账号服务可连接';status(response.message,'success');}catch(e){$('service-status').textContent='连接失败';throw e;}}));
$('website').addEventListener('click',()=>run('正在打开官网…',async()=>{await invoke('open_website');status('已请求在默认浏览器中打开官网。');}));
$('diagnostics').addEventListener('click',()=>run('正在打开诊断目录…',async()=>{await invoke('open_diagnostics');status('诊断目录已打开。');}));
$('login-form').addEventListener('submit',e=>{
  e.preventDefault();if(busy)return;
  const identifier=$('identifier').value.trim(), typed=$('password').value;
  const usingSaved=savedPassword&&typed===SAVED_PASSWORD_MASK;
  const password=usingSaved?'':typed;
  const invalid=platformIdentity?validatePlatformCredentials($('platform-login-method').value,identifier,password):validateCredentials(identifier,usingSaved?SAVED_PASSWORD_MASK:password);if(invalid){status(invalid,'error');return;}
  const directory=$('directory').value.trim();if(edition==='retro'&&!directory){status('请先在设置中选择客户端目录。','error');showPage('settings');return;}
  $('password').value='';$('password').type='password';$('toggle-password').textContent='显示';$('toggle-password').setAttribute('aria-label','显示密码');
  run('正在登录，请稍候…',async()=>{await persist();if(edition==='modern'){
    if(platformIdentity){authenticated=false;platformGameReady=false;++platformSessionEpoch;screen('login');updatePlatformStart();const account=await invoke('platform_login',{identifierType:$('platform-login-method').value,identifier,password});if(!/^\d{8}$/.test(account.uid)||typeof account.nickname!=='string'||typeof account.gameReady!=='boolean')throw{code:'invalid_response',message:'登录结果未确认，请重试。'};$('account-menu-uid').textContent='UID '+account.uid;$('account-menu-toggle').textContent=account.nickname;platformGameReady=account.gameReady;authenticated=true;bootInfo.nativeTargetReady=false;const epoch=++platformSessionEpoch;screen('home');updatePlatformStart();status(platformGameReady?'登录成功，正在连接游戏服务…':'登录成功，账号准备中。','success');if(platformGameReady)void refreshPlatformTarget(epoch);return;}
    try{const uid=await invoke('login_account',{identifier,password});const label=typeof uid==='string'?uid:identifier;$('account-menu-uid').textContent='UID '+label;$('account-menu-toggle').textContent='UID '+label;}
    catch(error){if(['auth_failed','invalid_identifier'].includes(error.code)){savedPassword=false;credentialState();}throw error;}
    let saved=false,saveWarning='';try{saved=usingSaved?true:await persistCredential(identifier,password);}catch(e){saveWarning=errorMessage(e);}
    authenticated=true;screen('home');
    status(saveWarning?'登录成功；本机凭据处理失败：'+saveWarning:saved?'登录成功，已在本机保存密码；请点击开始游戏。':'登录成功，请点击开始游戏。',saveWarning?'error':'success');
  }else{const response=await invoke('launch_game',{directory,identifier,password});music.stopForGame();status(response.message,'success');}});
});
async function initialize(){
  setBusy(true);
  try{if(bootError)throw bootError;const info=bootInfo;$('close-behavior').value=settings.closeToTray===false?'exit':'tray';$('restore-after-game').checked=settings.restoreAfterGame!==false;$('release-notes').textContent=info.releaseNotes||(info.edition==='sponsor'?'MeowLauncher 通用版':'MeowLauncher为Meowopia服务器定制版');$('directory').value=settings.clientDirectory;$('identifier').value=settings.identifier;$('remember').checked=settings.rememberIdentifier;summary();$('version').textContent=`${info.edition==='sponsor'?'MeowLauncher':'MeowLauncher Max'} ${info.version} · ${info.edition==='sponsor'?'通用版':'Meowopia 定制版'}`;status(info.warning?errorMessage(info.warning):'准备就绪。请先登录账号。',info.warning?'error':'neutral');await loadSavedCredential();}
  catch(e){status(errorMessage(e),'error');}finally{setBusy(false);}
  if(settings.clientDirectory)await run('正在检查已保存的客户端目录…',async()=>{await inspectDirectory(settings.clientDirectory);status(locationReady?'客户端已就绪。':'客户端目录需要检查。',locationReady?'success':'error');});
}
initialize();
updatePlatformStart();

$('logout-account')?.addEventListener('click',()=>run('正在退出账号…',async()=>{try{await invoke('logout_account');status('已退出账号。');authenticated=false;platformGameReady=false;++platformSessionEpoch;screen('login');updatePlatformStart();armSavedPassword();}catch(e){if(!platformIdentity){authenticated=false;screen('login');armSavedPassword();}throw e;}}));

let checkingUpdate=false;
async function checkUpdate(){
 if(checkingUpdate)return;checkingUpdate=true;$('check-update').disabled=true;$('update-status').textContent='正在在线检查…';$('download-update').hidden=true;
 try{const result=await invoke('check_update');$('update-status').textContent=result.available?`发现新版 ${result.version}，可下载并校验。`:`当前版本 ${bootInfo?.version||''} 已是最新。`;$('download-update').hidden=!result.available;}
 catch(e){$('update-status').textContent=e?.message||'暂时无法检查更新，请稍后重试。';}
 finally{checkingUpdate=false;$('check-update').disabled=false;}
}
$('check-update').addEventListener('click',checkUpdate);
$('download-update').addEventListener('click',async()=>{
 if(checkingUpdate)return;checkingUpdate=true;$('check-update').disabled=true;$('download-update').disabled=true;$('open-update-folder').hidden=true;
 $('update-status').textContent='正在下载新版并校验 SHA256，请稍候…';
 try{const result=await invoke('download_update');if(!result.verified||result.program!=='MeowLauncher')throw{message:'更新文件校验失败。'};$('update-status').textContent=`${result.version} 下载完成，SHA256 和程序身份校验通过。请退出后替换原目录中的启动器，保留 data。`;$('open-update-folder').hidden=false;}
 catch(e){$('update-status').textContent=e?.message||'下载失败，现有程序未修改。';}
 finally{checkingUpdate=false;$('check-update').disabled=false;$('download-update').disabled=false;}
});
$('open-update-folder').addEventListener('click',()=>run('正在打开新版文件夹…',()=>invoke('open_update')));
if(native&&!bootError)checkUpdate();

function byteSize(n){return n>=1073741824?(n/1073741824).toFixed(2)+' GB':n>=1048576?(n/1048576).toFixed(1)+' MB':(n/1024).toFixed(1)+' KB';}
let clientPolling=false;
async function pollClientProgress(){if(!clientPolling)return;try{const p=await invoke('client_distribution_progress');if(!clientPolling)return;$('pause-client-download').hidden=p.stage==='installing';const bar=$('client-download-progress');bar.hidden=false;bar.value=p.total?Math.min(100,p.received/p.total*100):0;$('client-download-state').textContent=p.message||(p.file?`${p.file} · ${byteSize(p.received)} / ${byteSize(p.total)}`:'正在处理客户端资源…');}catch{}if(clientPolling)setTimeout(pollClientProgress,600);}
$('check-client-update').addEventListener('click',()=>run('正在检查客户端资源版本…',async()=>{if(!$('directory').value.trim())throw{code:'missing_directory',message:'请先选择安装目录。'};await persist();const p=await invoke('client_distribution_check',{directory:settings.clientDirectory});$('client-download-state').textContent=(p.changedFiles||p.removedFiles)?`版本 ${p.version}，需要下载 ${p.changedFiles} 个文件${p.removedFiles?`，删除 ${p.removedFiles} 项受管旧资源`:""}，约 ${byteSize(p.downloadBytes)}${p.preservedSettings?`；保留 ${p.preservedSettings} 项本机设置`:""}。`:`客户端资源 ${p.version} 已通过清单校验。`;status((p.changedFiles||p.removedFiles)?'可点击下载 / 更新游戏。':'客户端资源已是最新。',(p.changedFiles||p.removedFiles)?'neutral':'success');}));
$('download-client').addEventListener('click',()=>run('正在安装或更新客户端…',async()=>{if(!$('directory').value.trim())throw{code:'missing_directory',message:'请先选择安装目录。'};await persist();clientPolling=true;$('pause-client-download').hidden=false;pollClientProgress();
 try{const result=await invoke('client_distribution_install',{directory:settings.clientDirectory});clientPolling=false;$('client-download-state').textContent=`版本 ${result.version} 安装完成${result.preservedSettings?`，保留 ${result.preservedSettings} 项本机设置`:""}。`;renderReport(await inspectDirectory(settings.clientDirectory));}
 catch(e){clientPolling=false;let file='';try{file=(await invoke('client_distribution_progress')).file||'';}catch{}$('client-download-state').textContent=(e?.message||'下载失败，原客户端未修改。')+(file?`（${file}）`:'');throw e;}
 finally{clientPolling=false;$('pause-client-download').hidden=true;}
}));
$('pause-client-download').addEventListener('click',async()=>{await invoke('client_distribution_cancel');$('client-download-state').textContent='正在暂停下载；已下载分段将保留用于续传。';});
$('manual-client-download').addEventListener('click',()=>run('正在打开手动下载页面…',()=>invoke('open_client_share')));

let recoveringUid=false,recoveryEpoch=0;
function clearRecovery(){recoveryEpoch++;$('recover-email').value='';$('recover-password').value='';}
$('recover-uid-open').onclick=()=>{clearRecovery();$('recover-uid-state').textContent='仅找回 UID，不登录、不启动游戏。';$('recover-uid-dialog').showModal();};
$('recover-uid-close').onclick=()=>{$('recover-uid-dialog').close();clearRecovery();};
$('recover-uid-dialog').addEventListener('close',clearRecovery);
$('recover-uid-dialog').addEventListener('cancel',clearRecovery);
$('recover-uid-form').onsubmit=async e=>{e.preventDefault();if(recoveringUid)return;const email=$('recover-email').value.trim();const password=$('recover-password').value;const epoch=recoveryEpoch;$('recover-password').value='';if(!/^[^\s@]+@[^\s@.][^\s@]*\.[^\s@.]+$/.test(email)||!password){$('recover-uid-state').textContent='请输入有效邮箱和账号密码。';return;}recoveringUid=true;$('recover-uid-submit').disabled=true;$('recover-uid-state').textContent='正在验证并查询 UID…';try{const uid=await invoke('recover_uid',{email,password});if(!/^[0-9]{8}$/.test(uid)||uid==='00000000')throw{code:'recovery_response',message:'找回 UID 响应无效。'};if(!$('recover-uid-dialog').open||epoch!==recoveryEpoch)return;$('identifier').value=uid;$('recover-uid-dialog').close();clearRecovery();status('已填入 UID，请输入密码登录。','success');}catch(error){if($('recover-uid-dialog').open&&epoch===recoveryEpoch)$('recover-uid-state').textContent=errorMessage(error);}finally{recoveringUid=false;$('recover-uid-submit').disabled=false;if(epoch===recoveryEpoch)$('recover-password').value='';}};
