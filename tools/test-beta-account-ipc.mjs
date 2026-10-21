import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {BetaAccount} from '../src/main/modules/beta-account.mjs';
const require=createRequire(import.meta.url),esbuild=require('../ui/node_modules/esbuild');
const bundle=await esbuild.build({entryPoints:['src/main/modules/ipc.mjs'],bundle:true,write:false,platform:'node',format:'esm',plugins:[{name:'offline-electron',setup(b){b.onResolve({filter:/^electron$/},()=>({path:'electron',namespace:'fixture'}));b.onLoad({filter:/.*/,namespace:'fixture'},()=>({contents:'export const BrowserWindow={},dialog={},screen={},shell={},nativeTheme={},WebContentsView={};'}));b.onResolve({filter:/max-account\.mjs$/},a=>({path:a.path,external:true}));}}]});
const {registerIpc}=await import('data:text/javascript;base64,'+Buffer.from(bundle.outputFiles[0].text).toString('base64'));
let launches=0,passed=0;
const frame={},contents={mainFrame:frame},event={sender:contents,senderFrame:frame};
function setup(edition,account){const h=new Map();registerIpc({ipcMain:{handle:(k,v)=>h.set(k,v),eventNames:()=>[...h.keys()]},version:{edition},windows:{window:{webContents:contents}},logger:{info(){},warn(){}},game:{launch(){launches++;return {ok:true};}},...(edition==='max'?{maxAccount:account}:{betaAccount:account})});return h;}
const client=new BetaAccount({provider:{login:async()=>({username:'Player',nativeAccountId:'42',authorityId:'owner',realmId:'realm'})}});
const beta=setup('beta',client);
assert.equal((await beta.get('ml:game:launch')(event,{})).error.code,'login_required');passed++;
assert.equal((await beta.get('ml:account:login')(event,{username:'Player',password:'pass'})).ok,true);passed++;
assert.equal((await beta.get('ml:game:launch')(event,{})).error.code,'game_adapter_pending');assert.equal(launches,0);passed++;
await beta.get('ml:account:logout')(event);assert.equal((await beta.get('ml:account:status')(event)).data,null);passed++;
const max=setup('max',{view:()=>({uid:'12345678'}),login:async()=>({uid:'12345678'})});
assert.equal((await max.get('ml:account:login')(event,{identifier:'MaxId',password:'pass'})).data.uid,'12345678');passed++;
assert.equal((await max.get('ml:game:launch')(event,{})).error.code,'game_adapter_pending');assert.equal(launches,0);passed++;
console.log(JSON.stringify({suite:'beta-max-account-ipc-isolation',passed,network:'none',gameStarted:false}));
