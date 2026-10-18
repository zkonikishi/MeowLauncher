// OWNER-COORDINATED actual GUI account test. Only loopback TLS fixture targets.
// A deliberately non-executable/wrong-hash host and NO DFO directory ensure
// the manual Start action can only exercise rejection, never start a game.
process.env.TEMP='C:/dev-toolchain/tmp';process.env.TMP=process.env.TEMP;
const {chromium}=require('playwright'),{spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'..'),delay=ms=>new Promise(r=>setTimeout(r,ms));
const check=(value)=>{if(!value)throw Error('acceptance assertion failed');};
const read=file=>{const bytes=fs.readFileSync(file);check(bytes.length<=65536);return JSON.parse(bytes.toString('utf8'));};
const privatePath=value=>{const absolute=path.resolve(value);check(absolute.toLowerCase().startsWith('d:\\ai\\codex\\tmp\\'));return absolute;};
(async()=>{
 let child,browser,metadata,receipt,phase='input';const report={passed:false,scope:'Actual generic EXE/WebView2 GUI registration and login against owner-managed TLS; negative pre-start only, not real strict WFP/DFO/game acceptance',noGame:true,noBackendStart:true};
 try{
  const [stateFile,release,expectedSha]=process.argv.slice(2);check(stateFile&&/^[A-Za-z0-9._-]+$/.test(release)&&/^[0-9a-f]{64}$/i.test(expectedSha));
  const state=read(stateFile);check(state.phase==='ready');metadata=read(privatePath(state.metadata_file));check(metadata.run_id===state.run_id);const candidateReceipt=privatePath(metadata.result_file);check(path.dirname(candidateReceipt)===path.dirname(privatePath(state.metadata_file))&&!fs.existsSync(candidateReceipt));receipt=candidateReceipt;
  const split=metadata.address.lastIndexOf(':');check(metadata.address.slice(0,split)==='127.0.0.1'&&metadata.server_name==='127.0.0.1');
  const port=metadata.address.slice(split+1),username=metadata.gui_registration_username;check(/^[A-Za-z0-9_-]{1,32}$/.test(username));
  const ca=privatePath(metadata.ca_der_file),targetFile=privatePath(metadata.player_target_file),readyFile=privatePath(metadata.ready_file);
  const source=path.join(root,'releases',release,'MeowLauncher.exe');check(crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex')===expectedSha.toLowerCase());
  const folder=fs.mkdtempSync('C:/dev-toolchain/tmp/generic-live-qa-');const exe=path.join(folder,'MeowLauncher.exe');fs.copyFileSync(source,exe);
  const fakeClient=path.join(folder,'no-client');fs.mkdirSync(fakeClient);const dummy=path.join(folder,'not-a-host.exe');fs.writeFileSync(dummy,'NOT AN EXECUTABLE: deliberate integrity rejection fixture');check(!fs.existsSync(path.join(fakeClient,'DFO.exe')));
  const reserve=net.createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const debug=reserve.address().port;await new Promise(r=>reserve.close(r));
  child=spawn(exe,[],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(folder,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${debug}`}});
  phase='window';let connected=false;for(let i=0;i<100;i++){try{if((await fetch(`http://127.0.0.1:${debug}/json/version`)).ok){connected=true;break;}}catch{}check(child.exitCode===null);await delay(250);}check(connected);
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${debug}`);let page;for(let i=0;i<60;i++){page=browser.contexts()[0].pages().find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(250);}check(page);await page.waitForFunction(()=>document.querySelector('#generic-register'));
  phase='configure';await page.click('#client-shortcut');await page.click('[data-settings-tab=download]');if(await page.locator('#generic-advanced').count())await page.click('#generic-advanced>summary');
  for(const [key,value] of Object.entries({brand:'Actual GUI verification',host:'127.0.0.1',port,serverName:metadata.server_name,authorityId:metadata.authority_id,realmId:metadata.realm_id,saveProfileId:metadata.save_profile_id,caCertificate:ca}))await page.fill('#generic-'+key,String(value));
  if(await page.locator('#generic-review-options').count())await page.click('#generic-review-options>summary');for(const [key,value] of Object.entries({hostExecutable:dummy,playerTargetFile:targetFile,readyFile}))await page.fill('#review-'+key,value);await page.check('#review-allow');await page.click('#generic-save');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('已保存'));await page.click('[data-settings-tab=storage]');await page.fill('#directory',fakeClient);await page.click('#save');await page.click('#close-settings');
  let password=fs.readFileSync(privatePath(metadata.password_file),'utf8');
  phase='register';await page.click('#register-account');await page.fill('#generic-register-user',username);await page.fill('#generic-register-password',password);await page.fill('#generic-register-confirm',password);await page.click('#generic-register-form button[type=submit]');await page.waitForFunction(()=>!document.querySelector('#generic-register').open,null,{timeout:20000});check((await page.textContent('#status')).includes('注册成功'));check(await page.inputValue('#generic-register-password')==='');report.guiRegistered=true;
  phase='login';await page.fill('#password',password);password=null;await page.click('#launch');await page.waitForFunction(()=>!document.querySelector('#home-view').hidden,null,{timeout:20000});check(await page.inputValue('#password')==='');const boot=await page.evaluate(()=>window.__TAURI__.core.invoke('bootstrap'));check(boot.authenticated===true&&boot.identity.username===username&&boot.identity.nativeAccountId===metadata.expected_native_account_id);report.guiLoggedIn=true;report.nativeAccountId=boot.identity.nativeAccountId;
  phase='negative-start';await page.click('#show-login');await page.waitForFunction(()=>document.querySelector('#status-dot').dataset.kind==='error',null,{timeout:25000});const code=await page.evaluate(async()=>{try{await window.__TAURI__.core.invoke('start_authenticated',{directory:document.getElementById('directory').value});return null;}catch(e){return e.code;}});check(['client_host_integrity','client_host_admin'].includes(code));check(await page.evaluate(()=>window.__TAURI__.core.invoke('current_instance'))===null);check(!fs.existsSync(path.join(folder,'data','instances')));report.nativeStartRejected=code;report.noInstanceCreated=true;
  phase='exit';await page.click('#client-shortcut');await page.click('[data-settings-tab=general]');await page.selectOption('#close-behavior','exit');await page.click('#save-preferences');await page.waitForFunction(()=>document.querySelector('#status').textContent==='设置已保存。');await page.click('#close-settings');await page.screenshot({path:path.join(root,'docs/qa/generic-gui-live-account.png')});await page.click('#close-window');for(let i=0;i<40&&child.exitCode===null;i++)await delay(100);check(child.exitCode!==null);report.windowClosed=true;report.passed=true;report.run_id=metadata.run_id;report.exe_sha256=expectedSha.toLowerCase();report.fixture=folder;
 }catch(e){report.phase=phase;report.error_type=e?.constructor?.name||'Error';}
 finally{
  if(browser)await browser.close().catch(()=>{});if(child?.exitCode===null)child.kill();
  report.updated_at=new Date().toISOString();fs.writeFileSync(path.join(root,'docs/qa/generic-gui-host-actual.json'),JSON.stringify(report,null,2));
  if(receipt&&metadata){const result={run_id:metadata.run_id,passed:report.passed,gui_registration:report.guiRegistered===true,gui_login:report.guiLoggedIn===true,start_precheck_rejected:!!report.nativeStartRejected,no_DFO:report.noGame===true,window_closed:report.windowClosed===true};const pending=receipt+'.pending';fs.writeFileSync(pending,JSON.stringify(result));fs.renameSync(pending,receipt);}
  console.log(JSON.stringify({passed:report.passed,phase:report.phase||'complete',report:path.join(root,'docs/qa/generic-gui-host-actual.json')}));if(!report.passed)process.exitCode=1;
 }
})();
