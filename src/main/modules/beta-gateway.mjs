// MEOWSIMPLE/1 native account transport. No HTTP fallback or native game claim.
import fs from 'node:fs';
import path from 'node:path';
import tls from 'node:tls';
import net from 'node:net';
import {X509Certificate} from 'node:crypto';
const invalid=()=>Object.assign(new Error('通用网关配置或认证响应无效。'),{code:'account_response'});
const keys=(v,allowed)=>v&&typeof v==='object'&&!Array.isArray(v)&&Object.keys(v).every(k=>allowed.includes(k));
export function validateTarget(t){
 if(!keys(t,['host','port','serverName','authorityId','realmId','saveProfileId','caCertificate'])||!Number.isInteger(t.port)||t.port<1||t.port>65535)throw invalid();
 for(const field of ['host','serverName','authorityId','realmId','saveProfileId'])if(typeof t[field]!=='string'||!t[field]||t[field].length>256||/[\x00-\x20\x7f]/.test(t[field]))throw invalid();
 for(const name of [t.host,t.serverName])if(!net.isIP(name)&&! /^(?=.{1,253}$)[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?$/.test(name))throw invalid();
 if(t.caCertificate!=null&&(typeof t.caCertificate!=='string'||!path.isAbsolute(t.caCertificate)))throw invalid();
 return Object.freeze({...t});
}
// Wire objects are flat. Reject duplicate keys (JSON.parse alone accepts them),
// nested values and unknown fields before trusting any identity or scope.
function flatJson(bytes){
 const text=new TextDecoder('utf-8',{fatal:true}).decode(bytes);let at=0;const found=new Set();
 const space=()=>{while(/\s/.test(text[at]||'!'))at++;};
 const string=()=>{const m=/^"(?:[^"\\\x00-\x1f]|\\(?:["\\/bfnrt]|u[0-9a-fA-F]{4}))*"/.exec(text.slice(at));if(!m)throw invalid();at+=m[0].length;return JSON.parse(m[0]);};
 space();if(text[at++]!=='{')throw invalid();space();
 if(text[at]!=='}')while(true){const key=string();if(found.has(key))throw invalid();found.add(key);space();if(text[at++]!==':')throw invalid();space();if(text[at]==='"')string();else if(text.startsWith('true',at))at+=4;else if(text.startsWith('false',at))at+=5;else throw invalid();space();if(text[at]!==',')break;at++;space();}
 if(text[at++]!=='}')throw invalid();space();if(at!==text.length)throw invalid();return JSON.parse(text);
}
function reader(socket){
 let ended=false,fault=null,wake=()=>{};
 const readable=()=>wake(),end=()=>{ended=true;wake();},failure=()=>{fault=invalid();wake();};
 socket.on('readable',readable);socket.on('end',end);socket.on('close',end);socket.on('error',failure);
 async function read(n){while(true){if(fault)throw fault;const value=socket.read(n);if(value!==null)return value;if(ended)throw invalid();await new Promise(r=>{wake=r;});}}
 return {read,frame:async()=>{const n=(await read(4)).readUInt32BE();if(n<1||n>4096)throw invalid();return flatJson(await read(n));},eof:async()=>{while(!ended&&!fault&&!socket.readableLength)await new Promise(r=>{wake=r;});if(fault||socket.readableLength||!ended)throw invalid();},detach:()=>{socket.off('readable',readable);socket.off('end',end);socket.off('close',end);socket.off('error',failure);socket.pause();}};
}
export class SimpleGatewayProvider {
 #target;#ca;#deadline;
 constructor(target,{timeoutMs=10000}={}){
  this.#target=validateTarget(target);this.#deadline=timeoutMs;
  if(!Number.isInteger(timeoutMs)||timeoutMs<1||timeoutMs>10000)throw invalid();
  if(target.caCertificate){const m=fs.lstatSync(target.caCertificate);if(!m.isFile()||m.isSymbolicLink()||m.size>262144)throw invalid();const cert=new X509Certificate(fs.readFileSync(target.caCertificate));this.#ca=cert.toString();}
 }
 login(input){return this.#exchange('login',input);}
 register(input){return this.#exchange('register',input);}
 // Native-only seam: fresh same-socket authentication, never a UI-issued identity.
 openNativeStream(input,{expectedNativeAccountId}={}){if(typeof expectedNativeAccountId!=='string'||! /^[1-9][0-9]*$/.test(expectedNativeAccountId))return Promise.reject(invalid());return this.#exchange('login',input,{retain:true,expectedNativeAccountId});}
 async #exchange(operation,{username,password},{retain=false,expectedNativeAccountId}={}){
  if(typeof username!=='string'||! /^[A-Za-z0-9_-]{1,32}$/.test(username)||typeof password!=='string'||Buffer.byteLength(password)<1||Buffer.byteLength(password)>72)throw invalid();
  const t=this.#target;
  const socket=tls.connect({host:t.host,port:t.port,servername:net.isIP(t.serverName)?undefined:t.serverName,ca:this.#ca,minVersion:'TLSv1.3',maxVersion:'TLSv1.3',rejectUnauthorized:true,checkServerIdentity:(_name,cert)=>tls.checkServerIdentity(t.serverName,cert)});
  const input=reader(socket);let handedOff=false;const timer=setTimeout(()=>socket.destroy(invalid()),this.#deadline);
  try{
   await new Promise((resolve,reject)=>{socket.once('secureConnect',resolve);socket.once('error',()=>reject(invalid()));});
   if(!(await input.read(13)).equals(Buffer.from('MEOWSIMPLE/1\n')))throw invalid();
   const challenge=await input.frame();const common=['type','mode','authority_id','realm_id','save_profile_id','connection_id'];
   const scoped=v=>v.mode==='simple'&&v.authority_id===t.authorityId&&v.realm_id===t.realmId&&v.save_profile_id===t.saveProfileId;
   if(!keys(challenge,[...common,'nonce'])||challenge.type!=='simple-challenge-v1'||!scoped(challenge)||! /^[a-f0-9]{32}$/.test(challenge.connection_id)||! /^[a-f0-9]{64}$/.test(challenge.nonce))throw invalid();
   const payload=Buffer.from(JSON.stringify({type:'simple-credentials-v1',operation,username,password,nonce:challenge.nonce}));const size=Buffer.alloc(4);size.writeUInt32BE(payload.length);
   const wire=Buffer.concat([size,payload]);try{await new Promise((resolve,reject)=>socket.write(wire,err=>err?reject(invalid()):resolve()));}finally{payload.fill(0);wire.fill(0);password='';}
   const response=await input.frame();
   if(!scoped(response)||response.connection_id!==challenge.connection_id)throw invalid();
   if(operation==='register'){
    if(!keys(response,[...common,'registered'])||response.type!=='simple-registered-v1'||response.registered!==true)throw invalid();await input.eof();return {registered:true};
   }
   if(!keys(response,[...common,'ready','username','native_account_id'])||response.type!=='simple-ready-v1'||response.ready!==true||response.username!==username||typeof response.native_account_id!=='string'||! /^[1-9][0-9]*$/.test(response.native_account_id)||BigInt(response.native_account_id)>9223372036854775807n)throw invalid();
   const identity={username,nativeAccountId:response.native_account_id,authorityId:t.authorityId,realmId:t.realmId};
   if(retain){if(response.native_account_id!==expectedNativeAccountId)throw invalid();input.detach();handedOff=true;return {identity,stream:socket};}
   return identity;
  }catch{throw invalid();}finally{clearTimeout(timer);if(!handedOff)socket.destroy();}
 }
}
export function loadGatewayProvider(exeDir){
 const file=path.join(exeDir,'beta.gateway.json');if(!fs.existsSync(file))return null;
 const m=fs.lstatSync(file);if(!m.isFile()||m.isSymbolicLink()||m.size>16384)throw invalid();
 const config=JSON.parse(fs.readFileSync(file,'utf8'));
 if(!keys(config,['schemaVersion','protocol','target'])||config.schemaVersion!==1||config.protocol!=='MEOWSIMPLE/1')throw invalid();
 return new SimpleGatewayProvider(config.target);
}
