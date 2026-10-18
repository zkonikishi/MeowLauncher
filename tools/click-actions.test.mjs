import assert from 'node:assert/strict';
import {bindClickActions} from '../apps/launcher/ui/click-actions.mjs';
const handlers={},jobs=new Map();let id=0,single=0,double=0;
const oldSet=globalThis.setTimeout,oldClear=globalThis.clearTimeout;
globalThis.setTimeout=f=>{jobs.set(++id,f);return id;};globalThis.clearTimeout=k=>jobs.delete(k);
const element={addEventListener:(n,f)=>handlers[n]=f,removeEventListener:n=>delete handlers[n]};
try{
 const dispose=bindClickActions(element,()=>single++,()=>double++);
 handlers.click({detail:1});assert.equal(single,0);[...jobs.values()].forEach(f=>f());jobs.clear();assert.equal(single,1);
 handlers.click({detail:1});handlers.click({detail:2});handlers.dblclick({preventDefault(){}});assert.equal(jobs.size,0);assert.equal(single,1);assert.equal(double,1);
 handlers.click({detail:0});assert.equal(single,2);
 handlers.click({detail:1});dispose();assert.equal(jobs.size,0);assert.equal(Object.keys(handlers).length,0);
 console.log('PASS: deferred single click, double-click isolation, keyboard activation, disposal');
}finally{globalThis.setTimeout=oldSet;globalThis.clearTimeout=oldClear;}
