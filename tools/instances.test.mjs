import test from 'node:test';
import assert from 'node:assert/strict';
import {createInstances} from '../apps/launcher/ui/instances.mjs';
const model=()=>{let id=0;return createInstances(()=>`instance-${++id}`);};
const identity=(mode='simple')=>({mode,accountId:mode==='simple'?'9223372036854775807':'00000001',accountLabel:'Test account',authorityId:'owner',realmId:'realm',saveProfileId:'save'});
test('A and B own independent release order; closing A never closes B',async()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Server A'}),b=registry.create({mode:'account',targetLabel:'Server B'}),closed=[];
 registry.resource(a.id,'session-A',async()=>closed.push('session-A'));registry.resource(a.id,'bridge-A',async()=>closed.push('bridge-A'));registry.resource(a.id,'job-A',async()=>closed.push('job-A'));registry.resource(b.id,'job-B',async()=>closed.push('job-B'));
 await registry.close(a.id);assert.deepEqual(closed,['job-A','bridge-A','session-A']);assert.equal(registry.get(b.id).status,'created');await registry.close(a.id);assert.equal(closed.length,3);await registry.close(b.id);assert.equal(closed.at(-1),'job-B');
});
test('double close shares one cleanup; failure still attempts only own remaining releases',async()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Server'}),b=registry.create({mode:'simple',targetLabel:'Server'}),closed=[];
 registry.resource(a.id,'session-A',async()=>closed.push('session-A'));registry.resource(a.id,'job-A',async()=>{closed.push('job-A');throw Error('fixture');});registry.resource(b.id,'job-B',async()=>closed.push('job-B'));
 const one=registry.close(a.id),two=registry.close(a.id);assert.equal(one,two);await one;assert.deepEqual(closed,['job-A','session-A']);assert.equal(registry.get(a.id).status,'failed');assert.equal(registry.get(b.id).status,'created');
});
test('same namespace lease cannot be assigned twice, even to a second account',()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Server'}),b=registry.create({mode:'simple',targetLabel:'Server'});registry.resource(a.id,'127.0.0.4:7002',async()=>{});assert.throws(()=>registry.resource(b.id,'127.0.0.4:7002',async()=>{}));
});
test('identity is server supplied, keeps IDs as strings and never contains credentials',()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Local or cloud'});registry.transition(a.id,'authenticating');registry.identity(a.id,identity());assert.equal(registry.get(a.id).accountLabel,'Test account');assert.equal(registry.get(a.id).characterLabel,null);assert(!JSON.stringify(registry.list()).includes('password'));
 const b=registry.create({mode:'account',targetLabel:'Meowopia'});registry.transition(b.id,'authenticating');assert.throws(()=>registry.identity(b.id,identity()));assert.throws(()=>registry.identity(b.id,{...identity('account'),password:'forbidden'}));registry.identity(b.id,identity('account'));
});
test('spawn alone and stale cross-instance event cannot mark running',()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Server'});registry.transition(a.id,'authenticating');registry.identity(a.id,identity());registry.transition(a.id,'starting');assert.throws(()=>registry.transition(a.id,'running'));assert.throws(()=>registry.nativeReady(a.id,{instanceId:'other',verified:true,hostPid:1,clientPid:2}));assert.throws(()=>registry.nativeReady(a.id,{instanceId:a.id,verified:false,hostPid:1,clientPid:2}));registry.nativeReady(a.id,{instanceId:a.id,verified:true,hostPid:1,clientPid:2});registry.character(a.id,'Server-provided role');assert.equal(registry.get(a.id).status,'running');
});
test('unknown input secrets and wrong numeric IDs are rejected',()=>{
 const registry=model();assert.throws(()=>registry.create({mode:'simple',targetLabel:'Server',password:'forbidden'}));const a=registry.create({mode:'simple',targetLabel:'Server'});registry.transition(a.id,'authenticating');for(const accountId of [1,'0','01','9223372036854775808'])assert.throws(()=>registry.identity(a.id,{...identity(),accountId}));
});
test('failed cleanup quarantines its own lease and PID until a targeted retry succeeds',async()=>{
 const registry=model(),a=registry.create({mode:'simple',targetLabel:'Server'}),b=registry.create({mode:'simple',targetLabel:'Server'});registry.transition(a.id,'authenticating');registry.identity(a.id,identity());registry.transition(a.id,'starting');registry.nativeReady(a.id,{instanceId:a.id,verified:true,hostPid:101,clientPid:102});
 let attempts=0;registry.resource(a.id,'owned-job-A',async()=>{if(++attempts===1)throw Error('cleanup uncertain');});
 await registry.close(a.id);assert.equal(registry.get(a.id).status,'failed');assert.equal(registry.get(a.id).clientPid,102);assert.throws(()=>registry.resource(b.id,'owned-job-A',async()=>{}));
 await registry.close(a.id);assert.equal(attempts,2);assert.equal(registry.get(a.id).status,'closed');assert.equal(registry.get(a.id).clientPid,null);registry.resource(b.id,'owned-job-A',async()=>{});
});
