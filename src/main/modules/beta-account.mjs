// Main-process account boundary. Providers are injected by native bootstrap;
// renderer input cannot select an endpoint or supply its own authenticated UID.
import {loadGatewayProvider} from './beta-gateway.mjs';
const problem=(code,message)=>Object.assign(new Error(message),{code});
const unavailable=()=>problem('account_provider_pending','尚未配置通用账号接入服务，请联系服主。');
export class BetaAccount {
  #provider; #credentials=null; #nativeStreams=new Set(); #session=null; #generation=0; #busy=false;
  constructor({provider=null}={}){this.#provider=provider;}
  #clear(){this.#generation++;this.#session=null;if(this.#credentials)this.#credentials.password.fill(0);this.#credentials=null;for(const stream of this.#nativeStreams)stream.destroy();this.#nativeStreams.clear();}
  view(){return this.#session ? {...this.#session} : null;}
  async logout(){this.#clear();return {loggedOut:true};}
  async login(input){
    if(this.#busy)throw problem('account_busy','账号操作正在处理中。');
    this.#clear();
    const username=input?.username, password=input?.password;
    if(typeof username!=='string'||! /^[A-Za-z0-9_-]{1,32}$/.test(username)||typeof password!=='string'||Buffer.byteLength(password)<1||Buffer.byteLength(password)>72)throw problem('account_input','请输入有效用户名和密码。');
    if(!this.#provider?.login)throw unavailable();
    const generation=this.#generation;this.#busy=true;
    try{
      const result=await this.#provider.login({username,password});
      if(generation!==this.#generation)throw problem('account_superseded','账号操作已取消，请重新登录。');
      if(!result||result.username!==username||typeof result.nativeAccountId!=='string'||! /^[1-9][0-9]*$/.test(result.nativeAccountId)||typeof result.authorityId!=='string'||!result.authorityId||typeof result.realmId!=='string'||!result.realmId)throw problem('account_response','账号服务返回了无效身份。');
      this.#session=Object.freeze({username,nativeAccountId:result.nativeAccountId,authorityId:result.authorityId,realmId:result.realmId});
      this.#credentials={username,password:Buffer.from(password)};
      return this.view();
    }finally{this.#busy=false;}
  }
  async openNativeStream(){
    if(!this.#session||!this.#credentials)throw problem('login_required','请先登录。');
    if(!this.#provider?.openNativeStream)throw problem('game_adapter_pending','原生同流接入尚未配置。');
    const generation=this.#generation,expected=this.#session;
    const connection=await this.#provider.openNativeStream({username:this.#credentials.username,password:this.#credentials.password.toString()},{expectedNativeAccountId:expected.nativeAccountId});
    if(generation!==this.#generation||!this.#session||connection.identity?.nativeAccountId!==expected.nativeAccountId||connection.identity?.authorityId!==expected.authorityId||connection.identity?.realmId!==expected.realmId){connection.stream?.destroy();throw problem('account_superseded','账号或目标已改变，游戏连接已关闭。');}
    this.#nativeStreams.add(connection.stream);connection.stream.once('close',()=>this.#nativeStreams.delete(connection.stream));return connection;
  }
  async register(input){
    if(this.#busy)throw problem('account_busy','账号操作正在处理中。');
    if(!this.#provider?.register)throw unavailable();
    const username=input?.username,password=input?.password;
    if(typeof username!=='string'||! /^[A-Za-z0-9_-]{1,32}$/.test(username)||typeof password!=='string'||Buffer.byteLength(password)<1||Buffer.byteLength(password)>72)throw problem('account_input','请输入有效用户名和密码。');
    this.#busy=true;
    try{const r=await this.#provider.register({username,password});if(r?.registered!==true)throw problem('account_response','注册尚未确认完成。');return {registered:true};}finally{this.#busy=false;}
  }
  async sendCode(){throw problem('account_recovery_unavailable','此通用接入服务尚未提供验证码接口。');}
  async resetPassword(){throw problem('account_recovery_unavailable','此通用接入服务尚未提供找回密码接口，请联系服主。');}
}
export function betaAccountHandler({client,windows,operation}){
  return async(event,input)=>{
    const win=windows.window;
    if(!win||event.sender!==win.webContents||event.senderFrame!==win.webContents.mainFrame)return {ok:false,error:{code:'account_window',message:'此窗口不能执行账号操作。'}};
    try{return {ok:true,data:await client[operation](input)};}
    catch(e){const messages={account_busy:'账号操作正在处理中。',account_input:'请输入有效用户名和密码。',account_response:'账号服务返回了无效身份。',account_provider_pending:'尚未配置通用账号接入服务，请联系服主。',account_superseded:'账号操作已取消，请重新登录。',account_recovery_unavailable:'此通用接入服务尚未配置找回接口，请联系服主。'};const code=Object.hasOwn(messages,e?.code)?e.code:'account_failed';return {ok:false,error:{code,message:messages[code]||'账号服务暂不可用，请稍后重试。'}};}
    finally{if(input&&typeof input==='object'){for(const key of ['password','confirmPassword','newPassword'])if(key in input)input[key]='';}}
  };
}

export function betaAccountFromDirectory(exeDir){
  try{return new BetaAccount({provider:loadGatewayProvider(exeDir)});}
  catch {const reject=async()=>{throw problem('account_response','通用网关配置无效，请联系服主。');};return new BetaAccount({provider:{login:reject,register:reject}});}
}
