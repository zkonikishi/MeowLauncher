process.env.TEMP='C:/dev-toolchain/tmp';process.env.TMP=process.env.TEMP;
// Run with NODE_PATH pointing to an installed playwright package.
// Uses an in-memory IPC fixture only: never authenticates or starts a game.
const { chromium }=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../apps/launcher/ui'),out=path.resolve(__dirname,'../docs/qa');
fs.mkdirSync(out,{recursive:true});
const types={'.html':'text/html','.js':'text/javascript','.mjs':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.ogg':'audio/ogg','.png':'image/png'};
const server=http.createServer((req,res)=>{const file=path.join(root,req.url==='/'?'index.html':req.url.split('?')[0]);if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}fs.readFile(file,(err,data)=>{if(err){res.writeHead(404).end();return;}res.setHeader('Content-Type',types[path.extname(file)]||'text/plain');res.end(data);});});
(async()=>{
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const browser=await chromium.launch({headless:true,channel:'msedge'});
  const page=await browser.newPage({viewport:{width:1280,height:728},deviceScaleFactor:1});
  const failures=[];page.on('pageerror',e=>failures.push(e.message));
  await page.addInitScript(()=>{
    window.testCalls=[];window.loginKinds=[];window.testMode='normal';
    window.testSettings={version:1,theme:location.search.includes('retro')?'retro':'modern',clientDirectory:'',identifier:'',rememberIdentifier:false};
    window.__TAURI__={core:{invoke:async(command,args)=>{
      // Record only command names and non-secret settings.
      window.testCalls.push(command);
      if(command==='load_content'){if(window.testMode==='content-error')throw{code:'content_unavailable',message:'官网暂不可用'};return{schemaVersion:1,updatedAt:new Date().toISOString(),items:[{id:'test',category:'notice',title:'官网统一公告',summary:'仅供界面测试',publishedAt:new Date().toISOString(),url:'https://launcher.invalid/news/?article=test',realms:['s4a21']}],maintenance:{status:'unknown',updatedAt:new Date().toISOString(),realms:['s4a21']}};}
      if(command==='client_distribution_check'){if(window.testMode==='client-source-offline')throw{code:'client_source_unavailable',message:'客户端下载清单尚未发布'};return{version:'1.0.0',changedFiles:2,downloadBytes:2048};}
      if(command==='client_distribution_install'){if(window.testMode==='client-hold'){window.testCancelled=false;while(!window.testCancelled)await new Promise(r=>setTimeout(r,25));throw{code:'client_update_cancelled',message:'已暂停下载，保留分段用于续传'};}return{version:'1.0.0'};}
      if(command==='client_distribution_progress')return{stage:'downloading',file:'Script.pvf',received:1024,total:2048};
      if(command==='client_distribution_cancel'){window.testCancelled=true;return;}
      if(command==='recover_uid'){if(window.testMode==='recover-auth')throw{code:'auth_failed',message:'账号或密码错误'};if(window.testMode==='recover-rate')throw{code:'rate_limited',message:'请求过于频繁'};if(window.testMode==='recover-hold')await new Promise(r=>setTimeout(r,300));return '00000001';}
      if(command==='request_code')return{retryAfter:1};
      if(command==='complete_registration')return{uid:'12345678',provisioningPending:true};
      if(command==='complete_password_reset')return null;
      if(command==='load_credential')return location.search.includes('saved')?{identifier:'10000001',hasPassword:true}:null;
      if(command==='clear_credential'){if(window.testMode==='clear-fail')throw{code:'credential_clear',message:'清除已保存密码失败'};return null;}
      if(command==='save_credential'){if(window.testMode==='save-fail')throw{code:'credential_save',message:'密码未能保存到本机'};return null;}
      if(command==='open_client_share')return null;
      if(command==='check_update'){if(window.testMode==='update-error')throw{message:'官网更新清单尚不可用'};return{available:window.testMode==='update-new',version:'0.5.0'};}
      if(command==='download_update')return{version:'0.5.1',program:'MeowLauncher',verified:true};
      if(command==='open_update')return null;
      if(command==='bootstrap')return {settings:window.testSettings,version:'0.1.0',edition:location.search.includes('ordinary')?'sponsor':'meowopia-custom',nativeTargetReady:false,accountMode:'platform-login-id-v1',publicConfig:location.search.includes('configured')?{brand:'测试发行服',accountBaseUrl:'https://accounts.example.com',realmId:'hk',realmName:'发行测试区'}:null};
      if(command==='game_window_preferences')return{size:null};
      if(command==='save_game_window_preferences'){window.savedGameWindow=args.preference;return args.preference;}
      if(command==='save_settings'){window.testSettings=args.settings;return args.settings;}
      if(command==='choose_directory')return 'D:\\示例客户端';
      if(command==='inspect_client')return {ready:!args.directory.includes('missing'),note:'测试检查结果；未连接真实客户端。',items:[{name:'DNF.exe',ok:!args.directory.includes('missing'),detail:'fixture'}]};
      if(command==='check_service'){await new Promise(r=>setTimeout(r,120));if(window.testMode==='timeout')throw{code:'timeout',message:'连接超时，请检查网络后重试。'};return{message:'账号服务可连接；登录能力需实际登录验证'};}
      if(command==='refresh_native_target'){if(window.testMode==='target-fail')throw{code:'target_unavailable',message:'目录暂不可用'};await new Promise(resolve=>{window.releaseTarget=resolve;});return true;}
      if(command==='platform_login'){window.loginKinds.push(args.identifierType);if(window.testMode==='ambiguous')throw{code:'ambiguous_identifier',message:'该标识对应多个账号，请明确选择 ID 或 UID 登录。'};return{uid:'00123456',nickname:'公开昵称',gameReady:['delayed-target','target-fail'].includes(window.testMode)};}
      if(command==='platform_register')return{uid:'00123456',provisioningPending:true};
      if(command==='login_account'||command==='start_authenticated'||command==='launch_game'){await new Promise(r=>setTimeout(r,150));if(window.testMode==='auth')throw{code:'auth_failed',message:'账号或密码错误，或账号暂不可用。'};return{message:'已创建游戏进程；连接成功与进入选角请在游戏内确认。'};}
    }}};
  });
  try{
    await page.goto(`http://127.0.0.1:${server.address().port}/`);await page.waitForFunction(()=>document.querySelector('#platform-login-method'));
    assert.equal(await page.locator('#remember-password').isVisible(),false);assert.equal(await page.locator('#platform-login-method option').count(),4);
    await page.click('#client-shortcut');await page.click('[data-settings-tab=general]');await page.selectOption('#game-window-size','1024x768');await page.click('#save-game-window');assert.deepEqual(await page.evaluate(()=>window.savedGameWindow),{size:{width:1024,height:768}});assert.match(await page.textContent('#game-window-state'),/下次启动/);await page.screenshot({path:path.join(out,'max-game-window-settings.png')});await page.click('#close-settings');
    await page.click('#register-account');assert.match(await page.textContent('.identity-registration-notice'),/不能自行修改/);
    await page.fill('#account-login-id','bad_id');await page.fill('#account-email','fixture@example.invalid');await page.fill('#account-code','123456');await page.fill('#account-password','fixture-only');await page.fill('#account-confirm','fixture-only');await page.click('#account-submit');assert.match(await page.textContent('#account-state'),/4–32/);assert.equal(await page.evaluate(()=>window.testCalls.includes('platform_register')),false);
    await page.fill('#account-login-id','Fixture123');await page.fill('#account-nickname','公开昵称');await page.click('#account-send');await page.waitForFunction(()=>document.querySelector('#account-state').textContent.includes('已受理'));await page.click('#account-submit');await page.waitForFunction(()=>!document.querySelector('#account-dialog').open);assert.equal(await page.inputValue('#identifier'),'Fixture123');assert.equal(await page.inputValue('#platform-login-method'),'id');assert.equal(await page.inputValue('#account-password'),'');
    await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>!document.querySelector('#home-view').hidden);assert.equal(await page.textContent('#account-menu-toggle'),'公开昵称');assert.equal(await page.textContent('#account-menu-uid'),'UID 00123456');assert.equal(await page.locator('#show-login').isDisabled(),true);assert.equal(await page.textContent('#start-label'),'账号准备中');
    await page.click('#account-menu-toggle');await page.click('#logout-account');await page.waitForFunction(()=>!document.querySelector('#login-view').hidden);await page.evaluate(()=>window.testMode='ambiguous');await page.selectOption('#platform-login-method','auto');await page.fill('#identifier','10000001');await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('对应多个账号'));assert.equal(await page.locator('#home-view').isVisible(),false);assert.deepEqual(await page.evaluate(()=>window.loginKinds),['id','auto']);
    await page.evaluate(()=>window.testMode='delayed-target');await page.fill('#identifier','Fixture123');await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>!document.querySelector('#home-view').hidden&&window.releaseTarget);assert.equal(await page.locator('#show-login').isDisabled(),true);assert.equal(await page.textContent('#start-label'),'正在连接…');await page.screenshot({path:path.join(out,'max-platform-pending-target.png')});await page.click('#account-menu-toggle');await page.click('#logout-account');await page.waitForFunction(()=>!document.querySelector('#login-view').hidden);await page.evaluate(()=>window.releaseTarget());await page.waitForTimeout(40);assert.equal(await page.locator('#show-login').isDisabled(),true);assert.equal(await page.locator('#home-view').isVisible(),false);
    assert.equal(await page.locator('.account-links>span').count(),0);assert.equal(await page.evaluate(()=>getComputedStyle(document.querySelector('#forgot-password'),'::before').content),'"·"');
    await page.evaluate(()=>window.testMode='target-fail');await page.fill('#identifier','Fixture123');await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('确认重试'));const loginsBeforeRetry=await page.evaluate(()=>window.loginKinds.length);await page.evaluate(()=>window.testMode='retry-target');await page.click('#select-server');await page.click('#server-confirm');await page.waitForFunction(()=>window.releaseTarget&&document.querySelector('#status').textContent.includes('重新连接'));await page.evaluate(()=>window.releaseTarget());await page.waitForFunction(()=>!document.querySelector('#show-login').disabled);assert.equal(await page.evaluate(()=>window.loginKinds.length),loginsBeforeRetry);
    assert.equal(await page.evaluate(()=>window.testCalls.some(c=>['login_account','launch_game','start_authenticated','load_credential'].includes(c))),false);assert.deepEqual(failures,[]);
    await page.screenshot({path:path.join(out,'max-platform-login.png')});fs.writeFileSync(path.join(out,'max-platform-ui.json'),JSON.stringify({passed:true,scope:'Mocked native IPC only; no production identity creation or game',checks:['typed login ID/UID/email','nickname display only','immutable identity registration notice','login ID required','registration returns to ID login without session','UID leading zeroes preserved','game readiness pending blocks start','ambiguous selector stays outside lobby','no legacy auth/credential fallback','lobby visible while target pending; Start disabled','logout invalidates late target result','two visible links have one separator','window size preference saved for next launch only','target retry requires no re-login and is manually initiated'],noGame:true},null,2));console.log('Max platform GUI checks passed; mocked IPC, no account/mail/game.');
  } finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
})().catch(e=>{console.error(e);process.exitCode=1;});
