import assert from 'node:assert/strict';
import fs from 'node:fs';
import tls from 'node:tls';
import {SimpleGatewayProvider,validateTarget,loadGatewayProvider} from '../src/main/modules/beta-gateway.mjs';
const fixture=process.argv[2]||'D:/Ai/Codex/artifacts/beta-gateway-20261021/fixture';
const certificate=fs.readFileSync(fixture+'/cert.pem'),key=fs.readFileSync(fixture+'/key.pem');
let mode='login',credentials=0,passed=0;const sockets=new Set();
const frame=v=>{const body=Buffer.from(typeof v==='string'?v:JSON.stringify(v)),size=Buffer.alloc(4);size.writeUInt32BE(body.length);return Buffer.concat([size,body]);};
const common={mode:'simple',authority_id:'fixture-authority',realm_id:'fixture-realm',save_profile_id:'fixture-profile',connection_id:'a'.repeat(32)};
const server=tls.createServer({cert:certificate,key,minVersion:'TLSv1.3',maxVersion:'TLSv1.3'},socket=>{
 const testMode=mode;sockets.add(socket);socket.on('error',()=>{});socket.on('close',()=>sockets.delete(socket));
 if(testMode==='timeout')return;
 const challenge={type:'simple-challenge-v1',...common,nonce:'b'.repeat(64)};
 if(testMode==='wrong-scope')challenge.realm_id='wrong-realm';
 if(testMode==='bad-magic'){socket.end('X'.repeat(13));return;}
 if(testMode==='oversize'){socket.write('MEOWSIMPLE/1\n');const n=Buffer.alloc(4);n.writeUInt32BE(4097);socket.write(n);return;}
 const wire=Buffer.concat([Buffer.from('MEOWSIMPLE/1\n'),frame(challenge)]);
 socket.write(wire.subarray(0,5));queueMicrotask(()=>socket.write(wire.subarray(5)));
 let buffer=Buffer.alloc(0);
 socket.on('data',chunk=>{
  buffer=Buffer.concat([buffer,chunk]);if(buffer.length<4||buffer.length<4+buffer.readUInt32BE())return;
  const request=JSON.parse(buffer.subarray(4,4+buffer.readUInt32BE()).toString());credentials++;assert.equal(request.password,'isolated-password');assert.equal(request.nonce,challenge.nonce);
  if(request.operation==='register') {socket.end(Buffer.concat([frame({type:'simple-registered-v1',...common,registered:true}),testMode==='register-trailing'?Buffer.from('x'):Buffer.alloc(0)]));return;}
  let response={type:'simple-ready-v1',...common,ready:true,username:request.username,native_account_id:'42'};
  if(testMode==='wrong-user')response.username='Other';if(testMode==='wrong-connection')response.connection_id='c'.repeat(32);if(testMode==='huge-id')response.native_account_id='9223372036854775808';
  if(testMode==='duplicate')response=JSON.stringify(response).replace('"native_account_id":"42"','"native_account_id":"42","native_account_id":"43"');
  socket.end(frame(response));
 });
});
server.on('tlsClientError',()=>{});await new Promise(r=>server.listen(0,'127.0.0.1',r));
const target={host:'127.0.0.1',port:server.address().port,serverName:'beta-fixture',authorityId:'fixture-authority',realmId:'fixture-realm',saveProfileId:'fixture-profile',caCertificate:fixture+'/cert.pem'};
const input=()=>({username:'Player',password:'isolated-password'});
const test=async(name,fn)=>{await fn();passed++;};
try{
 await test('real TLS login and split frame',async()=>{assert.deepEqual(await new SimpleGatewayProvider(target).login(input()),{username:'Player',nativeAccountId:'42',authorityId:'fixture-authority',realmId:'fixture-realm'});});
 await test('registration confirms EOF',async()=>{assert.deepEqual(await new SimpleGatewayProvider(target).register(input()),{registered:true});});
 for(const value of ['wrong-scope','bad-magic','oversize','wrong-user','wrong-connection','huge-id','duplicate'])await test(value,async()=>{mode=value;const before=credentials;await assert.rejects(new SimpleGatewayProvider(target).login(input()),{code:'account_response'});if(['wrong-scope','bad-magic','oversize'].includes(value))assert.equal(credentials,before,'credentials sent before trusted challenge');});
 await test('registration rejects trailing native bytes',async()=>{mode='register-trailing';await assert.rejects(new SimpleGatewayProvider(target).register(input()));});
 await test('timeout bounded',async()=>{mode='timeout';await assert.rejects(new SimpleGatewayProvider(target,{timeoutMs:40}).login(input()));});
 await test('TLS requires trusted CA',async()=>{mode='login';const t={...target};delete t.caCertificate;await assert.rejects(new SimpleGatewayProvider(t).login(input()));});
 await test('TLS requires correct certificate identity',async()=>{await assert.rejects(new SimpleGatewayProvider({...target,serverName:'wrong-fixture'}).login(input()));});
 await test('public config rejects private unknown fields',async()=>{assert.throws(()=>validateTarget({...target,databasePassword:'not-allowed'}));});
 await test('no default endpoint',async()=>{assert.equal(loadGatewayProvider(fixture),null);});
 console.log(JSON.stringify({suite:'beta-gateway-real-tls',passed,network:'loopback TLS1.3 only',productionChanged:false,gameStarted:false}));
}finally{for(const socket of sockets)socket.destroy();await new Promise(r=>server.close(r));}
