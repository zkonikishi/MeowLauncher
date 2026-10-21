import {markup} from './modern.mjs';
import {validateSimpleCredentials,nativeAccountId} from './auth-modes.mjs';
import {errorMessage} from './model.mjs';
import {describeAdapter} from './admission-state.mjs';
import {reviewConfig} from './generic-settings.mjs';
import {mountAccountExtensions} from './cloud-auth-ui.mjs';
const $=id=>document.getElementById(id);
const invoke=(name,args)=>window.__TAURI__.core.invoke(name,args);
const info=await invoke('bootstrap');
if(info.edition!=='sponsor'||info.accountMode!=='simple')throw Error('Wrong edition entry');
$('edition').innerHTML=markup;
document.documentElement.dataset.edition='modern';
document.documentElement.dataset.accountMode='simple';
let settings=info.settings,busy=false,authenticated=false;
let localReview=info.genericProfile?.localReview||null,currentRun=null;
const ended=new Map();
const {mountMusic}=await import('./music.mjs');const music=mountMusic();
const {mountScene}=await import('./scene.mjs');mountScene(document.querySelector('.modern-stage'));
function status(message,kind='neutral'){$('status').textContent=message;$('status-dot').dataset.kind=kind;}
function screen(home){document.querySelector('.modern-stage').dataset.screen=home?'home':'login';$('login-view').hidden=home;$('home-view').hidden=!home;$('account-menu-toggle').hidden=!home;$('account-menu').hidden=true;}
function clearSessionUI(){authenticated=false;screen(false);$('password').value='';$('account-menu-toggle').textContent='账号';$('account-menu-uid').textContent='';}
async function run(fn){if(busy)return;busy=true;$('login-fields').disabled=true;$('generic-register-fields').disabled=true;$('generic-save').disabled=true;try{await fn();}catch(e){status(errorMessage(e),'error');}finally{busy=false;$('login-fields').disabled=false;$('generic-register-fields').disabled=false;$('generic-save').disabled=false;}}
function element(tag,text){const el=document.createElement(tag);el.textContent=text;return el;}
// No UID/email input or platform registration is mounted in this edition.
$('identifier').maxLength=32;$('identifier').placeholder='请输入用户名';$('identifier').setAttribute('aria-label','用户名');
$('identifier').value=settings.identifier||'';$('directory').value=settings.clientDirectory||'';
$('remember').checked=settings.rememberIdentifier;
$('remember-password').closest('label').hidden=true;
$('recover-uid-open').hidden=true;$('forgot-password').hidden=true;
for(const separator of document.querySelectorAll('.account-links>span'))separator.hidden=true;
$('account-dialog').remove();
$('edition-usage-notice').hidden=false;$('edition-usage-details').hidden=false;
$('release-notes').textContent='MeowLauncher 通用版';$('version').textContent=`MeowLauncher ${info.version} · 通用版`;
$('content-state').textContent='此服务器尚未配置资讯来源。';$('content-retry').hidden=true;
$('maintenance-state').textContent='服务器状态未检测';
document.querySelector('.town-feature small').textContent='115US';
for(const button of document.querySelectorAll('#news-tabs button'))button.onclick=()=>{for(const b of document.querySelectorAll('#news-tabs button'))b.classList.toggle('selected',b===button);};
$('select-server').textContent=info.genericProfile?.brand||'配置服务器';
$('select-server').onclick=()=>{$('settings-dialog').showModal();tab('download');};
$('directory-summary').textContent=settings.clientDirectory||'尚未选择客户端目录';
// Installation/game readiness is never inferred from a successful account login.
for(const id of ['check-summary','check-note','check-items','service-status'])$(id).hidden=true;
document.querySelector('[data-settings-panel="storage"] h3').textContent='游戏客户端';
$('show-login').onclick=()=>run(async()=>{
 if(!authenticated){screen(false);return;}
 if(!localReview){status('本地客户端候选验证尚未配置；登录会话不代表已进入游戏。','error');return;}
 await saveSettings();status('正在核验文件、宿主监听与严格隔离就绪状态…','busy');
 const started=await invoke('start_authenticated',{directory:settings.clientDirectory});
 if(!started||typeof started.instanceId!=='string'||!Number.isSafeInteger(started.processId)||started.processId<=0)throw{code:'client_host_ready',message:'未收到可核验的客户端就绪结果。'};
 if(ended.has(started.instanceId)){if(ended.get(started.instanceId)){currentRun={...started,status:'cleanup_failed'};renderInstance();status('实例清理状态需检查，当前候选不复用其资源。','error');}else status('客户端进程已退出。');return;}
 currentRun=started;renderInstance();music.stopForGame();status(started.message,'success');
});
for(const id of ['inspect','probe','manual-client-download','check-client-update','download-client','check-update','clear-credential','diagnostics'])$(id).hidden=true;
document.querySelector('.vault-settings').hidden=true;
const targetPanel=document.querySelector('[data-settings-panel="download"]');targetPanel.classList.add('generic-connection');targetPanel.replaceChildren(element('h3','服务器连接'),element('p','填写服主提供的连接信息。保存后需重新登录。'));
function disclosure(title,id){const details=document.createElement('details');details.className='settings-disclosure';details.id=id;details.append(element('summary',title));const body=document.createElement('div');body.className='disclosure-body';details.append(body);return{details,body};}
function fieldGrid(){const grid=document.createElement('div');grid.className='settings-field-grid';return grid;}
function labeledInput(key,label,value,prefix,grid){const row=element('label','');row.className='settings-field';const caption=element('span',label),input=document.createElement('input');input.id=`${prefix}-${key}`;input.autocomplete='off';input.spellcheck=false;input.value=value;input.maxLength=key==='caCertificate'||prefix==='review'?4096:256;row.append(caption,input);grid.append(row);return input;}
const basic=fieldGrid(),advanced=disclosure('高级连接设置','generic-advanced'),advancedGrid=fieldGrid();
const fields=[['brand','服务器名称'],['host','服务器地址'],['port','TLS 端口'],['serverName','证书名称'],['authorityId','认证标识（Authority）'],['realmId','区服标识（Realm）'],['saveProfileId','存档标识（Profile）'],['caCertificate','信任证书路径（DER，可选）']];
const inputs={};for(const [key,label] of fields){const value=key==='brand'?(info.genericProfile?.brand||''):(info.effectiveTarget?.[key]??info.genericProfile?.target?.[key]??'');inputs[key]=labeledInput(key,label,value,'generic',['brand','host','port'].includes(key)?basic:advancedGrid);}
inputs.host.placeholder='域名或 IP 地址';inputs.port.placeholder='服主提供的端口';inputs.port.inputMode='numeric';inputs.caCertificate.placeholder='选择服主提供的可信证书文件';
advanced.body.append(element('p','以下信息由服主提供，请保持与服务端一致。'),advancedGrid);
targetPanel.append(basic,advanced.details);
const admissionNote=element('p','正在读取原生准入能力…');admissionNote.id='generic-admission-capability';targetPanel.append(admissionNote);
try{admissionNote.textContent=describeAdapter(await invoke('admission_capabilities'));}catch{admissionNote.textContent='无法确认新版准入能力；保留现有网关认证，不启用未知适配器。';}
const review=disclosure('原生游戏连接（高级）','generic-review-options');review.body.append(element('p','选择服主网关的远端频道配置，或本机同一轮就绪文件。可使用内置固定摘要宿主；退出启动器会关闭本次候选实例。'));
const reviewGrid=fieldGrid(),reviewInputs={};for(const [key,label] of [['hostExecutable','客户端宿主路径'],['playerTargetFile','频道配置路径（player-target.json）'],['readyFile','就绪文件路径（ready.json）']]){reviewInputs[key]=labeledInput(key,label,localReview?.[key]||'','review',reviewGrid);}
const modeSelect=document.createElement('select');modeSelect.id='review-mode';for(const [value,label] of [['remote','远端服主网关'],['local','本地宿主联调']]){const option=element('option',label);option.value=value;modeSelect.append(option);}modeSelect.value=localReview?.mode||'remote';const modeRow=element('label','连接模式');modeRow.append(modeSelect);review.body.append(modeRow);
const bundled=element('input','');bundled.type='checkbox';bundled.id='review-bundled';bundled.checked=localReview?.useBundledHost??!localReview?.hostExecutable;const bundledRow=element('label','使用启动器内置的固定摘要宿主');bundledRow.prepend(bundled);review.body.append(bundledRow);
review.body.append(reviewGrid);
const consent=element('label','允许运行未签名的本机测试宿主');consent.className='settings-check';const allow=document.createElement('input');allow.type='checkbox';allow.id='review-allow';allow.checked=localReview?.allowUnsignedCandidate===true;consent.prepend(allow);review.body.append(consent);targetPanel.append(review.details);
function reflectNativeOptions(){reviewInputs.hostExecutable.disabled=bundled.checked;reviewInputs.readyFile.closest('label').hidden=modeSelect.value==='remote';consent.hidden=bundled.checked;}bundled.onchange=reflectNativeOptions;modeSelect.onchange=reflectNativeOptions;reflectNativeOptions();
const save=element('button','保存连接');save.id='generic-save';save.type='button';save.className='settings-primary';const actions=document.createElement('div');actions.className='settings-actions';actions.append(save);targetPanel.append(actions);
const aboutPanel=document.querySelector('[data-settings-panel="about"]'),usage=$('edition-usage-details'),version=$('version');
const brandCard=document.createElement('section');brandCard.className='settings-brand-card';brandCard.append(element('h3','MeowLauncher'),element('p',`通用版 · ${info.version}`));const author=element('p','作者：zkonikishi');author.id='author-credit';brandCard.append(author);
aboutPanel.replaceChildren(brandCard,usage);const aboutFooter=document.createElement('footer');aboutFooter.append(version);aboutPanel.append(aboutFooter);
const authorMark=element('small','by zkonikishi');authorMark.className='launcher-author';document.querySelector('.modern-stage').append(authorMark);
document.querySelector('[data-settings-tab="download"]').textContent='服务器连接';
function tab(name){for(const panel of document.querySelectorAll('[data-settings-panel]'))panel.hidden=panel.dataset.settingsPanel!==name;for(const b of document.querySelectorAll('[data-settings-tab]'))b.classList.toggle('selected',b.dataset.settingsTab===name);}
for(const b of document.querySelectorAll('[data-settings-tab]'))b.onclick=()=>tab(b.dataset.settingsTab);
const settingsDialog=$('settings-dialog');$('client-shortcut').onclick=()=>settingsDialog.showModal();$('close-settings').onclick=()=>settingsDialog.close();$('home-directory').onclick=()=>{settingsDialog.showModal();tab('storage');};
save.onclick=()=>run(async()=>{const port=Number(inputs.port.value);if(!Number.isInteger(port)||port<1||port>65535){status('请输入有效 TLS 端口。','error');return;}const target=Object.fromEntries(fields.filter(([k])=>!['brand','port'].includes(k)).map(([k])=>[k,inputs[k].value]));target.port=port;target.caCertificate=target.caCertificate||null;const review=Object.values(reviewInputs).some(i=>i.value)||allow.checked?reviewConfig({...Object.fromEntries(Object.entries(reviewInputs).map(([k,i])=>[k,i.value])),mode:modeSelect.value,useBundledHost:bundled.checked,allowUnsignedCandidate:allow.checked}):null;await invoke('save_generic_profile',{profile:{schemaVersion:1,brand:inputs.brand.value,target,localReview:review}});localReview=review;clearSessionUI();$('select-server').textContent=inputs.brand.value;status('服务器配置已保存。登录时将校验证书和目标身份。','success');});
async function saveSettings(){settings={...settings,identifier:$('remember').checked?$('identifier').value:'',rememberIdentifier:$('remember').checked,clientDirectory:$('directory').value,closeToTray:$('close-behavior').value==='tray',restoreAfterGame:$('restore-after-game').checked};settings=await invoke('save_settings',{settings});$('directory-summary').textContent=settings.clientDirectory||'尚未选择客户端目录';}
$('close-behavior').value=settings.closeToTray===false?'exit':'tray';$('restore-after-game').checked=settings.restoreAfterGame!==false;
for(const id of ['save','save-preferences'])$(id).onclick=()=>run(async()=>{await saveSettings();status('设置已保存。','success');});
$('choose').onclick=()=>run(async()=>{const directory=await invoke('choose_directory');if(directory){$('directory').value=directory;await saveSettings();}});
$('login-form').onsubmit=e=>{e.preventDefault();void run(async()=>{const identifier=$('identifier').value;const password=$('password').value;$('password').value='';const invalid=validateSimpleCredentials(identifier,password);if(invalid){status(invalid,'error');return;}clearSessionUI();status('正在验证服务器身份并登录…','busy');const identity=await invoke('login_account',{identifier,password});nativeAccountId(identity.nativeAccountId);if(identity.username!==identifier)throw{code:'simple_protocol',message:'登录响应账号不匹配。'};authenticated=true;screen(true);$('account-menu-toggle').textContent=identity.username;$('account-menu-uid').textContent=identity.username;try{await saveSettings();status('登录成功。','success');}catch{status('登录成功，但本机设置未能保存。','error');}});};
$('toggle-password').onclick=()=>{const show=$('password').type==='password';$('password').type=show?'text':'password';$('toggle-password').textContent=show?'隐藏':'显示';};
$('account-menu-toggle').onclick=()=>$('account-menu').hidden=!$('account-menu').hidden;
$('account-menu-settings').onclick=()=>{settingsDialog.showModal();$('account-menu').hidden=true;};
$('logout-account').onclick=()=>run(async()=>{const result=await invoke('logout_account');clearSessionUI();status(result?.revocationConfirmed===false?'本机已退出，云端撤销尚未确认。':'已退出账号。');});
// Registration is native UI -> native TLS transport, never platform web forms.
const registration=document.createElement('dialog');registration.id='generic-register';registration.setAttribute('aria-label','注册通用服务器账号');
registration.innerHTML='<header><h2>注册账号</h2><button id="generic-register-close" type="button">关闭</button></header><form id="generic-register-form"><fieldset id="generic-register-fields"><label>用户名<input id="generic-register-user" autocomplete="username" maxlength="32"></label><label>密码<input id="generic-register-password" type="password" autocomplete="new-password" maxlength="72"></label><label>确认密码<input id="generic-register-confirm" type="password" autocomplete="new-password" maxlength="72"></label><button type="submit">注册</button></fieldset></form><p id="generic-register-state" role="status">注册目标服账号，不需要邮箱或平台 UID。</p>';
document.body.append(registration);
function clearRegistration(){$('generic-register-password').value='';$('generic-register-confirm').value='';}
$('register-account').onclick=()=>{if(!busy){clearRegistration();$('generic-register-state').textContent='注册目标服账号，不需要邮箱或平台 UID。';registration.showModal();}};
$('generic-register-close').onclick=()=>registration.close();registration.addEventListener('close',clearRegistration);
$('generic-register-form').onsubmit=e=>{e.preventDefault();void run(async()=>{const username=$('generic-register-user').value;const password=$('generic-register-password').value;const confirm=$('generic-register-confirm').value;clearRegistration();const invalid=validateSimpleCredentials(username,password);if(invalid||password!==confirm){$('generic-register-state').textContent=invalid||'两次密码不一致。';return;}try{await invoke('register_generic',{username,password});registration.close();$('identifier').value=username;status('注册成功，请输入密码登录。','success');}catch(e){$('generic-register-state').textContent=errorMessage(e);}});};
for(const [id,action] of [['minimize','minimize'],['close-window','close']])$(id).onclick=()=>invoke('window_action',{action}).catch(e=>status(errorMessage(e),'error'));
$('website').hidden=true;
$('toolbox-toggle').onclick=()=>{$('toolbox-panel').hidden=!$('toolbox-panel').hidden;};
for(const id of ['toolbox-update','toolbox-diagnostics','toolbox-website'])$(id).hidden=true;
$('toolbox-client').onclick=()=>{$('toolbox-panel').hidden=true;settingsDialog.showModal();tab('storage');};
const instancePanel=document.createElement('section');instancePanel.append(element('h3','游戏实例'));const instanceState=element('p','尚无游戏实例');instanceState.id='generic-instance-state';instanceState.setAttribute('role','status');instancePanel.append(instanceState);
const stop=element('button','退出当前实例');stop.id='generic-stop-instance';stop.type='button';stop.disabled=true;instancePanel.append(stop);
const add=element('button','新增启动实例');add.hidden=true;add.id='generic-new-instance';add.type='button';add.disabled=true;add.title='当前客户端互斥、地址映射和多实例隔离尚未验收';instancePanel.append(add);document.querySelector('[data-settings-panel="general"]').prepend(instancePanel);
function renderInstance(){instancePanel.hidden=!currentRun;instanceState.textContent=currentRun?`${$('account-menu-toggle').textContent} · ${$('select-server').textContent} · PID ${currentRun.processId} · 角色：游戏内选择 · ${currentRun.status==='cleanup_failed'?'清理状态需检查':'运行中'}`:'尚无游戏实例';stop.disabled=!currentRun||currentRun.status==='cleanup_failed';}
stop.onclick=()=>run(async()=>{if(!currentRun)return;const id=currentRun.instanceId;try{await invoke('stop_instance',{instanceId:id});}catch(e){if(e.code==='client_cleanup_failed'&&currentRun?.instanceId===id){currentRun.status='cleanup_failed';renderInstance();}throw e;}if(currentRun?.instanceId===id)currentRun=null;renderInstance();status('本次实例已退出。');});
if(window.__TAURI__?.event?.listen)await window.__TAURI__.event.listen('game-exited',e=>{const value=e.payload;if(!value||typeof value.instanceId!=='string')return;ended.set(value.instanceId,value.cleanupFailed===true);if(ended.size>64)ended.delete(ended.keys().next().value);if(currentRun?.instanceId===value.instanceId){if(value.cleanupFailed===true){currentRun.status='cleanup_failed';status('实例清理状态需检查，当前候选不复用其资源。','error');}else{currentRun=null;status('客户端进程已退出。');}renderInstance();if(value.cleanupFailed!==true)music.resumeAfterGame();}});
try{currentRun=await invoke('current_instance');renderInstance();}catch{}
if(info.authenticated===true&&info.identity){try{nativeAccountId(info.identity.nativeAccountId);authenticated=true;$('account-menu-toggle').textContent=info.identity.username;$('account-menu-uid').textContent=info.identity.username;}catch{authenticated=false;}}
screen(authenticated);renderInstance();status(authenticated?'已恢复当前进程中的登录会话。':info.genericProfile?'准备就绪，请登录目标服务器。':'请先打开设置，填写服主提供的服务器连接。');

mountAccountExtensions({info,invoke,panel:targetPanel,onAuthenticated(identity){nativeAccountId(identity.nativeAccountId);authenticated=true;screen(true);$('account-menu-toggle').textContent=identity.username;$('account-menu-uid').textContent=identity.username;status('账号认证已完成，游戏准入仍需目标网关校验。','success');}});
