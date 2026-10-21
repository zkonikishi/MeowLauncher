import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import http from 'node:http';
import {createRequire} from 'node:module';
import {fileURLToPath} from 'node:url';
const require=createRequire(import.meta.url);
let chromium;try{({chromium}=require('playwright'));}catch{({chromium}=require(path.join(process.env.USERPROFILE,'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright')));}
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const out=process.env.MEOW_TEST_OUT||'D:/Ai/Codex/artifacts/beta-tauri-cloud-20261021';fs.mkdirSync(out,{recursive:true});
const server=http.createServer((req,res)=>{if(req.url==='/cloud-auth-ui.mjs'){res.setHeader('content-type','application/javascript');res.end(fs.readFileSync(path.join(root,'apps/launcher/ui/cloud-auth-ui.mjs')));}else{res.setHeader('content-type','text/html');res.end('<!doctype html><html><body><section id="login-view"><form id="login-form"><input aria-label="existing login"></form><button id="register-account">注册</button></section><section id="panel"></section></body></html>');}});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const browserPath=[process.env.MEOW_TEST_BROWSER,chromium.executablePath(),'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe','C:/Program Files/Google/Chrome/Application/chrome.exe'].find(p=>p&&fs.existsSync(p));if(!browserPath)throw Error('No installed Chromium-family browser; no automatic download');
const context=await chromium.launchPersistentContext(path.join(out,'browser-profile'),{executablePath:browserPath,headless:true,args:['--disk-cache-size=1']});const page=await context.newPage();
const results=[];const test=async(name,fn)=>{await fn();results.push({name,passed:true});};
async function setup({local=false,delayLogin=false,delayBegin=false,delayPoll=false,delayAccept=false}={}){
 await page.goto(`http://127.0.0.1:${server.address().port}`);
 await page.evaluate(async options=>{
  const {mountAccountExtensions}=await import('/cloud-auth-ui.mjs');window.calls=[];window.authenticated=[];
  const local={id:'gateway',label:'Gateway',connection:{kind:'local-gateway',target:{host:'localhost'}}};
  const cloud={id:'cloud',label:'Cloud <img src=x onerror=alert(1)>',connection:{kind:'cloud-account',methods:['username-password','email-code','phone-code','third-party']}};
  const info={authConfig:{schemaVersion:1,activeProvider:options.local?'gateway':'cloud',providers:[local,cloud]}};
  const identity={username:'Player',nativeAccountId:'42'};
  const invoke=async(command,args)=>{
   window.calls.push({command,args:args?structuredClone(args):null});
   if(command==='login_cloud'){if(options.delayLogin)return await new Promise(r=>window.finishLogin=()=>r(identity));return identity;}
   if(command==='accept_cloud_request'&&options.delayAccept)return await new Promise(r=>window.finishAccept=()=>r({}));
   if(command==='request_cloud_code')return {expiresIn:300,retryAfter:30};
   if(command==='begin_cloud_login'){const result={handle:42,interval:2,expiresIn:300,authorizationUrl:'https://operator.example/flow'};if(options.delayBegin)return await new Promise(r=>window.finishBegin=()=>r(result));return result;}
   if(command==='poll_cloud_login'){if(options.delayPoll)return await new Promise(r=>window.finishPoll=()=>r(identity));return identity;}
   return {};
  };
  mountAccountExtensions({info,invoke,panel:document.querySelector('#panel'),onAuthenticated(identity){window.authenticated.push(identity);}});
 },{local,delayLogin,delayBegin,delayPoll,delayAccept});
}
async function open(method='email-code',purpose='login'){
 await page.click('#cloud-auth-open');await page.selectOption('#cloud-auth-purpose',purpose);await page.selectOption('#cloud-auth-method',method);
 if(method!=='third-party')await page.fill('#cloud-auth-identifier',method==='phone-code'?'+12025550123':method==='email-code'?'p@example.com':'Player');
 if(purpose!=='login'){await page.fill('#cloud-auth-password','fixture-password');await page.fill('#cloud-auth-confirm','fixture-password');}
 if(purpose==='register')await page.fill('#cloud-auth-username','Player');
 if(method==='email-code'||method==='phone-code')await page.fill('#cloud-auth-code','123456');
}
try{
 await test('local provider has no cloud or third-party entrance',async()=>{await setup({local:true});assert.equal(await page.locator('#cloud-auth-open').count(),0);assert.equal(await page.locator('#login-form').isHidden(),false);});
 await test('configured labels are text, not HTML',async()=>{await setup();assert.equal(await page.locator('img').count(),0);});
 await test('email code login is confirmed through native before UI authenticates',async()=>{await setup();await open();await page.click('#cloud-auth-send-code');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.authenticated.length===1);const calls=await page.evaluate(()=>window.calls);assert.equal(calls.find(c=>c.command==='request_cloud_code').args.purpose,'login');assert.ok(calls.some(c=>c.command==='accept_cloud_request'));assert.equal(await page.inputValue('#cloud-auth-password'),'');});
 await test('phone registration is distinct from login',async()=>{await setup();await open('phone-code','register');await page.click('#cloud-auth-send-code');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>document.querySelector('#cloud-auth-status').textContent.includes('注册已确认'));assert.equal(await page.evaluate(()=>window.authenticated.length),0);assert.ok(await page.evaluate(()=>window.calls.some(c=>c.command==='register_cloud'&&c.args.method==='phone-code')));});
 await test('reset uses reset challenge and never logs in',async()=>{await setup();await open('email-code','reset');await page.click('#cloud-auth-send-code');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.calls.some(c=>c.command==='reset_cloud_password'));assert.ok(await page.evaluate(()=>window.calls.some(c=>c.command==='request_cloud_code'&&c.args.purpose==='reset')));assert.equal(await page.evaluate(()=>window.authenticated.length),0);});
 await test('password mismatch does not reach native registration',async()=>{await setup();await open('phone-code','register');await page.fill('#cloud-auth-confirm','other');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>document.querySelector('#cloud-auth-status').textContent.includes('不一致'));assert.ok(!await page.evaluate(()=>window.calls.some(c=>c.command==='register_cloud')));});
 await test('closing pending password/code login cancels matching native request',async()=>{await setup({delayLogin:true});await open();await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.finishLogin);await page.click('#cloud-auth-close');await page.evaluate(()=>window.finishLogin());await page.waitForFunction(()=>!document.querySelector('#cloud-auth-submit').closest('fieldset').disabled);assert.equal(await page.evaluate(()=>window.authenticated.length),0);assert.ok(await page.evaluate(()=>window.calls.some(c=>c.command==='cancel_cloud_request'&&c.args.requestId)));});
 await test('close during native acknowledgement never restores UI session',async()=>{await setup({delayAccept:true});await open();await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.finishAccept);await page.click('#cloud-auth-close');await page.evaluate(()=>window.finishAccept());await page.waitForFunction(()=>!document.querySelector('#cloud-auth-submit').closest('fieldset').disabled);assert.equal(await page.evaluate(()=>window.authenticated.length),0);assert.ok(await page.evaluate(()=>window.calls.some(c=>c.command==='cancel_cloud_request')));});
 await test('late browser begin is cancelled by matching handle',async()=>{await setup({delayBegin:true});await open('third-party');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.finishBegin);await page.click('#cloud-auth-close');await page.evaluate(()=>window.finishBegin());await page.waitForFunction(()=>window.calls.some(c=>c.command==='cancel_cloud_login'));assert.ok(await page.evaluate(()=>window.calls.some(c=>c.command==='cancel_cloud_login'&&c.args.handle===42)));assert.equal(await page.evaluate(()=>window.authenticated.length),0);});
 await test('late browser poll cannot recreate cancelled UI identity',async()=>{await setup({delayPoll:true});await open('third-party');await page.click('#cloud-auth-submit');await page.waitForFunction(()=>window.finishPoll,{},{timeout:6000});await page.click('#cloud-auth-close');await page.evaluate(()=>window.finishPoll());await page.waitForFunction(()=>window.calls.some(c=>c.command==='cancel_cloud_login'));assert.equal(await page.evaluate(()=>window.authenticated.length),0);assert.ok(!await page.evaluate(()=>window.calls.some(c=>c.command==='accept_cloud_browser')));});
 await setup();await open('email-code','register');await page.screenshot({path:path.join(out,'cloud-register-controls.png')});
 fs.writeFileSync(path.join(out,'ui-report.json'),JSON.stringify({passed:results.length,tests:results,kind:'headless Chromium DOM/native-boundary fixture; not real provider or game acceptance'},null,2));console.log(JSON.stringify({suite:'tauri-cloud-account-ui',passed:results.length,out}));
}finally{await context.close();await new Promise(r=>server.close(r));}
