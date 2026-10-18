// Actual Max/WebView2 with temporary data. Only public GET diagnostics and
// local rejection gates: no account creation, mail, player login or DFO.
process.env.TEMP='C:/dev-toolchain/tmp';process.env.TMP=process.env.TEMP;
const {chromium}=require('playwright'),{spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'..'),release=process.argv[2]||'0.7.1-max-online-r4',delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const temp=fs.mkdtempSync('C:/dev-toolchain/tmp/max-native-qa-'),exe=path.join(temp,'MeowLauncher Max.exe');fs.copyFileSync(path.join(root,'releases',release,'MeowLauncher Max.exe'),exe);
 const sha256=crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex');
 const reserve=net.createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
 const child=spawn(exe,[],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(temp,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});let browser;
 try{
  for(let i=0;i<100;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}if(child.exitCode!==null)throw Error('Max desktop exited before QA connection');await delay(250);}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);let page;for(let i=0;i<60;i++){page=browser.contexts()[0].pages().find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(250);}assert(page);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.waitForFunction(()=>document.querySelector('#platform-login-method'));await page.waitForFunction(()=>!document.querySelector('#login-fields').disabled);
  const boot=await page.evaluate(()=>window.__TAURI__.core.invoke('bootstrap'));assert.equal(boot.product,'MeowLauncher Max');assert.equal(boot.accountMode,'platform-login-id-v1');assert.equal(boot.profileId,'115us');
  const screenshots=path.join(root,`docs/qa/max-native/${release}`);fs.mkdirSync(screenshots,{recursive:true});await page.screenshot({path:path.join(screenshots,'login.png')});
  const invoke=(command,args)=>page.evaluate(async({command,args})=>{try{await window.__TAURI__.core.invoke(command,args);return null;}catch(e){return e.code;}},{command,args});
  assert.equal(await invoke('platform_login',{identifierType:'id',identifier:'invalid id',password:'fixture-only'}),'invalid_identifier');
  assert.equal(await invoke('login_account',{identifier:'10000001',password:'fixture-only'}),'protocol_unavailable');
  assert.equal(await invoke('start_authenticated',{directory:'D:/not-a-game'}),'login_required');
  assert.equal(await invoke('platform_register',{loginId:'Fixture123',nickname:null,email:'fixture@example.invalid',code:'123456',password:'fixture-only'}),'code_not_requested');
  assert.equal(fs.existsSync(path.join(temp,'data/runtime')),false);
  assert.equal(await page.locator('#show-login').isDisabled(),true);assert.equal(await page.textContent('#start-label'),'账号准备中');
  await page.click('#register-account');assert.match(await page.textContent('.identity-registration-notice'),/不能自行修改/);assert.equal(await page.locator('#account-login-id').count(),1);await page.screenshot({path:path.join(screenshots,'register.png')});await page.click('#account-close');
  const service=await page.evaluate(async()=>{try{const s=await window.__TAURI__.core.invoke('check_service');return{available:s.available};}catch(e){return{available:false,code:e.code};}});
  const target=await page.evaluate(async()=>{try{return{configured:await window.__TAURI__.core.invoke('refresh_native_target')};}catch(e){return{configured:false,code:e.code};}});
  await page.click('#client-shortcut');await page.click('[data-settings-tab=about]');assert.match(await page.textContent('#author-credit'),/zkonikishi/);await page.screenshot({path:path.join(screenshots,'about.png')});
  await page.click('[data-settings-tab=general]');await page.selectOption('#close-behavior','exit');await page.click('#save-preferences');await page.click('#close-settings');await page.click('#close-window');for(let i=0;i<40&&child.exitCode===null;i++)await delay(100);assert.notEqual(child.exitCode,null);assert.deepEqual(errors,[]);
  const report={passed:true,release,sha256,scope:'Actual Max GUI/IPC, local rejection gates and public production diagnostics; not authenticated player/native-game acceptance',checks:['platform typed identity entry','old credential/ticket route blocked','unauthenticated Start fails before files/process','registration requires current challenge','no host materialization on boot','author visible','game unavailable label','normal exit'],publicService:service,publicTarget:target,screenshots,noPlayerLogin:true,noAccountCreation:true,noMail:true,noGame:true};fs.writeFileSync(path.join(root,`docs/qa/max-native-${release}.json`),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{if(browser)await browser.close().catch(()=>{});if(child.exitCode===null)child.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
