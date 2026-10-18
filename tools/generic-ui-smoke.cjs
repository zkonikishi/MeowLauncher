process.env.TEMP='C:/dev-toolchain/tmp';process.env.TMP=process.env.TEMP;
// Browser + native IPC mocks only; never authenticates to any real server.
const {chromium}=require('playwright');
const http=require('node:http'),fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../apps/launcher/ui');
const types={'.html':'text/html','.mjs':'text/javascript','.js':'text/javascript','.css':'text/css','.svg':'image/svg+xml','.png':'image/png','.ogg':'audio/ogg'};
const server=http.createServer((req,res)=>{const pathname=req.url.split('?')[0];const file=path.resolve(root,'.'+(pathname==='/'?'/index.html':pathname));if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}fs.readFile(file,(err,data)=>{if(err){res.writeHead(404).end();return;}res.setHeader('Content-Type',types[path.extname(file)]||'text/plain');res.end(data);});});
(async()=>{
 let browser;
 try{
  await new Promise(r=>server.listen(0,'127.0.0.1',r));browser=await chromium.launch({headless:true,channel:'msedge'});const page=await browser.newPage({viewport:{width:1280,height:728}});const failures=[];page.on('pageerror',e=>failures.push(e.message));
  await page.addInitScript(()=>{
   window.calls=[];window.stopIds=[];window.failLogin=false;window.hostMode='failure';window.hostInstance='instance-fixture-A';window.events={};
   window.__TAURI__={event:{listen:async(name,callback)=>{window.events[name]=callback;return()=>{};}},core:{invoke:async(name,args)=>{
    window.calls.push(name);
    if(name==='bootstrap')return{edition:'sponsor',accountMode:'simple',version:'0.7.1',genericProfile:location.search.includes('review')?{brand:'Fixture',target:{host:'127.0.0.1',port:443,serverName:'localhost',authorityId:'fixture',realmId:'realm',saveProfileId:'save'},localReview:{hostExecutable:'D:/fixture/host.exe',playerTargetFile:'D:/fixture/player-target.json',readyFile:'D:/fixture/ready.json',allowUnsignedCandidate:true}}:null,settings:{version:1,theme:'modern',clientDirectory:'',rememberIdentifier:false,identifier:'',closeToTray:true,restoreAfterGame:true}};
    if(name==='current_instance')return null;
    if(name==='save_settings')return args.settings;
    if(name==='login_account'){if(window.failLogin)throw{code:'simple_rejected',message:'注册或登录未完成，请检查服务器与账号信息。'};return{nativeAccountId:'9223372036854775807',username:args.identifier};}
    if(name==='start_authenticated'){if(window.hostMode==='failure')throw{code:'client_host_ready',message:'未收到严格宿主就绪状态。'};return{instanceId:window.hostInstance,hostPid:123,processId:345,status:'running',message:'模拟原生就绪，仅作UI测试。'};}
    if(name==='stop_instance'){window.stopIds.push(args.instanceId);window.events['game-exited']?.({payload:{instanceId:args.instanceId,cleanupFailed:false}});return null;}
    if(['register_generic','logout_account','save_generic_profile','window_action'].includes(name))return null;
    throw{code:'unexpected',message:'未实现的接口'};
   }}};
  });
  await page.goto(`http://127.0.0.1:${server.address().port}`);await page.waitForFunction(()=>document.querySelector('#generic-register'));
  await page.click('#client-shortcut');await page.click('[data-settings-tab=download]');await page.screenshot({path:path.resolve(__dirname,'../docs/qa/generic-settings-clean-r4.png')});await page.click('[data-settings-tab=about]');assert.match(await page.textContent('#author-credit'),/zkonikishi/);await page.screenshot({path:path.resolve(__dirname,'../docs/qa/generic-about-author-r4.png')});await page.click('#close-settings');
  assert.equal(await page.getAttribute('#identifier','maxlength'),'32');assert.equal(await page.locator('#recover-uid-open').isVisible(),false);assert.equal(await page.locator('#account-email').count(),0);
  await page.click('#register-account');await page.fill('#generic-register-user','Player_A');await page.fill('#generic-register-password','example-only');await page.fill('#generic-register-confirm','example-only');await page.click('#generic-register-form button[type=submit]');await page.waitForFunction(()=>!document.querySelector('#generic-register').open);assert.equal(await page.inputValue('#identifier'),'Player_A');assert.equal(await page.locator('#home-view').isVisible(),false);assert.equal(await page.inputValue('#generic-register-password'),'');
  await page.fill('#password','example-only');await page.click('#launch');await page.waitForFunction(()=>!document.querySelector('#home-view').hidden);assert.equal(await page.textContent('#account-menu-toggle'),'Player_A');assert.equal(await page.inputValue('#password'),'');
  await page.click('#show-login');assert.match(await page.textContent('#status'),/尚未配置/);await page.click('#account-menu-toggle');await page.click('#logout-account');await page.waitForFunction(()=>!document.querySelector('#login-view').hidden);
  await page.evaluate(()=>window.failLogin=true);await page.fill('#password','wrong-example');await page.click('#launch');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('未完成'));assert.equal(await page.locator('#home-view').isVisible(),false);
  await page.click('#client-shortcut');await page.click('[data-settings-tab=download]');if(await page.locator('#generic-advanced').count())await page.click('#generic-advanced>summary');for(const [id,value] of Object.entries({brand:'Fixture server',host:'server.example.invalid',port:'443',serverName:'server.example.invalid',authorityId:'fixture',realmId:'realm',saveProfileId:'save'}))await page.fill('#generic-'+id,value);await page.click('#generic-save');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('已保存'));assert.equal(await page.locator('#home-view').isVisible(),false);
  assert.deepEqual(failures,[]);const calls=await page.evaluate(()=>window.calls);assert(!calls.some(n=>['request_code','recover_uid','complete_registration','load_content','launch_game','start_authenticated'].includes(n)));
  await page.click('#close-settings');await page.screenshot({path:path.resolve(__dirname,'../docs/qa/generic-login.png')});
  await page.goto(`http://127.0.0.1:${server.address().port}/?review`);await page.waitForFunction(()=>document.querySelector('#generic-register'));
  await page.fill('#identifier','Player_A');await page.fill('#password','example-only');await page.click('#launch');await page.waitForFunction(()=>!document.querySelector('#home-view').hidden);
  await page.click('#show-login');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('未收到严格'));assert.equal(await page.locator('#generic-stop-instance').isEnabled(),false);
  await page.evaluate(()=>window.hostMode='ready');await page.click('#show-login');await page.waitForFunction(()=>document.querySelector('#generic-instance-state').textContent.includes('PID 345'));
  await page.evaluate(()=>window.events['game-exited']({payload:{instanceId:'instance-other-B',cleanupFailed:false}}));assert.match(await page.textContent('#generic-instance-state'),/PID 345/);
  await page.click('#client-shortcut');assert.equal(await page.locator('#generic-new-instance').isEnabled(),false);await page.click('#generic-stop-instance');await page.waitForFunction(()=>document.querySelector('#generic-instance-state').textContent==='尚无游戏实例');assert.deepEqual(await page.evaluate(()=>window.stopIds),['instance-fixture-A']);
  await page.click('#close-settings');await page.evaluate(()=>window.hostInstance='instance-fixture-C');await page.click('#show-login');await page.waitForFunction(()=>document.querySelector('#generic-instance-state').textContent.includes('PID 345'));await page.evaluate(()=>window.events['game-exited']({payload:{instanceId:'instance-fixture-C',cleanupFailed:true}}));await page.click('#client-shortcut');assert.match(await page.textContent('#generic-instance-state'),/清理状态需检查/);assert.equal(await page.locator('#generic-stop-instance').isEnabled(),false);assert.deepEqual(await page.evaluate(()=>window.stopIds),['instance-fixture-A']);
  assert.deepEqual(failures,[]);
  const report={passed:true,scope:'Browser UI with mocked native IPC; no real TLS/server/game acceptance',checks:['ordinary entry isolation','username not UID','no platform email forms','registration returns to login','password cleared','native int64 remains string','login gates lobby','logout clears UI','failed login stays outside lobby','server config exits account','no platform/native game fallback','failed strict readiness never shows running','verified native response shows own PID','other instance exit cannot clear current row','stop sends exact owned instance ID','multi-open remains disabled','failed cleanup retains quarantined instance without a false exited state'],failures};fs.writeFileSync(path.resolve(__dirname,'../docs/qa/generic-ui-smoke.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{if(browser)await browser.close();await new Promise(r=>server.close(r));}
})().catch(e=>{console.error(e);process.exitCode=1;});
