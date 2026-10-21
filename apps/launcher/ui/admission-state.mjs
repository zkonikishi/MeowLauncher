// UI consumes summaries only. It never receives or constructs a game ticket.
const labels=Object.freeze({idle:'等待启动',authenticating:'正在认证',preparing:'正在检查客户端',connecting:'正在连接服务器',binding:'正在确认游戏会话',ready:'游戏会话已确认',cancelling:'正在确认取消结果',cancelled:'启动已取消',failed:'游戏准入未完成'});
export function presentAdmission(value, expectedRequestId){
 if(!value||value.contract!=='launcher-admission-v2'||value.requestId!==expectedRequestId||!Object.hasOwn(labels,value.phase)||!Number.isSafeInteger(value.revision)||value.revision<0||typeof value.pendingConfirmation!=='boolean')throw Error('invalid_admission_summary');
 // Unknown fields are ignored, never interpolated into status text or DOM.
 const pending=value.pendingConfirmation;
 if(value.phase==='ready'&&pending)throw Error('unconfirmed_admission_ready');
 return Object.freeze({phase:value.phase,label:pending?'结果尚未确认，正在等待服务器确认':labels[value.phase],kind:value.phase==='ready'?'success':value.phase==='failed'?'error':pending?'busy':'neutral',mayRetry:!pending&&['failed','cancelled'].includes(value.phase)});
}
export function describeAdapter(capabilities){
 if(capabilities?.contract!=='launcher-admission-v2'||capabilities.edition!=='sponsor'||capabilities.requiresGateway!==true)throw Error('invalid_admission_capability');
 return capabilities.nativeAdapterReady===true?'准入适配器已配置；进入游戏仍需服务端确认。':'当前保留原网关接入；新版准入适配器待接通，不能据此报告进服成功。';
}
