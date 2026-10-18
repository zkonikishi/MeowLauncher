process.env.TEMP='C:/dev-toolchain/tmp';process.env.TMP=process.env.TEMP;
// Actual ordinary EXE/WebView2. Missing target + invalid credentials only:
// no server network, account creation, game, native host or backend startup.
const {chromium}=require('playwright'),{spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..'),delay=ms=>new Promise(r=>setTimeout(r,ms));
const releaseName=process.argv[2]||'0.7.1-generic-auth-review';
(async()=>{
 const folder=fs.mkdtempSync('C:/dev-toolchain/tmp/generic-native-qa-');
 const exe=path.join(folder,'MeowLauncher.exe');fs.copyFileSync(path.join(root,'releases',releaseName,'MeowLauncher.exe'),exe);
 const artifactSha256=require('node:crypto').createHash('sha256').update(fs.readFileSync(exe)).digest('hex');
 const reserve=net.createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
 const child=spawn(exe,[],{windowsHide:true,stdio:'ignore',env:{...process.env,TEMP:'C:/dev-toolchain/tmp',TMP:'C:/dev-toolchain/tmp',WEBVIEW2_USER_DATA_FOLDER:path.join(folder,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
 let browser;
 try {
  for(let i=0;i<100;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}if(child.exitCode!==null)throw Error('ordinary desktop exited');await delay(250);}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);let page;for(let i=0;i<60;i++){page=browser.contexts()[0].pages().find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(250);}assert(page);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.waitForFunction(()=>document.querySelector('#generic-register'));
  const boot=await page.evaluate(()=>window.__TAURI__.core.invoke('bootstrap'));assert.equal(boot.accountMode,'simple');assert.equal(boot.product,'MeowLauncher');assert.equal(boot.edition,'sponsor');assert.equal(boot.genericProfile,null);
  if(await page.locator('#author-credit').count()){
   assert.equal(await page.locator('#generic-instance-state').isVisible(),false);
   const auditFolder=path.join(root,'docs/qa/settings-audit-20261016',releaseName);fs.mkdirSync(auditFolder,{recursive:true});
   const panels=[];await page.click('#client-shortcut');
   for(const tab of ['general','storage','download','about']){
    await page.click(`[data-settings-tab=${tab}]`);await page.screenshot({path:path.join(auditFolder,`${tab}.png`)});
    panels.push(await page.locator(`[data-settings-panel=${tab}]`).evaluate(el=>({tab:el.dataset.settingsPanel,fields:[...el.querySelectorAll('input,select,button')].filter(n=>n.getClientRects().length).map(n=>({id:n.id,label:n.labels?.[0]?.textContent.trim()||n.textContent.trim(),radius:getComputedStyle(n).borderRadius})),horizontalOverflow:el.scrollWidth>el.clientWidth+1})));
   }
   assert.match(await page.textContent('#author-credit'),/zkonikishi/);assert(panels.every(p=>!p.horizontalOverflow));
   await page.click('[data-settings-tab=download]');assert.equal(await page.locator('#generic-advanced').getAttribute('open'),null);assert.equal(await page.locator('#generic-review-options').getAttribute('open'),null);
   assert.equal(await page.locator('#generic-host').evaluate(el=>getComputedStyle(el).borderRadius),'22px');
   await page.click('#generic-advanced>summary');await page.screenshot({path:path.join(auditFolder,'advanced.png')});await page.click('#generic-advanced>summary');
   await page.click('#generic-review-options>summary');await page.screenshot({path:path.join(auditFolder,'local-review.png')});await page.click('#generic-review-options>summary');
   fs.writeFileSync(path.join(auditFolder,'fields.json'),JSON.stringify({releaseName,artifactSha256,scope:'Fresh actual EXE temporary-data settings captures before saving test preferences; user instance untouched',panels},null,2));
   await page.click('#close-settings');
  }
  const startFailure=await page.evaluate(async()=>{try{await window.__TAURI__.core.invoke('start_authenticated',{directory:'D:/not-a-client'});return null;}catch(e){return e.code;}});assert.equal(startFailure,'login_required');
  assert.equal(await page.getAttribute('#identifier','maxlength'),'32');assert.equal(await page.locator('#account-email').count(),0);
  const failure=await page.evaluate(async()=>{try{await window.__TAURI__.core.invoke('login_account',{identifier:'Player_A',password:'fixture-only'});return null;}catch(e){return e.code;}});assert.equal(failure,'simple_config');
  await page.click('#client-shortcut');await page.click('[data-settings-tab=download]');if(await page.locator('#generic-advanced').count())await page.click('#generic-advanced>summary');for(const [id,value] of Object.entries({brand:'Native UI fixture',host:'server.example.invalid',port:'443',serverName:'server.example.invalid',authorityId:'fixture',realmId:'realm',saveProfileId:'save'}))await page.fill('#generic-'+id,value);await page.click('#generic-save');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('已保存'));
  const inputFailure=await page.evaluate(async()=>{try{await window.__TAURI__.core.invoke('register_generic',{username:'invalid name',password:'fixture-only'});return null;}catch(e){return e.code;}});assert.equal(inputFailure,'simple_input');
  const blockedPlatform=await page.evaluate(async()=>{try{await window.__TAURI__.core.invoke('complete_registration',{email:'test@example.invalid',password:'fixture-only',code:'000000'});return false;}catch{return true;}});assert.equal(blockedPlatform,true);
  await page.click('[data-settings-tab=general]');await page.selectOption('#close-behavior','exit');await page.click('#save-preferences');await page.waitForFunction(()=>document.querySelector('#status').textContent==='设置已保存。');await page.click('#close-settings');await page.screenshot({path:path.join(root,'docs/qa/generic-login-native.png')});
  await page.click('#close-window');for(let i=0;i<40&&child.exitCode===null;i++)await delay(100);assert.notEqual(child.exitCode,null);assert.deepEqual(errors,[]);
  const report={passed:true,releaseName,artifactSha256,scope:'Actual generic EXE/WebView2/IPC/config persistence and pre-network rejection only; no real TLS authentication or game acceptance',checks:['no platform AccountClient entry','simple bootstrap','unauthenticated start rejects before file/process work','username UI','config missing rejects before network','native profile save','invalid credentials reject before network','platform registration IPC unavailable','exit preference closes process'],fixture:folder,noServerNetwork:true,noGame:true};fs.writeFileSync(path.join(root,'docs/qa/generic-native-smoke.json'),JSON.stringify(report,null,2));fs.writeFileSync(path.join(root,'docs/qa',`generic-native-${releaseName}.json`),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 } finally {if(browser)await browser.close().catch(()=>{});if(child.exitCode===null)child.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
