// Tests the compiled Tauri/WebView2 app, using a temporary local debugging port.
// Debugging is enabled ONLY in this child process environment, never in the build.
const {chromium}=require('playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const suite=path.resolve(__dirname,'..'),out=path.join(suite,'docs/qa'),runtime=path.join(suite,'target/native-smoke');
const delay=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 fs.mkdirSync(runtime,{recursive:true});fs.mkdirSync(out,{recursive:true});
 const exe=path.join(runtime,'meow-launcher.exe');fs.copyFileSync(path.join(suite,'target/release/meow-launcher.exe'),exe);
 const reservation=net.createServer();await new Promise(r=>reservation.listen(0,'127.0.0.1',r));const port=reservation.address().port;await new Promise(r=>reservation.close(r));
 const child=spawn(exe,[],{cwd:runtime,stdio:'ignore',env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`,WEBVIEW2_USER_DATA_FOLDER:path.join(runtime,'webview')}});
 let browser;
 try{
   for(let i=0;i<60;i++){try{const res=await fetch(`http://127.0.0.1:${port}/json/version`);if(res.ok)break;}catch{}await delay(250);}
   browser=await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
   const context=browser.contexts()[0];let page;
   for(let i=0;i<40;i++){page=context.pages().find(p=>p.url().includes('tauri.localhost'));if(page)break;await delay(250);}
   assert.ok(page,'Tauri page is available');
   await page.waitForFunction(()=>document.getElementById('status')?.textContent.includes('准备就绪'));
   await page.click('#launch');assert.match(await page.textContent('#status'),/8 位 UID/);
   await page.click('[data-page="settings"]');
   await page.fill('#directory',path.join(runtime,'missing'));await page.click('#inspect');await page.waitForFunction(()=>document.getElementById('status').textContent.includes('请选择包含'));
   const client='D:\\Server\\Arad\\86JP S4A21\\Client\\S4A21_CN';
   await page.fill('#directory',client);await page.click('#save');await page.waitForFunction(()=>document.getElementById('status').textContent==='设置已保存。');
   assert.equal(JSON.parse(fs.readFileSync(path.join(runtime,'data/settings.json'))).clientDirectory,client);
   await page.click('#inspect');await page.waitForFunction(()=>document.getElementById('check-summary').textContent==='文件条件已满足');assert.equal(await page.locator('.check-row').count(),17);
   await page.screenshot({path:path.join(out,'native-checks.png')});
   await page.click('#probe');await page.waitForFunction(()=>document.getElementById('status').textContent.includes('账号服务可连接'),{},{timeout:15000});
   await page.click('#retro');await page.waitForFunction(()=>document.documentElement.dataset.theme==='retro');await page.waitForFunction(()=>document.getElementById('status').textContent==='界面设置已保存。');
   await page.reload();await page.waitForFunction(()=>document.getElementById('status')?.textContent.includes('准备就绪'));assert.equal(await page.getAttribute('html','data-theme'),'retro');assert.equal(await page.inputValue('#directory'),client);
   await page.screenshot({path:path.join(out,'native-retro.png')});
   // Exercise a real IPC launch rejection before authentication with an empty fixture directory.
   const empty=path.join(runtime,'empty-client');fs.mkdirSync(empty,{recursive:true});
   await page.click('[data-page="settings"]');await page.fill('#directory',empty);await page.click('#save');await page.waitForFunction(()=>document.getElementById('status').textContent==='设置已保存。');
   await page.click('[data-page="home"]');await page.fill('#identifier','12345678');await page.fill('#password','fixture-only');await page.click('#launch');
   await page.waitForFunction(()=>document.getElementById('status').textContent.includes('客户端检查未通过'));
   assert.equal(await page.inputValue('#password'),'');
   const log=fs.readFileSync(path.join(runtime,'data/launcher.log'),'utf8');assert.ok(log.includes('client_not_ready'));assert.ok(!log.includes('fixture-only'));assert.ok(!log.includes('12345678'));
   const config=JSON.parse(fs.readFileSync(path.join(runtime,'data/settings.json')));assert.equal(config.identifier,'');assert.equal('password' in config,false);
   const report={passed:true,scope:'Real Tauri IPC and WebView2; no credentials sent; no game process launched',checks:['native empty form','invalid path error','settings file persisted','17 client checks','pinned live status request','retro persisted across reload','real launch IPC rejects missing client','password cleared','diagnostic log contains no fixture credentials']};
   fs.writeFileSync(path.join(out,'native-smoke.json'),JSON.stringify(report,null,2));console.log(JSON.stringify(report));
 }finally{if(browser)await browser.close();child.kill();}
})().catch(e=>{console.error(e);process.exitCode=1;});
