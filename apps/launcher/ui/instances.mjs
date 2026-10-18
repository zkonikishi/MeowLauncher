// UI ownership/state model only. Native verification remains authoritative.
// No credentials, endpoint allocation, protocol changes or process spawning.
const transitions=Object.freeze({
 created:['authenticating','closing','failed'],
 authenticating:['ready','closing','failed'],
 ready:['starting','closing','failed'],
 starting:['running','closing','failed'],
 running:['exited','closing','failed'],
 exited:['closing'],failed:['closing'],closing:['closed','failed'],closed:[]
});
const keys=(value,allowed)=>value&&typeof value==='object'&&!Array.isArray(value)&&Object.keys(value).every(k=>allowed.includes(k));
const text=(value,max)=>typeof value==='string'&&value.length>0&&value.length<=max&&!/[\u0000-\u001f]/.test(value);
export function createInstances(makeId=()=>crypto.randomUUID()){
 const instances=new Map(),owners=new Map();
 function require(id){const row=instances.get(id);if(!row)throw Error('Unknown instance');return row;}
 function snapshot(row){return Object.freeze({id:row.id,mode:row.mode,targetLabel:row.targetLabel,accountLabel:row.accountLabel,characterLabel:row.characterLabel,status:row.status,clientPid:row.clientPid});}
 return Object.freeze({
  create(input){
   if(!keys(input,['mode','targetLabel'])||!['account','simple'].includes(input.mode)||!text(input.targetLabel,256))throw Error('Invalid instance input');
   const id=makeId();if(!text(id,128)||instances.has(id))throw Error('Invalid instance ID');
   const row={id,mode:input.mode,targetLabel:input.targetLabel,accountLabel:null,characterLabel:null,status:'created',clientPid:null,resources:[],closing:null};instances.set(id,row);return snapshot(row);
  },
  transition(id,status){const row=require(id);if(!transitions[row.status].includes(status))throw Error('Invalid instance transition');if(status==='running')throw Error('Running requires verified native readiness');row.status=status;return snapshot(row);},
  identity(id,input){
   const row=require(id);
   if(!keys(input,['mode','accountId','accountLabel','authorityId','realmId','saveProfileId'])||row.status!=='authenticating'||input.mode!==row.mode||!text(input.accountLabel,80)||!['authorityId','realmId','saveProfileId'].every(k=>text(input[k],256)))throw Error('Invalid instance identity');
   if(input.mode==='simple'){
    if(typeof input.accountId!=='string'||! /^[1-9][0-9]{0,18}$/.test(input.accountId)||BigInt(input.accountId)>9223372036854775807n)throw Error('Invalid native account ID');
   }else if(typeof input.accountId!=='string'||! /^[0-9]{8}$/.test(input.accountId)||input.accountId==='00000000')throw Error('Invalid platform account ID');
   row.accountLabel=input.accountLabel;row.status='ready';return snapshot(row);
  },
  nativeReady(id,proof){
   const row=require(id);
   if(row.status!=='starting'||!keys(proof,['instanceId','verified','hostPid','clientPid'])||proof.instanceId!==id||proof.verified!==true||!Number.isSafeInteger(proof.hostPid)||proof.hostPid<=0||!Number.isSafeInteger(proof.clientPid)||proof.clientPid<=0||proof.hostPid===proof.clientPid)throw Error('Invalid native instance readiness');
   // "verified" is a native-layer result, never derived from child spawn or UI.
   row.clientPid=proof.clientPid;row.status='running';return snapshot(row);
  },
  character(id,label){const row=require(id);if(row.status!=='running'||!text(label,80))throw Error('Invalid character state');row.characterLabel=label;return snapshot(row);},
  resource(id,leaseId,release){
   const row=require(id);if(['closing','closed'].includes(row.status)||!text(leaseId,256)||typeof release!=='function'||owners.has(leaseId))throw Error('Resource already owned or invalid');
   owners.set(leaseId,id);row.resources.push({leaseId,release});
  },
  close(id){
   const row=require(id);if(row.closing)return row.closing;if(row.status==='closed')return Promise.resolve(snapshot(row));row.status='closing';
   const task=(async()=>{const retained=[];for(const resource of row.resources.splice(0).reverse()){try{if(owners.get(resource.leaseId)!==id)throw Error('Resource owner mismatch');await resource.release();if(owners.get(resource.leaseId)===id)owners.delete(resource.leaseId);}catch{retained.push(resource);}}row.resources=retained.reverse();const failed=retained.length>0;if(!failed){row.clientPid=null;row.characterLabel=null;}row.status=failed?'failed':'closed';return snapshot(row);})();
   row.closing=task;void task.finally(()=>{if(row.closing===task&&row.status!=='closed')row.closing=null;});return task;
  },
  list(){return [...instances.values()].map(snapshot);},
  get(id){return snapshot(require(id));}
 });
}
