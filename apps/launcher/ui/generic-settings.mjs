// Only configuration assembly; native validation and host proofs remain mandatory.
export function reviewConfig({mode='remote',useBundledHost=true,hostExecutable='',playerTargetFile='',readyFile='',allowUnsignedCandidate=false}){
 if(!['local','remote'].includes(mode))throw Error('请选择本地或远端连接模式。');
 if(!playerTargetFile.trim())throw Error('请选择服主提供的频道配置文件。');
 if(mode==='local'&&!readyFile.trim())throw Error('本地模式需要本轮 ready.json。');
 if(!useBundledHost&&!hostExecutable.trim())throw Error('请选择匹配的客户端宿主。');
 return {mode,useBundledHost,hostExecutable:useBundledHost?'':hostExecutable,playerTargetFile,readyFile:mode==='remote'?'':readyFile,allowUnsignedCandidate};
}
