// Inspect embedded wallpaper in an isolated copy of the actual desktop executable.
const {chromium}=require('playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'..'),delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const folder=fs.mkdtempSync(path.join(root,'target/wallpaper-qa-'));
 const exe=path.join(folder,'MeowLauncher.exe');fs.copyFileSync(path.join(root,'target/release/meow-launcher.exe'),exe);
 const reserve=net.createServer();await new Promise(r=>reserve.listen(0,'127.0.0.1',r));const port=reserve.address().port;await new Promise(r=>reserve.close(r));
 const child=spawn(exe,[],{windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_USER_DATA_FOLDER:path.join(folder,'webview'),WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
 let browser;
 try {
  for(let i=0;i<100;i++){try{if((await fetch(`http://127.0.0.1:${port}/json/version`)).ok)break;}catch{}if(child.exitCode!==null)throw Error('desktop exited');await delay(250);}
  browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
  let page;for(let i=0;i<60;i++){page=browser.contexts()[0].pages().find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(250);}
  assert.ok(page);await page.waitForFunction(()=>document.querySelector('.scene-still')?.naturalWidth===3200);
  assert.match(await page.locator('.scene-still').getAttribute('src'),/wallpaper-apocalypse\.png$/);
  assert.equal(await page.locator('.game-logo').getAttribute('alt'),'Arad');
  await page.waitForFunction(()=>document.querySelector('#launch span')?.textContent==='登录');
  assert.equal(await page.textContent('#login-title'),'账号密码登录');
  assert.match(await page.locator('#identifier').getAttribute('placeholder'),/8 位 UID/);
  assert.match(await page.textContent('.region'),/86s4a21/);
  assert.deepEqual(await page.locator('#server-dialog input').evaluateAll(nodes=>nodes.map(n=>({value:n.value,disabled:n.disabled,checked:n.checked}))),[
   {value:'86s4a12',disabled:true,checked:false},{value:'86s4a21',disabled:false,checked:true},{value:'95s5a9',disabled:true,checked:false},{value:'115us',disabled:true,checked:false},{value:'115tw',disabled:true,checked:false},{value:'115kr',disabled:true,checked:false}
  ]);
  const about=await page.evaluate(async()=>{const b=await window.__TAURI__.core.invoke('bootstrap');return {version:b.version,product:b.product,edition:b.edition,notes:b.releaseNotes,mode:b.accountMode};});
  assert.deepEqual(about,{version:'0.7.1',product:'MeowLauncher Max',edition:'meowopia-custom',notes:'MeowLauncher为Meowopia服务器定制版',mode:'legacy-online'});
  await page.click('#client-shortcut');await page.click('[data-settings-tab=download]');assert.equal(await page.locator('#download-client').isVisible(),false);assert.equal(await page.locator('#manual-client-download').isVisible(),true);const progress=await page.evaluate(()=>window.__TAURI__.core.invoke('client_distribution_progress'));assert.equal(typeof progress.stage,'string');await page.click('[data-settings-tab=about]');assert.equal(await page.locator('#check-update').isVisible(),true);assert.match(await page.textContent('#release-notes'),/MeowLauncher为Meowopia服务器定制版/);await page.click('[data-settings-tab=general]');await page.screenshot({path:path.join(root,'docs/qa/launcher-0.7-settings-native.png')});await page.click('[data-settings-tab=download]');
  assert.match(await page.textContent('#client-download-state'),/次元彼端S4A21客户端\.rar/);
  assert.match(await page.textContent('#client-download-state'),/khug/);
  await page.click('#close-settings');await page.click('#register-account');await page.fill('#account-email','fixture@example.invalid');assert.equal(await page.inputValue('#account-email'),'fixture@example.invalid');await page.click('#account-close');
  await page.click('#music-toggle');await page.click('#music-toggle');await page.dblclick('#motion-toggle');assert.equal(await page.locator('.modern-stage').getAttribute('data-rotation'),'off');await page.dblclick('#motion-toggle');
  await page.screenshot({path:path.join(root,'docs/qa/launcher-official-wallpaper-native.png')});
  assert.equal(await page.locator('#toolbox-toggle').isVisible(),false);await page.click('#client-shortcut');await page.click('[data-settings-tab=storage]');assert.equal(await page.locator('[data-settings-panel=storage]').isVisible(),true);await page.click('[data-settings-tab=general]');await page.selectOption('#close-behavior','tray');await page.click('#save-preferences');await page.waitForFunction(()=>document.getElementById('status').textContent==='设置已保存。');await page.click('#close-settings');
  await page.evaluate(()=>window.__TAURI__.core.invoke('window_action',{action:'close'}));await delay(350);assert.equal(child.exitCode,null);await page.evaluate(()=>window.__TAURI__.core.invoke('window_action',{action:'restore'}));await page.click('#client-shortcut');await page.selectOption('#close-behavior','exit');await page.click('#save-preferences');await page.waitForFunction(()=>document.getElementById('status').textContent==='设置已保存。');await page.click('#close-settings');
  const report={passed:true,scope:'Actual EXE/WebView2 embedded wallpaper, login labels, native product/version/edition and update settings; registration form input, settings tabs, toolbox, tray close/restore; no real login, registration, mail, or game launch',wallpaper:'末世录纯净版',dimensions:[3200,1800]};
  await page.click('#close-window');for(let i=0;i<30&&child.exitCode===null;i++)await delay(100);assert.notEqual(child.exitCode,null,'exit preference closes the process');fs.writeFileSync(path.join(root,'docs/qa/launcher-wallpaper-native.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 } finally {if(browser)await browser.close().catch(()=>{});if(child.exitCode===null)child.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
