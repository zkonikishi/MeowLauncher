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
    window.testCalls=[];window.testMode='normal';
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
      if(command==='bootstrap')return {settings:window.testSettings,version:'0.1.0',edition:location.search.includes('ordinary')?'sponsor':'meowopia-custom',accountMode:location.search.includes('legacy')?'legacy-online':'platform-v1',publicConfig:location.search.includes('configured')?{brand:'测试发行服',accountBaseUrl:'https://accounts.example.com',realmId:'hk',realmName:'发行测试区'}:null};
      if(command==='save_settings'){window.testSettings=args.settings;return args.settings;}
      if(command==='choose_directory')return 'D:\\示例客户端';
      if(command==='inspect_client')return {ready:!args.directory.includes('missing'),note:'测试检查结果；未连接真实客户端。',items:[{name:'DNF.exe',ok:!args.directory.includes('missing'),detail:'fixture'}]};
      if(command==='check_service'){await new Promise(r=>setTimeout(r,120));if(window.testMode==='timeout')throw{code:'timeout',message:'连接超时，请检查网络后重试。'};return{message:'账号服务可连接；登录能力需实际登录验证'};}
      if(command==='login_account'||command==='start_authenticated'||command==='launch_game'){await new Promise(r=>setTimeout(r,150));if(window.testMode==='auth')throw{code:'auth_failed',message:'账号或密码错误，或账号暂不可用。'};return{message:'已创建游戏进程；连接成功与进入选角请在游戏内确认。'};}
    }}};
  });
  try{
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    await page.waitForFunction(()=>document.getElementById('status').textContent.includes('准备就绪'));
    await page.waitForFunction(()=>document.querySelector('.scene-still').complete);
    assert.match(await page.locator('.scene-still').getAttribute('src'),/wallpaper-apocalypse\.png$/);
    assert.equal(await page.locator('.scene-still').evaluate(e=>e.naturalWidth),3200);
    assert.equal(await page.locator('#toolbox-toggle').isVisible(),false);
    assert.equal(await page.locator('#settings-dialog #music-volume').count(),0);await page.click('#music-toggle');await page.waitForTimeout(550);assert.equal(await page.locator('.music-popover').isVisible(),true);await page.locator('#music-volume').fill('60');assert.equal(await page.locator('#launcher-bgm').evaluate(a=>a.volume),.6);assert.equal(await page.textContent('#music-percent'),'60%');await page.screenshot({path:path.join(out,'music-top-controls.png')});await page.mouse.move(500,500);
    const previous=await page.locator('.scene-still').getAttribute('src');await page.click('#motion-toggle');await page.waitForTimeout(550);assert.notEqual(await page.locator('.scene-still').getAttribute('src'),previous);const beforeDouble=await page.locator('.scene-still').getAttribute('src');
    assert.equal(await page.locator('canvas').count(),0);
    assert.equal(await page.locator('video').count(),0);
    await page.dblclick('#motion-toggle');assert.equal(await page.locator('.modern-stage').getAttribute('data-rotation'),'off');await page.waitForTimeout(550);assert.equal(await page.locator('.scene-still').getAttribute('src'),beforeDouble);
    await page.reload();await page.waitForFunction(()=>document.querySelector('.modern-stage')?.dataset.rotation);assert.equal(await page.locator('#motion-toggle').getAttribute('aria-pressed'),'false');
    await page.dblclick('#motion-toggle');assert.equal(await page.locator('.modern-stage').getAttribute('data-rotation'),'on');
    await page.click('#client-shortcut');await page.selectOption('#wallpaper-select','castle');await page.waitForFunction(()=>document.querySelector('.scene-still').naturalWidth>0&&document.querySelector('.scene-still').src.includes('castle'));
    await page.setInputFiles('#wallpaper-file',path.join(root,'assets/wallpaper-apocalypse.png'));
    await page.waitForFunction(()=>document.getElementById('wallpaper-select').value==='custom');
    assert.equal(await page.locator('#motion-toggle').getAttribute('aria-pressed'),'false');
    await page.reload();await page.waitForFunction(()=>document.querySelector('.scene-still')?.src.startsWith('blob:'));
    await page.click('#client-shortcut');await page.click('#wallpaper-reset');await page.waitForFunction(()=>document.getElementById('wallpaper-select').value==='apocalypse');await page.click('#close-settings');
    await page.screenshot({path:path.join(out,'modern.png'),fullPage:true});
    await page.click('#launch');assert.match(await page.textContent('#status'),/8 位 UID/);
    assert.equal((await page.evaluate(()=>window.testCalls)).includes('launch_game'),false);
    assert.equal(await page.locator('#show-home').count(),0); await page.click('#register-account'); assert.equal(await page.textContent('#account-title'),'注册账号');await page.click('#account-close');await page.click('#forgot-password');assert.equal(await page.textContent('#account-title'),'重设密码');await page.click('#account-close'); await page.fill('#identifier','10000002');await page.fill('#password','fixture-only');await page.click('#client-shortcut');
    assert.equal(await page.locator('#settings-dialog').isVisible(),true); await page.waitForFunction(()=>document.getElementById('launcher-bgm').currentTime>0); await page.evaluate(()=>document.getElementById('music-toggle').dispatchEvent(new MouseEvent('dblclick',{bubbles:true}))); assert.equal(await page.locator('#launcher-bgm').evaluate(a=>a.paused),true); await page.evaluate(()=>document.getElementById('music-toggle').dispatchEvent(new MouseEvent('dblclick',{bubbles:true})));await page.click('[data-settings-tab=storage]');
    await page.click('[data-settings-tab=storage]');await page.fill('#directory','D:\\missing');await page.click('#inspect');await page.waitForFunction(()=>document.getElementById('check-summary').textContent==='需要处理');
    await page.click('#choose');await page.waitForFunction(()=>document.getElementById('check-summary').textContent==='文件条件已满足');
    assert.equal(await page.evaluate(()=>window.testSettings.clientDirectory),'D:\\示例客户端');
    await page.evaluate(()=>window.testMode='timeout');await page.click('#probe');
    assert.equal(await page.isDisabled('#save'),true);
    await page.waitForFunction(()=>document.getElementById('status').textContent.includes('连接超时'));assert.equal(await page.isDisabled('#save'),false);
    await page.click('#close-settings');
    
    await page.check('#remember');await page.evaluate(()=>window.testMode='auth');await page.click('#launch');
    await page.waitForFunction(()=>document.getElementById('status').textContent.includes('账号或密码错误'));
    assert.equal(await page.inputValue('#password'),'');
    const stored=await page.evaluate(()=>window.testSettings);assert.equal(stored.identifier,'10000002');assert.equal('password' in stored,false);assert.equal('token' in stored,false);
    await page.evaluate(()=>window.testMode='normal');await page.fill('#password','fixture-only');await page.click('#launch');
    await page.waitForFunction(()=>document.getElementById('status').textContent.includes('登录成功')); assert.equal((await page.evaluate(()=>window.testCalls)).includes('start_authenticated'),false); assert.equal(await page.locator('#launcher-bgm').evaluate(a=>a.paused),false); await page.waitForFunction(()=>document.getElementById('home-directory').hidden); await page.click('#show-login'); await page.waitForFunction(()=>document.getElementById('status').textContent.includes('已创建游戏进程'));
    assert.match(await page.textContent('#status'),/游戏内确认/); assert.equal(await page.locator('#launcher-bgm').evaluate(a=>a.paused),true);
    assert.match(await page.textContent('#content-items'),/官网统一公告/);await page.click('[data-category=news]');assert.match(await page.textContent('#content-items'),/暂无资讯/);await page.click('[data-category=all]');await page.evaluate(()=>window.testMode='content-error');await page.click('#content-retry');await page.waitForFunction(()=>document.getElementById('content-state').textContent.includes('刷新失败'));assert.match(await page.textContent('#maintenance-state'),/未知/);assert.match(await page.textContent('#content-items'),/官网统一公告/);await page.evaluate(()=>window.testMode='normal');await page.click('#content-retry');await page.waitForFunction(()=>document.getElementById('content-state').textContent.includes('官网更新'));
    assert.deepEqual(await page.locator('#news-tabs button').allTextContents(),['综合','新闻','公告','活动']);await page.click('[data-category=activity]');assert.match(await page.textContent('#content-items'),/暂无资讯/);await page.click('[data-category=all]');
    assert.equal(await page.locator('#toolbox-toggle').isVisible(),true);await page.click('#toolbox-toggle');assert.equal(await page.locator('#toolbox-panel').isVisible(),true);await page.click('#toolbox-toggle');
    await page.screenshot({path:path.join(out,'modern-home.png'),fullPage:true});
    await page.click('#select-server');assert.deepEqual(await page.locator('.server-card strong').allTextContents(),['86S4A12','86S4A21','95S5A9','115US','115TW','115KR']);assert.equal(await page.locator('.server-card .server-status').count(),0);assert.deepEqual(await page.locator('.server-dot').evaluateAll(nodes=>nodes.map(n=>n.dataset.status)),Array(6).fill('closed'));assert.deepEqual(await page.locator('.server-dot').evaluateAll(nodes=>nodes.map(n=>n.getAttribute('aria-label'))),Array(6).fill('维护'));assert.equal(await page.locator('.server-card-content').first().evaluate(n=>n.getBoundingClientRect().height),48);assert.equal(await page.locator('.server-card small').count(),0);await page.click('#server-close');assert.equal(await page.locator('#server-dialog').isVisible(),false);await page.click('#select-server');await page.keyboard.press('Escape');assert.equal(await page.locator('#server-dialog').isVisible(),false);await page.click('#select-server');assert.equal(await page.locator('#server-dialog input:disabled').count(),5);await page.locator('#server-cancel').click();await page.click('#select-server');assert.equal(await page.locator('#server-dialog input:checked').inputValue(),'86s4a21');await page.screenshot({path:path.join(out,'server-selection.png')});await page.locator('#server-confirm').click();
    await page.goto('http://127.0.0.1:'+server.address().port+'/index.html?retro');
    await page.waitForFunction(()=>document.getElementById('identifier') && document.documentElement.dataset.edition==='modern');
    assert.equal(await page.locator('.game-logo').getAttribute('alt'),'Arad');
    assert.ok(await page.locator('.game-logo').evaluate(e=>e.complete&&e.naturalWidth>0));
    await page.goto('http://127.0.0.1:'+server.address().port+'/index.html?configured');
    await page.waitForSelector('.account-destination');
    assert.match(await page.textContent('.account-destination'),/accounts.example.com/);
    assert.equal(await page.locator('#directory').getAttribute('readonly'),'');
    assert.equal(await page.locator('#choose').isDisabled(),true);
    await page.screenshot({path:path.join(out,'launcher-configured.png')});
    await page.goto('http://127.0.0.1:'+server.address().port+'/index.html?legacy');
    await page.waitForFunction(()=>document.getElementById('status').textContent.includes('准备就绪'));
    assert.equal(await page.textContent('#login-title'),'账号密码登录');assert.equal(await page.textContent('#launch span'),'登录');
    const beforeRecovery=await page.evaluate(()=>window.testCalls.filter(x=>['login_account','start_authenticated','launch_game'].includes(x)).length);await page.click('#recover-uid-open');await page.fill('#recover-email','test10000001@example.invalid');await page.fill('#recover-password','fixture-only');await page.click('#recover-uid-submit');await page.waitForFunction(()=>!document.getElementById('recover-uid-dialog').open);assert.equal(await page.inputValue('#identifier'),'00000001');assert.equal(await page.inputValue('#recover-password'),'');assert.equal(await page.inputValue('#recover-email'),'');assert.equal(await page.evaluate(()=>window.testCalls.filter(x=>['login_account','start_authenticated','launch_game'].includes(x)).length),beforeRecovery);
    await page.click('#recover-uid-open');await page.evaluate(()=>window.testMode='recover-rate');await page.fill('#recover-email','test10000001@example.invalid');await page.fill('#recover-password','fixture-only');await page.click('#recover-uid-submit');await page.waitForFunction(()=>document.getElementById('recover-uid-state').textContent.includes('频繁'));assert.equal(await page.inputValue('#recover-password'),'');await page.click('#recover-uid-close');await page.evaluate(()=>window.testMode='normal');

    const authBeforeRegistration=await page.evaluate(()=>window.testCalls.filter(x=>x==='login_account').length);
    await page.click('#register-account');await page.fill('#account-email','fixture@example.invalid');await page.click('#account-send');await page.waitForFunction(()=>document.getElementById('account-state').textContent.includes('已受理'));await page.fill('#account-code','123456');await page.fill('#account-password','fixture-only');await page.fill('#account-confirm','fixture-only');await page.click('#account-submit');await page.waitForFunction(()=>!document.getElementById('account-dialog').open);assert.equal(await page.inputValue('#identifier'),'12345678');assert.equal(await page.inputValue('#account-password'),'');assert.equal(await page.evaluate(()=>window.testCalls.filter(x=>x==='login_account').length),authBeforeRegistration);
    await page.fill('#identifier','10000002');await page.fill('#password','fixture-only');await page.click('#launch');
    await page.waitForFunction(()=>!document.getElementById('home-view').hidden);assert.equal(await page.inputValue('#password'),'');
    assert.equal(await page.evaluate(()=>window.testCalls.includes('login_account')),true);
    assert.equal(await page.evaluate(()=>window.testCalls.includes('launch_game')||window.testCalls.includes('start_authenticated')),false);
    await page.click('#home-directory');await page.click('[data-settings-tab=storage]');await page.fill('#directory','D:\\示例客户端');await page.click('#save');await page.click('#close-settings');
    await page.click('#show-login');await page.waitForFunction(()=>document.getElementById('status').textContent.includes('游戏内确认'));
    assert.equal(await page.evaluate(()=>window.testCalls.includes('start_authenticated')),true);
    await page.click('#account-menu-toggle');await page.click('#logout-account');await page.waitForFunction(()=>document.getElementById('home-view').hidden);
    assert.equal(await page.inputValue('#password'),'');await page.fill('#identifier','00000001');await page.check('#remember');await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>!document.getElementById('home-view').hidden);
    const rememberedEmail=await page.evaluate(()=>window.testSettings);assert.equal(rememberedEmail.identifier,'00000001');assert.equal('password' in rememberedEmail,false);assert.equal('token' in rememberedEmail,false);
    await page.click('#account-menu-toggle');await page.click('#logout-account');await page.waitForFunction(()=>document.getElementById('home-view').hidden);assert.equal(await page.inputValue('#password'),'');
    await page.click('#client-shortcut');
    await page.click('[data-settings-tab=about]');await page.evaluate(()=>{window.testMode='update-new';});await page.click('#check-update');
    await page.waitForFunction(()=>!document.getElementById('download-update').hidden);
    assert.match(await page.textContent('#update-status'),/0.5.0/);await page.click('#download-update');
    await page.waitForFunction(()=>window.testCalls.includes('download_update'));
    await page.waitForFunction(()=>!document.getElementById('open-update-folder').hidden);await page.click('#open-update-folder');
    await page.waitForFunction(()=>window.testCalls.includes('open_update'));
    await page.evaluate(()=>{window.testMode='update-error';});await page.click('#check-update');
    await page.waitForFunction(()=>document.getElementById('update-status').textContent.includes('尚不可用'));
    assert.equal(await page.locator('#download-update').isVisible(),false);
    assert.equal(await page.locator('#launch span').textContent(),'登录');
    assert.equal(await page.locator('#download-client').isVisible(),false);
    assert.equal(await page.locator('#check-client-update').isVisible(),false);
    // Exercise retained development handlers explicitly; production controls remain hidden.
    await page.evaluate(()=>{document.getElementById('download-client').hidden=false;document.getElementById('check-client-update').hidden=false;});
    await page.evaluate(()=>{window.testMode='client-source-offline';});await page.click('[data-settings-tab=download]');await page.click('#check-client-update');await page.waitForFunction(()=>document.getElementById('status').textContent.includes('尚未发布'));
    await page.click('#manual-client-download');await page.waitForFunction(()=>window.testCalls.includes('open_client_share'));
    await page.evaluate(()=>{window.testMode='client-hold';});await page.click('#download-client');await page.waitForSelector('#pause-client-download:visible');await page.click('#pause-client-download');await page.waitForFunction(()=>document.getElementById('client-download-state').textContent.includes('已暂停'));
    assert.equal(await page.locator('#download-client').isEnabled(),true);
    await page.evaluate(()=>{window.testMode='normal';});await page.click('#download-client');await page.waitForFunction(()=>document.getElementById('client-download-state').textContent.includes('安装完成'));
    assert.equal(await page.locator('#pause-client-download').isVisible(),false);
    await page.goto('http://127.0.0.1:'+server.address().port+'/index.html?saved');
    await page.waitForFunction(()=>document.getElementById('password').value==='••••••••••');
    await page.fill('#identifier','10000002');assert.equal(await page.inputValue('#password'),'');assert.equal(await page.textContent('#credential-hint'),'');
    await page.evaluate(()=>window.testMode='save-fail');await page.fill('#password','fixture-only');await page.click('#launch');await page.waitForFunction(()=>!document.getElementById('home-view').hidden);assert.match(await page.textContent('#status'),/未能保存/);
    await page.route('**/scene-config.mjs',route=>route.fulfill({contentType:'text/javascript',body:fs.readFileSync(path.join(root,'scene-config.mjs'),'utf8').replace('intervalMs:60000','intervalMs:200')}));
    await page.goto('http://127.0.0.1:'+server.address().port+'/');await page.waitForFunction(()=>document.querySelector('.modern-stage')?.dataset.rotation);
    if(await page.locator('#motion-toggle').getAttribute('aria-pressed')==='false')await page.dblclick('#motion-toggle');
    const initialWallpaper=await page.locator('.scene-still').getAttribute('src');await page.waitForFunction(src=>document.querySelector('.scene-still').getAttribute('src')!==src,initialWallpaper);
    await page.dblclick('#motion-toggle');const fixedWallpaper=await page.locator('.scene-still').getAttribute('src');await page.waitForTimeout(500);assert.equal(await page.locator('.scene-still').getAttribute('src'),fixedWallpaper);
    await page.click('#client-shortcut');await page.screenshot({path:path.join(out,'wallpaper-settings.png')});await page.click('#close-settings');
    await page.unroute('**/scene-config.mjs');
    await page.goto('http://127.0.0.1:'+server.address().port+'/index.html?ordinary');await page.waitForSelector('#edition-usage-notice:not([hidden])');assert.match(await page.textContent('#edition-usage-notice'),/禁止倒卖/);await page.click('#client-shortcut');await page.click('[data-settings-tab=about]');assert.equal(await page.locator('#edition-usage-details').isVisible(),true);await page.goto('http://127.0.0.1:'+server.address().port+'/');await page.waitForSelector('#music-toggle');assert.equal(await page.locator('#edition-usage-notice').isVisible(),false);
    assert.deepEqual(failures,[]);
    const report={passed:true,scope:'Browser UI with mocked native IPC; not native/game integration',checks:['client source outage preserves existing client','manual share is separate from automatic source','pause and retry client installation','automatic and manual online update checks','new version download action','update outage leaves login usable','static wallpapers have no particles or video','rotation toggle persists','custom image persists across reload','restore bundled wallpaper','independent persistent rotation and music controls','local BGM plays and can be disabled','BGM stops after game process creation','empty input','missing directory','failed client check','directory picker persistence','ready card turns green then hides after visible dwell','timeout and disabled state','legacy settings use unified interface','registration fills UID without login','saved password cleared when UID changes','credential failure remains visible after login','auth failure clears password','remember identifier only','process-created wording','no uncaught JS errors']};
    fs.writeFileSync(path.join(out,'ui-smoke.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
  }finally{await browser.close();server.close();}
})().catch(e=>{console.error(e);server.close();process.exitCode=1;});











